/*
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! The C# redaction test corpus (`TextRedactorBehaviourTests`, `RedactionRuleControlsTests`,
//! `RedactionRuleCatalogTests`, `TextRedactorPerformanceTests`), exact output per rule with the
//! cases that must survive, plus the Rust-side contracts: the input cap, the fail-closed budget
//! and the stable pseudonyms.

use std::time::{Duration, Instant};

use super::*;

fn mask(input: &str) -> String {
    redact(input, RedactionOptions::default())
        .expect("redaction finishes")
        .text
}

fn pseudonymize(input: &str) -> String {
    redact(
        input,
        RedactionOptions {
            pseudonymize_hosts: true,
        },
    )
    .expect("redaction finishes")
    .text
}

fn assert_masked(cases: &[(&str, &str)]) {
    for (input, expected) in cases {
        assert_eq!(mask(input), *expected, "input: {input:?}");
    }
}

fn assert_survives(inputs: &[&str]) {
    for input in inputs {
        assert_eq!(mask(input), *input, "input: {input:?}");
    }
}

const MIXED_SAMPLE: &str = concat!(
    "$ ssh bob@hostA\n",
    "Password: hunter2\n",
    "Last login: Mon Oct 19 on pts/0\n",
    "bob@hostA:~$ export DB_PASSWORD=hunter2\n",
    "bob@hostA:~$ mysql -u root -pS3cret mydb\n",
    "curl -H 'Authorization: Bearer eyJhbGciOiJI.eyJzdWIiOiIx.c2lnbmF0dXJl' https://web.example.com\n",
    "token=abc123 postgres://admin:s3cr3t@10.1.2.3:5432/app\n",
    "AKIAIOSFODNN7EXAMPLE xoxb-1234567890-abcdefghij\n",
    "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----\n",
    "Password:\nhunter2\n",
    "max_tokens=4096 v2026.091401.1\n",
);

// Catalogue.

#[test]
fn catalogue_rule_ids_match_the_pinned_list() {
    assert_eq!(
        rule_ids(),
        [
            "private-key-block",
            "authorization-header",
            "cookie-header",
            "url-credentials",
            "cli-password",
            "password-prompt",
            "key-value-secret",
            "aws-access-key",
            "github-token",
            "slack-token",
            "jwt",
            "ipv4-address",
            "host-name",
        ]
    );
}

#[test]
fn catalogue_rule_ids_are_unique_kebab_case() {
    let ids = rule_ids();
    for (index, id) in ids.iter().enumerate() {
        assert!(!ids[index + 1..].contains(id), "duplicate {id}");
        assert!(
            id.split('-').all(|part| !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())),
            "{id} is not kebab-case"
        );
    }
}

#[test]
fn catalogue_only_the_pseudonym_rules_require_the_option() {
    let opt_in: Vec<&str> = CATALOGUE
        .iter()
        .filter(|rule| rule.requires_pseudonymization)
        .map(|rule| rule.id)
        .collect();
    assert_eq!(opt_in, ["ipv4-address", "host-name"]);
}

#[test]
fn every_pattern_compiles() {
    let started = Instant::now();
    patterns::warm_up();
    eprintln!("pattern compilation took {:?}", started.elapsed());
}

#[test]
fn mask_token_is_built_from_the_constant_format() {
    assert_eq!(mask_token("jwt"), "[masked:jwt]");
    assert!(is_mask_token("[masked:jwt]"));
    assert!(!is_mask_token("masked:jwt"));
    assert!(!is_mask_token("[masked:unknown-rule]"));
    assert!(!is_mask_token("[masked:jwt]hunter2]"));
}

// Behaviour.

#[test]
fn counts_contain_every_rule_with_zero_for_untouched_rules() {
    let result = redact("hello world", RedactionOptions::default()).unwrap();

    let ids: Vec<&str> = result.counts.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, rule_ids());
    assert!(result.counts.iter().all(|(_, n)| *n == 0));
    assert!(!result.truncated);
}

#[test]
fn counts_two_planted_secrets_of_one_rule_count_two() {
    let result = redact(
        "password=a1b2\nDB_PASSWORD=c3d4\n",
        RedactionOptions::default(),
    )
    .unwrap();

    assert_eq!(
        result.text,
        "password=[masked:key-value-secret]\nDB_PASSWORD=[masked:key-value-secret]\n"
    );
    assert_eq!(result.count("key-value-secret"), 2);
    assert_eq!(result.count("jwt"), 0);
    assert_eq!(result.count("cli-password"), 0);
}

#[test]
fn overlap_bearer_jwt_is_one_token_counted_for_the_authorization_rule() {
    let result = redact(
        "Authorization: Bearer eyJhbGciOiJI.eyJzdWIiOiIx.c2lnbmF0dXJl",
        RedactionOptions::default(),
    )
    .unwrap();

    assert_eq!(
        result.text,
        "Authorization: Bearer [masked:authorization-header]"
    );
    assert_eq!(result.count("authorization-header"), 1);
    assert_eq!(result.count("jwt"), 0);
}

#[test]
fn overlap_jwt_as_key_value_is_counted_once_for_key_value_secret() {
    let result = redact(
        "token=eyJhbGciOiJI.eyJzdWIiOiIx.c2lnbmF0dXJl",
        RedactionOptions::default(),
    )
    .unwrap();

    assert_eq!(result.text, "token=[masked:key-value-secret]");
    assert_eq!(result.count("key-value-secret"), 1);
    assert_eq!(result.count("jwt"), 0);
}

#[test]
fn overlap_prompt_with_an_earlier_masked_value_is_not_counted_twice() {
    let result = redact("Password: --password hunter2", RedactionOptions::default()).unwrap();

    assert_eq!(result.total(), 1);
}

#[test]
fn mixed_sample_masks_every_secret_and_keeps_the_rest() {
    assert_eq!(
        mask(MIXED_SAMPLE),
        concat!(
            "$ ssh bob@hostA\n",
            "Password: [masked:password-prompt]\n",
            "Last login: Mon Oct 19 on pts/0\n",
            "bob@hostA:~$ export DB_PASSWORD=[masked:cli-password]\n",
            "bob@hostA:~$ mysql -u root -p[masked:cli-password] mydb\n",
            "curl -H 'Authorization: Bearer [masked:authorization-header]' https://web.example.com\n",
            "token=[masked:key-value-secret] postgres://admin:[masked:url-credentials]@10.1.2.3:5432/app\n",
            "[masked:aws-access-key] [masked:slack-token]\n",
            "[masked:private-key-block]\n",
            "Password:\n[masked:password-prompt]\n",
            "max_tokens=4096 v2026.091401.1\n",
        )
    );
}

#[test]
fn redaction_is_idempotent_and_the_second_pass_counts_zero() {
    for pseudonymize_hosts in [false, true] {
        let options = RedactionOptions { pseudonymize_hosts };
        let sample =
            format!("{MIXED_SAMPLE}ping 10.1.2.4 web.example.com carol@hostB\nPassword: x y z\n");

        let first = redact(&sample, options).unwrap();
        let second = redact(&first.text, options).unwrap();

        assert_eq!(first.text, second.text);
        assert_eq!(second.total(), 0);
    }
}

#[test]
fn pseudonyms_same_host_same_pseudonym_new_host_next_number() {
    let options = RedactionOptions {
        pseudonymize_hosts: true,
    };

    let result = redact(
        "a@hostA x\nb@hostA y\nc@hostB z\n10.0.0.1 10.0.0.2 10.0.0.1",
        options,
    )
    .unwrap();

    assert_eq!(
        result.text,
        "a@host-1 x\nb@host-1 y\nc@host-2 z\nip-1 ip-2 ip-1"
    );
    assert_eq!(result.count("host-name"), 3);
    assert_eq!(result.count("ipv4-address"), 3);
}

#[test]
fn pseudonyms_are_stable_across_the_two_host_forms_and_ignore_case() {
    let result = pseudonymize(
        "ssh bob@Web.Example.com\ncurl https://web.example.com/x\nping WEB.example.COM",
    );

    assert_eq!(result, "ssh bob@host-1\ncurl https://host-1/x\nping host-1");
}

#[test]
fn pseudonyms_restart_at_one_for_every_call() {
    assert_eq!(pseudonymize("10.0.0.9"), "ip-1");
    assert_eq!(pseudonymize("10.0.0.8 10.0.0.9"), "ip-1 ip-2");
}

#[test]
fn pseudonyms_are_off_by_default() {
    let input = "a@hostA 10.0.0.1 web.example.com";

    assert_eq!(mask(input), input);
    assert!(!RedactionOptions::default().pseudonymize_hosts);
}

// Private keys.

#[test]
fn private_key_full_block_is_replaced_by_one_token() {
    assert_masked(&[
        (
            "before\n-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\nabcd+/==\n-----END RSA PRIVATE KEY-----\nafter",
            "before\n[masked:private-key-block]\nafter",
        ),
        (
            "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkq\n-----END PRIVATE KEY-----",
            "[masked:private-key-block]",
        ),
        (
            "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk=\n-----END OPENSSH PRIVATE KEY-----\nok",
            "[masked:private-key-block]\nok",
        ),
        (
            "-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,ABCDEF\n\nMIIEowIBAAKC\n-----END RSA PRIVATE KEY-----\nafter",
            "[masked:private-key-block]\nafter",
        ),
        (
            "-----BEGIN PRIVATE KEY-----\r\nMIIEvQIB\r\n-----END PRIVATE KEY-----\r\nafter",
            "[masked:private-key-block]\r\nafter",
        ),
        (
            "{\"k\":\"-----BEGIN PRIVATE KEY-----\\nMIIEvQ\\n-----END PRIVATE KEY-----\\n\"}",
            "{\"k\":\"[masked:private-key-block]\\n\"}",
        ),
    ]);
}

#[test]
fn private_key_end_without_begin_masks_the_base64_lines_before_it() {
    let input = "MIIEowIBAAKCAQEA\nabcd+/==\n-----END RSA PRIVATE KEY-----\nuser@host:~$ ls";

    assert_eq!(mask(input), "[masked:private-key-block]\nuser@host:~$ ls");
}

#[test]
fn private_key_begin_without_end_masks_up_to_the_end_of_the_base64_lines() {
    let input = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkq\nAbCdEfGh\nuser@host:~$ ls";

    assert_eq!(mask(input), "[masked:private-key-block]\nuser@host:~$ ls");
}

#[test]
fn private_key_certificates_and_public_keys_survive() {
    assert_survives(&[
        "-----BEGIN CERTIFICATE-----\nMIIDdzCCAl+gAwIBAgI\n-----END CERTIFICATE-----",
        "-----BEGIN PUBLIC KEY-----\nMFwwDQYJKoZIhvcNAQE\n-----END PUBLIC KEY-----",
    ]);
}

// Authorization headers.

#[test]
fn authorization_header_token_is_masked() {
    assert_masked(&[
        (
            "curl -H 'Authorization: Bearer abcdef123456' https://x",
            "curl -H 'Authorization: Bearer [masked:authorization-header]' https://x",
        ),
        (
            "Authorization: Basic dXNlcjpwYXNzd29yZA==",
            "Authorization: Basic [masked:authorization-header]",
        ),
        (
            "authorization:bearer abcdef123456",
            "authorization:bearer [masked:authorization-header]",
        ),
        (
            "Authorization: Token abcdef123456",
            "Authorization: Token [masked:authorization-header]",
        ),
        (
            "Authorization: Digest username=\"bob\", realm=\"x\", response=\"6629fae49393a05397450978507c4ef1\"",
            "Authorization: Digest [masked:authorization-header]",
        ),
    ]);
}

#[test]
fn authorization_header_unrelated_text_survives() {
    assert_survives(&[
        "WWW-Authenticate: Basic realm=\"x\"",
        "Visual Basic 6.0",
        "Basic auth enabled",
        "Authorization: Token",
        "Authorization: Digest",
    ]);
}

// URL credentials.

#[test]
fn url_credentials_password_is_masked() {
    assert_masked(&[
        (
            "postgres://admin:s3cr3t@db.local:5432/app",
            "postgres://admin:[masked:url-credentials]@db.local:5432/app",
        ),
        (
            "ssh://bob:p%40ss@host",
            "ssh://bob:[masked:url-credentials]@host",
        ),
    ]);
}

#[test]
fn url_credentials_unrelated_text_survives() {
    assert_survives(&[
        "user@host",
        "scp file user@host:/tmp",
        "12:30:45@x",
        "http://host:8080/path",
        "ssh://user@host:22",
    ]);
}

// Command-line passwords.

#[test]
fn cli_password_is_masked() {
    assert_masked(&[
        (
            "mysql -u root -pS3cret mydb",
            "mysql -u root -p[masked:cli-password] mydb",
        ),
        (
            "mysql -u root -p S3cret mydb",
            "mysql -u root -p [masked:cli-password] mydb",
        ),
        (
            "mysqldump -h db -p'a b c' x",
            "mysqldump -h db -p'[masked:cli-password]' x",
        ),
        ("mariadb -pS3cret", "mariadb -p[masked:cli-password]"),
        (
            "mysqladmin -uroot -pS3cret status",
            "mysqladmin -uroot -p[masked:cli-password] status",
        ),
        (
            "run --password hunter2",
            "run --password [masked:cli-password]",
        ),
        (
            "run --password=hunter2",
            "run --password=[masked:cli-password]",
        ),
        ("run --passwd=hunter2", "run --passwd=[masked:cli-password]"),
        (
            "run --password=\"a b\"",
            "run --password=\"[masked:cli-password]\"",
        ),
        (
            "sshpass -p hunter2 ssh bob@host",
            "sshpass -p [masked:cli-password] ssh bob@host",
        ),
        (
            "sshpass -p'x y' ssh h",
            "sshpass -p'[masked:cli-password]' ssh h",
        ),
        (
            "export DB_PASSWORD=hunter2",
            "export DB_PASSWORD=[masked:cli-password]",
        ),
        (
            "export ADMIN_PWD=x1y2",
            "export ADMIN_PWD=[masked:cli-password]",
        ),
        (
            "export DBPASSWD=hunter2",
            "export DBPASSWD=[masked:cli-password]",
        ),
        (
            "PGPASSWORD=hunter2 psql -h db",
            "PGPASSWORD=[masked:cli-password] psql -h db",
        ),
        (
            "MYSQL_PWD=hunter2 mysql -u root",
            "MYSQL_PWD=[masked:cli-password] mysql -u root",
        ),
    ]);
}

#[test]
fn cli_password_unrelated_text_survives() {
    assert_survives(&[
        "ssh -p22 host",
        "ssh -p 2222 host",
        "ps -p 1234",
        "lsof -p1234",
        "docker run -p8080:80",
        "mkdir -pv a",
        "tcpdump -pni eth0",
        "gcc -pthread",
        "grep -P x",
        "PWD=/home/user",
        "OLDPWD=/tmp",
        "cmd --password-file /etc/x",
        "mysql -P 3306 -h host",
        "mysql -u root -p",
        "mysql --protocol=tcp -h h",
    ]);
}

// Key-value secrets.

#[test]
fn key_value_secret_value_is_masked() {
    assert_masked(&[
        ("password=hunter2", "password=[masked:key-value-secret]"),
        ("Password=Hunter2", "Password=[masked:key-value-secret]"),
        (
            "DB_PASSWORD_X=1 DB_PASSWORD=\"hunter two\"",
            "DB_PASSWORD_X=1 DB_PASSWORD=\"[masked:key-value-secret]\"",
        ),
        (
            "{\"password\":\"x1y2\",\"user\":\"a\"}",
            "{\"password\":\"[masked:key-value-secret]\",\"user\":\"a\"}",
        ),
        (
            "aws_secret_access_key = AbC/dEf+1",
            "aws_secret_access_key = [masked:key-value-secret]",
        ),
        (
            "client_secret: abc123",
            "client_secret: [masked:key-value-secret]",
        ),
        (
            "api-key=zzz999&x=1",
            "api-key=[masked:key-value-secret]&x=1",
        ),
        ("apikey: zzz999", "apikey: [masked:key-value-secret]"),
        (
            "client-key-data: LS0tLS1CRUdJTg==",
            "client-key-data: [masked:key-value-secret]",
        ),
        (
            "GET /x?token=abc123&y=1",
            "GET /x?token=[masked:key-value-secret]&y=1",
        ),
        (
            "'secret': 'abc 123'",
            "'secret': '[masked:key-value-secret]'",
        ),
    ]);
}

#[test]
fn key_value_secret_unrelated_text_survives() {
    assert_survives(&[
        "max_tokens=4096",
        "TOKEN_TTL=3600",
        "token.json",
        "./secrets:",
        "cat secret.txt",
        "tokenizer = x",
        "secret=",
    ]);
}

// Token shapes.

#[test]
fn token_shapes_are_masked() {
    assert_masked(&[
        (
            "key AKIAIOSFODNN7EXAMPLE end",
            "key [masked:aws-access-key] end",
        ),
        ("ASIAIOSFODNN7EXAMPLE", "[masked:aws-access-key]"),
        ("xoxb-1234567890-abcdefghij", "[masked:slack-token]"),
        (
            "hook xoxp-1234567890-abcdefghij-zz ok",
            "hook [masked:slack-token] ok",
        ),
        (
            "x eyJhbGciOiJI.eyJzdWIiOiIx.c2lnbmF0dXJl y",
            "x [masked:jwt] y",
        ),
        (
            "github_pat_11ABCDEFG0abcdefghijkl_mnopqrstuv",
            "[masked:github-token]",
        ),
    ]);
}

#[test]
fn github_classic_token_is_masked() {
    let token = format!("ghp_{}", "a".repeat(36));

    assert_eq!(
        mask(&format!("push {token} done")),
        "push [masked:github-token] done"
    );
}

#[test]
fn token_shapes_unrelated_text_survives() {
    assert_survives(&[
        "3f786850e387550fdab836ed7e6dc881de23001b",
        "v2026.091401.1",
        "10.0.26200",
        "api.github.com",
        "Heimdall.Core.Redaction",
        "ghp_test.sh",
        "fooxoxb-1234567890abc",
        "AKIAIOSFODNN7EXAMPLEX",
        "xAKIAIOSFODNN7EXAMPLE",
        "eyJhbGci.short",
    ]);
}

// Echoed password prompts.

#[test]
fn password_prompt_echoed_password_is_masked() {
    assert_masked(&[
        ("Password: hunter2", "Password: [masked:password-prompt]"),
        (
            "[sudo] password for bob: hunter2",
            "[sudo] password for bob: [masked:password-prompt]",
        ),
        (
            "Password for 'https://bob@host': hunter2",
            "Password for 'https://bob@host': [masked:password-prompt]",
        ),
        (
            "Enter passphrase for key '/home/bob/.ssh/id': my phrase",
            "Enter passphrase for key '/home/bob/.ssh/id': [masked:password-prompt]",
        ),
        (
            "Mot de passe : secret",
            "Mot de passe : [masked:password-prompt]",
        ),
        ("Passwort: geheim", "Passwort: [masked:password-prompt]"),
        (
            "Contrase\u{F1}a: clave",
            "Contrase\u{F1}a: [masked:password-prompt]",
        ),
        (
            "CONTRASE\u{D1}A: clave",
            "CONTRASE\u{D1}A: [masked:password-prompt]",
        ),
        ("Contrasena: clave", "Contrasena: [masked:password-prompt]"),
        (
            "Password:\nhunter2\nuser@host:~$ ls",
            "Password:\n[masked:password-prompt]\nuser@host:~$ ls",
        ),
        (
            "Password:\n-bash: hunter2: command not found",
            "Password:\n-bash: [masked:password-prompt]: command not found",
        ),
        (
            "Login incorrect\nbash: hunter2: command not found",
            "Login incorrect\nbash: [masked:password-prompt]: command not found",
        ),
        (
            "Password:\r\nhunter2\r\nok now",
            "Password:\r\n[masked:password-prompt]\r\nok now",
        ),
    ]);
}

#[test]
fn password_prompt_unrelated_text_survives() {
    assert_survives(&[
        "Password:\nLast login: Mon Oct 19 10:00:00 on pts/0",
        "Password:\nuser@host:~$ ls",
        "Password:\nuser@host:~$",
        "Password:\nroot#",
        "Password:\n\nhunter2",
        "Password:\nSorry, try again.",
        "Password:\nPermission denied, please try again.",
        "Password:\nPassword:",
        "Password:",
        "Password: \"\"",
        "Password: ''",
        "bash: foo: command not found",
        "Password:\nLast login: x\nfoo bar\nbaz qux\nbash: x: command not found",
        "the password policy is strict",
    ]);
}

// Pseudonyms.

#[test]
fn pseudonyms_when_enabled_replace_addresses_and_hosts() {
    for (input, expected) in [
        (
            "ssh 10.1.2.3 and 10.1.2.4 then 10.1.2.3",
            "ssh ip-1 and ip-2 then ip-1",
        ),
        ("ping 192.168.0.10.", "ping ip-1."),
        ("bob@hostA:~$ ls", "bob@host-1:~$ ls"),
        ("curl https://web.example.com/x", "curl https://host-1/x"),
        ("a.b.example.org", "host-1"),
        ("api.github.com", "host-1"),
    ] {
        assert_eq!(pseudonymize(input), expected, "input: {input:?}");
    }
}

#[test]
fn pseudonyms_when_enabled_unrelated_text_survives() {
    for input in [
        "0.0.0.0",
        "127.0.0.1",
        "255.255.255.255",
        "255.255.255.0",
        "255.255.240.0",
        "255.0.0.0",
        "999.1.1.1",
        "1.2.3.4.5",
        "10.0.26200",
        "10.0.26200.1234",
        "config.yaml",
        "Heimdall.Core.dll",
        "a.b.json",
        "foo.tar.gz",
        "app.min.js",
        "libssl.so.3",
        "backup.2026.sql.bak",
        "site.min.css.map",
        "server.cert.pem",
        "v2026.091401.1",
        "ip-1 host-1",
        "bob@host-1:~$",
    ] {
        assert_eq!(pseudonymize(input), input, "input: {input:?}");
    }
}

#[test]
fn pseudonyms_when_disabled_everything_survives() {
    assert_survives(&["ssh 10.1.2.3 as bob@hostA", "web.example.com 192.168.0.10"]);
}

// Performance and adversarial input.

const BENIGN_BUFFER_BYTES: usize = 5 * 1024 * 1024;

/// Generous: the reference run is a fraction of it, CI runners and debug builds are slow.
const BENIGN_BOUND: Duration = Duration::from_secs(50);

/// Far above the production budget: an unoptimised build runs the regex engine many times slower,
/// and these tests are about the work being linear, not about the speed of a debug build.
const TEST_RULE_BUDGET: Duration = Duration::from_secs(120);

const RUN_LENGTH: usize = 4096;

const BEGIN_LINE_COUNT: usize = 1000;

const REPEATED_KEY_COUNT: usize = 100_000;

const BENIGN_LINES: [&str; 12] = [
    "drwxr-xr-x  2 bob bob 4096 Oct 19 12:00 .",
    "-rw-r--r--  1 bob bob  512 Oct 19 12:00 token.json",
    "bob@host:~$ ls -la /var/log",
    "max_tokens=4096 TOKEN_TTL=3600 retries=3",
    "ssh -p 2222 host && ps -p 1234 && docker run -p8080:80 img",
    "commit 3f786850e387550fdab836ed7e6dc881de23001b (tag: v2026.091401.1)",
    "Microsoft Windows [Version 10.0.26200.1234]",
    "[INFO] Heimdall.Core.Redaction started in 12 ms",
    "-----BEGIN CERTIFICATE-----",
    "MIIDdzCCAl+gAwIBAgIJAKx2",
    "-----END CERTIFICATE-----",
    "Last login: Mon Oct 19 10:00:00 2026 from 10.0.0.5",
];

#[test]
fn adversarial_inputs_finish_without_a_timeout() {
    let inputs: Vec<(&str, String)> = vec![
        ("word run", "a".repeat(RUN_LENGTH)),
        ("underscore run", "a_".repeat(RUN_LENGTH / 2)),
        ("dot chain", "ab.".repeat(RUN_LENGTH / 3)),
        ("dash chain", "a-".repeat(RUN_LENGTH / 2)),
        ("password word run", "password".repeat(RUN_LENGTH / 8)),
        ("jwt prefix run", "eyJ".repeat(RUN_LENGTH / 3)),
        ("at-sign run", "a@".repeat(RUN_LENGTH / 2)),
        (
            "begin lines without end",
            "-----BEGIN RSA PRIVATE KEY-----\n".repeat(BEGIN_LINE_COUNT),
        ),
        (
            "end lines without begin",
            "-----END RSA PRIVATE KEY-----\n".repeat(BEGIN_LINE_COUNT),
        ),
        ("password prompts", "Password:\n".repeat(BEGIN_LINE_COUNT)),
        (
            "repeated keys on one line",
            "password=".repeat(REPEATED_KEY_COUNT),
        ),
        (
            "repeated keys on lines",
            "password=x\n".repeat(REPEATED_KEY_COUNT),
        ),
        (
            "mysql run",
            format!("mysql {}", "-x ".repeat(RUN_LENGTH / 3)),
        ),
    ];
    for (name, input) in inputs {
        for pseudonymize_hosts in [false, true] {
            let options = RedactionOptions { pseudonymize_hosts };
            let started = Instant::now();
            let result = redact_with_limits(&input, options, MAX_INPUT_BYTES, TEST_RULE_BUDGET);
            eprintln!("{name}: {:?}", started.elapsed());
            assert!(result.is_ok(), "{name} timed out");
        }
    }
}

#[test]
fn five_megabyte_benign_buffer_finishes_and_masks_nothing() {
    let mut buffer = String::with_capacity(BENIGN_BUFFER_BYTES + 256);
    let mut line = 0;
    while buffer.len() < BENIGN_BUFFER_BYTES {
        buffer.push_str(BENIGN_LINES[line % BENIGN_LINES.len()]);
        buffer.push('\n');
        line += 1;
    }
    let started = Instant::now();

    let result = redact(
        &buffer,
        RedactionOptions {
            pseudonymize_hosts: true,
        },
    )
    .expect("a benign buffer is not refused");

    assert!(
        started.elapsed() < BENIGN_BOUND,
        "took {:?}",
        started.elapsed()
    );
    assert!(result.truncated);
    assert_eq!(result.count("key-value-secret"), 0);
    assert_eq!(result.count("jwt"), 0);
    assert_eq!(result.count("cli-password"), 0);
    assert_eq!(result.count("private-key-block"), 0);
}

// The input cap.

#[test]
fn cap_keeps_a_short_text_whole() {
    assert_eq!(cap_input("abc\ndef", MAX_INPUT_BYTES), ("abc\ndef", false));
    assert_eq!(MAX_INPUT_BYTES, 4 * 1024 * 1024);
}

#[test]
fn cap_never_cuts_inside_a_utf8_sequence_and_starts_on_a_line() {
    let text = "\u{E9}\u{E9}\u{E9}\nab\u{20AC}c\nlast\u{1F600}\n";
    for max_bytes in 0..text.len() {
        let (capped, truncated) = cap_input(text, max_bytes);

        assert!(truncated, "max {max_bytes}");
        assert!(text.ends_with(capped), "max {max_bytes}");
        assert!(capped.len() <= max_bytes, "max {max_bytes}");
        let start = text.len() - capped.len();
        assert!(
            capped.is_empty() || start == 0 || text[..start].ends_with('\n'),
            "max {max_bytes}: starts mid-line"
        );
    }
}

#[test]
fn cap_drops_the_partial_first_line_and_the_context_lines_after_it() {
    let tail = "l1\nl2\nl3\nl4\nl5\npassword=hunter2\n";
    let text = format!("{}\n{tail}", "a".repeat(20));

    // The cut falls inside the first line: it goes, and so do the four lines after it.
    let (capped, truncated) = cap_input(&text, tail.len() + 5);

    assert!(truncated);
    assert_eq!(capped, "l5\npassword=hunter2\n");
}

#[test]
fn cap_does_not_keep_the_answer_of_a_prompt_that_was_cut_away() {
    let text = "Password:\nhunter2\nl1\nl2\nl3\nl4\nl5\n";
    // Cut exactly after "Password:\n": the retained text starts with the echoed answer.
    let max_bytes = text.len() - "Password:\n".len();

    let result = redact_with_limits(
        text,
        RedactionOptions::default(),
        max_bytes,
        RULE_TIME_BUDGET,
    )
    .unwrap();

    assert!(result.truncated);
    assert!(!result.text.contains("hunter2"), "{:?}", result.text);
}

#[test]
fn redact_keeps_the_last_four_mebibytes_cut_at_a_character_boundary() {
    // Two-byte characters: an odd byte offset falls inside one.
    let filler = "\u{E9}".repeat(MAX_INPUT_BYTES / 2 + 10);
    let text = format!("{filler}\nl1\nl2\nl3\nl4\npassword=hunter2\n");
    assert!(text.len() > MAX_INPUT_BYTES);

    let result = redact(&text, RedactionOptions::default()).unwrap();

    assert!(result.truncated);
    assert_eq!(result.text, "password=[masked:key-value-secret]\n");
    assert_eq!(result.count("key-value-secret"), 1);
}

#[test]
fn redact_with_a_text_at_the_cap_is_not_truncated() {
    let text = "a".repeat(MAX_INPUT_BYTES);

    let result = redact(&text, RedactionOptions::default()).unwrap();

    assert!(!result.truncated);
    assert_eq!(result.text.len(), MAX_INPUT_BYTES);
}

// Fail closed.

#[test]
fn a_rule_out_of_time_refuses_and_returns_no_text() {
    let result = redact_with_limits(
        "Authorization: Bearer abcdef123456 password=hunter2",
        RedactionOptions::default(),
        MAX_INPUT_BYTES,
        Duration::ZERO,
    );

    // The first rule already has no time: nothing is returned, not even a partial result.
    assert_eq!(
        result,
        Err(RedactionError::Timeout {
            rule: "private-key-block"
        })
    );
}

#[test]
fn a_rule_in_the_middle_of_the_catalogue_out_of_time_fails_closed() {
    let budget = Budget::new("jwt", Duration::ZERO);
    let mut run = Run::new();

    let result = maskers::mask_groups(
        "x eyJhbGciOiJI.eyJzdWIiOiIx.c2lnbmF0dXJl y",
        &[&patterns::JWT],
        "jwt",
        &mut run,
        &budget,
    );

    assert_eq!(result, Err(RedactionError::Timeout { rule: "jwt" }));
}

#[test]
fn a_timeout_names_the_first_rule_that_needs_to_scan() {
    // The private key rule has nothing to scan without a marker, so the first scan is the
    // authorization header rule's; with a marker it is the private key rule's.
    let with_marker = redact_with_limits(
        "-----BEGIN PRIVATE KEY-----\nMIIEvQIB\n",
        RedactionOptions::default(),
        MAX_INPUT_BYTES,
        Duration::ZERO,
    );

    assert_eq!(
        with_marker,
        Err(RedactionError::Timeout {
            rule: "private-key-block"
        })
    );
}

#[test]
fn a_timeout_in_a_pseudonym_rule_is_fail_closed_too() {
    let budget = Budget::new("host-name", Duration::ZERO);
    let mut run = Run::new();

    let result = maskers::pseudonymize_hosts("bob@hostA", "host-name", &mut run, &budget);

    assert_eq!(result, Err(RedactionError::Timeout { rule: "host-name" }));
}

#[test]
fn a_rule_that_finds_nothing_but_ran_out_of_time_is_refused() {
    // Nothing for any rule to scan: only the check after each rule can refuse.
    let result = redact_with_limits(
        "hello",
        RedactionOptions::default(),
        MAX_INPUT_BYTES,
        Duration::ZERO,
    );

    assert!(matches!(result, Err(RedactionError::Timeout { .. })));
}

#[test]
fn overlapping_edits_are_merged_so_no_masked_text_survives() {
    // A second edit that starts before the end of the first, and before its start.
    let edits = [
        scan::Edit {
            start: 13,
            end: 66,
            replacement: "[masked:a]".to_owned(),
        },
        scan::Edit {
            start: 0,
            end: 40,
            replacement: "[masked:b]".to_owned(),
        },
    ];
    let text = "0123456789".repeat(8);

    let out = scan::apply_edits(&text, &edits);

    assert_eq!(out, format!("[masked:b]{}", &text[66..]));
}

#[test]
fn an_orphan_end_marker_after_a_complete_block_masks_the_lines_before_it() {
    let input = "QUJDREVGR0g=\n-----BEGIN PRIVATE KEY-----x-----END PRIVATE KEY----- -----END PRIVATE KEY-----";

    let out = mask(input);

    assert!(!out.contains("QUJDREVGR0g="), "{out:?}");
    assert!(!out.contains("PRIVATE KEY"), "{out:?}");
}

#[test]
fn the_timeout_error_carries_no_input() {
    let error = RedactionError::Timeout {
        rule: "key-value-secret",
    };

    assert_eq!(
        error.to_string(),
        "redaction rule key-value-secret ran out of time"
    );
}

// Leaks the stricter checks close (C# skips a value that merely looks like a token).

#[test]
fn a_secret_wearing_a_mask_outline_is_still_masked() {
    assert_eq!(
        mask("password=[masked:jwt]hunter2]"),
        "password=[masked:key-value-secret]"
    );
    assert_eq!(
        mask("Password: [masked:jwt] hunter2 [masked:jwt]"),
        "Password: [masked:password-prompt]"
    );
    assert_eq!(
        mask("Password:\n[masked:jwt]hunter2"),
        "Password:\n[masked:password-prompt]"
    );
}

#[test]
fn a_real_mask_token_is_left_alone() {
    assert_eq!(
        mask("password=[masked:key-value-secret]"),
        "password=[masked:key-value-secret]"
    );
    assert_eq!(
        mask("Password:\n[masked:password-prompt]"),
        "Password:\n[masked:password-prompt]"
    );
}

#[test]
fn secrets_survive_nowhere_in_the_output_of_the_mixed_sample() {
    let text = mask(MIXED_SAMPLE);

    for secret in [
        "hunter2",
        "S3cret",
        "abc123",
        "s3cr3t",
        "AKIAIOSFODNN7EXAMPLE",
        "xoxb-",
        "MIIEowIBAAKCAQEA",
        "c2lnbmF0dXJl",
    ] {
        assert!(!text.contains(secret), "{secret} leaked");
    }
}

// Codex review of PR #534: masking is best effort, and each input below leaked before.

fn assert_absent(output: &str, leaked: &[&str]) {
    for piece in leaked {
        assert!(!output.contains(piece), "{piece:?} left in {output:?}");
    }
}

#[test]
fn cap_inside_a_private_key_drops_the_rest_of_its_body() {
    let input = "-----BEGIN PRIVATE KEY-----\n".to_owned()
        + &"MIIEvQIBADANBgkq\n".repeat(16)
        + &"log\n".repeat((MAX_INPUT_BYTES - 128) / 4);

    let result = redact_unhurried(&input).expect("redaction finishes");

    assert!(result.truncated);
    assert_absent(&result.text, &["MIIEvQIBADANBgkq"]);
    assert!(result.text.starts_with("log\n"));
}

#[test]
fn cap_inside_an_indented_crlf_private_key_drops_the_rest_of_its_body() {
    let input = "  -----BEGIN PRIVATE KEY-----\r\n".to_owned()
        + &"  MIIEvQIBADANBgkq\r\n".repeat(40)
        + &"log\r\n".repeat((MAX_INPUT_BYTES - 200) / 5);

    let result = redact_unhurried(&input).expect("redaction finishes");

    assert!(result.truncated);
    assert_absent(&result.text, &["MIIEvQIBADANBgkq"]);
}

#[test]
fn cap_outside_a_private_key_keeps_the_lines_after_the_context() {
    let input = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkq\n-----END PRIVATE KEY-----\n"
        .to_owned()
        + &"log\n".repeat(MAX_INPUT_BYTES / 4);

    let result = redact_unhurried(&input).expect("redaction finishes");

    assert!(result.truncated);
    assert!(result.text.starts_with("log\n"));
}

#[test]
fn authorization_folded_headers_are_masked_whole() {
    assert_masked(&[
        (
            "Authorization:\r\n Bearer abcdef123456",
            "Authorization:\r\n Bearer [masked:authorization-header]",
        ),
        (
            "Authorization: Bearer abcdef123456\r\n ghijkl789012",
            "Authorization: Bearer [masked:authorization-header]",
        ),
        (
            "Authorization:\n\tBasic dXNlcjpw\n\tYXNzd29yZA==\nnext: line",
            "Authorization:\n\tBasic [masked:authorization-header]\nnext: line",
        ),
        (
            "Authorization: Digest username=\"bob\",\r\n    response=\"6629fae4\"\r\nHost: x",
            "Authorization: Digest [masked:authorization-header]\r\nHost: x",
        ),
    ]);
}

#[test]
fn authorization_fold_across_a_chunk_boundary_is_masked() {
    let filler = "x\n".repeat(scan::CHUNK_BYTES / 2 - 3);
    let input = format!("{filler}Authorization:\r\n Bearer abcdef123456\n");

    assert_absent(&mask(&input), &["abcdef123456"]);
}

#[test]
fn authorization_short_credentials_after_a_scheme_are_masked() {
    assert_masked(&[
        (
            "Authorization: Basic YTpi",
            "Authorization: Basic [masked:authorization-header]",
        ),
        (
            "Authorization: Bearer abc123",
            "Authorization: Bearer [masked:authorization-header]",
        ),
        (
            "Authorization: Basic abc",
            "Authorization: Basic [masked:authorization-header]",
        ),
    ]);
}

#[test]
fn url_password_with_raw_delimiters_is_masked_to_the_last_at_sign() {
    assert_masked(&[
        (
            "postgres://alice:pa@ss:wo%25rd#?@db.local/app",
            "postgres://alice:[masked:url-credentials]@db.local/app",
        ),
        (
            "postgres://alice:pa/ss@db.local/app",
            "postgres://alice:[masked:url-credentials]@db.local/app",
        ),
        (
            "postgres://alice:p%40ss%3A%2F%23%3F@db.local/app",
            "postgres://alice:[masked:url-credentials]@db.local/app",
        ),
    ]);
    for password in [
        "pa@ss", "pa:ss", "pa%ss", "pa/ss", "pa#ss", "pa?ss", "@", "/@/", "#?:",
    ] {
        let output = mask(&format!("mysql://root:{password}@db.local/app"));
        assert_eq!(output, "mysql://root:[masked:url-credentials]@db.local/app");
    }
}

#[test]
fn url_password_in_a_json_string_stops_at_the_closing_quote() {
    assert_eq!(
        mask("{\"u\":\"ssh://bob:p@ss@h\",\"mail\":\"x@y.com\"}"),
        "{\"u\":\"ssh://bob:[masked:url-credentials]@h\",\"mail\":\"x@y.com\"}"
    );
}

#[test]
fn length_bounds_never_expose_a_secret_suffix() {
    let long_key = format!("api_key={}TAILSECRET", "a".repeat(4096));
    let long_bearer = format!("Authorization: Bearer {}TAILSECRET", "a".repeat(8192));
    let long_quoted = format!("api_key=\"{}\"", "a".repeat(4097));
    let long_url = format!("ssh://bob:{}TAILSECRET@host", "a".repeat(2048));
    let long_jwt = format!("eyJhbGciOiJI.{}TAILSECRET.c2lnbmF0dXJl", "a".repeat(9000));
    let long_github = format!("ghp_{}TAILSECRET", "a".repeat(300));
    let long_command = format!("bash: {}TAILSECRET: command not found", "a".repeat(300));
    for input in [
        &long_key,
        &long_bearer,
        &long_quoted,
        &long_url,
        &long_jwt,
        &long_github,
    ] {
        let output = mask(input);
        assert_absent(&output, &["TAILSECRET", "aaaa"]);
    }
    let prompted = mask(&format!("Password:\n{long_command}"));
    assert_absent(&prompted, &["TAILSECRET"]);
}

#[test]
fn escaped_quotes_do_not_end_a_quoted_secret() {
    assert_masked(&[
        (
            r#"{"password":"abc\"TAILSECRET"}"#,
            r#"{"password":"[masked:key-value-secret]"}"#,
        ),
        (
            r#"--password "abc\"TAILSECRET" next"#,
            r#"--password "[masked:cli-password]" next"#,
        ),
        (
            r"token='it\'s TAILSECRET' ok",
            "token='[masked:key-value-secret]' ok",
        ),
        (
            r#"{"password":"back\\"}"#,
            r#"{"password":"[masked:key-value-secret]"}"#,
        ),
    ]);
}

#[test]
fn an_unterminated_quoted_secret_is_masked_to_the_end_of_the_line() {
    assert_eq!(
        mask("password=\"hunter2 and more\nnext"),
        "password=\"[masked:key-value-secret]\nnext"
    );
}

#[test]
fn private_key_final_truncated_fragment_is_masked() {
    for tail in ["AbC", "A", "ab="] {
        let input = format!("  -----BEGIN PRIVATE KEY-----\r\n  MIIEvQIBADANBgkq\r\n  {tail}");
        assert_eq!(
            mask(&input),
            "  [masked:private-key-block]",
            "tail {tail:?}"
        );
    }
}

#[test]
fn private_key_short_line_in_the_middle_of_the_text_ends_the_block() {
    let input = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkq\nls\nMore text here\n";

    assert_eq!(
        mask(input),
        "[masked:private-key-block]\nls\nMore text here\n"
    );
}

#[test]
fn realistic_leaks_are_masked_in_a_session_transcript() {
    let transcript = concat!(
        "deploy@build:~$ cat key.pem\r\n",
        "    -----BEGIN RSA PRIVATE KEY-----\r\n",
        "    MIIEowIBAAKCAQEAu1SU1LfVLPHCozMxH2Mo4lgOEePzNm0tRgeLezV6ffAt0gun\r\n",
        "    VTLw7onLRnrq0/IzW7yWR7QkrmBL7jTKEn5u+qKhbwKfBstIs+bMY2Zkp18gnTxK\r\n",
        "    Lxo=\r\n",
        "    -----END RSA PRIVATE KEY-----\r\n",
        "deploy@build:~$ curl -H 'Authorization:\r\n",
        "    Bearer ya29.a0AfH6SMBx1234567890abcdef' https://api.example.com\r\n",
        "deploy@build:~$ psql postgres://svc:p@ss:w/rd%25#?x@db.internal:5432/app\r\n",
    );

    let output = mask(transcript);

    assert_absent(
        &output,
        &[
            "MIIEowIB",
            "VTLw7on",
            "Lxo=",
            "ya29.a0AfH6SMBx",
            "w/rd%25",
            "p@ss",
        ],
    );
    assert!(output.contains("deploy@build:~$ cat key.pem"));
    assert!(output.contains("@db.internal:5432/app"));
}

#[test]
fn redacted_debug_never_prints_the_text() {
    let result =
        redact("note: nothing secret", RedactionOptions::default()).expect("redaction finishes");

    let printed = format!("{result:?}");

    assert!(!printed.contains("nothing secret"), "{printed}");
    assert!(printed.contains("truncated"));
}

#[test]
fn documentation_states_best_effort_and_never_promises_safety() {
    let module_doc = include_str!("mod.rs");

    assert!(module_doc.contains("Best effort, not a guarantee"));
    assert!(!module_doc.to_lowercase().contains("safe to share"));
}

// Codex review of PR #534, round 1: each input below leaked or misbehaved before.

#[test]
fn authorization_fold_after_trailing_blanks_is_masked() {
    assert_masked(&[
        (
            "Authorization: Basic dXNlcjpw \r\n YXNzd29yZA==",
            "Authorization: Basic [masked:authorization-header]",
        ),
        (
            "Authorization: Bearer abcdef123456  \t\n\tghijkl789012\nafter",
            "Authorization: Bearer [masked:authorization-header]\nafter",
        ),
    ]);
}

#[test]
fn cap_inside_a_folded_authorization_header_drops_its_continuation() {
    let input = "Authorization: Bearer FIRST\n".to_owned()
        + &" CONTINUEDSECRET\n".repeat(8)
        + &"x\n".repeat(2_097_084);

    let result = redact_unhurried(&input).expect("redaction finishes");

    assert!(result.truncated);
    assert_absent(&result.text, &["CONTINUEDSECRET"]);
}

#[test]
fn cap_inside_a_private_key_drops_a_final_truncated_fragment() {
    let input = "-----BEGIN PRIVATE KEY-----\n".to_owned()
        + &("A".repeat(64) + "\n").repeat(64_528)
        + "AbC";

    let result = redact_unhurried(&input).expect("redaction finishes");

    assert!(result.truncated);
    assert_absent(&result.text, &["AbC", "AAAA"]);
}

#[test]
fn private_key_body_lines_of_any_length_are_masked() {
    let input = "-----BEGIN PRIVATE KEY-----\n".to_owned()
        + &"QUJD".repeat(257)
        + "\n-----END PRIVATE KEY-----";

    assert_eq!(mask(&input), "[masked:private-key-block]");
}

#[test]
fn url_password_with_an_empty_user_or_an_apostrophe_is_masked() {
    assert_masked(&[
        (
            "redis://:hunter2@localhost:6379/0",
            "redis://:[masked:url-credentials]@localhost:6379/0",
        ),
        (
            "postgres://alice:pa'ss@db.local/app",
            "postgres://alice:[masked:url-credentials]@db.local/app",
        ),
    ]);
}

#[test]
fn quoted_values_glued_to_more_text_are_masked_whole() {
    assert_masked(&[
        (
            "run --password=\"first\"TAILSECRET",
            "run --password=\"[masked:cli-password]",
        ),
        (
            "api_key: 'first''TAILSECRET'",
            "api_key: '[masked:key-value-secret]",
        ),
    ]);
    assert_eq!(
        mask("{\"password\":\"x\",\"user\":\"bob\"}"),
        "{\"password\":\"[masked:key-value-secret]\",\"user\":\"bob\"}"
    );
}

#[test]
fn bare_key_value_secrets_run_past_commas_and_semicolons() {
    assert_masked(&[
        (
            "DB_PASSWORD=first,TAILSECRET",
            "DB_PASSWORD=[masked:key-value-secret]",
        ),
        (
            "password=a;b&next=1",
            "password=[masked:key-value-secret]&next=1",
        ),
    ]);
}

#[test]
fn an_echoed_password_ending_in_a_prompt_character_is_masked() {
    assert_eq!(
        mask("Password:\nTr0ub4dor#\n"),
        "Password:\n[masked:password-prompt]\n"
    );
    for prompt in [
        "$",
        "#",
        "root#",
        "bash-5.1$",
        "user@host:~$",
        "dev@build-01.lan:/srv#",
    ] {
        let input = format!("Password:\n{prompt}\n");
        assert_eq!(mask(&input), input, "prompt {prompt:?}");
    }
}

#[test]
fn the_output_of_a_masking_that_grows_the_text_is_capped_and_stable() {
    let input = "api_key=x\n".repeat(150_000);

    let first = redact_unhurried(&input).expect("redaction finishes");
    let second = redact_unhurried(&first.text).expect("redaction finishes");

    assert!(first.text.len() <= MAX_INPUT_BYTES);
    assert!(first.truncated);
    assert_eq!(second.text, first.text);
    assert!(!second.truncated);
    assert_eq!(second.total(), 0);
}

#[test]
fn mysql_password_option_does_not_cross_a_command_separator() {
    assert_survives(&[
        "mysql --version; ssh -p 2222 host",
        "mysql --version | ssh -p 2222 host",
        "mysql --version && ssh -p 2222 host",
    ]);
    assert_masked(&[
        (
            "mysql -u 'a;b' -pS3cret db",
            "mysql -u 'a;b' -p[masked:cli-password] db",
        ),
        (
            "mysql -h db -u root -pS3cret",
            "mysql -h db -u root -p[masked:cli-password]",
        ),
    ]);
}

/// Redaction of a large input with the generous test budget: a debug build on a busy machine
/// must not turn a correctness test into a timeout.
fn redact_unhurried(input: &str) -> Result<Redacted, RedactionError> {
    redact_with_limits(
        input,
        RedactionOptions::default(),
        MAX_INPUT_BYTES,
        TEST_RULE_BUDGET,
    )
}

// Review round: false negatives of quoted, encoded and prefixed forms.

#[test]
fn authorization_json_quoted_header_is_masked() {
    assert_masked(&[
        (
            r#"{"Authorization": "Bearer abcdef123456"}"#,
            r#"{"Authorization": "Bearer [masked:authorization-header]"}"#,
        ),
        (
            "{'Authorization': 'Bearer abcdef123456'}",
            "{'Authorization': 'Bearer [masked:authorization-header]'}",
        ),
        (
            r#"{"headers":"{\"Authorization\":\"Bearer abcdef123456\"}"}"#,
            r#"{"headers":"{\"Authorization\":\"Bearer [masked:authorization-header]\"}"}"#,
        ),
    ]);
}

#[test]
fn authorization_whole_token_is_masked_up_to_whitespace_or_quote() {
    assert_masked(&[
        (
            "Authorization: Bearer 12|hJ8kLmNoPqRsTuVw",
            "Authorization: Bearer [masked:authorization-header]",
        ),
        (
            "Authorization: Basic alice:hunter2",
            "Authorization: Basic [masked:authorization-header]",
        ),
        (
            "curl -H 'Authorization: Bearer 12|hJ8kLmNoPqRsTuVw' https://x",
            "curl -H 'Authorization: Bearer [masked:authorization-header]' https://x",
        ),
        (
            "Authorization: Bearer 12|hJ8k\r\n LmNoPqRsTuVw\r\nHost: x",
            "Authorization: Bearer [masked:authorization-header]\r\nHost: x",
        ),
    ]);
}

#[test]
fn authorization_any_scheme_or_none_is_masked() {
    assert_masked(&[
        (
            "Authorization: Negotiate YIIabc123def456",
            "Authorization: Negotiate [masked:authorization-header]",
        ),
        (
            "Authorization: NTLM TlRMTVNTUAABAAAA",
            "Authorization: NTLM [masked:authorization-header]",
        ),
        (
            "Authorization: lin_api_AbC123xyz456",
            "Authorization: [masked:authorization-header]",
        ),
        (
            "Authorization: AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-date, Signature=fe5f80f77d5fa3beca038a248ff027d0445342fe2855ddc963176630326f1024",
            "Authorization: AWS4-HMAC-SHA256 [masked:authorization-header]",
        ),
        (
            "Authorization: AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/s3/aws4_request,\r\n SignedHeaders=host,\r\n Signature=fe5f80f7\r\nHost: x",
            "Authorization: AWS4-HMAC-SHA256 [masked:authorization-header]\r\nHost: x",
        ),
        (
            "Proxy-Authorization: Negotiate YIIabc123def456",
            "Proxy-Authorization: Negotiate [masked:authorization-header]",
        ),
        (
            "Proxy-Authorization: Basic YWxpY2U6aHVudGVyMg==",
            "Proxy-Authorization: Basic [masked:authorization-header]",
        ),
    ]);
}

#[test]
fn authorization_masking_is_idempotent_for_every_scheme() {
    for input in [
        "Authorization: Negotiate YIIabc123def456",
        "Authorization: lin_api_AbC123xyz456",
        "Authorization: Bearer 12|hJ8kLmNoPqRsTuVw",
    ] {
        let once = mask(input);
        let again = redact(&once, RedactionOptions::default()).unwrap();
        assert_eq!(again.text, once, "input: {input:?}");
        assert_eq!(again.total(), 0, "input: {input:?}");
    }
}

#[test]
fn double_encoded_json_secrets_are_masked() {
    assert_masked(&[
        (
            r#"{"log":"{\"password\":\"hunter2\",\"user\":\"a\"}"}"#,
            r#"{"log":"{\"password\":\"[masked:key-value-secret]\",\"user\":\"a\"}"}"#,
        ),
        (
            r#"msg="{\"db_password\": \"hunter2\"}""#,
            r#"msg="{\"db_password\": \"[masked:key-value-secret]\"}""#,
        ),
        (
            r#"msg="{\"api_key\":\"abc"#,
            r#"msg="{\"api_key\":\"[masked:key-value-secret]"#,
        ),
    ]);
}

#[test]
fn url_user_part_with_a_raw_at_sign_keeps_the_password_masked() {
    assert_masked(&[
        (
            "https://alice@corp.com:S3cr3t@git.corp.com/repo.git",
            "https://alice@corp.com:[masked:url-credentials]@git.corp.com/repo.git",
        ),
        (
            "https://alice@corp.com:S3@cr3t@git.corp.com/repo.git",
            "https://alice@corp.com:[masked:url-credentials]@git.corp.com/repo.git",
        ),
    ]);
}

#[test]
fn url_user_only_token_is_masked_and_plain_users_survive() {
    assert_masked(&[(
        "https://glpat-AbCdEf123456@gitlab.com/x",
        "https://[masked:url-credentials]@gitlab.com/x",
    )]);
    assert_survives(&[
        "git clone ssh://git@github.com/org/repo.git",
        "https://alice@corp.com/x",
        "contact alice@corp.com for access",
    ]);
}

#[test]
fn private_key_with_a_diff_prefix_is_masked() {
    assert_masked(&[
        (
            "diff\n------BEGIN RSA PRIVATE KEY-----\n-MIIEowIBAAKCAQEA\n-VTLw7onLRnrq0/Iz\n------END RSA PRIVATE KEY-----\nafter",
            "diff\n-[masked:private-key-block]\nafter",
        ),
        (
            "------BEGIN RSA PRIVATE KEY-----\n-MIIEowIBAAKCAQEA\n-VTLw7onLRnrq0/Iz\nafter this",
            "-[masked:private-key-block]\nafter this",
        ),
        (
            "+-----BEGIN RSA PRIVATE KEY-----\n+MIIEowIBAAKCAQEA\n+VTLw7onLRnrq0/Iz\n+-----END RSA PRIVATE KEY-----",
            "+[masked:private-key-block]",
        ),
    ]);
}

#[test]
fn private_key_with_line_numbers_is_masked() {
    assert_masked(&[
        (
            "     1\t-----BEGIN RSA PRIVATE KEY-----\n     2\tMIIEowIBAAKCAQEA\n     3\tVTLw7onLRnrq0/Iz\n     4\t-----END RSA PRIVATE KEY-----\n",
            "     1\t[masked:private-key-block]\n",
        ),
        (
            "     1\t-----BEGIN RSA PRIVATE KEY-----\n     2\tMIIEowIBAAKCAQEA\n     3\tVTLw7onLRnrq0/Iz\nafter this\n",
            "     1\t[masked:private-key-block]\nafter this\n",
        ),
        (
            "     2\tMIIEowIBAAKCAQEA\n     3\tVTLw7onLRnrq0/Iz\n     4\t-----END RSA PRIVATE KEY-----\n",
            "[masked:private-key-block]\n",
        ),
    ]);
}

#[test]
fn private_key_with_a_log_prefix_of_the_begin_width_is_masked() {
    assert_masked(&[(
        "build| -----BEGIN RSA PRIVATE KEY-----\nbuild| MIIEowIBAAKCAQEA\nbuild| VTLw7onLRnrq0/Iz\nbuild| finished ok",
        "build| [masked:private-key-block]\nbuild| finished ok",
    )]);
}

#[test]
fn colon_form_bare_values_with_spaces_are_masked_to_the_end_of_the_line() {
    assert_masked(&[
        (
            "secret: correct horse battery",
            "secret: [masked:key-value-secret]",
        ),
        (
            "passwd: two words\nnext: line",
            "passwd: [masked:key-value-secret]\nnext: line",
        ),
        (
            "api_secret: correct horse battery staple",
            "api_secret: [masked:key-value-secret]",
        ),
        (
            r#"{"token": 123, "next": 4}"#,
            r#"{"token": [masked:key-value-secret] "next": 4}"#,
        ),
    ]);
}

#[test]
fn key_names_pass_pwd_long_prefixes_arrows_and_xml_elements_are_masked() {
    assert_masked(&[
        ("DB_PASS=hunter2", "DB_PASS=[masked:key-value-secret]"),
        ("db.pwd=hunter2", "db.pwd=[masked:key-value-secret]"),
        ("dbPass=hunter2", "dbPass=[masked:key-value-secret]"),
        ("pass=hunter2", "pass=[masked:key-value-secret]"),
        (
            "keyVaultReferenceIdentityClientApplicationSecret=hunter2",
            "keyVaultReferenceIdentityClientApplicationSecret=[masked:key-value-secret]",
        ),
        (
            "'password' => 'hunter2'",
            "'password' => '[masked:key-value-secret]'",
        ),
        (
            r#""password"=>"hunter2""#,
            r#""password"=>"[masked:key-value-secret]""#,
        ),
        (
            "<password>hunter2</password>",
            "<password>[masked:key-value-secret]</password>",
        ),
        (
            "<db.Password type=\"x\">hunter2</db.Password>",
            "<db.Password type=\"x\">[masked:key-value-secret]</db.Password>",
        ),
    ]);
}

#[test]
fn short_key_endings_do_not_mask_ordinary_names() {
    assert_survives(&[
        "PWD=/home/alice",
        "OLDPWD=/home/alice",
        "bypass=true",
        "compass: north",
        "pwd: /home/alice",
        "<compass>north</compass>",
    ]);
}

#[test]
fn blank_line_inside_an_open_key_block_does_not_end_it() {
    assert_masked(&[
        (
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n\nVTLw7onLRnrq0/Iz\nplain text",
            "[masked:private-key-block]\nplain text",
        ),
        (
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n\n\nVTLw7onLRnrq0/Iz\nplain text",
            "[masked:private-key-block]\nplain text",
        ),
        (
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n\nplain text",
            "[masked:private-key-block]\n\nplain text",
        ),
    ]);
}

#[test]
fn long_host_names_are_pseudonymised_whatever_their_label_count() {
    assert_eq!(
        pseudonymize("ping node1.k8s.prod.eu-west-1.aws.corp.example.com now"),
        "ping host-1 now"
    );
    assert_eq!(
        pseudonymize("ssh bob@a.b.c.d.e.f.g.example.com"),
        "ssh bob@host-1"
    );
    assert_survives(&["mail alice@corp.com about it"]);
}

// Second review round.

#[test]
fn authorization_in_go_header_dumps_is_masked() {
    assert_masked(&[
        (
            "map[Authorization:[Bearer abcdef123456] Accept:[*/*]]",
            "map[Authorization:[Bearer [masked:authorization-header] Accept:[*/*]]",
        ),
        (
            r#"{"Authorization":["Bearer abcdef123456"]}"#,
            r#"{"Authorization":["Bearer [masked:authorization-header]"]}"#,
        ),
    ]);
}

