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

//! The family the terminals' text is drawn in, as the C# `TerminalFontFamily`: the embedded
//! one unless another is chosen, among the monospace families desktops commonly have.
//!
//! iced draws a family by name from the faces it was given and those of the computer, which
//! its text system reads when it starts. A family it lacks would be drawn in a fallback of
//! its own choosing, proportional on most desktops, and a terminal's columns would no longer
//! line up. So a family chosen is drawn only once it is found on the computer, monospace;
//! otherwise the embedded one is, and the Settings page says so. The C# box offers every
//! installed family and takes any name typed; here only [`FAMILIES`] are offered, a family
//! named in the settings file by hand among them, whatever its case.
//!
//! A cell's size follows the family: its advance and its height are read from the face's
//! own tables (`hmtx`, `hhea`), as they were measured for the embedded face.

use std::sync::OnceLock;

use heimdall_core::settings::TERMINAL_FONT_FAMILY_DEFAULT;
use iced::advanced::graphics::text::{cosmic_text::fontdb, font_system};

/// Family of the embedded terminal font, the settings' default.
pub const FONT_FAMILY: &str = TERMINAL_FONT_FAMILY_DEFAULT;

/// The families offered: the embedded one first, then those Windows ships, then those of
/// Linux desktops.
pub const FAMILIES: [&str; 9] = [
    FONT_FAMILY,
    "Cascadia Mono",
    "Cascadia Code",
    "Consolas",
    "Courier New",
    "Lucida Console",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
];

/// The characters whose advance must be the same as that of `0` for a face to be monospace.
const MONOSPACE_PROBES: [char; 4] = ['i', 'M', 'W', ' '];

/// The character a face's cell width is read from.
const ADVANCE_PROBE: char = '0';

/// The proportions of a monospace face, in em.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceMetrics {
    /// Advance of every glyph.
    pub advance: f32,
    /// Ascender plus descender, no line gap.
    pub line: f32,
}

impl FaceMetrics {
    /// The embedded Source Code Pro's, measured in its four faces on 2026-09-26 (`hmtx`,
    /// `hhea`): every glyph advances 600 units of a 1000-unit em; ascender 984, descender
    /// 273, no line gap.
    pub const EMBEDDED: Self = Self {
        advance: 0.6,
        line: 1.257,
    };

    /// The proportions of face `index` of the font file `data`; `None` when it cannot be
    /// read, or is not monospace.
    #[must_use]
    pub fn measure(data: &[u8], index: u32) -> Option<Self> {
        let face = ttf_parser::Face::parse(data, index).ok()?;
        let advance = |c: char| {
            face.glyph_index(c)
                .and_then(|glyph| face.glyph_hor_advance(glyph))
        };
        let width = advance(ADVANCE_PROBE).filter(|width| *width > 0)?;
        if MONOSPACE_PROBES
            .iter()
            .any(|probe| advance(*probe) != Some(width))
        {
            return None;
        }
        let em = f32::from(face.units_per_em());
        let hhea = face.tables().hhea;
        let line = f32::from(hhea.ascender) - f32::from(hhea.descender);
        (em > 0.0 && line > 0.0).then(|| Self {
            advance: f32::from(width) / em,
            line: line / em,
        })
    }
}

/// The font terminals are drawn in: its family, and the proportions of its cells.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerminalFont {
    /// The family, by name.
    pub family: &'static str,
    /// Its proportions.
    pub metrics: FaceMetrics,
}

impl TerminalFont {
    /// The embedded font.
    pub const EMBEDDED: Self = Self {
        family: FONT_FAMILY,
        metrics: FaceMetrics::EMBEDDED,
    };

    /// The font the settings name as `chosen`, looked for on this computer.
    #[must_use]
    pub fn chosen(chosen: &str) -> Self {
        Self::resolve(chosen, installed)
    }

