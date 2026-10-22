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
        "Authorization: Basic abc",
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
fn cap_drops_the_partial_first_line_so_no_secret_fragment_survives() {
    let text = "aaaaaaaaaaaaaaaaaaaa\npassword=hunter2\n";

    let (capped, truncated) = cap_input(text, 25);

    assert!(truncated);
    assert_eq!(capped, "password=hunter2\n");
}

#[test]
fn redact_keeps_the_last_four_mebibytes_cut_at_a_character_boundary() {
    // Two-byte characters: an odd byte offset falls inside one.
    let filler = "\u{E9}".repeat(MAX_INPUT_BYTES / 2 + 10);
    let text = format!("{filler}\npassword=hunter2\n");
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

    assert_eq!(
        result,
        Err(RedactionError::Timeout {
            rule: "authorization-header"
        })
    );
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