#[test]
fn raw_ampersand_inside_a_value_does_not_end_it() {
    assert_masked(&[
        (
            "DB_PASSWORD=Abc&123xyz",
            "DB_PASSWORD=[masked:key-value-secret]",
        ),
        (
            "client_secret: Abc&123xyz",
            "client_secret: [masked:key-value-secret]",
        ),
        (
            "password=abc&user=bob",
            "password=[masked:key-value-secret]&user=bob",
        ),
        (
            "?token=abc&other=1&password=zz",
            "?token=[masked:key-value-secret]&other=1&password=[masked:key-value-secret]",
        ),
    ]);
}

#[test]
fn double_encoded_value_with_an_encoded_quote_is_masked_whole() {
    assert_masked(&[(
        r#"{"log":"{\"password\":\"hun\\\"ter2\"}"}"#,
        r#"{"log":"{\"password\":\"[masked:key-value-secret]\"}"}"#,
    )]);
}

#[test]
fn odbc_connection_string_password_is_masked_but_the_shell_pwd_is_not() {
    assert_masked(&[
        (
            "Driver={ODBC Driver 18};Server=db;Uid=sa;Pwd=hunter2;",
            "Driver={ODBC Driver 18};Server=db;Uid=sa;Pwd=[masked:key-value-secret];",
        ),
        (
            "Pwd=hunter2;Server=db",
            "Pwd=[masked:key-value-secret];Server=db",
        ),
    ]);
    assert_survives(&["PWD=/home/x", "PWD=~", "export PWD=/tmp; cd x"]);
}

