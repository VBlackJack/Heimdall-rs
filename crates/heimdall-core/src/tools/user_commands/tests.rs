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

use super::*;

fn draft(title: &str, linux: &str) -> Draft {
    Draft {
        title: title.to_owned(),
        category: "Network".to_owned(),
        platform: UserPlatform::Linux,
        linux_pattern: linux.to_owned(),
        ..Draft::default()
    }
}

fn linux_template(pattern: &str, parameters: Vec<UserParameter>) -> UserTemplate {
    UserTemplate {
        platform: UserPlatform::Linux,
        name: String::new(),
        command_pattern: pattern.to_owned(),
        parameters,
    }
}

#[test]
fn a_value_is_quoted_for_the_shell_of_its_platform() {
    let template = linux_template("echo {text}", vec![UserParameter::new("text")]);
    let typed = |name: &str| (name == "text").then_some("it's");
    assert_eq!(
        generate(&template, TemplatePlatform::Linux, typed).as_deref(),
        Ok("echo 'it'\\''s'")
    );
    assert_eq!(
        generate(&template, TemplatePlatform::Windows, typed).as_deref(),
        Ok("echo 'it''s'")
    );
}

#[test]
fn a_blank_value_takes_the_default_and_typed_values_go_in_bare() {
    let mut host = UserParameter::new("host");
    host.kind = "hostname".to_owned();
    host.default_value = Some("example.com".to_owned());
    let mut port = UserParameter::new("port");
    port.kind = "int".to_owned();
    let template = linux_template("nc {host} {port}", vec![host, port]);
    let rendered = generate(&template, TemplatePlatform::Linux, |name| match name {
        "host" => Some("  "),
        "port" => Some("22"),
        _ => None,
    });
    assert_eq!(rendered.as_deref(), Ok("nc example.com 22"));
}

// A value between double quotes is not protected by the quotes the generator adds, so
// the shell characters are refused there (bare, they are quoted and pass: generator_tests).
#[test]
fn a_value_that_would_run_more_or_is_wrong_is_refused() {
    let mut port = UserParameter::new("port");
    port.kind = "int".to_owned();
    let template = linux_template(
        "x \"{text}\" {port}",
        vec![UserParameter::new("text"), port.clone()],
    );
    let refused = |text: &'static str, port: &'static str| {
        generate(&template, TemplatePlatform::Linux, move |name| {
            Some(if name == "text" { text } else { port })
        })
    };
    assert_eq!(
        refused("a\nrm -rf /", "1"),
        Err(GenerateError::Values(vec![ValueError::Control(
            "text".to_owned()
        )]))
    );
    assert_eq!(
        refused("a; b", "x"),
        Err(GenerateError::Values(vec![
            ValueError::Dangerous("text".to_owned()),
            ValueError::NotInteger("port".to_owned()),
        ]))
    );
    port.required = true;
    let required = linux_template("x {port}", vec![port]);
    assert_eq!(
        generate(&required, TemplatePlatform::Linux, |_| None),
        Err(GenerateError::Values(vec![ValueError::Required(
            "port".to_owned()
        )]))
    );
}

#[test]
fn a_choice_takes_only_what_is_offered() {
    let mut level = UserParameter::new("level");
    level.kind = CHOICE_TYPE.to_owned();
    level.allowed_values = Some(vec!["debug".to_owned(), "info".to_owned()]);
    let template = linux_template("log {level}", vec![level]);
    let with =
        |value: &'static str| generate(&template, TemplatePlatform::Linux, move |_| Some(value));
    assert_eq!(with("INFO").as_deref(), Ok("log INFO"));
    assert!(matches!(with("trace"), Err(GenerateError::Values(_))));
}

#[test]
fn an_inline_value_is_escaped_only_and_only_inside_quotes_is_kept() {
    let mut text = UserParameter::new("text");
    text.quoting = Some(Quoting::InlineInQuotes);
    let template = linux_template("echo 'a {text}'", vec![text]);
    assert_eq!(
        generate(&template, TemplatePlatform::Linux, |_| Some("it's")).as_deref(),
        Ok("echo 'a it'\\''s'")
    );
    assert!(placeholder_in_single_quotes(
        "echo 'a {text}'",
        "text",
        TemplatePlatform::Linux
    ));
    assert!(!placeholder_in_single_quotes(
        "echo {text} 'b'",
        "text",
        TemplatePlatform::Linux
    ));
    assert_eq!(placeholders("a {x} {y_1} {x} {} {a-b}"), ["x", "y_1"]);
}

#[test]
fn a_command_is_made_changed_and_deleted_and_the_file_round_trips() {
    let folder = tempfile::tempdir().expect("folder");
    let path = folder.path().join(LIBRARY_FILE);
    let mut library = UserLibrary::load(&path).expect("no file is an empty library");
    assert!(library.commands().is_empty());

    let id = library
        .upsert(&draft("Ping", "ping -c 3 {host}"))
        .expect("made");
    library.save(&path).expect("saved");
    let mut loaded = UserLibrary::load(&path).expect("loaded");
    assert_eq!(loaded.commands().len(), 1);
    let made = loaded.get(&id).expect("kept");
    assert!(made.is_user_created);
    assert_eq!(made.template_platforms(), [TemplatePlatform::Linux]);
    assert_eq!(
        made.template(TemplatePlatform::Linux)
            .map(|t| t.parameters.len()),
        Some(1)
    );

    let mut edit = Draft::of(made);
    edit.title = "Ping three".to_owned();
    edit.linux_pattern = "ping -c 3 {host} {extra}".to_owned();
    assert_eq!(loaded.upsert(&edit).as_deref(), Ok(id.as_str()));
    assert_eq!(loaded.commands().len(), 1);
    assert_eq!(
        loaded.get(&id).map(|c| c.title.as_str()),
        Some("Ping three")
    );
    assert_eq!(
        loaded
            .get(&id)
            .and_then(|c| c.template(TemplatePlatform::Linux))
            .map(|t| t.parameters.len()),
        Some(2)
    );

    assert!(loaded.delete(&id));
    assert!(!loaded.delete(&id));
    loaded.save(&path).expect("saved");
    assert!(
        UserLibrary::load(&path)
            .expect("loaded")
            .commands()
            .is_empty()
    );
}

