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

//! The OpenSSH configuration read as the C# Heimdall reads it: its parser's tests, case for
//! case, then what the import adds.

use std::collections::HashSet;
use std::path::Path;

use heimdall_core::import::openssh::{
    Candidate, Code, Diagnostic, Level, Status, assess, parse, plan,
};
use heimdall_core::profile::{ProfileId, SshGateway};

fn read(text: &str) -> heimdall_core::import::openssh::Parsed {
    parse(text, None)
}

fn only(text: &str) -> Candidate {
    let parsed = read(text);
    assert_eq!(parsed.candidates.len(), 1, "{:?}", parsed.candidates);
    parsed.candidates.into_iter().next().expect("one")
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<Code> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn an_empty_file_gives_nothing() {
    assert_eq!(read(""), heimdall_core::import::openssh::Parsed::default());
}

#[test]
fn a_host_gives_every_field() {
    let candidate = only(
        "Host prod\n    HostName server.example.com\n    Port 2222\n    User alice\n    IdentityFile C:\\keys\\id_ed25519\n",
    );
    assert_eq!(candidate.alias, "prod");
    assert_eq!(candidate.host_name, "server.example.com");
    assert_eq!(candidate.port, 2222);
    assert_eq!(candidate.user.as_deref(), Some("alice"));
    assert_eq!(
        candidate.identity_file.as_deref(),
        Some(r"C:\keys\id_ed25519")
    );
    assert_eq!(candidate.line, 1);
    assert!(candidate.proxy_jump.is_empty());
}

#[test]
fn without_hostname_the_alias_is_the_host_and_it_is_said() {
    let parsed = read("Host prod\n    User alice\n");
    assert_eq!(parsed.candidates[0].host_name, "prod");
    assert_eq!(
        parsed.diagnostics,
        [Diagnostic {
            level: Level::Info,
            line: 1,
            code: Code::HostNameFallbackToAlias,
            context: Some("prod".to_owned()),
        }]
    );
}

#[test]
fn hostname_expands_the_alias_token_and_a_doubled_percent() {
    assert_eq!(
        only("Host app\n    HostName %h.internal.example\n").host_name,
        "app.internal.example"
    );
    assert_eq!(
        only("Host app\n    HostName host%%name.example\n").host_name,
        "host%name.example"
    );
}

#[test]
fn hostname_with_a_runtime_token_is_left_out_and_said() {
    for value in ["%r.internal.example", "%p-host.example", "%n.example"] {
        let parsed = read(&format!("Host app\n    HostName {value}\n"));
        assert!(parsed.candidates.is_empty(), "{value}");
        let said: Vec<&Diagnostic> = parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == Code::HostNameToken)
            .collect();
        assert_eq!(said.len(), 1, "{value}");
        assert_eq!(said[0].level, Level::Warning);
        assert_eq!(said[0].line, 2);
        assert_eq!(said[0].context.as_deref(), Some(value));
    }
}

#[test]
fn the_first_value_of_a_directive_wins() {
    let parsed = read(
        "Host repeated\n    HostName first.example.com\n    HostName second.example.com\n    Port 2201\n    Port 2202\n    User first-user\n    User second-user\n    IdentityFile C:\\keys\\first_ed25519\n    IdentityFile C:\\keys\\second_ed25519\n    ProxyJump first-jump\n    ProxyJump second-jump\nHost first-jump\n    HostName jump1.example.com\nHost second-jump\n    HostName jump2.example.com\n",
    );
    let candidate = &parsed.candidates[0];
    assert_eq!(candidate.alias, "repeated");
    assert_eq!(candidate.host_name, "first.example.com");
    assert_eq!(candidate.port, 2201);
    assert_eq!(candidate.user.as_deref(), Some("first-user"));
    assert_eq!(
        candidate.identity_file.as_deref(),
        Some(r"C:\keys\first_ed25519")
    );
    assert_eq!(candidate.proxy_jump.len(), 1);
    assert_eq!(candidate.proxy_jump[0].host_name, "jump1.example.com");
}

#[test]
fn a_host_line_with_several_aliases_gives_one_server_each() {
    let parsed = read("Host prod1 prod2 bastion\n    HostName server.example.com\n");
    let aliases: Vec<&str> = parsed.candidates.iter().map(|c| c.alias.as_str()).collect();
    assert_eq!(aliases, ["prod1", "prod2", "bastion"]);
}

#[test]
fn wildcard_and_negated_aliases_are_left_out_and_said() {
    let parsed = read("Host * web-? !prod prod\n    HostName server.example.com\n");
    assert_eq!(parsed.candidates.len(), 1);
    assert_eq!(parsed.candidates[0].alias, "prod");
    assert_eq!(codes(&parsed.diagnostics), [Code::WildcardAliasIgnored; 3]);
}