#[test]
fn ini_cdata_block_scalar_and_name_value_pairs_are_masked() {
    assert_masked(&[
        (
            "password = correct horse battery",
            "password = [masked:key-value-secret]",
        ),
        (
            "<password><![CDATA[hunter2]]></password>",
            "<password><![CDATA[[masked:key-value-secret]]]></password>",
        ),
        (
            "password: |\n  line one\n  line two\nnext: x",
            "password: [masked:key-value-secret]\nnext: x",
        ),
        (
            r#"{"name": "Authorization", "value": "Bearer abc123"}"#,
            r#"{"name": "Authorization", "value": "[masked:key-value-secret]"}"#,
        ),
        (
            r#"{"name": "db_password", "value": "hunter2"}"#,
            r#"{"name": "db_password", "value": "[masked:key-value-secret]"}"#,
        ),
    ]);
}

#[test]
fn test_reports_and_prose_with_secret_words_stay_unchanged() {
    assert_survives(&[
        "PASS: test_login",
        "pass: 12 fail: 0",
        "lexer: unexpected token: ident foo at line 3",
        "checking authorization: user alice allowed",
    ]);
}

// Final review round.

#[test]
fn colon_secrets_after_a_log_prefix_are_masked() {
    assert_masked(&[
        (
            "2026-10-22 10:00:00 INFO client_secret: abc123xyz",
            "2026-10-22 10:00:00 INFO client_secret: [masked:key-value-secret]",
        ),
        (
            "2026-10-22 10:00:00 INFO token: abc123xyz",
            "2026-10-22 10:00:00 INFO token: [masked:key-value-secret]",
        ),
        (
            "web_1  | api_key: abc123xyz",
            "web_1  | api_key: [masked:key-value-secret]",
        ),
        (
            "web-1  | secret: correct horse battery",
            "web-1  | secret: [masked:key-value-secret]",
        ),
        (
            "| api_token: abc123xyz |",
            "| api_token: [masked:key-value-secret]",
        ),
        (
            "retry=3; api_token: abc123xyz",
            "retry=3; api_token: [masked:key-value-secret]",
        ),
        (
            "using api_key: abc123xyz for request",
            "using api_key: [masked:key-value-secret] for request",
        ),
    ]);
}