#[test]
fn a_draft_that_cannot_be_kept_is_refused_and_changes_nothing() {
    let mut library = UserLibrary::default();
    assert_eq!(
        library.upsert(&draft(" ", "ls")),
        Err(DraftError::TitleMissing)
    );
    assert_eq!(
        library.upsert(&draft("a", " ")),
        Err(DraftError::PatternMissing)
    );
    assert_eq!(
        library.upsert(&draft("a", "ls\nrm x")),
        Err(DraftError::PatternMultiline)
    );
    let mut no_category = draft("a", "ls");
    no_category.category = String::new();
    assert_eq!(
        library.upsert(&no_category),
        Err(DraftError::CategoryMissing)
    );
    let mut gone = draft("a", "ls");
    gone.id = Some("missing".to_owned());
    assert_eq!(library.upsert(&gone), Err(DraftError::Gone));
    assert!(library.commands().is_empty());
}

#[test]
fn a_csharp_export_is_imported_with_its_fields_kept_and_unsafe_quoting_reset() {
    let json = r#"{
        "schemaVersion": "1.0", "totalActions": 3,
        "actions": [
          {"id": "a1", "publicId": "p1", "title": "Disk", "description": "d", "category": "System",
           "platform": 1, "level": 2, "tags": ["x"], "notes": "kept",
           "linuxCommandTemplate": {"platform": 1, "name": "n", "commandPattern": "du {dir}",
             "parameters": [{"name": "dir", "label": "Dir", "type": "path", "quoting": "InlineInQuotes"}]},
           "examples": [{"command": "du /", "description": "e", "platform": 2}]},
          {"id": "a2", "title": "", "category": "System"},
          {"id": "a3", "title": "Odd", "category": "System", "platform": 99, "level": "bogus"}
        ]}"#;
    let mut library = UserLibrary::default();
    let report = library.import(json).expect("imported");
    assert_eq!(
        report,
        ImportReport {
            imported: 2,
            updated: 0,
            skipped: 1
        }
    );
    let disk = library.get("a1").expect("disk");
    assert!(disk.is_user_created);
    assert_eq!(disk.level, Criticality::Dangerous);
    assert_eq!(disk.platform, UserPlatform::Linux);
    assert_eq!(
        disk.template(TemplatePlatform::Linux)
            .and_then(|t| t.parameters[0].quoting),
        Some(Quoting::ShellQuote)
    );
    let odd = library.get("a3").expect("odd");
    assert_eq!(odd.platform, UserPlatform::Both);
    assert_eq!(odd.level, Criticality::Dangerous);
    // What the C# keeps and this build does not edit goes back out.
    let exported: Value = serde_json::from_str(&library.export()).expect("json");
    assert_eq!(exported["actions"][0]["notes"], "kept");
    assert_eq!(exported["actions"][0]["examples"][0]["command"], "du /");
    assert_eq!(exported["actions"][0]["platform"], 1);
    assert_eq!(exported["totalActions"], 2);
    // Imported again: the same commands are updated, not doubled.
    let again = library.import(&library.export()).expect("again");
    assert_eq!(again.updated, 2);
    assert_eq!(library.commands().len(), 2);
    assert!(matches!(
        UserLibrary::default().import("{\"nothing\": 1}"),
        Err(LibraryError::Format)
    ));
}

#[test]
fn a_file_that_cannot_be_read_as_a_library_is_an_error_not_an_empty_library() {
    let folder = tempfile::tempdir().expect("folder");
    let path = folder.path().join(LIBRARY_FILE);
    fs::write(&path, "not json").expect("written");
    assert!(matches!(
        UserLibrary::load(&path),
        Err(LibraryError::Format)
    ));
}

#[test]
fn the_first_template_shown_is_this_systems_else_the_other() {
    let mut library = UserLibrary::default();
    let mut both = draft("a", "ls");
    both.platform = UserPlatform::Both;
    both.windows_pattern = "dir".to_owned();
    let id = library.upsert(&both).expect("made");
    let command = library.get(&id).expect("kept");
    assert_eq!(
        command.preferred_platform(true),
        Some(TemplatePlatform::Windows)
    );
    assert_eq!(
        command.preferred_platform(false),
        Some(TemplatePlatform::Linux)
    );
    let id = library.upsert(&draft("b", "ls")).expect("made");
    assert_eq!(
        library.get(&id).and_then(|c| c.preferred_platform(true)),
        Some(TemplatePlatform::Linux)
    );
}

#[test]
fn a_copy_draft_has_no_identity_and_its_own_title() {
    let mut library = UserLibrary::default();
    let id = library.upsert(&draft("a", "ls {dir}")).expect("made");
    let original = library.get(&id).expect("kept").clone();
    let copy = Draft::copy_of(&original, "Copy of a".to_owned());
    assert_eq!(copy.id, None);
    assert_eq!(copy.title, "Copy of a");
    assert_eq!(copy.linux_pattern, "ls {dir}");
    let copy_id = library.upsert(&copy).expect("copy made");
    assert_ne!(copy_id, id);
    assert_eq!(library.commands().len(), 2);
}
