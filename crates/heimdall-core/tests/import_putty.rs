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

//! `PuTTY`'s saved sessions read as the C# Heimdall reads them, from the registry's values or
//! from a session file.

use std::collections::HashSet;

use heimdall_core::import::openssh::{Status, assess, plan};
use heimdall_core::import::putty::{Code, Level, RawSession, Value, decode_name, parse};
use heimdall_core::profile::ProfileId;

fn session(name: &str, values: &[(&str, Value)]) -> RawSession {
    RawSession::new(
        name.to_owned(),
        values
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone())),
    )
}

fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
}

#[test]
fn an_ssh_session_gives_its_host_port_user_and_key() {
    let parsed = parse(&[session(
        "Web%20server",
        &[
            ("Protocol", text("ssh")),
            ("HostName", text("web.lab")),
            ("PortNumber", Value::Number(2222)),
            ("UserName", text("ops")),
            ("PublicKeyFile", text(r"C:\keys\web")),
        ],
    )]);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let web = &parsed.candidates[0];
    assert_eq!(web.alias, "Web server", "the name decoded");
    assert_eq!(
        (web.host_name.as_str(), web.port, web.user.as_deref()),
        ("web.lab", 2222, Some("ops"))
    );
    assert_eq!(web.identity_file.as_deref(), Some(r"C:\keys\web"));
    assert!(web.proxy_jump.is_empty());
}

#[test]
fn default_settings_and_other_protocols_are_left_out_and_said() {
    let parsed = parse(&[
        session("Default%20Settings", &[("Protocol", text("ssh"))]),
        session(
            "switch",
            &[("Protocol", text("telnet")), ("HostName", text("sw"))],
        ),
        session("serial", &[]),
    ]);
    assert!(parsed.candidates.is_empty());
    let said: Vec<(Code, Option<&str>)> = parsed
        .diagnostics
        .iter()
        .map(|d| (d.code, d.context.as_deref()))
        .collect();
    assert_eq!(
        said,
        [
            (Code::DefaultSettingsSkipped, None),
            (Code::NotSsh, Some("telnet")),
            (Code::NotSsh, Some("")),
        ]
    );
}

#[test]
fn an_invalid_port_is_22_and_said_whether_number_or_text() {
    for value in [Value::Number(0), Value::Number(70000), text("ssh")] {
        let parsed = parse(&[session(
            "a",
            &[
                ("Protocol", text("SSH")),
                ("HostName", text("a")),
                ("PortNumber", value.clone()),
            ],
        )]);
        assert_eq!(parsed.candidates[0].port, 22, "{value:?}");
        assert_eq!(parsed.diagnostics[0].code, Code::InvalidPort, "{value:?}");
        assert_eq!(parsed.diagnostics[0].level, Level::Warning);
    }
}

#[test]
fn what_is_not_imported_is_said() {
    let long = "x".repeat(100);
    let parsed = parse(&[session(
        "a",
        &[
            ("Protocol", text("ssh")),
            ("HostName", text("a")),
            ("PublicKeyFile", text(r"C:\keys\a.PPK")),
            ("ProxyMethod", text("3")),
            ("ProxyHost", text("proxy.lab")),
            ("PortForwardings", text("L8080=web:80,R9000=localhost:9000")),
            ("RemoteCommand", text(&long)),
        ],
    )]);
    let codes: Vec<Code> = parsed.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        [
            Code::PpkKey,
            Code::ProxyNotMapped,
            Code::ForwardingsNotMapped,
            Code::RemoteCommandNotMapped
        ]
    );
    assert_eq!(parsed.diagnostics[2].context.as_deref(), Some("2"));
    let command = parsed.diagnostics[3].context.as_deref().expect("said");
    assert_eq!(command.len(), 83, "80 characters and ...");
    assert!(command.ends_with("..."));
    // A proxy method of 0 with nothing else is no proxy.
    let none = parse(&[session(
        "b",
        &[
            ("Protocol", text("ssh")),
            ("HostName", text("b")),
            ("ProxyMethod", Value::Number(0)),
        ],
    )]);
    assert!(none.diagnostics.is_empty());
}

#[test]
fn a_session_without_a_host_is_listed_invalid_and_never_imported() {
    let parsed = parse(&[session("empty", &[("Protocol", text("ssh"))])]);
    assert_eq!(parsed.diagnostics[0].code, Code::MissingHost);
    let assessed = assess(&parsed.candidates, &HashSet::new(), &[]);
    assert_eq!(assessed[0].status, Status::Invalid);
    let mut next = 0;
    let plan = plan(&parsed.candidates, &HashSet::new(), &[], &mut || {
        next += 1;
        ProfileId::new(format!("id-{next}"))
    });
    assert!(plan.profiles.is_empty());
    assert_eq!(plan.invalid, ["empty"]);
}

#[test]
fn a_session_file_reads_as_the_registry_does() {
    let file = RawSession::from_file(
        "lab%2Fweb".to_owned(),
        "HostName=web.lab\nPortNumber=2200\nProtocol=ssh\n UserName=ops\nRemoteCommand=\n",
    );
    let parsed = parse(&[file]);
    let web = &parsed.candidates[0];
    assert_eq!(web.alias, "lab/web");
    assert_eq!((web.host_name.as_str(), web.port), ("web.lab", 2200));
    assert_eq!(
        web.user.as_deref(),
        Some("ops"),
        "a name read whatever the spaces around it"
    );
    assert!(parsed.diagnostics.is_empty(), "an empty command is none");
}

#[test]
fn names_decode_percent_bytes_and_keep_anything_else() {
    assert_eq!(decode_name("My%20Server%3A2"), "My Server:2");
    assert_eq!(decode_name("100%"), "100%");
    assert_eq!(decode_name("a%zzb"), "a%zzb");
    assert_eq!(decode_name("caf%C3%A9"), "café");
}