#[test]
fn authorization_with_a_long_credential_is_masked_after_a_log_prefix() {
    assert_masked(&[
        (
            "2026-10-22T10:00:00Z DEBUG Authorization: Negotiate YIIGhgYJKoZIhvcSAQICAQBuggZ1",
            "2026-10-22T10:00:00Z DEBUG Authorization: Negotiate [masked:authorization-header]",
        ),
        (
            "[2026-10-22 10:00:00] DEBUG Authorization: lin_api_abcdef123456",
            "[2026-10-22 10:00:00] DEBUG Authorization: [masked:authorization-header]",
        ),
        (
            "curl -H Authorization:lin_api_abcdef123456 https://x",
            "curl -H Authorization:[masked:authorization-header] https://x",
        ),
    ]);
}

#[test]
fn authorization_with_an_equals_sign_is_masked() {
    assert_masked(&[
        (
            r#"Invoke-RestMethod -Uri $u -Headers @{Authorization="Bearer abcdef123456"}"#,
            r#"Invoke-RestMethod -Uri $u -Headers @{Authorization="Bearer [masked:authorization-header]"}"#,
        ),
        (
            "$h = @{ 'Authorization' = 'Bearer abcdef123456' }",
            "$h = @{ 'Authorization' = 'Bearer [masked:authorization-header]' }",
        ),
        (
            r#"-Headers @{Authorization = "Basic YWxpY2U6aHVudGVy"}"#,
            r#"-Headers @{Authorization = "Basic [masked:authorization-header]"}"#,
        ),
        (
            "Authorization=Bearer abcdef123456",
            "Authorization=Bearer [masked:authorization-header]",
        ),
        (
            "HTTP_AUTHORIZATION=Bearer abcdef123456",
            "HTTP_AUTHORIZATION=Bearer [masked:authorization-header]",
        ),
        (
            "authorization=abcdef123456",
            "authorization=[masked:authorization-header]",
        ),
    ]);
}

