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

//! The permanent contrast check: every text and every line the window draws, on every
//! surface it is drawn on, in each of the 18 themes with each of the nine accents, read back
//! from the style functions the widgets use. WCAG 2: 4.5:1 for text, 3:1 for a line or a
//! glyph that carries meaning; disabled controls are exempt.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use heimdall_app::{ProfileKind, SessionState};
use heimdall_core::settings::{Accent, AppTheme};
use iced::widget::button::{Status as ButtonStatus, Style as ButtonStyle};
use iced::widget::checkbox::Status as CheckStatus;
use iced::widget::container::Style as BoxStyle;
use iced::widget::pick_list::Status as ListStatus;
use iced::widget::radio::Status as RadioStatus;
use iced::widget::text_input::Status as FieldStatus;
use iced::{Background, Color, Theme};

use crate::themes::{self, contrast_ratio, over};
use crate::{detail_view, palette, styles, tokens, tree_view};

/// What small text must reach.
const TEXT: f64 = themes::MIN_TEXT_CONTRAST;

/// What a line or a glyph that carries meaning must reach.
const LINE: f64 = themes::MIN_BOUNDARY_CONTRAST;

/// The least number of pairs one theme with one accent is checked on: a guard against the
/// walk quietly checking nothing.
const MIN_PAIRS_PER_THEME: usize = 250;

/// How far above its requirement a pair is still reported as a narrow pass.
const NARROW: f64 = 0.15;

/// How many failing pairs the assertion message lists.
const REPORTED: usize = 60;

/// A pair under its requirement.
struct Failure {
    theme: AppTheme,
    accent: Accent,
    pair: String,
    ratio: f64,
    required: f64,
}

/// The pairs checked, and the ones that fell short.
#[derive(Default)]
struct Audit {
    checked: usize,
    failures: Vec<Failure>,
    /// The pairs that pass by less than [`NARROW`]: the lowest ratio, and how often.
    narrow: BTreeMap<String, (f64, usize)>,
}

/// One theme with one accent, being walked.
struct Walk<'a> {
    audit: &'a mut Audit,
    theme: AppTheme,
    accent: Accent,
}

impl Walk<'_> {
    /// Checks `foreground` on `ground` against `required`.
    fn pair(&mut self, name: &str, foreground: Color, ground: Color, required: f64) {
        self.audit.checked += 1;
        let ratio = contrast_ratio(over(foreground, ground), ground);
        if (required..required + NARROW).contains(&ratio) {
            let entry = self
                .audit
                .narrow
                .entry(name.to_owned())
                .or_insert((f64::MAX, 0));
            entry.0 = entry.0.min(ratio);
            entry.1 += 1;
        }
        if ratio < required {
            self.audit.failures.push(Failure {
                theme: self.theme,
                accent: self.accent,
                pair: name.to_owned(),
                ratio,
                required,
            });
        }
    }
}

/// The colour of a solid background.
fn solid(background: Background) -> Color {
    match background {
        Background::Color(color) => color,
        Background::Gradient(_) => panic!("a gradient has no single colour"),
    }
}

/// The colour of an optional background, `fallback` without one.
fn fill_of(background: Option<Background>, fallback: Color) -> Color {
    background.map_or(fallback, solid)
}

