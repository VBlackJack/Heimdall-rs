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

//! The Command Library tool drawn headless: its runbooks panel, docked under the list of
//! commands as the C# `RunbooksPanel`, and not a page that replaces the list.
//!
//! Strings are the fallback language's (English): the tests never select a language.

mod common;

use std::path::Path;

use heimdall_app::tools::ToolId;
use heimdall_app::{App, AppConfig, TabId};
use heimdall_core::tools::runbooks::{RUNBOOKS_FILE, Runbook, RunbookStore, Step};
use heimdall_core::tools::user_commands::{
    Criticality, LIBRARY_FILE, TemplatePlatform, UserLibrary, UserPlatform,
};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::tools::{
    CommandField, CommandLibraryMessage, ParamField, RunbookMessage, ToolMessage,
};
use iced::{Settings, Size};

/// Size of the simulated window, in logical pixels.
const WINDOW: Size = Size::new(1200.0, 900.0);

/// The name of the runbook the tests keep.
const RUNBOOK: &str = "Restart web";

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WINDOW, shell.view())
}

/// The library opened in a window whose folder keeps one runbook.
fn opened(dir: &Path) -> (Shell, TabId) {
    let (shell, tab, _) = opened_with_id(dir);
    (shell, tab)
}

/// [`opened`], and the identity of the runbook.
fn opened_with_id(dir: &Path) -> (Shell, TabId, String) {
    let mut runbook = Runbook::new(RUNBOOK);
    let id = runbook.public_id.clone();
    runbook.steps = vec![Step::literal(
        "systemctl restart nginx",
        TemplatePlatform::Linux,
    )];
    let mut store = RunbookStore::default();
    store.keep(runbook, &|_: &str| None).expect("kept");
    store.save(&dir.join(RUNBOOKS_FILE)).expect("saved");
    let mut shell = Shell::with_app(app(dir));
    let _ = shell.update(Message::OpenTool(ToolId::CmdLib));
    let tab = shell.app().tabs[0].id;
    (shell, tab, id)
}

fn runbooks(tab: TabId, message: RunbookMessage) -> Message {
    Message::Tool(
        tab,
        ToolMessage::CmdLib(CommandLibraryMessage::Runbooks(message)),
    )
}

#[test]
fn the_runbooks_panel_is_docked_under_the_list_and_does_not_replace_it() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path());
    {
        let mut ui = simulator(&shell);
        ui.find("Search commands").expect("the list's search");
        assert!(ui.find(RUNBOOK).is_err(), "the panel is closed");
    }
    let _ = shell.update(runbooks(tab, RunbookMessage::Toggle));
    let mut ui = simulator(&shell);
    // The list keeps its search and its filters, with the panel under them.
    ui.find("Search commands").expect("the list is still there");
    ui.find("Runbooks").expect("the panel's title");
    ui.find(RUNBOOK).expect("the runbook's row");
    assert!(
        ui.find("New runbook").is_err(),
        "the panel's actions are icons named by tooltips, not text chips"
    );
}

#[test]
fn a_press_on_a_row_selects_the_runbook() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path());
    let _ = shell.update(runbooks(tab, RunbookMessage::Toggle));
    let mut ui = simulator(&shell);
    ui.click(RUNBOOK).expect("the row");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::Tool(
            id,
            ToolMessage::CmdLib(CommandLibraryMessage::Runbooks(RunbookMessage::Select(_)))
        ) if id == tab
    )));
}

#[test]
fn a_run_keeps_its_buttons_in_the_window_at_800_by_600() {
    const SMALL: Size = Size::new(800.0, 600.0);
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, id) = opened_with_id(dir.path());
    let _ = shell.update(runbooks(tab, RunbookMessage::Toggle));
    let _ = shell.update(runbooks(tab, RunbookMessage::Run(id)));
    let _ = shell.update(runbooks(tab, RunbookMessage::RunBegin));
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    let mut ui = common::simulator(settings, SMALL, shell.view());
    for label in ["Send to terminal", "Insert", "Skip", "Stop"] {
        let bounds = ui.find(label).expect(label).bounds();
        assert!(
            bounds.x >= 0.0
                && bounds.y >= 0.0
                && bounds.x + bounds.width <= SMALL.width
                && bounds.y + bounds.height <= SMALL.height,
            "{label} is outside the window: {bounds:?}"
        );
    }
}