#[test]
fn a_tilde_in_identityfile_is_the_home_folder_and_is_said() {
    let home = Path::new("/home/alice");
    let parsed = parse(
        "Host prod\n    IdentityFile ~/.ssh/id_ed25519\n",
        Some(home),
    );
    let expected = home.join([".ssh", "id_ed25519"].join(std::path::MAIN_SEPARATOR_STR));
    assert_eq!(
        parsed.candidates[0].identity_file.as_deref(),
        Some(expected.to_string_lossy().as_ref())
    );
    assert!(codes(&parsed.diagnostics).contains(&Code::IdentityFileTildeExpanded));
    // Another user's home is not this one's.
    let other = parse("Host prod\n    IdentityFile ~bob/key\n", Some(home));
    assert_eq!(
        other.candidates[0].identity_file.as_deref(),
        Some("~bob/key")
    );
}

#[test]
fn include_is_not_followed_and_the_rest_is_read() {
    let parsed = read("Include ~/.ssh/config.d/*\nHost prod\n    HostName server.example.com\n");
    assert_eq!(parsed.candidates.len(), 1);
    assert_eq!(
        parsed.diagnostics,
        [Diagnostic {
            level: Level::Warning,
            line: 1,
            code: Code::IncludeIgnored,
            context: Some("~/.ssh/config.d/*".to_owned()),
        }]
    );
}

#[test]
fn a_match_block_is_skipped_until_the_next_host() {
    let parsed = read(
        "Host prod\n    HostName server.example.com\nMatch host logs\n    User should-not-apply\nHost logs\n    HostName logs.example.com\n",
    );
    assert_eq!(parsed.candidates.len(), 2);
    assert_eq!(parsed.candidates[1].user, None);
    assert!(codes(&parsed.diagnostics).contains(&Code::MatchBlockIgnored));
}

#[test]
fn proxyjump_forms_give_their_hop() {
    for (jump, user, host, port) in [
        ("bastion.example.com", None, "bastion.example.com", 22),
        (
            "alice@bastion.example.com",
            Some("alice"),
            "bastion.example.com",
            22,
        ),
        (
            "bastion.example.com:2200",
            None,
            "bastion.example.com",
            2200,
        ),
        (
            "alice@bastion.example.com:2200",
            Some("alice"),
            "bastion.example.com",
            2200,
        ),
    ] {
        let parsed = read(&format!("Host prod\n    ProxyJump {jump}\n"));
        let hop = &parsed.candidates[0].proxy_jump;
        assert_eq!(hop.len(), 1, "{jump}");
        assert_eq!(
            (
                hop[0].user.as_deref(),
                hop[0].host_name.as_str(),
                hop[0].port
            ),
            (user, host, port),
            "{jump}"
        );
        assert!(
            !parsed.diagnostics.iter().any(|d| matches!(
                d.code,
                Code::ProxyJumpSyntax | Code::ProxyJumpCycle | Code::ProxyJumpToken
            )),
            "{jump}"
        );
    }
}

#[test]
fn several_hops_keep_their_order_and_a_named_hop_takes_its_block() {
    let candidate = only("Host prod\n    ProxyJump u1@h1:22,u2@h2:2222,h3\n");
    let hops: Vec<(&str, Option<&str>, u16)> = candidate
        .proxy_jump
        .iter()
        .map(|hop| (hop.host_name.as_str(), hop.user.as_deref(), hop.port))
        .collect();
    assert_eq!(
        hops,
        [
            ("h1", Some("u1"), 22),
            ("h2", Some("u2"), 2222),
            ("h3", None, 22)
        ]
    );
    let parsed = read(
        "Host prod\n    ProxyJump jump\nHost jump\n    HostName jump.example.com\n    Port 2022\n    User ops\n    IdentityFile C:\\keys\\jump\n",
    );
    let hop = &parsed.candidates[0].proxy_jump[0];
    assert_eq!(hop.host, "jump");
    assert_eq!(hop.host_name, "jump.example.com");
    assert_eq!((hop.port, hop.user.as_deref()), (2022, Some("ops")));
    assert_eq!(hop.identity_file.as_deref(), Some(r"C:\keys\jump"));
}

#[test]
fn proxyjump_none_is_no_chain() {
    let parsed = read("Host prod\n    ProxyJump none\n");
    assert!(parsed.candidates[0].proxy_jump.is_empty());
    assert!(!codes(&parsed.diagnostics).contains(&Code::ProxyJumpSyntax));
}

#[test]
fn proxycommand_alone_or_with_proxyjump_is_not_imported() {
    let alone = read("Host prod\n    ProxyCommand ssh -W %h:%p bastion\n");
    assert!(alone.candidates[0].proxy_jump.is_empty());
    assert!(codes(&alone.diagnostics).contains(&Code::ProxyCommandUnsupported));
    let both = read("Host prod\n    ProxyJump bastion\n    ProxyCommand ssh -W %h:%p bastion\n");
    assert!(both.candidates[0].proxy_jump.is_empty());
    assert!(codes(&both.diagnostics).contains(&Code::ProxyJumpWithProxyCommand));
}