/// Every surface text is drawn on, by name: the window (the sidebar, the menus, the tooltips
/// and the status bar are the same colour), a card (the fields too), a row under the pointer,
/// the selected row, and the selected row under the pointer.
fn surfaces(theme: &Theme) -> [(&'static str, Color); 5] {
    let window = theme.palette().background;
    let card = styles::card_color(theme);
    let hover = themes::selection_of(theme);
    let accent = theme.extended_palette().primary.base.color;
    let selected = over(accent.scale_alpha(tokens::SELECTED_ROW_ALPHA), window);
    let selected_hover = over(accent.scale_alpha(tokens::SELECTED_ROW_ALPHA), hover);
    [
        ("window", window),
        ("card", card),
        ("hovered row", hover),
        ("selected row", selected),
        ("selected hovered row", selected_hover),
    ]
}

/// The states a button is read in: disabled ones are exempt.
const STATES: [(&str, ButtonStatus); 3] = [
    ("rest", ButtonStatus::Active),
    ("hover", ButtonStatus::Hovered),
    ("pressed", ButtonStatus::Pressed),
];

type ButtonFn = Box<dyn Fn(&Theme, ButtonStatus) -> ButtonStyle>;

/// Every button style of the window, by name.
fn buttons() -> Vec<(&'static str, ButtonFn)> {
    vec![
        ("primary button", Box::new(styles::primary)),
        ("secondary button", Box::new(styles::secondary)),
        ("danger button", Box::new(styles::danger)),
        ("subtle button", Box::new(styles::subtle)),
        ("status pill button", Box::new(styles::status_pill)),
        ("broadcast pill", Box::new(styles::status_pill_active)),
        ("tool card", Box::new(styles::tool_card)),
        ("bare row", Box::new(styles::bare)),
        ("toggle off", Box::new(styles::toggle(false))),
        ("toggle on", Box::new(styles::toggle(true))),
        ("sidebar tab", Box::new(styles::sidebar_tab(false))),
        ("sidebar tab selected", Box::new(styles::sidebar_tab(true))),
        ("rail item", Box::new(styles::rail_item(false))),
        ("rail item selected", Box::new(styles::rail_item(true))),
        ("session tab", Box::new(styles::tab(false))),
        ("session tab selected", Box::new(styles::tab(true))),
        ("quick connect row", Box::new(styles::palette_row(false))),
        (
            "quick connect row chosen",
            Box::new(styles::palette_row(true)),
        ),
        ("tree menu entry", Box::new(tree_view::menu_style)),
        ("tree menu delete", Box::new(tree_view::danger_style)),
    ]
}

/// The text colours of the window, by name.
fn texts(theme: &Theme) -> [(&'static str, Color); 8] {
    let primary = theme.palette().text;
    [
        ("primary text", primary),
        (
            "secondary text",
            iced::widget::text::secondary(theme)
                .color
                .unwrap_or(primary),
        ),
        ("muted text", themes::derived(theme, themes::Derived::Muted)),
        ("accent text", styles::accent_text_color(theme)),
        ("error text", styles::error_text_color(theme)),
        ("warning text", styles::warning_text_color(theme)),
        ("success text", styles::success_text_color(theme)),
        ("info text", themes::derived(theme, themes::Derived::Info)),
    ]
}

/// Walks one theme with one accent.
fn walk(walk: &mut Walk<'_>, theme: &Theme) {
    let grounds = surfaces(theme);
    let window = grounds[0].1;
    let card = grounds[1].1;
    let primary = theme.palette().text;

    // Text colours, on every surface.
    for (what, colour) in texts(theme) {
        for (on, ground) in grounds {
            walk.pair(&format!("{what} on {on}"), colour, ground, TEXT);
        }
    }

    // Boxes drawn with their own text.
    let boxes: [(&str, BoxStyle); 7] = [
        ("status counter", styles::status_pill_box(theme)),
        ("folder count badge", styles::count_badge(theme)),
        ("tooltip", styles::tooltip(theme)),
        ("panel", styles::panel(theme)),
        ("bar", styles::bar(theme)),
        ("folder chip", styles::folder_chip(theme)),
        ("error count badge", styles::error_badge(theme)),
    ];
    for (what, style) in boxes {
        let text = style.text_color.unwrap_or(primary);
        walk.pair(
            &format!("{what} text"),
            text,
            fill_of(style.background, window),
            TEXT,
        );
    }

    // Buttons: the label on the fill, or on the window and a card where there is no fill.
    for (what, style) in buttons() {
        // A menu entry is drawn on the menu, which is the window's colour, never on a card.
        let on_card = !what.starts_with("tree menu");
        for (state, status) in STATES {
            let look = style(theme, status);
            let name = format!("{what} {state} label");
            if let Some(background) = look.background {
                let fill = over(solid(background), window);
                walk.pair(&name, look.text_color, fill, TEXT);
            } else {
                walk.pair(&format!("{name} on window"), look.text_color, window, TEXT);
                if on_card {
                    walk.pair(&format!("{name} on card"), look.text_color, card, TEXT);
                }
            }
        }
    }

    fields(walk, theme, window);
    controls(walk, theme, window, card);
    tree(walk, theme, &grounds);
}

/// Text fields and drop-downs.
fn fields(walk: &mut Walk<'_>, theme: &Theme, window: Color) {
    for (state, status) in [
        ("rest", FieldStatus::Active),
        ("hover", FieldStatus::Hovered),
        ("focus", FieldStatus::Focused { is_hovered: false }),
    ] {
        let look = styles::text_input(theme, status);
        let fill = solid(look.background);
        let name = format!("field {state}");
        walk.pair(&format!("{name} value"), look.value, fill, TEXT);
        walk.pair(&format!("{name} placeholder"), look.placeholder, fill, TEXT);
        walk.pair(&format!("{name} icon"), look.icon, fill, LINE);
        walk.pair(
            &format!("{name} border on fill"),
            look.border.color,
            fill,
            LINE,
        );
        walk.pair(
            &format!("{name} border on window"),
            look.border.color,
            window,
            LINE,
        );
        walk.pair(
            &format!("{name} selected text"),
            look.value,
            over(look.selection, fill),
            TEXT,
        );
    }
    for (state, status) in [
        ("rest", ListStatus::Active),
        ("hover", ListStatus::Hovered),
        ("open", ListStatus::Opened { is_hovered: false }),
    ] {
        let look = styles::pick_list(theme, status);
        let fill = solid(look.background);
        let name = format!("dropdown {state}");
        walk.pair(&format!("{name} text"), look.text_color, fill, TEXT);
        walk.pair(
            &format!("{name} placeholder"),
            look.placeholder_color,
            fill,
            TEXT,
        );
        walk.pair(&format!("{name} handle"), look.handle_color, fill, LINE);
        walk.pair(
            &format!("{name} border on fill"),
            look.border.color,
            fill,
            LINE,
        );
        walk.pair(
            &format!("{name} border on window"),
            look.border.color,
            window,
            LINE,
        );
    }
    let menu = styles::menu(theme);
    walk.pair("menu text", menu.text_color, solid(menu.background), TEXT);
    walk.pair(
        "menu entry under the pointer",
        menu.selected_text_color,
        solid(menu.selected_background),
        TEXT,
    );
}

/// Check boxes, radio buttons, bars and the lines that carry meaning.
fn controls(walk: &mut Walk<'_>, theme: &Theme, window: Color, card: Color) {
    let both = [("window", window), ("card", card)];
    for checked in [false, true] {
        for hovered in [false, true] {
            let status = if hovered {
                CheckStatus::Hovered {
                    is_checked: checked,
                }
            } else {
                CheckStatus::Active {
                    is_checked: checked,
                }
            };
            let look = styles::checkbox(theme, status);
            let name = format!("checkbox checked={checked} hovered={hovered}");
            let fill = solid(look.background);
            for (on, ground) in both {
                walk.pair(
                    &format!("{name} border on {on}"),
                    look.border.color,
                    ground,
                    LINE,
                );
                if checked {
                    walk.pair(&format!("{name} fill on {on}"), fill, ground, LINE);
                }
            }
            if checked {
                walk.pair(&format!("{name} tick"), look.icon_color, fill, LINE);
            }
            let status = if hovered {
                RadioStatus::Hovered {
                    is_selected: checked,
                }
            } else {
                RadioStatus::Active {
                    is_selected: checked,
                }
            };
            let look = styles::radio(theme, status);
            let name = format!("radio chosen={checked} hovered={hovered}");
            for (on, ground) in both {
                walk.pair(
                    &format!("{name} border on {on}"),
                    look.border_color,
                    ground,
                    LINE,
                );
            }
            if checked {
                walk.pair(
                    &format!("{name} dot"),
                    look.dot_color,
                    solid(look.background),
                    LINE,
                );
            }
        }
    }

    let progress = styles::progress(theme);
    walk.pair(
        "progress bar on its track",
        solid(progress.bar),
        solid(progress.background),
        LINE,
    );
    let rule = fill_of(styles::strong_rule(theme).background, window);
    let bar = fill_of(styles::underline(true)(theme).background, window);
    let dot = fill_of(styles::accent_dot(theme).background, window);
    let slider = styles::slider(theme, iced::widget::slider::Status::Active);
    for (on, ground) in both {
        walk.pair(&format!("strong rule on {on}"), rule, ground, LINE);
        walk.pair(&format!("selected tab bar on {on}"), bar, ground, LINE);
        walk.pair(&format!("accent dot on {on}"), dot, ground, LINE);
        walk.pair(
            &format!("slider thumb on {on}"),
            solid(slider.handle.background),
            ground,
            LINE,
        );
        walk.pair(
            &format!("slider filled rail on {on}"),
            solid(slider.rail.backgrounds.0),
            ground,
            LINE,
        );
    }
    walk.pair(
        "modified badge border",
        styles::badge(theme).border.color,
        window,
        LINE,
    );
}

/// The session tree and its badges: dots and protocol marks on the sidebar, a row under the
/// pointer and the selected row.
fn tree(walk: &mut Walk<'_>, theme: &Theme, grounds: &[(&'static str, Color); 5]) {
    let card = grounds[1].1;
    for (what, missing) in [("gateway badge", false), ("gateway missing badge", true)] {
        let (border, text) = tree_view::gateway_badge_colors(theme, missing);
        walk.pair(&format!("{what} text"), text, card, TEXT);
        walk.pair(&format!("{what} border"), border, card, LINE);
    }
    let rows = [grounds[0], grounds[2], grounds[3], grounds[4]];
    for (what, state) in [
        ("connected", Some(SessionState::Connected)),
        ("connecting", Some(SessionState::Connecting)),
        ("failed", Some(SessionState::Failed)),
        ("unknown", None),
    ] {
        for (on, ground) in rows {
            walk.pair(
                &format!("state dot {what} on {on}"),
                tree_view::state_color(theme, state),
                ground,
                LINE,
            );
        }
    }
    for kind in ProfileKind::ALL {
        let colour = crate::icons::protocol_color(theme, kind);
        for (on, ground) in rows {
            walk.pair(
                &format!("protocol icon {kind:?} on {on}"),
                colour,
                ground,
                LINE,
            );
        }
        let (text, fill) = detail_view::pill_colors(theme, kind);
        walk.pair(&format!("protocol pill {kind:?}"), text, fill, TEXT);
        for (on, ground) in [grounds[0], grounds[1], grounds[2]] {
            let tint = over(colour.scale_alpha(palette::BADGE_ALPHA), ground);
            walk.pair(
                &format!("quick connect badge {kind:?} on {on}"),
                palette::badge_text(theme, colour),
                tint,
                TEXT,
            );
        }
    }
}