#[test]
fn block_scalar_stops_at_a_line_that_is_not_indented_deeper_than_its_key() {
    assert_masked(&[
        (
            "  secret: >-\n    abc def\n  other: 1",
            "  secret: [masked:key-value-secret]\n  other: 1",
        ),
        (
            "password: |\n  line one\n  line two\nnext: x",
            "password: [masked:key-value-secret]\nnext: x",
        ),
    ]);
}

#[test]
fn cookie_values_are_masked_and_attributes_kept() {
    assert_masked(&[
        (
            "< Set-Cookie: session=abc123secret; Path=/",
            "< Set-Cookie: session=[masked:cookie-header]; Path=/",
        ),
        (
            "Cookie: a=1; sid=abc.def; theme=dark",
            "Cookie: a=[masked:cookie-header]; sid=[masked:cookie-header]; theme=[masked:cookie-header]",
        ),
        (
            "curl -H 'Cookie: sid=abc123' https://x",
            "curl -H 'Cookie: sid=[masked:cookie-header]' https://x",
        ),
        (
            "Set-Cookie: id=xyz789; Expires=Wed, 21 Oct 2026 07:28:00 GMT; HttpOnly; Secure",
            "Set-Cookie: id=[masked:cookie-header]; Expires=Wed, 21 Oct 2026 07:28:00 GMT; HttpOnly; Secure",
        ),
    ]);
    assert_survives(&["Cookie: policy accepted", "we use a cookie: yes"]);
    let once = mask("Set-Cookie: session=abc123secret; Path=/");
    assert_eq!(mask(&once), once);
}