#[test]
fn a_token_in_proxyjump_is_not_imported() {
    let parsed = read("Host prod\n    ProxyJump %h\n");
    assert!(parsed.candidates[0].proxy_jump.is_empty());
    assert!(codes(&parsed.diagnostics).contains(&Code::ProxyJumpToken));
}

#[test]
fn malformed_proxyjump_is_said() {
    for jump in [
        "host1, host2",
        "\"host1\"",
        "host1,,host2",
        "a@b@c",
        "host:0",
        "host:x",
    ] {
        let parsed = read(&format!("Host prod\n    ProxyJump {jump}\n"));
        assert!(parsed.candidates[0].proxy_jump.is_empty(), "{jump}");
        assert!(
            codes(&parsed.diagnostics).contains(&Code::ProxyJumpSyntax),
            "{jump}"
        );
    }
}

#[test]
fn a_chain_back_to_itself_is_said_and_not_imported() {
    let parsed = read("Host prod\n    ProxyJump bastion\nHost bastion\n    ProxyJump prod\n");
    let prod = parsed
        .candidates
        .iter()
        .find(|candidate| candidate.alias == "prod")
        .expect("prod");
    assert!(prod.proxy_jump.is_empty());
    assert!(codes(&parsed.diagnostics).contains(&Code::ProxyJumpCycle));
    // A hop repeated in one chain.
    let twice = read("Host prod\n    ProxyJump a,b,a\n");
    assert!(twice.candidates[0].proxy_jump.is_empty());
}

#[test]
fn an_invalid_port_is_22_and_said() {
    for raw in ["-1", "65536", "0", "ssh"] {
        let parsed = read(&format!("Host prod\n    Port {raw}\n"));
        assert_eq!(parsed.candidates[0].port, 22, "{raw}");
        let said: Vec<&Diagnostic> = parsed
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::InvalidPort)
            .collect();
        assert_eq!(said.len(), 1, "{raw}");
        assert_eq!(said[0].context.as_deref(), Some(raw));
    }
}

#[test]
fn an_alias_seen_before_is_the_first_one_whatever_the_case() {
    let parsed = read(
        "Host prod\n    HostName first.example.com\nHost PROD\n    HostName second.example.com\n",
    );
    assert_eq!(parsed.candidates.len(), 1);
    assert_eq!(parsed.candidates[0].host_name, "first.example.com");
    let said: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == Code::DuplicateAlias)
        .collect();
    assert_eq!(said.len(), 1);
    assert_eq!(said[0].context.as_deref(), Some("PROD"));
}

#[test]
fn comments_case_quotes_blank_lines_and_crlf_are_tolerated() {
    let parsed = read(
        "# comment\r\n\r\nHoSt \"prod\"   # trailing comment\r\n    HoStNaMe \"server.example.com\"\r\n    uSeR alice\r\n    UnknownThing yes\r\n",
    );
    let candidate = &parsed.candidates[0];
    assert_eq!(candidate.alias, "prod");
    assert_eq!(candidate.host_name, "server.example.com");
    assert_eq!(candidate.user.as_deref(), Some("alice"));
    let unknown: Vec<&Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.code == Code::UnknownDirectiveIgnored)
        .collect();
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].line, 6, "lines counted from 1 across CRLF");
}

#[test]
fn keyword_equals_value_is_read_as_openssh_reads_it() {
    let candidate = only("Host prod\n    HostName=server.example.com\n    Port = 2201\n");
    assert_eq!(candidate.host_name, "server.example.com");
    assert_eq!(candidate.port, 2201);
}

fn gateway(
    id: &str,
    name: &str,
    host: &str,
    user: Option<&str>,
    parent: Option<&str>,
) -> SshGateway {
    SshGateway {
        id: ProfileId::new(id),
        name: name.to_owned(),
        host: host.to_owned(),
        port: 22,
        username: user.map(str::to_owned),
        key_path: None,
        parent: parent.map(ProfileId::new),
    }
}

fn counter() -> impl FnMut() -> ProfileId {
    let mut next = 0;
    move || {
        next += 1;
        ProfileId::new(format!("id-{next}"))
    }
}