    /// The font `chosen` names: one of [`FAMILIES`], whatever its case, with the proportions
    /// `installed` finds it at; the embedded one for any other name, or one `installed` does
    /// not find.
    #[must_use]
    pub fn resolve(chosen: &str, installed: impl Fn(&'static str) -> Option<FaceMetrics>) -> Self {
        match offered(chosen) {
            Some(family) if family != FONT_FAMILY => {
                installed(family).map_or(Self::EMBEDDED, |metrics| Self { family, metrics })
            }
            _ => Self::EMBEDDED,
        }
    }

    /// Whether this is the family `chosen` names: false when another is drawn in its place.
    #[must_use]
    pub fn is(&self, chosen: &str) -> bool {
        self.family.eq_ignore_ascii_case(chosen.trim())
    }
}

/// The family of [`FAMILIES`] `chosen` names, whatever its case and the spaces around it.
fn offered(chosen: &str) -> Option<&'static str> {
    let chosen = chosen.trim();
    FAMILIES
        .into_iter()
        .find(|family| family.eq_ignore_ascii_case(chosen))
}

/// The families of [`FAMILIES`] that can be drawn, as `installed` finds them: the embedded
/// one always, first.
#[must_use]
pub fn available(installed: impl Fn(&'static str) -> Option<FaceMetrics>) -> Vec<&'static str> {
    FAMILIES
        .into_iter()
        .filter(|family| *family == FONT_FAMILY || installed(family).is_some())
        .collect()
}

/// The proportions of `family`, one of [`FAMILIES`], as this computer has it: `None` when
/// it has no face of that name, or one that is not monospace. Read once, at the first
/// question: a font installed while the application runs is not seen, as iced's text
/// system does not see it either.
#[must_use]
pub fn installed(family: &'static str) -> Option<FaceMetrics> {
    static FOUND: OnceLock<Vec<Option<FaceMetrics>>> = OnceLock::new();
    let found = FOUND.get_or_init(|| {
        let Ok(mut system) = font_system().write() else {
            return Vec::new();
        };
        let database = system.raw().db();
        FAMILIES
            .iter()
            .map(|family| look_up(database, family))
            .collect()
    });
    FAMILIES
        .iter()
        .position(|offered| *offered == family)
        .and_then(|index| found.get(index).copied().flatten())
}

/// The proportions of the regular face of `family` in `database`.
fn look_up(database: &fontdb::Database, family: &str) -> Option<FaceMetrics> {
    let id = database.query(&fontdb::Query {
        families: &[fontdb::Family::Name(family)],
        ..fontdb::Query::default()
    })?;
    database.with_face_data(id, FaceMetrics::measure).flatten()
}

#[cfg(test)]
mod tests {
    use super::{FAMILIES, FONT_FAMILY, FaceMetrics, TerminalFont, available};
    use crate::terminal_view::FONTS;

    /// A computer with only Consolas, at made-up proportions.
    fn consolas_only(family: &'static str) -> Option<FaceMetrics> {
        (family == "Consolas").then_some(FaceMetrics {
            advance: 0.55,
            line: 1.2,
        })
    }

    #[test]
    fn the_embedded_proportions_are_those_its_faces_hold() {
        for face in FONTS {
            let measured = FaceMetrics::measure(face, 0).expect("monospace");
            assert!((measured.advance - FaceMetrics::EMBEDDED.advance).abs() < 1e-4);
            assert!((measured.line - FaceMetrics::EMBEDDED.line).abs() < 1e-4);
        }
    }

    #[test]
    fn what_is_not_a_font_is_not_measured() {
        assert_eq!(FaceMetrics::measure(b"not a font", 0), None);
        assert_eq!(FaceMetrics::measure(FONTS[0], 1), None, "no second face");
    }

    #[test]
    fn a_family_found_is_drawn_and_any_other_falls_back_to_the_embedded_one() {
        let chosen = TerminalFont::resolve(" consolas ", consolas_only);
        assert_eq!(chosen.family, "Consolas", "whatever its case");
        assert!((chosen.metrics.advance - 0.55).abs() < f32::EPSILON);
        assert!(chosen.is("Consolas"));
        for missing in ["Cascadia Mono", "Comic Sans MS", "", FONT_FAMILY] {
            let drawn = TerminalFont::resolve(missing, consolas_only);
            assert_eq!(drawn, TerminalFont::EMBEDDED, "{missing:?}");
        }
        assert!(!TerminalFont::resolve("Cascadia Mono", consolas_only).is("Cascadia Mono"));
    }

    #[test]
    fn the_families_offered_are_those_found_the_embedded_one_first() {
        assert_eq!(available(consolas_only), [FONT_FAMILY, "Consolas"]);
        assert_eq!(available(|_| None), [FONT_FAMILY]);
        assert_eq!(FAMILIES[0], FONT_FAMILY);
    }
}
