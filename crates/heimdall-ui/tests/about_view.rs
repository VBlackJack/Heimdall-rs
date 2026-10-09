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

//! The About page of the window's navigation: what it says, laid out as the C# About page,
//! and its diagnostics log switch.
//!
//! Setting `HEIMDALL_SNAPSHOT_DIR` writes a PNG of the page, for a visual pass.

mod common;

use std::path::Path;

use heimdall_app::{App, AppConfig, SystemCredentials};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::about_view::{COLUMN_WIDTH, LABEL_WIDTH, column_id, components};
use heimdall_ui::shell::{Destination, Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Rectangle, Settings, Size};
use iced_test::selector::Candidate;

/// A window tall enough for the whole tab.
const WINDOW: Size = Size::new(1100.0, 1400.0);

/// A window narrower than the column, as tall.
const NARROW_WINDOW: Size = Size::new(420.0, 1400.0);

/// Space around the column, as the C# page's `SpacingXl` margin.
const PAGE_MARGIN: f32 = 24.0;

/// Space inside a section card, as the C# `Padding="20"`.
const CARD_PADDING: f32 = 20.0;

/// How far a laid-out edge may stray from where it is expected: a pixel's rounding.
const TOLERANCE: f32 = 1.0;

/// Environment variable naming a directory for PNG snapshots.
const SNAPSHOT_VARIABLE: &str = "HEIMDALL_SNAPSHOT_DIR";

/// A folder name long enough that a path holding it is wider than any card.
const LONG_FOLDER: &str = "a-folder-named-at-length-so-that-its-path-is-wider-than-any-card";

fn shell(dir: &Path) -> Shell {
    shell_with_config(dir, dir)
}

/// The shell of an application whose profiles are kept in `config`, on the About page.
fn shell_with_config(dir: &Path, config: &Path) -> Shell {
    let mut shell = Shell::with_app(App::new(AppConfig {
        profiles_file: config.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }));
    let _ = shell.update(Message::Navigate(Destination::About));
    shell
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    simulator_at(shell, WINDOW)
}

fn simulator_at(shell: &Shell, size: Size) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, size, shell.view())
}