#[test]
fn the_list_and_the_panel_share_a_tall_window_without_a_blank_strip() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path());
    let _ = shell.update(runbooks(tab, RunbookMessage::Toggle));
    let mut ui = simulator(&shell);
    let panel = ui.find(RUNBOOK).expect("the row").bounds();
    let search = ui.find("Search commands").expect("the search").bounds();
    // The panel's row is in the lower part of the window and the list above it has room.
    assert!(panel.y > search.y + 128.0, "{panel:?} {search:?}");
    assert!(panel.y + panel.height <= WINDOW.height, "{panel:?}");
}

fn library(tab: TabId, message: CommandLibraryMessage) -> Message {
    Message::Tool(tab, ToolMessage::CmdLib(message))
}

fn type_into(shell: &mut Shell, tab: TabId, field: CommandField, value: &str) {
    let _ = shell.update(library(
        tab,
        CommandLibraryMessage::Field(field, value.to_owned()),
    ));
}

fn param(shell: &mut Shell, tab: TabId, index: usize, field: ParamField, value: &str) {
    let _ = shell.update(library(
        tab,
        CommandLibraryMessage::ParamText(TemplatePlatform::Linux, index, field, value.to_owned()),
    ));
}

/// The user's commands as the file holds them.
fn kept(dir: &Path) -> UserLibrary {
    UserLibrary::load(&dir.join(LIBRARY_FILE)).expect("a library")
}

/// What the dialog says of values written in the pattern and declared nowhere.
const UNDECLARED: &str = "Written in the command but not declared as parameters, so they are left exactly as they are: host, count";

#[test]
fn the_runbook_editor_is_a_dialog_over_the_list_and_escape_cancels_it() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path());
    let _ = shell.update(runbooks(tab, RunbookMessage::Toggle));
    let _ = shell.update(runbooks(tab, RunbookMessage::New));
    {
        let mut ui = simulator(&shell);
        ui.find("New runbook").expect("the dialog's title");
        ui.find("When a step fails").expect("the mode, now a combo");
        ui.find("Save").expect("Save");
        ui.find("Cancel").expect("Cancel");
        ui.find("Search commands")
            .expect("the list stays under the veil");
    }
    let _ = shell.update(Message::DialogKey { confirm: false });
    let mut ui = simulator(&shell);
    assert!(ui.find("Save").is_err(), "Escape closed the dialog");
}

#[test]
fn a_runbook_dialog_saves_on_enter() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path());
    let _ = shell.update(runbooks(tab, RunbookMessage::Toggle));
    let _ = shell.update(runbooks(tab, RunbookMessage::New));
    let _ = shell.update(runbooks(tab, RunbookMessage::Name("Second".to_owned())));
    let _ = shell.update(runbooks(tab, RunbookMessage::AddLiteral));
    let _ = shell.update(runbooks(
        tab,
        RunbookMessage::Command(0, "uptime".to_owned()),
    ));
    let _ = shell.update(Message::DialogKey { confirm: true });
    let store =
        RunbookStore::load(&dir.path().join(RUNBOOKS_FILE), &|_: &str| None).expect("loaded");
    assert!(store.runbooks().iter().any(|each| each.name == "Second"));
    let mut ui = simulator(&shell);
    assert!(ui.find("Save").is_err(), "Enter saved and closed it");
}