#[test]
fn cookie_is_masked_in_quoted_equals_and_curl_option_forms() {
    assert_masked(&[
        (
            r#"{"Cookie": "session=abc123secret"}"#,
            r#"{"Cookie": "session=[masked:cookie-header]"}"#,
        ),
        (
            "{'Cookie': 'session=abc; theme=dark'}",
            "{'Cookie': 'session=[masked:cookie-header]; theme=[masked:cookie-header]'}",
        ),
        (
            r#"{\"Cookie\": \"session=abc123secret\"}"#,
            r#"{\"Cookie\": \"session=[masked:cookie-header]\"}"#,
        ),
        (
            r#"Invoke-WebRequest -Headers @{Cookie="session=abc123secret"} https://x"#,
            r#"Invoke-WebRequest -Headers @{Cookie="session=[masked:cookie-header]"} https://x"#,
        ),
        (
            "curl -b 'session=abc123secret' https://x",
            "curl -b 'session=[masked:cookie-header]' https://x",
        ),
        (
            r#"curl --cookie "session=abc; theme=dark" https://x"#,
            r#"curl --cookie "session=[masked:cookie-header]; theme=[masked:cookie-header]" https://x"#,
        ),
        (
            "curl -b session=abc123secret https://x",
            "curl -b session=[masked:cookie-header] https://x",
        ),
        (
            "curl --cookie=session=abc123secret https://x",
            "curl --cookie=session=[masked:cookie-header] https://x",
        ),
    ]);
    assert_survives(&[
        "curl -b jar.txt https://x",
        "curl --cookie cookies.txt https://x",
        "cookie=accepted",
    ]);
    let once = mask("curl -b 'session=abc123secret' https://x");
    assert_eq!(mask(&once), once);
}

#[test]
fn log_prefix_limits_are_the_documented_ones() {
    // Module doc, "Known limits": a short letters-only value after a log prefix is prose.
    assert_survives(&["INFO token: ident"]);
    // Several words after a log prefix: the first one only, the rest stays.
    assert_masked(&[(
        "WARN secret: blue-sky-9 mountain lake",
        "WARN secret: [masked:key-value-secret] mountain lake",
    )]);
    // Where the key starts the line the whole value goes.
    assert_masked(&[(
        "secret: blue sky mountain",
        "secret: [masked:key-value-secret]",
    )]);
    // A known shape is masked wherever it sits.
    assert_masked(&[(
        "INFO token: abc123xyz",
        "INFO token: [masked:key-value-secret]",
    )]);
}