/// Walks the 18 themes with the nine accents.
fn run() -> (Audit, usize) {
    let mut audit = Audit::default();
    let mut combinations = 0;
    for each in AppTheme::ALL {
        for accent in Accent::ALL {
            let theme = themes::theme(each, accent);
            walk(
                &mut Walk {
                    audit: &mut audit,
                    theme: each,
                    accent,
                },
                &theme,
            );
            combinations += 1;
        }
    }
    (audit, combinations)
}

/// The failing pairs per theme, each pair counted once whatever the accent.
fn per_theme(audit: &Audit) -> BTreeMap<String, usize> {
    let mut names: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for failure in &audit.failures {
        names
            .entry(format!("{:?}", failure.theme))
            .or_default()
            .insert(&failure.pair);
    }
    names.into_iter().map(|(k, v)| (k, v.len())).collect()
}

#[test]
fn every_text_and_line_reaches_its_contrast_on_every_surface_in_every_theme_and_accent() {
    let (audit, combinations) = run();
    assert_eq!(AppTheme::ALL.len(), 18);
    assert_eq!(Accent::ALL.len(), 9);
    assert_eq!(combinations, 18 * 9, "every theme with every accent");
    assert!(
        audit.checked >= combinations * MIN_PAIRS_PER_THEME,
        "only {} pairs checked",
        audit.checked
    );
    assert_eq!(
        audit.checked % combinations,
        0,
        "every theme and accent is checked on the same pairs"
    );
    let mut report = String::new();
    for failure in audit.failures.iter().take(REPORTED) {
        let _ = writeln!(
            report,
            "{:?}/{:?}: {} is {:.2}:1, wants {}",
            failure.theme, failure.accent, failure.pair, failure.ratio, failure.required
        );
    }
    assert!(
        audit.failures.is_empty(),
        "{} pairs fall short; per theme (distinct pairs): {:?}\n{report}",
        audit.failures.len(),
        per_theme(&audit)
    );
}

/// Prints the failing pairs per theme: `cargo test -p heimdall-ui contrast_table -- --ignored
/// --nocapture`.
#[test]
#[ignore = "a report, not a check"]
fn contrast_table() {
    let (audit, combinations) = run();
    println!(
        "checked {} pairs over {combinations} themes with accents",
        audit.checked
    );
    for (theme, count) in per_theme(&audit) {
        println!("TABLE {theme} {count}");
    }
    let mut distinct: BTreeMap<&str, (f64, usize)> = BTreeMap::new();
    for failure in &audit.failures {
        let entry = distinct.entry(&failure.pair).or_insert((f64::MAX, 0));
        entry.0 = entry.0.min(failure.ratio);
        entry.1 += 1;
    }
    for (pair, (worst, count)) in distinct {
        println!("PAIR {pair} worst {worst:.2} x{count}");
    }
    for (pair, (lowest, count)) in &audit.narrow {
        println!("NARROW {pair} lowest {lowest:.2} x{count}");
    }
}