/// A picture of the window, written as `name` to the folder [`SNAPSHOT_VARIABLE`] names.
fn snapshot(shell: &Shell, name: &str) {
    let Some(dir) = std::env::var_os(SNAPSHOT_VARIABLE) else {
        return;
    };
    let path = Path::new(&dir).join(name);
    // iced names the picture after its renderer: clear every variant, or an old picture is
    // only compared against and never replaced.
    let stem = name.trim_end_matches(".png");
    if let Ok(entries) = std::fs::read_dir(Path::new(&dir)) {
        for entry in entries.flatten() {
            let file = entry.file_name();
            let file = file.to_string_lossy();
            if file == name || file.starts_with(&format!("{stem}-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    simulator(shell)
        .snapshot(&shell.theme())
        .expect("drawn")
        .matches_image(path)
        .expect("written");
}

/// The column of cards.
fn column_of(ui: &mut common::Drawn<'_>) -> Rectangle {
    ui.find(column_id()).expect("the column").bounds()
}

/// Where the text `label` is drawn in `area`: the navigation and the status bar say
/// "Sessions" and "0" too.
fn text_in(ui: &mut common::Drawn<'_>, label: &str, area: Rectangle) -> Rectangle {
    let wanted = label.to_owned();
    ui.find(move |candidate: Candidate<'_>| match candidate {
        Candidate::Text {
            content, bounds, ..
        } if content == wanted && area.intersects(&bounds) => Some(bounds),
        _ => None,
    })
    .unwrap_or_else(|_| panic!("{label} in {area:?}"))
}

fn near(found: f32, expected: f32) -> bool {
    (found - expected).abs() <= TOLERANCE
}

#[test]
fn the_about_tab_says_the_version_and_where_the_data_is_and_turns_the_log_off() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let messages: Vec<Message> = {
        let mut ui = simulator(&shell);
        let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
        let config = dir.path().display().to_string();
        for label in [
            version.as_str(),
            "System",
            "Data",
            "Quick access",
            "Open config folder",
            "Open logs folder",
            config.as_str(),
        ] {
            ui.find(label).expect(label);
        }
        ui.click("Write the application diagnostics log (Heimdall's own events and errors)")
            .expect("the switch");
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = shell.update(message);
    }
    assert!(!shell.app().settings().diagnostics_log, "off, and saved");
}

#[test]
fn the_build_date_is_said_when_the_build_knows_it_and_left_out_otherwise() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = shell(dir.path());
    let mut ui = simulator(&shell);
    ui.find("Platform").expect("the system card");
    match heimdall_ui::about_view::build_date() {
        Some(date) => {
            // `YYYY-MM-DD`, as the C# writes it.
            let parts: Vec<&str> = date.split('-').collect();
            assert_eq!(
                parts.iter().map(|part| part.len()).collect::<Vec<_>>(),
                [4, 2, 2],
                "{date}"
            );
            assert!(
                parts
                    .iter()
                    .all(|part| part.bytes().all(|b| b.is_ascii_digit())),
                "{date}"
            );
            ui.find("Build date").expect("the C# row");
            ui.find(date.as_str()).expect("the day");
        }
        None => assert!(ui.find("Build date").is_err(), "nothing known, no row"),
    }
}

#[test]
fn the_page_is_one_centred_column_as_wide_as_the_csharps_with_every_card_in_it() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = shell(dir.path());
    let mut ui = simulator(&shell);
    let column = column_of(&mut ui);
    assert!(near(column.width, COLUMN_WIDTH), "{column:?}");
    assert!(
        near(column.center_x(), WINDOW.width / 2.0),
        "centred in the window: {column:?}"
    );

    // The application's card: its name and version centred over the column, then its
    // author and licence, labels in one column and values beside them.
    let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
    let mut above = column.y;
    for label in ["Heimdall", version.as_str()] {
        let bounds = text_in(&mut ui, label, column);
        assert!(
            near(bounds.center_x(), column.center_x()),
            "{label} centred: {bounds:?}"
        );
        assert!(bounds.y > above, "{label} under what comes before it");
        above = bounds.y;
    }
    let author = text_in(&mut ui, "Author", column);
    let license = text_in(&mut ui, "License", column);
    assert!(author.y > above, "the author under the version");
    assert!(near(author.x, license.x), "one column of labels");
    assert!(license.y > author.y);
    for value in [env!("CARGO_PKG_AUTHORS"), env!("CARGO_PKG_LICENSE")] {
        let bounds = text_in(&mut ui, value, column);
        assert!(
            near(bounds.x, author.x + LABEL_WIDTH),
            "{value}: {bounds:?}"
        );
    }

    // The cards under it, the C#'s in its order then Heimdall-rs's own, each titled at the
    // same left edge: cards as wide as each other, in one column.
    let card_left = column.x + CARD_PADDING;
    let mut above = license.y;
    for title in [
        "System",
        "Data",
        "Quick access",
        "Settings file",
        "Diagnostics",
    ] {
        let bounds = text_in(&mut ui, title, column);
        assert!(
            near(bounds.x, card_left),
            "{title}: {bounds:?} in {column:?}"
        );
        assert!(bounds.y > above, "{title} under the card before it");
        above = bounds.y;
    }

    // Each card's lines: the label at the card's edge, the value in the next column.
    let platform = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    let config = dir.path().display().to_string();
    let mut rows: Vec<(String, Option<String>)> = vec![
        ("Platform".to_owned(), Some(platform)),
        ("Sessions".to_owned(), Some("0".to_owned())),
        ("Gateways".to_owned(), Some("0".to_owned())),
        ("Config".to_owned(), Some(config)),
        ("Logs".to_owned(), None),
    ];
    rows.extend(
        components()
            .into_iter()
            .map(|(label, version)| (label, Some(version.to_owned()))),
    );
    for (label, value) in rows {
        let bounds = text_in(&mut ui, &label, column);
        assert!(near(bounds.x, card_left), "{label}: {bounds:?}");
        if let Some(value) = value {
            let row = Rectangle {
                x: column.x,
                width: column.width,
                ..bounds
            };
            let shown = text_in(&mut ui, &value, row);
            assert!(
                near(shown.x, card_left + LABEL_WIDTH),
                "{label} = {value}: {shown:?}"
            );
        }
    }

    // Every button of the cards, in the column.
    for button in [
        "Open config folder",
        "Open logs folder",
        "Open notes folder",
        "GitHub",
        "Export settings...",
        "Import settings...",
    ] {
        let bounds = text_in(&mut ui, button, column);
        assert!(
            bounds.x >= column.x && bounds.x + bounds.width <= column.x + column.width,
            "{button} in the column: {bounds:?}"
        );
    }
    text_in(
        &mut ui,
        "Write the application diagnostics log (Heimdall's own events and errors)",
        column,
    );
    drop(ui);
    snapshot(&shell, "about-after.png");
}

#[test]
fn the_system_card_names_the_components_this_build_holds_at_the_locked_versions() {
    let named: Vec<String> = components().into_iter().map(|(label, _)| label).collect();
    assert_eq!(named, ["iced", "russh", "IronRDP"], "each locked once");
    let lock =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"))
            .expect("the workspace's lock");
    for ((_, version), package) in components().into_iter().zip(["iced", "russh", "ironrdp"]) {
        assert_eq!(
            heimdall_ui::component_versions::locked_version(&lock, package).as_deref(),
            Some(version),
            "{package}: the version built with"
        );
    }
}

#[test]
fn a_narrow_window_narrows_the_column_and_wraps_a_long_path_inside_it() {
    let dir = tempfile::tempdir().expect("dir");
    let config = dir.path().join(LONG_FOLDER).join(LONG_FOLDER);
    std::fs::create_dir_all(&config).expect("config folder");
    let shell = shell_with_config(dir.path(), &config);
    let mut ui = simulator_at(&shell, NARROW_WINDOW);
    let column = column_of(&mut ui);
    assert!(
        near(column.width, NARROW_WINDOW.width - 2.0 * PAGE_MARGIN),
        "{column:?}"
    );
    let label = text_in(&mut ui, "Config", column);
    let path = text_in(&mut ui, &config.display().to_string(), column);
    assert!(
        path.x + path.width <= column.x + column.width - CARD_PADDING + TOLERANCE,
        "wrapped in its card: {path:?} in {column:?}"
    );
    assert!(
        path.height > 2.0 * label.height,
        "on several lines: {path:?}"
    );
}
