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

//! The shipped catalogue held to its schema: every file reads, identities are unique,
//! categories and parameter types are known, and every action makes its command from its
//! defaults through the hardened generator.

use std::collections::BTreeSet;

use super::*;
use crate::tools::user_commands::{Criticality, UserParameter};

/// The fields the C# `Action` writes in a seed file; any other is not part of the format.
const KNOWN_FIELDS: [&str; 17] = [
    "id",
    "title",
    "description",
    "category",
    "platform",
    "level",
    "tags",
    "examples",
    "translations",
    "notes",
    "links",
    "windowsCommandTemplateId",
    "windowsCommandTemplate",
    "linuxCommandTemplateId",
    "linuxCommandTemplate",
    "linuxExamples",
    "windowsExamples",
];

/// The templates whose defaults the Rust generator refuses and the C# accepted: the
/// generator is stricter on purpose (`user_commands/generator_tests.rs`), so these are
/// listed, not allowed. Each is `identity/platform`. A new refusal fails the test below; so
/// does a listed one that is accepted since.
const KNOWN_REFUSALS: [&str; 11] = [
    "ansible-pull/windows",
    "at-schedule-task/linux",
    "build-dotnet-publish/windows",
    "defender-enable-realtime/windows",
    "docker-exec/windows",
    "git-clone/windows",
    "ldapsearch-query-directory/linux",
    "register-scheduledtask/windows",
    "security-audit-ntfs-permissions/windows",
    "unified-find-files/windows",
    "watch-repeat-command/linux",
];

/// The templates whose pattern holds a line break in the C# seed.
const KNOWN_MULTILINE: [&str; 2] = ["perf-memory-analysis/linux", "perf-memory-analysis/windows"];

fn raw_files() -> Vec<Value> {
    shipped_sources()
        .iter()
        .map(|text| serde_json::from_str(text).expect("a shipped file is JSON"))
        .collect()
}

fn platform_name(platform: TemplatePlatform) -> &'static str {
    match platform {
        TemplatePlatform::Windows => "windows",
        TemplatePlatform::Linux => "linux",
    }
}

#[test]
fn every_shipped_file_reads_and_the_catalogue_holds_them_all() {
    let catalogue = Catalogue::from_sources(shipped_sources()).expect("every file is an action");
    assert_eq!(catalogue.entries().len(), shipped_sources().len());
    assert_eq!(shipped().entries().len(), shipped_sources().len());
    assert!(
        catalogue.entries().len() >= 500,
        "the C# seed is 527 actions"
    );
}

