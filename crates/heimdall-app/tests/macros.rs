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

//! Terminal macros, as the C# ones: recorded from what is typed, named, kept, typed again.

use std::path::Path;
use std::sync::{Arc, Mutex};

use heimdall_app::macro_player::MacroOutcome;
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, EntryField, EntryProblem,
    InputSink, KeyInput, MacroDraft, MacroEdit, MacroMessage, MacroProblem, Message, Notice, TabId,
};
use heimdall_core::macros::{Macros, macros_path};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::{GridSize, Key, KeyLocation, Modifiers, NamedKey};

/// Records what the application sends to a session.
#[derive(Debug, Default)]
struct RecordingSink {
    written: Mutex<Vec<u8>>,
}

impl RecordingSink {
    fn taken(&self) -> String {
        String::from_utf8(std::mem::take(&mut *self.written.lock().expect("written")))
            .expect("text")
    }
}

impl InputSink for RecordingSink {
    fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        self.written.lock().expect("written").extend(bytes);
        Ok(())
    }
    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "Web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

/// A connected session of profile "web", and what it is sent.
fn connected(app: &mut App) -> (TabId, AttemptId, Arc<RecordingSink>) {
    let (tab, attempt) = match app
        .update(Message::OpenProfile(ProfileId::new("web")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let sink = Arc::new(RecordingSink::default());
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: sink.clone(),
        },
    });
    (tab, attempt, sink)
}

fn typed(app: &mut App, tab: TabId, text: &str) {
    for c in text.chars() {
        app.update(Message::Key {
            tab,
            input: KeyInput {
                key: Key::Character(c),
                text: Some(c.to_string()),
                physical_digit: None,
                location: KeyLocation::Standard,
                modifiers: Modifiers::default(),
            },
        });
    }
}

fn enter(app: &mut App, tab: TabId) {
    app.update(Message::Key {
        tab,
        input: KeyInput {
            key: Key::Named(NamedKey::Enter),
            text: None,
            physical_digit: None,
            location: KeyLocation::Standard,
            modifiers: Modifiers::default(),
        },
    });
}

fn macros(app: &mut App, message: MacroMessage) -> Vec<Effect> {
    app.update(Message::Macro(message))
}

#[test]
fn what_is_typed_is_recorded_named_and_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _, sink) = connected(&mut app);
    let menu = app
        .macro_menu(app.tab(tab).expect("tab"))
        .expect("a terminal takes macros");
    assert_eq!(menu.recording, None);
    assert!(menu.macros.is_empty());

    macros(&mut app, MacroMessage::Record(tab));
    typed(&mut app, tab, "uptime");
    enter(&mut app, tab);
    assert_eq!(sink.taken(), "uptime\r", "typed as ever");
    let menu = app.macro_menu(app.tab(tab).expect("tab")).expect("menu");
    assert_eq!(menu.recording, Some(1), "the letters of a line, one input");

    macros(&mut app, MacroMessage::StopRecording(tab));
    assert!(matches!(&app.dialog, Some(Dialog::SaveMacro { entries, .. }) if entries.len() == 1));
    // No name: asked again.
    app.update(Message::ConfirmDialog);
    assert!(matches!(app.dialog, Some(Dialog::SaveMacro { .. })));
    macros(&mut app, MacroMessage::NameEdited(" Uptime ".to_owned()));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.dialog, None);
    assert_eq!(app.notice(), Some(&Notice::MacroSaved("Uptime".to_owned())));
    let kept = Macros::load(&macros_path(&dir.path().join("profiles.toml"))).expect("kept");
    let uptime = kept.get("uptime").expect("by name");
    assert_eq!(uptime.entries[0].input, "uptime\r");
    assert_eq!(uptime.entries[0].delay_ms, 0);

    // Nothing typed: nothing kept.
    macros(&mut app, MacroMessage::Record(tab));
    macros(&mut app, MacroMessage::StopRecording(tab));
    assert_eq!(app.dialog, None);
    assert_eq!(app.notice(), Some(&Notice::MacroNothingRecorded));
}

