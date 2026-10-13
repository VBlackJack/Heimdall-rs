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
//! one unless another is chosen, among the monospace families installed on the computer.
//!
//! The C# box offers every installed family (`SettingsViewModel.InstalledFontFamilies`,
//! WPF's `Fonts.SystemFontFamilies`) and takes any name typed. iced draws a family by name
//! from the faces it was given and those of the computer, which its text system reads when
//! it starts. A family it lacks would be drawn in a fallback of its own choosing,
//! proportional on most desktops, and a terminal's columns would no longer line up. So the
//! families offered are the installed ones whose faces say they are monospace, read from
//! iced's own font database, and a family chosen is drawn only once it is found there,
//! monospace; otherwise (a family uninstalled since it was chosen, or named in the settings
//! file by hand) the embedded one is, and the Settings page says so.
//!
//! A cell's size follows the family: its advance and its height are read from the face's
//! own tables (`hmtx`, `hhea`), as they were measured for the embedded face.

use std::sync::OnceLock;

use heimdall_core::settings::TERMINAL_FONT_FAMILY_DEFAULT;
use iced::advanced::graphics::text::{cosmic_text::fontdb, font_system};

/// Family of the embedded terminal font, the settings' default.
pub const FONT_FAMILY: &str = TERMINAL_FONT_FAMILY_DEFAULT;

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
        Self::resolve(chosen, installed())
    }

    /// The font `chosen` names among `installed`, whatever its case and the spaces around
    /// it; the embedded one for its own name, or a name `installed` does not hold.
    #[must_use]
    pub fn resolve(chosen: &str, installed: &[Self]) -> Self {
        let chosen = chosen.trim();
        if chosen.eq_ignore_ascii_case(FONT_FAMILY) {
            return Self::EMBEDDED;
        }
        installed
            .iter()
            .find(|font| font.family.eq_ignore_ascii_case(chosen))
            .copied()
            .unwrap_or(Self::EMBEDDED)
    }

    /// Whether this is the family `chosen` names: false when another is drawn in its place.
    #[must_use]
    pub fn is(&self, chosen: &str) -> bool {
        self.family.eq_ignore_ascii_case(chosen.trim())
    }
}

/// The families offered, as the C# box lists installed ones: the embedded one first, then
/// those of `installed` in their order, the embedded one not twice.
#[must_use]
pub fn available(installed: &[TerminalFont]) -> Vec<&'static str> {
    std::iter::once(FONT_FAMILY)
        .chain(
            installed
                .iter()
                .map(|font| font.family)
                .filter(|family| !family.eq_ignore_ascii_case(FONT_FAMILY)),
        )
        .collect()
}

/// The monospace families this computer has, as iced's text system read them when it
/// started: see [`monospace_families`]. Read once, at the first question: a font installed
/// while the application runs is not seen, as iced's text system does not see it either.
#[must_use]
pub fn installed() -> &'static [TerminalFont] {
    static FOUND: OnceLock<Vec<(String, FaceMetrics)>> = OnceLock::new();
    static FONTS: OnceLock<Vec<TerminalFont>> = OnceLock::new();
    FONTS.get_or_init(|| {
        FOUND
            .get_or_init(|| {
                font_system().write().map_or_else(
                    |_| Vec::new(),
                    |mut system| monospace_families(system.raw().db()),
                )
            })
            .iter()
            .map(|(family, metrics)| TerminalFont {
                family: family.as_str(),
                metrics: *metrics,
            })
            .collect()
    })
}

/// The families of `database` whose faces say they are monospace (the `post` table's
/// `isFixedPitch`, which fontdb reads), by their English name, sorted whatever their case
/// and each once, as the C# list is; with the proportions of their regular face, those whose
/// regular face does not measure as monospace left out.
#[must_use]
pub fn monospace_families(database: &fontdb::Database) -> Vec<(String, FaceMetrics)> {
    let mut families: Vec<&str> = database
        .faces()
        .filter(|face| face.monospaced)
        .filter_map(|face| face.families.first())
        .map(|(family, _)| family.trim())
        .filter(|family| !family.is_empty())
        .collect();
    families.sort_by_key(|family| family.to_lowercase());
    families.dedup_by(|one, other| one.eq_ignore_ascii_case(other));
    families
        .into_iter()
        .filter_map(|family| look_up(database, family).map(|metrics| (family.to_owned(), metrics)))
        .collect()
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
    use iced::advanced::graphics::text::{FIRA_SANS_REGULAR, cosmic_text::fontdb};

    use super::{FONT_FAMILY, FaceMetrics, TerminalFont, available, monospace_families};
    use crate::terminal_view::FONTS;

    /// A computer with Consolas and Cascadia Mono, at made-up proportions.
    const INSTALLED: [TerminalFont; 2] = [
        TerminalFont {
            family: "Cascadia Mono",
            metrics: FaceMetrics {
                advance: 0.58,
                line: 1.3,
            },
        },
        TerminalFont {
            family: "Consolas",
            metrics: FaceMetrics {
                advance: 0.55,
                line: 1.2,
            },
        },
    ];

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
    fn the_installed_monospace_families_are_listed_once_and_proportional_ones_left_out() {
        let mut database = fontdb::Database::new();
        for face in FONTS {
            database.load_font_data(face.to_vec());
        }
        database.load_font_data(FIRA_SANS_REGULAR.to_vec());
        assert_eq!(database.faces().count(), FONTS.len() + 1);
        let found = monospace_families(&database);
        assert_eq!(found.len(), 1, "Fira Sans is proportional: {found:?}");
        assert_eq!(found[0].0, FONT_FAMILY, "four faces, one family");
        assert!((found[0].1.advance - FaceMetrics::EMBEDDED.advance).abs() < 1e-4);
        assert!(monospace_families(&fontdb::Database::new()).is_empty());
    }

    #[test]
    fn a_family_installed_is_drawn_and_any_other_falls_back_to_the_embedded_one() {
        let chosen = TerminalFont::resolve(" consolas ", &INSTALLED);
        assert_eq!(chosen.family, "Consolas", "whatever its case");
        assert!((chosen.metrics.advance - 0.55).abs() < f32::EPSILON);
        assert!(chosen.is("Consolas"));
        // A family uninstalled since it was saved, a proportional one, none, the embedded.
        for missing in ["Fira Code", "Comic Sans MS", "", FONT_FAMILY] {
            let drawn = TerminalFont::resolve(missing, &INSTALLED);
            assert_eq!(drawn, TerminalFont::EMBEDDED, "{missing:?}");
        }
        assert!(!TerminalFont::resolve("Fira Code", &INSTALLED).is("Fira Code"));
        assert_eq!(
            TerminalFont::resolve("Consolas", &[]),
            TerminalFont::EMBEDDED
        );
    }

    #[test]
    fn the_families_offered_are_those_installed_the_embedded_one_first_and_once() {
        assert_eq!(
            available(&INSTALLED),
            [FONT_FAMILY, "Cascadia Mono", "Consolas"]
        );
        assert_eq!(available(&[]), [FONT_FAMILY]);
        let with_embedded = [TerminalFont::EMBEDDED, INSTALLED[1]];
        assert_eq!(available(&with_embedded), [FONT_FAMILY, "Consolas"]);
    }
}