#[test]
fn the_command_dialog_edits_a_parameter_row_and_saves_it() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path());
    let _ = shell.update(library(tab, CommandLibraryMessage::New));
    {
        let mut ui = simulator(&shell);
        ui.find("Title").expect("the dialog's fields");
        ui.find("Risk Level").expect("the risk combo's label");
        ui.find("+ Parameter").expect("a template asks for rows");
    }
    type_into(&mut shell, tab, CommandField::Title, "Ping");
    type_into(&mut shell, tab, CommandField::Category, "Network");
    let _ = shell.update(library(
        tab,
        CommandLibraryMessage::EditPlatform(UserPlatform::Linux),
    ));
    type_into(
        &mut shell,
        tab,
        CommandField::LinuxPattern,
        "ping {host} {count}",
    );
    {
        let mut ui = simulator(&shell);
        ui.find(UNDECLARED).expect("the C# warning");
    }
    let add = || {
        library(
            tab,
            CommandLibraryMessage::AddParam(TemplatePlatform::Linux),
        )
    };
    let _ = shell.update(add());
    param(&mut shell, tab, 0, ParamField::Name, "host");
    param(&mut shell, tab, 0, ParamField::Label, "Target");
    let _ = shell.update(library(
        tab,
        CommandLibraryMessage::ParamRequired(TemplatePlatform::Linux, 0, true),
    ));
    let _ = shell.update(add());
    param(&mut shell, tab, 1, ParamField::Name, "count");
    // A third row, removed again.
    let _ = shell.update(add());
    let _ = shell.update(library(
        tab,
        CommandLibraryMessage::RemoveParam(TemplatePlatform::Linux, 2),
    ));
    {
        let mut ui = simulator(&shell);
        assert!(ui.find(UNDECLARED).is_err(), "both are declared now");
    }
    let _ = shell.update(Message::DialogKey { confirm: true });
    let library = kept(dir.path());
    let command = library.commands().first().expect("saved");
    let template = command.template(TemplatePlatform::Linux).expect("template");
    assert_eq!(template.parameters.len(), 2);
    assert_eq!(template.parameters[0].name, "host");
    assert_eq!(template.parameters[0].label, "Target");
    assert!(template.parameters[0].required);
    assert_eq!(template.parameters[1].name, "count");
}

#[test]
fn escape_cancels_the_command_dialog_and_keeps_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path());
    let _ = shell.update(library(tab, CommandLibraryMessage::New));
    type_into(&mut shell, tab, CommandField::Title, "Lost");
    type_into(&mut shell, tab, CommandField::Category, "Network");
    type_into(&mut shell, tab, CommandField::WindowsPattern, "Get-Date");
    let _ = shell.update(Message::DialogKey { confirm: false });
    assert!(kept(dir.path()).commands().is_empty());
    let mut ui = simulator(&shell);
    assert!(ui.find("Risk Level").is_err(), "the dialog is closed");
}

#[test]
fn the_risk_level_is_a_combo_box_whose_choice_is_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path());
    let _ = shell.update(library(tab, CommandLibraryMessage::New));
    {
        // One drop-down under its label, and no longer a chip per level: the three names are
        // not buttons of the form any more (a pick list draws the one chosen, not the rest).
        let mut ui = simulator(&shell);
        ui.find("Risk Level").expect("the drop-down's label");
        ui.find("Platforms")
            .expect("the platform drop-down beside it");
        assert!(ui.find("Dangerous").is_err(), "no chip for a level");
    }
    type_into(&mut shell, tab, CommandField::Title, "Wipe");
    type_into(&mut shell, tab, CommandField::Category, "Disk");
    let _ = shell.update(library(
        tab,
        CommandLibraryMessage::EditPlatform(UserPlatform::Linux),
    ));
    type_into(&mut shell, tab, CommandField::LinuxPattern, "sync");
    let _ = shell.update(library(
        tab,
        CommandLibraryMessage::EditLevel(Criticality::Dangerous),
    ));
    let _ = shell.update(Message::DialogKey { confirm: true });
    let library = kept(dir.path());
    assert_eq!(library.commands()[0].level, Criticality::Dangerous);
}