#[tokio::test]
async fn a_macro_kept_is_typed_into_a_session_one_at_a_time() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt, sink) = connected(&mut app);
    macros(&mut app, MacroMessage::Record(tab));
    typed(&mut app, tab, "id");
    enter(&mut app, tab);
    macros(&mut app, MacroMessage::StopRecording(tab));
    macros(&mut app, MacroMessage::NameEdited("who".to_owned()));
    app.update(Message::ConfirmDialog);
    sink.taken();

    let play = || MacroMessage::Play {
        tab,
        name: "WHO".to_owned(),
    };
    let run = match macros(&mut app, play()).into_iter().next() {
        Some(Effect::PlayMacro { tab: played, run }) if played == tab => run,
        other => panic!("{other:?}"),
    };
    let menu = app.macro_menu(app.tab(tab).expect("tab")).expect("menu");
    assert_eq!(menu.playing.as_deref(), Some("who"));
    assert!(macros(&mut app, play()).is_empty(), "one at a time");
    // What the session shows reaches the macro while it plays.
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Output(b"$ ".to_vec()),
    });
    let outcome = run.await;
    assert_eq!(outcome, MacroOutcome::Completed);
    assert_eq!(sink.taken(), "id\r");
    macros(&mut app, MacroMessage::Finished { tab, outcome });
    assert_eq!(
        app.notice(),
        Some(&Notice::MacroEnded {
            name: "who".to_owned(),
            outcome: MacroOutcome::Completed,
        })
    );
    assert_eq!(
        app.macro_menu(app.tab(tab).expect("tab"))
            .expect("menu")
            .playing,
        None
    );

    // Deleted once agreed to.
    macros(&mut app, MacroMessage::AskDelete("Who".to_owned()));
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmDeleteMacro("Who".to_owned()))
    );
    app.update(Message::DismissDialog);
    assert_eq!(app.macros().len(), 1);
    macros(&mut app, MacroMessage::AskDelete("Who".to_owned()));
    app.update(Message::ConfirmDialog);
    assert!(app.macros().is_empty());
    assert_eq!(app.notice(), Some(&Notice::MacroDeleted("Who".to_owned())));
}

/// A macro of one input, `id` then Enter, kept as "who".
fn recorded(app: &mut App, tab: TabId) {
    macros(app, MacroMessage::Record(tab));
    typed(app, tab, "id");
    enter(app, tab);
    macros(app, MacroMessage::StopRecording(tab));
    macros(app, MacroMessage::NameEdited("who".to_owned()));
    app.update(Message::ConfirmDialog);
}

fn draft(app: &mut App, edit: MacroEdit) {
    macros(app, MacroMessage::Draft(edit));
}

fn edited(app: &App) -> &MacroDraft {
    match &app.dialog {
        Some(Dialog::EditMacro(draft)) => draft,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_macro_is_edited_its_inputs_written_with_their_escapes_and_checked_before_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _, _) = connected(&mut app);
    recorded(&mut app, tab);

    macros(&mut app, MacroMessage::Edit("WHO".to_owned()));
    let shown = edited(&app);
    assert_eq!(shown.name, "who");
    assert_eq!(
        shown.entries[0].input, r"id\r",
        "Enter written as the C# writes it"
    );

    // A step waiting for the prompt, moved first.
    draft(&mut app, MacroEdit::Add { expects: true });
    draft(
        &mut app,
        MacroEdit::Field {
            entry: 1,
            field: EntryField::Pattern("[".to_owned()),
        },
    );
    draft(
        &mut app,
        MacroEdit::Field {
            entry: 1,
            field: EntryField::Regex(true),
        },
    );
    draft(&mut app, MacroEdit::MoveUp(1));
    draft(&mut app, MacroEdit::Name("whoami".to_owned()));
    // A regular expression that does not compile: not kept, said.
    app.update(Message::ConfirmDialog);
    assert!(matches!(
        edited(&app).problem,
        Some(MacroProblem::Entry {
            entry: 1,
            problem: EntryProblem::Regex(_)
        })
    ));
    draft(
        &mut app,
        MacroEdit::Field {
            entry: 0,
            field: EntryField::Pattern(r"\$ $".to_owned()),
        },
    );
    draft(
        &mut app,
        MacroEdit::Field {
            entry: 1,
            field: EntryField::Input(r"whoami\r".to_owned()),
        },
    );
    app.update(Message::ConfirmDialog);
    assert_eq!(app.dialog, None);

    // Renamed: in place of the old one.
    let names: Vec<&str> = app.macros().iter().map(|kept| kept.name.as_str()).collect();
    assert_eq!(names, ["whoami"]);
    let kept = &app.macros()[0];
    assert_eq!(kept.entries.len(), 2);
    assert_eq!(kept.entries[0].input, "");
    assert!(
        kept.entries[0]
            .expect
            .as_ref()
            .is_some_and(|expect| expect.regex)
    );
    assert_eq!(kept.entries[1].input, "whoami\r");
}

#[test]
fn a_macro_without_a_name_or_with_a_wrong_input_is_not_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _, _) = connected(&mut app);
    recorded(&mut app, tab);
    macros(&mut app, MacroMessage::Edit("who".to_owned()));
    draft(&mut app, MacroEdit::Name("  ".to_owned()));
    app.update(Message::ConfirmDialog);
    assert_eq!(edited(&app).problem, Some(MacroProblem::NameRequired));
    draft(&mut app, MacroEdit::Name("who".to_owned()));
    draft(
        &mut app,
        MacroEdit::Field {
            entry: 0,
            field: EntryField::Delay("soon".to_owned()),
        },
    );
    app.update(Message::ConfirmDialog);
    assert_eq!(
        edited(&app).problem,
        Some(MacroProblem::Entry {
            entry: 1,
            problem: EntryProblem::Delay
        })
    );
    // Cancelled: as it was.
    app.update(Message::DismissDialog);
    assert_eq!(app.macros()[0].entries[0].input, "id\r");
}