#[test]
fn a_file_that_is_not_an_action_or_an_identity_held_twice_is_refused() {
    let action = r#"{"id":"a","title":"T","category":"security","windowsCommandTemplate":{"commandPattern":"hostname"}}"#;
    assert_eq!(
        Catalogue::from_sources(&[action, "[]"]),
        Err(CatalogueError::Entry(1))
    );
    assert_eq!(
        Catalogue::from_sources(&[action, action]),
        Err(CatalogueError::Duplicate("a".to_owned()))
    );
    assert_eq!(
        Catalogue::from_sources(&[r#"{"title":"no identity"}"#]),
        Err(CatalogueError::Entry(0))
    );
}

#[test]
fn the_files_use_the_fields_of_the_format_and_numbers_of_its_enumerations() {
    for file in raw_files() {
        let object = file.as_object().expect("an object");
        let id = object["id"].as_str().expect("an id");
        for field in object.keys() {
            assert!(KNOWN_FIELDS.contains(&field.as_str()), "{id}: {field}");
        }
        for field in ["platform", "level"] {
            assert!(
                matches!(object[field].as_u64(), Some(0..=2)),
                "{id}: {field}"
            );
        }
    }
}

#[test]
fn every_action_has_the_templates_its_platform_says() {
    for entry in shipped().entries() {
        let command = entry.command();
        let has = command.template_platforms();
        let (windows, linux) = (
            has.contains(&TemplatePlatform::Windows),
            has.contains(&TemplatePlatform::Linux),
        );
        let fits = match command.platform {
            UserPlatform::Windows => windows,
            UserPlatform::Linux => linux,
            UserPlatform::Both => windows && linux,
        };
        assert!(fits, "{}: {:?} {has:?}", command.id, command.platform);
    }
}

#[test]
fn every_action_holds_to_the_schema() {
    for entry in shipped().entries() {
        let command = entry.command();
        assert_eq!(structure_issue(command), None, "{}", command.id);
        assert!(!command.is_user_created, "{}", command.id);
        assert!(
            is_known_category(&command.category),
            "{}: {}",
            command.id,
            command.category
        );
        for platform in [TemplatePlatform::Windows, TemplatePlatform::Linux] {
            for parameter in command
                .template(platform)
                .map(|template| template.parameters.as_slice())
                .unwrap_or_default()
            {
                let kind = parameter.kind.to_lowercase();
                assert!(
                    SUPPORTED_PARAMETER_TYPES.contains(&kind.as_str()),
                    "{}: {}",
                    command.id,
                    parameter.kind
                );
            }
        }
    }
}

#[test]
fn the_categories_cover_the_catalogue_and_are_distinct() {
    let used: BTreeSet<&str> = shipped()
        .entries()
        .iter()
        .map(|entry| entry.command().category.as_str())
        .collect();
    for category in &used {
        assert!(CATEGORY_KEYS.contains(category), "{category}");
    }
    let distinct: BTreeSet<&str> = CATEGORY_KEYS.iter().copied().collect();
    assert_eq!(distinct.len(), CATEGORY_KEYS.len());
}

#[test]
fn every_action_makes_its_command_from_its_defaults_but_the_known_refusals() {
    let mut refused: Vec<String> = Vec::new();
    for entry in shipped().entries() {
        let command = entry.command();
        for platform in refused_defaults(command) {
            refused.push(format!("{}/{}", command.id, platform_name(platform)));
        }
    }
    refused.sort();
    let mut known: Vec<String> = KNOWN_REFUSALS
        .iter()
        .map(|each| (*each).to_owned())
        .collect();
    known.sort();
    assert_eq!(refused, known, "refused now, against the list kept");
}

#[test]
fn the_patterns_with_a_line_break_are_the_known_ones() {
    let mut multiline: Vec<String> = Vec::new();
    for entry in shipped().entries() {
        let command = entry.command();
        for platform in command.template_platforms() {
            let template = command.template(platform).expect("listed");
            if template
                .command_pattern
                .chars()
                .any(|c| c == '\n' || c == '\r')
            {
                multiline.push(format!("{}/{}", command.id, platform_name(platform)));
            }
        }
    }
    multiline.sort();
    let known: Vec<String> = KNOWN_MULTILINE
        .iter()
        .map(|each| (*each).to_owned())
        .collect();
    assert_eq!(multiline, known);
}

#[test]
fn the_french_text_shows_in_french_and_spanish_shows_the_pivot() {
    let entry = shipped().get("ad-check-fsmo-roles").expect("shipped");
    assert_eq!(entry.command().title, "Check the FSMO roles");
    for pivot in ["en", "EN", "es", "es-ES", "", "de"] {
        assert!(
            matches!(entry.localized(pivot), Cow::Borrowed(_)),
            "{pivot}"
        );
        assert_eq!(entry.localized(pivot).title, "Check the FSMO roles");
    }
    for french in ["fr", "FR", "fr-FR"] {
        assert_eq!(
            entry.localized(french).title,
            "Vérifier les rôles FSMO",
            "{french}"
        );
    }
    assert!(
        entry
            .notes("fr")
            .is_some_and(|notes| notes.starts_with("Les 5 rôles"))
    );
    assert!(
        entry
            .notes("es")
            .is_some_and(|notes| notes.starts_with("The 5 FSMO"))
    );
}

#[test]
fn a_translated_parameter_label_replaces_the_pivot_and_the_name_stays() {
    let entry = shipped().get("ad-add-user-to-group").expect("shipped");
    let pivot = entry
        .command()
        .windows_command_template
        .clone()
        .expect("windows");
    let french = entry.localized("fr");
    let template = french.windows_command_template.as_ref().expect("windows");
    assert_eq!(template.command_pattern, pivot.command_pattern);
    let group = template
        .parameters
        .iter()
        .find(|parameter| parameter.name == "groupName")
        .expect("groupName");
    assert_eq!(group.label, "Nom du groupe");
    assert_eq!(pivot.parameters[0].name, template.parameters[0].name);
}

#[test]
fn every_action_is_translated_into_french_and_titles_are_not_blank() {
    for entry in shipped().entries() {
        let id = &entry.command().id;
        assert_eq!(entry.languages().collect::<Vec<_>>(), ["fr"], "{id}");
        assert!(!entry.localized("fr").title.trim().is_empty(), "{id}");
    }
}

#[test]
fn the_legacy_ids_lead_to_entries_of_the_catalogue() {
    for (old, new) in LEGACY_SNIPPET_IDS {
        assert!(shipped().get(new).is_some(), "{old} -> {new}");
        assert_eq!(legacy_snippet(old), Some(new));
    }
    assert_eq!(legacy_snippet("linux-kill-port"), None);
    let distinct: BTreeSet<&str> = LEGACY_SNIPPET_IDS.iter().map(|(old, _)| *old).collect();
    assert_eq!(distinct.len(), LEGACY_SNIPPET_IDS.len());
}

fn imported(pattern: &str, parameter: Option<UserParameter>) -> UserCommand {
    let text = serde_json::json!({
        "id": "imported",
        "title": "Imported",
        "category": "my own",
        "platform": 1,
        "linuxCommandTemplate": {
            "platform": 1,
            "commandPattern": pattern,
            "parameters": parameter.map(|each| vec![each]).unwrap_or_default(),
        }
    });
    serde_json::from_value(text).expect("an action")
}

fn typed(name: &str, kind: &str, default: Option<&str>) -> UserParameter {
    let mut parameter = UserParameter::new(name);
    parameter.kind = kind.to_owned();
    parameter.default_value = default.map(str::to_owned);
    parameter
}

#[test]
fn an_import_is_held_to_the_schema_and_the_generator() {
    assert_eq!(validate_import(&imported("hostname", None)), None);
    assert_eq!(
        validate_import(&imported(
            "ping {h}",
            Some(typed("h", "hostname", Some("db-1")))
        )),
        None
    );
    // A type the library does not know.
    assert_eq!(
        validate_import(&imported("ping {h}", Some(typed("h", "magic", None)))),
        Some(CommandIssue::UnknownParameterType(
            TemplatePlatform::Linux,
            "magic".to_owned()
        ))
    );
    // A default the generator refuses: shell syntax in a free string.
    assert_eq!(
        validate_import(&imported(
            "echo {t}",
            Some(typed("t", "string", Some("x; rm -rf /")))
        )),
        Some(CommandIssue::RefusedDefault(TemplatePlatform::Linux))
    );
    // A required value with no default is asked for, not refused.
    let mut required = typed("t", "string", None);
    required.required = true;
    assert_eq!(validate_import(&imported("echo {t}", Some(required))), None);
    // A line break would run a second line once typed into a terminal.
    assert_eq!(
        validate_import(&imported("id\nwhoami", None)),
        Some(CommandIssue::PatternMultiline(TemplatePlatform::Linux))
    );
    assert_eq!(
        validate_import(&imported(&"a".repeat(MAX_PATTERN_LENGTH + 1), None)),
        Some(CommandIssue::PatternTooLong(TemplatePlatform::Linux))
    );
    // No command at all: kept, as the C# import keeps it; the shipped entries need one.
    assert_eq!(
        structure_issue(&imported("", None)),
        Some(CommandIssue::NoTemplate)
    );
    assert_eq!(validate_import(&imported("", None)), None);
    let mut choice = typed("c", "choice", None);
    choice.allowed_values = Some(Vec::new());
    assert_eq!(
        validate_import(&imported("run {c}", Some(choice))),
        Some(CommandIssue::ChoiceWithoutValues(
            TemplatePlatform::Linux,
            "c".to_owned()
        ))
    );
}

#[test]
fn the_platform_filter_and_the_search_keep_the_commands_asked_for() {
    assert!(on_platform(UserPlatform::Linux, None));
    assert!(on_platform(UserPlatform::Both, Some(UserPlatform::Windows)));
    assert!(on_platform(UserPlatform::Linux, Some(UserPlatform::Linux)));
    assert!(!on_platform(
        UserPlatform::Linux,
        Some(UserPlatform::Windows)
    ));
    assert!(!on_platform(
        UserPlatform::Windows,
        Some(UserPlatform::Both)
    ));
    assert!(on_platform(UserPlatform::Both, Some(UserPlatform::Both)));
    let entry = shipped().get("ad-check-fsmo-roles").expect("shipped");
    let command = entry.command();
    assert!(matches_query(
        command,
        "Active Directory",
        "ACTIVE directory"
    ));
    assert!(matches_query(command, "", "netdom query"));
    assert!(matches_query(command, "", "fsmo"));
    assert!(matches_query(command, "", "  "));
    assert!(!matches_query(command, "", "nmap"));
    assert_eq!(command.level, Criticality::Info);
}

#[test]
fn a_session_starts_the_platform_filter_on_its_own_platform() {
    assert_eq!(
        session_platform_filter(RemoteOs::Linux),
        Some(UserPlatform::Linux)
    );
    assert_eq!(
        session_platform_filter(RemoteOs::Windows),
        Some(UserPlatform::Windows)
    );
    // Undetermined leaves every platform shown.
    assert_eq!(session_platform_filter(RemoteOs::Auto), None);
}

#[test]
fn a_host_parameter_takes_the_session_host_by_its_type_or_its_name() {
    let host = Some("web.lab");
    let by_type = typed("whatever", "hostname", None);
    let by_name = typed("ComputerName", "string", None);
    let other = typed("path", "string", None);
    assert_eq!(session_value(&by_type, host, Some("bob")).as_deref(), host);
    assert_eq!(session_value(&by_name, host, Some("bob")).as_deref(), host);
    assert_eq!(session_value(&other, host, Some("bob")), None);
    // The host is trimmed, and a blank one fills nothing.
    assert_eq!(
        session_value(&by_name, Some("  web.lab "), None).as_deref(),
        host
    );
    assert_eq!(session_value(&by_name, Some("   "), None), None);
    assert_eq!(session_value(&by_name, None, None), None);
}

#[test]
fn an_ip_address_parameter_takes_an_address_and_never_a_name() {
    let address = typed("target_ip", "ipaddress", None);
    assert_eq!(
        session_value(&address, Some("10.0.0.5"), None).as_deref(),
        Some("10.0.0.5")
    );
    assert_eq!(
        session_value(&address, Some("::1"), None).as_deref(),
        Some("::1")
    );
    assert_eq!(session_value(&address, Some("web.lab"), None), None);
}

#[test]
fn a_user_parameter_takes_the_session_user_by_its_name_alone() {
    for name in ["user", "UserName", "login", "account", "samaccountname"] {
        let parameter = typed(name, "string", None);
        assert_eq!(
            session_value(&parameter, Some("web.lab"), Some(" bob ")).as_deref(),
            Some("bob"),
            "{name}"
        );
    }
    assert_eq!(
        session_value(&typed("owner", "string", None), None, Some("bob")),
        None
    );
    assert_eq!(
        session_value(&typed("user", "string", None), None, Some("  ")),
        None
    );
    // The host wins when a parameter is both, as the C# checks it first.
    let both = typed("host", "hostname", None);
    assert_eq!(
        session_value(&both, Some("web.lab"), Some("bob")).as_deref(),
        Some("web.lab")
    );
}

#[test]
fn the_session_values_reach_the_command_through_the_generator_checks() {
    // A prefilled host is a value like a typed one: quoted, and refused when it holds a
    // character the shell could read.
    let command = imported("ping {host}", Some(typed("host", "hostname", None)));
    let template = command.template(TemplatePlatform::Linux).expect("template");
    let parameter = &template.parameters[0];
    let given = session_value(parameter, Some("web.lab"), None).expect("a value");
    let made = generate(template, TemplatePlatform::Linux, |name| {
        (name == "host").then_some(given.as_str())
    });
    assert_eq!(made.expect("generated"), "ping web.lab");
    let hostile = generate(template, TemplatePlatform::Linux, |name| {
        (name == "host").then_some("web.lab; rm -rf /")
    });
    assert!(hostile.is_err(), "a prefilled value is not trusted more");
}

#[test]
fn a_command_is_previewed_on_one_line_and_cut_at_the_limit() {
    assert_eq!(command_preview("ls -l"), "ls -l");
    assert_eq!(command_preview("a\r\nb\nc\rd"), "a b c d");
    assert_eq!(command_preview(""), "");
    let exact = "x".repeat(PREVIEW_MAX_CHARS);
    assert_eq!(command_preview(&exact), exact);
    let long = "y".repeat(PREVIEW_MAX_CHARS + 5);
    let cut = command_preview(&long);
    assert_eq!(cut.chars().count(), PREVIEW_MAX_CHARS + 3);
    assert!(cut.ends_with("..."));
    // Characters are counted, not bytes.
    let accented = "\u{e9}".repeat(PREVIEW_MAX_CHARS + 1);
    assert!(command_preview(&accented).ends_with("..."));
}