#[test]
fn the_import_adds_profiles_through_gateway_chains_reusing_saved_ones() {
    let parsed = read(
        "Host web\n    HostName web.lab\n    User ops\n    ProxyJump alice@edge.lab,inner.lab\nHost db\n    HostName db.lab\n    ProxyJump alice@edge.lab,inner.lab\n",
    );
    let saved = [gateway("edge", "edge", "EDGE.lab", Some("alice"), None)];
    let mut ids = counter();
    let plan = plan(&parsed.candidates, &HashSet::new(), &saved, &mut ids);
    assert_eq!(
        plan.gateways.len(),
        1,
        "edge reused, inner made once for both"
    );
    let inner = &plan.gateways[0];
    assert_eq!(
        (
            inner.host.as_str(),
            inner.parent.as_ref().map(ProfileId::as_str)
        ),
        ("inner.lab", Some("edge"))
    );
    assert_eq!(inner.name, "inner.lab");
    assert_eq!(plan.profiles.len(), 2);
    for profile in &plan.profiles {
        assert_eq!(
            profile.gateway.as_ref(),
            Some(&inner.id),
            "{}",
            profile.name
        );
    }
    let web = &plan.profiles[0];
    assert_eq!(
        (
            web.name.as_str(),
            web.host.as_str(),
            web.username.as_deref()
        ),
        ("web", "web.lab", Some("ops"))
    );
}

#[test]
fn a_host_reached_two_ways_gets_a_gateway_for_each_way() {
    let parsed =
        read("Host a\n    ProxyJump edge.lab,inner.lab\nHost b\n    ProxyJump inner.lab\n");
    let plan = plan(&parsed.candidates, &HashSet::new(), &[], &mut counter());
    let inner: Vec<&SshGateway> = plan
        .gateways
        .iter()
        .filter(|gateway| gateway.host == "inner.lab")
        .collect();
    assert_eq!(inner.len(), 2, "through edge, and directly");
    assert_ne!(inner[0].parent, inner[1].parent);
    assert_eq!(inner[1].name, "inner.lab (2)", "a name no gateway has");
}

#[test]
fn a_name_already_a_profile_is_left_out_and_the_preview_says_so() {
    let parsed = read("Host Web\n    HostName web.lab\nHost db\n    HostName db.lab\n");
    let names: HashSet<String> = ["web".to_owned()].into();
    let assessed = assess(&parsed.candidates, &names, &[]);
    let statuses: Vec<Status> = assessed.iter().map(|a| a.status).collect();
    assert_eq!(statuses, [Status::Duplicate, Status::New]);
    let plan = plan(&parsed.candidates, &names, &[], &mut counter());
    assert_eq!(plan.duplicates, ["Web"]);
    assert_eq!(plan.profiles.len(), 1);
}

#[test]
fn the_preview_names_a_saved_gateway_it_reuses() {
    let parsed = read("Host web\n    ProxyJump alice@edge.lab\n");
    let saved = [gateway("edge", "Edge", "edge.lab", Some("alice"), None)];
    let assessed = assess(&parsed.candidates, &HashSet::new(), &saved);
    assert_eq!(assessed[0].gateways.len(), 1);
    assert_eq!(assessed[0].gateways[0].reused.as_deref(), Some("Edge"));
    let other_user = read("Host web\n    ProxyJump bob@edge.lab\n");
    let assessed = assess(&other_user.candidates, &HashSet::new(), &saved);
    assert_eq!(assessed[0].gateways[0].reused, None, "another account");
}

#[test]
fn what_a_match_block_holds_is_skipped_without_a_word_of_its_own() {
    let parsed =
        read("Match host x\n    User y\n    Include other\nHost prod\n    HostName p.lab\n");
    assert_eq!(codes(&parsed.diagnostics), [Code::MatchBlockIgnored]);
    assert_eq!(parsed.candidates[0].user, None);
}

#[test]
fn a_hash_in_quotes_is_part_of_the_value() {
    let candidate = only(concat!(
        "Host prod\n",
        r#"    IdentityFile "C:\keys\#1\id" # the key"#,
        "\n"
    ));
    assert_eq!(candidate.identity_file.as_deref(), Some(r"C:\keys\#1\id"));
}

#[test]
fn diagnostics_are_in_the_files_order_and_name_a_directive_as_written() {
    let parsed =
        read("Host legacy\n    ProxyCommand nc %h %p\n    ServerAliveInterval 30\nMatch host x\n");
    let lines: Vec<usize> = parsed.diagnostics.iter().map(|d| d.line).collect();
    assert_eq!(
        lines,
        [1, 2, 3, 4],
        "found when the servers were built, said in order"
    );
    let unknown = parsed
        .diagnostics
        .iter()
        .find(|d| d.code == Code::UnknownDirectiveIgnored)
        .expect("said");
    assert_eq!(unknown.context.as_deref(), Some("ServerAliveInterval"));
}

#[test]
fn a_directive_before_any_host_is_named_as_written() {
    let parsed = read("IdentitiesOnly yes\nHost a\n");
    assert_eq!(parsed.diagnostics[0].code, Code::UnknownDirectiveIgnored);
    assert_eq!(
        parsed.diagnostics[0].context.as_deref(),
        Some("IdentitiesOnly")
    );
}
