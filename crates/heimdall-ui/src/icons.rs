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

//! The C# Heimdall's vector icons, from its `Themes/IconGeometries.xaml`, drawn on a canvas:
//! each is its path data, fitted to the size asked as WPF's `Stretch="Uniform"` fits it, and
//! filled or stroked in a colour the theme gives.
//!
//! The data is WPF's path mini-language: SVG path data (M, L, H, V, C, S, Q, A, Z and their
//! relative forms here), with an optional fill rule first, `F0` even-odd, WPF's default, or
//! `F1` non-zero. The C#'s geometries use M, L, C, A and Z only.
//!
//! The C# draws its window's chrome, a toolbar's or the status bar's buttons, in Segoe MDL2
//! Assets glyphs rather than geometries; those are drawn here as lines on the same 16-unit
//! grid, each named after the glyph it stands for.

use std::cell::Cell;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use heimdall_app::ProfileKind;
use iced::widget::canvas::{self, Cache, Fill, Frame, Geometry, Path, Stroke, fill::Rule};
use iced::{Color, Element, Point, Rectangle, Renderer, Size, Theme, mouse};

/// Width of a stroked icon's line, as the C# tree expander's arrow.
const STROKE_WIDTH: f32 = 1.5;

/// Width of a glyph's line, as Segoe MDL2 Assets draws its glyphs at the C#'s sizes.
const GLYPH_STROKE_WIDTH: f32 = 1.0;

/// Side of a square icon button, as the C#'s 32 by 32 toolbar buttons.
pub const BUTTON_SIDE: f32 = 32.0;

/// Side of the icon in it, as the C#'s glyphs at `FontSizeBody` and `GeoIconSmall`.
pub const GLYPH_SIDE: f32 = 14.0;

/// Points taken on a curve to find how far it reaches.
const CURVE_SAMPLES: u8 = 16;

/// Below this, a length is taken for none.
const EPSILON: f32 = 1e-4;

/// An icon of the C#.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// `Geo.Protocol.Rdp`: a monitor with a pointer.
    Rdp,
    /// `Geo.Protocol.Ssh`: a terminal with a padlock.
    Ssh,
    /// `Geo.Protocol.WinRm`: a window with a prompt and a circle.
    WinRm,
    /// `Geo.Protocol.Sftp`: a folder with an arrow up.
    Sftp,
    /// `Geo.Protocol.Vnc`: a monitor with an eye.
    Vnc,
    /// `Geo.Protocol.Telnet`: a rounded terminal.
    Telnet,
    /// `Geo.Protocol.Ftp`: a folder with arrows.
    Ftp,
    /// `Geo.Protocol.Citrix`: a window with a cloud.
    Citrix,
    /// `Geo.Protocol.LocalShell`: a window with a prompt.
    LocalShell,
    /// `Geo.Tree.Group`: a folder of the tree.
    Folder,
    /// The tree's expander closed: the arrow of the C# `TreeViewExpanderToggleButtonStyle`.
    ChevronRight,
    /// The expander open: the same arrow turned down.
    ChevronDown,
    /// `Geo.Status.QuickConnect`, outlined as the status bar's `LightningBolt` glyph: the
    /// tunnels.
    Lightning,
    /// The `Filter` glyph: a funnel.
    Filter,
    /// The `Add` glyph: a plus.
    Add,
    /// The `More` glyph: three dots.
    More,
    /// The `ClosePane` glyph: a pane put away, the sidebar hidden.
    ClosePane,
    /// The `ViewAll` glyph: a square in four, a session's view split.
    Split,
    /// The `Cancel` glyph: a cross, a tab closed.
    Close,
    /// The `Pinned` glyph: a pushpin.
    Pin,
    /// The status bar's broadcast glyph: a point sending waves both ways.
    Broadcast,
    /// The `AllApps` glyph: a list, broadcast input's scope.
    List,
    /// The `Back` glyph: an arrow left, a file browser's Back.
    Back,
    /// The `Up` glyph: an arrow up, a file browser's parent folder.
    Up,
    /// The `Home` glyph: a house, a file browser's home folder.
    Home,
    /// The `Refresh` glyph: a turning arrow, a folder listed again.
    Refresh,
    /// The `Folder` glyph: a folder of a file list.
    FolderGlyph,
    /// The `Page` glyph: a file of a file list.
    Page,
    /// The `Link` glyph: a link of a file list.
    Link,
    /// The `Info` glyph: a pipe, a socket or a device of a file list.
    Info,
    /// The `NewFolder` glyph: a folder with a plus.
    NewFolder,
    /// The `FavoriteStar` glyph: a star, the bookmarks.
    FavoriteStar,
    /// The `FavoriteStarFill` glyph: the star filled, a session marked as a favourite.
    FavoriteStarFill,
    /// The `Admin` glyph: a shield, browsing as root.
    Admin,
    /// The `UpdateRestore` glyph: an arrow turning back, a setting reset.
    Restore,
    /// The `Warning` glyph: a triangle with an exclamation mark, a question that warns.
    Warning,
    /// The `ErrorBadge` glyph: a circle with a cross, a report of a failure.
    ErrorBadge,
    /// The `CommandPrompt` glyph: a window with a prompt, a script of a file list.
    CommandPrompt,
    /// The `Setting` glyph: a gear, a configuration file of a file list.
    Setting,
    /// The `Package` glyph: a box, an archive of a file list.
    Package,
    /// The `Zoom` glyph: a magnifier, an executable of a file list.
    Zoom,
    /// The `Photo2` glyph: a picture, an image of a file list.
    Photo,
}

impl Icon {
    /// Every icon.
    pub const ALL: [Self; 42] = [
        Self::Rdp,
        Self::Ssh,
        Self::WinRm,
        Self::Sftp,
        Self::Vnc,
        Self::Telnet,
        Self::Ftp,
        Self::Citrix,
        Self::LocalShell,
        Self::Folder,
        Self::ChevronRight,
        Self::ChevronDown,
        Self::Lightning,
        Self::Filter,
        Self::Add,
        Self::More,
        Self::ClosePane,
        Self::Split,
        Self::Close,
        Self::Pin,
        Self::Broadcast,
        Self::List,
        Self::Back,
        Self::Up,
        Self::Home,
        Self::Refresh,
        Self::FolderGlyph,
        Self::Page,
        Self::Link,
        Self::Info,
        Self::NewFolder,
        Self::FavoriteStar,
        Self::FavoriteStarFill,
        Self::Admin,
        Self::Restore,
        Self::Warning,
        Self::ErrorBadge,
        Self::CommandPrompt,
        Self::Setting,
        Self::Package,
        Self::Zoom,
        Self::Photo,
    ];

    /// The icon of `kind`, as the C# `ConnectionTypeToGeometryConverter` picks it.
    #[must_use]
    pub const fn of(kind: ProfileKind) -> Self {
        match kind {
            ProfileKind::Rdp => Self::Rdp,
            ProfileKind::Ssh => Self::Ssh,
            ProfileKind::WinRm => Self::WinRm,
            ProfileKind::Sftp => Self::Sftp,
            ProfileKind::Vnc => Self::Vnc,
            ProfileKind::Telnet => Self::Telnet,
            ProfileKind::Ftp => Self::Ftp,
            ProfileKind::Citrix => Self::Citrix,
            ProfileKind::Local => Self::LocalShell,
        }
    }

    /// Its path data, as the C# holds it.
    #[expect(clippy::too_many_lines, reason = "one path per icon")]
    const fn data(self) -> &'static str {
        match self {
            Self::Rdp => {
                "M1,1 L15,1 L15,11 L1,11 Z M6,11 L10,11 L10,13 L6,13 Z M4,13 L12,13 L12,15 \
                 L4,15 Z M9,4 L12,7 L10.5,7 L10.5,9 L8,9 L8,7 L6.5,7 Z"
            }
            Self::Ssh => {
                "M1,2 L13,2 L13,14 L1,14 Z M3,6 L6,8 L3,10 M7,10 L10,10 M14,5 L14,4 \
                 A1,1 0 1 1 16,4 L16,5 L16,8 L14,8 Z M14.5,6.5 A0.5,0.5 0 1 1 15.5,6.5 \
                 A0.5,0.5 0 1 1 14.5,6.5 Z"
            }
            Self::WinRm => {
                "M1,2 L15,2 L15,14 L1,14 Z M1,4 L15,4 M4,7 L7,9.5 L4,12 M8,12 L11,12 \
                 M11.5,7 A2.5,2.5 0 1 1 11.5,12 A2.5,2.5 0 1 1 11.5,7 Z"
            }
            Self::Sftp => "M1,3 L6,3 L7,5 L15,5 L15,14 L1,14 Z M8,7 L8,12 M5.5,9.5 L8,7 L10.5,9.5",
            Self::Vnc => {
                "M1,1 L15,1 L15,11 L1,11 Z M6,11 L10,11 L10,13 L6,13 Z M4,13 L12,13 L12,15 \
                 L4,15 Z M4,6 C4,3 12,3 12,6 C12,9 4,9 4,6 Z M8,4.5 A1.5,1.5 0 1 1 8,7.5 \
                 A1.5,1.5 0 1 1 8,4.5 Z"
            }
            Self::Telnet => {
                "M2,1 A1,1 0 0 0 1,2 L1,12 A1,1 0 0 0 2,13 L14,13 A1,1 0 0 0 15,12 L15,2 \
                 A1,1 0 0 0 14,1 Z M3,3 L13,3 L13,11 L3,11 Z M5,6 L7.5,7.5 L5,9 M8,9 L11,9"
            }
            Self::Ftp => "M1,3 L6,3 L7,5 L15,5 L15,14 L1,14 Z M6,7 L8,9 L10,7 M6,12 L8,10 L10,12",
            Self::Citrix => {
                "M1,2 L15,2 L15,14 L1,14 Z M1,4 L15,4 M5,11 A2,2 0 0 1 5,7 \
                 A2.5,2.5 0 0 1 10,6.5 A2,2 0 0 1 13,8 A1.5,1.5 0 0 1 12,11 Z"
            }
            Self::LocalShell => {
                "M1,2 L15,2 L15,14 L1,14 Z M1,4 L15,4 M4,7 L7,9.5 L4,12 M8,12 L12,12"
            }
            Self::Folder => {
                "M3,1 L7,1 L8,3 L14,3 L14,9 L3,9 Z M1,5 L3,5 M1,5 L1,13 L12,13 L12,10 \
                 M3,9 L3,13"
            }
            Self::ChevronRight => "M 0 0 L 6 6 L 0 12",
            Self::ChevronDown => "M 0 0 L 6 6 L 12 0",
            Self::Lightning => "M9,1 L4,9 L7.5,9 L6,15 L12,7 L8.5,7 Z",
            Self::Filter => "M1.5,2.5 L14.5,2.5 L9.5,8.5 L9.5,14 L6.5,12.5 L6.5,8.5 Z",
            Self::Add => "M8,1.5 L8,14.5 M1.5,8 L14.5,8",
            Self::More => {
                "M1,8 A1,1 0 1 1 3,8 A1,1 0 1 1 1,8 Z M7,8 A1,1 0 1 1 9,8 A1,1 0 1 1 7,8 Z \
                 M13,8 A1,1 0 1 1 15,8 A1,1 0 1 1 13,8 Z"
            }
            Self::ClosePane => {
                "M1.5,3.5 L14.5,3.5 L14.5,12.5 L1.5,12.5 Z M4,8 L10,8 M8,6 L10,8 L8,10 \
                 M12,5.5 L12,10.5"
            }
            Self::Split => {
                "M2.5,2.5 L13.5,2.5 L13.5,13.5 L2.5,13.5 Z M8,2.5 L8,13.5 M2.5,8 L13.5,8"
            }
            Self::Close => "M3,3 L13,13 M13,3 L3,13",
            Self::Pin => "M5.5,1.5 L10.5,1.5 M7,1.5 L7,6 L4.5,9 L11.5,9 L9,6 L9,1.5 M8,9 L8,14.5",
            Self::Broadcast => {
                "M7,7 A1,1 0 1 1 9,7 A1,1 0 1 1 7,7 Z M8,8 L8,14 M5,4.5 A3.5,3.5 0 0 0 5,9.5 \
                 M11,4.5 A3.5,3.5 0 0 1 11,9.5 M3,2.5 A6,6 0 0 0 3,11.5 \
                 M13,2.5 A6,6 0 0 1 13,11.5"
            }
            Self::List => {
                "M2,3.5 L3,3.5 M5,3.5 L14,3.5 M2,8 L3,8 M5,8 L14,8 M2,12.5 L3,12.5 \
                 M5,12.5 L14,12.5"
            }
            Self::Back => "M14.5,8 L1.5,8 M7,2.5 L1.5,8 L7,13.5",
            Self::Up => "M8,14.5 L8,1.5 M2.5,7 L8,1.5 L13.5,7",
            Self::Home => {
                "M1.5,8 L8,1.5 L14.5,8 M3.5,6 L3.5,14.5 L12.5,14.5 L12.5,6 \
                 M6.5,14.5 L6.5,10 L9.5,10 L9.5,14.5"
            }
            Self::Refresh => "M13.5,8 A5.5,5.5 0 1 1 11.9,4.1 M12,1 L12,4.5 L8.5,4.5",
            Self::FolderGlyph => "M1.5,3.5 L6,3.5 L7.5,5 L14.5,5 L14.5,13 L1.5,13 Z",
            Self::Page => "M3.5,1.5 L10,1.5 L12.5,4 L12.5,14.5 L3.5,14.5 Z M10,1.5 L10,4 L12.5,4",
            Self::Link => {
                "M9.5,6.5 L6.5,9.5 M8,4.5 L9.5,3 A2.5,2.5 0 0 1 13,6.5 L11.5,8 \
                 M8,11.5 L6.5,13 A2.5,2.5 0 0 1 3,9.5 L4.5,8"
            }
            Self::Info => {
                "M8,1.5 A6.5,6.5 0 1 1 8,14.5 A6.5,6.5 0 1 1 8,1.5 Z M8,7 L8,11.5 \
                 M8,4.5 L8,5.5"
            }
            Self::NewFolder => {
                "M1.5,3.5 L6,3.5 L7.5,5 L14.5,5 L14.5,13 L1.5,13 Z M8,7 L8,11 M6,9 L10,9"
            }
            Self::FavoriteStar | Self::FavoriteStarFill => {
                "M8,1.5 L9.65,5.73 L14.18,5.99 L10.66,8.87 L11.82,13.26 L8,10.8 L4.18,13.26 \
                 L5.34,8.87 L1.82,5.99 L6.35,5.73 Z"
            }
            Self::Admin => {
                "M8,1.5 L13.5,3.5 L13.5,8 C13.5,11 11,13.5 8,14.5 C5,13.5 2.5,11 2.5,8 \
                 L2.5,3.5 Z"
            }
            Self::Restore => "M2.5,8 A5.5,5.5 0 1 0 4.1,4.1 M4,1 L4,4.5 L7.5,4.5",
            Self::Warning => "M8,1.5 L14.5,13.5 L1.5,13.5 Z M8,5.5 L8,9.5 M8,11 L8,12",
            Self::ErrorBadge => {
                "M8,1.5 A6.5,6.5 0 1 1 8,14.5 A6.5,6.5 0 1 1 8,1.5 Z M5.5,5.5 L10.5,10.5 \
                 M10.5,5.5 L5.5,10.5"
            }
            Self::CommandPrompt => {
                "M1.5,2.5 L14.5,2.5 L14.5,13.5 L1.5,13.5 Z M1.5,4.5 L14.5,4.5 \
                 M4,7 L6.5,9 L4,11 M8,11 L11.5,11"
            }
            Self::Setting => {
                "M3,8 A5,5 0 1 1 13,8 A5,5 0 1 1 3,8 Z M6,8 A2,2 0 1 1 10,8 A2,2 0 1 1 6,8 Z \
                 M8,1.5 L8,3 M8,13 L8,14.5 M1.5,8 L3,8 M13,8 L14.5,8 \
                 M3.4,3.4 L4.46,4.46 M11.54,11.54 L12.6,12.6 \
                 M12.6,3.4 L11.54,4.46 M4.46,11.54 L3.4,12.6"
            }
            Self::Package => {
                "M1.5,4.5 L8,1.5 L14.5,4.5 L14.5,11.5 L8,14.5 L1.5,11.5 Z \
                 M1.5,4.5 L8,7.5 L14.5,4.5 M8,7.5 L8,14.5"
            }
            Self::Zoom => "M2,6.5 A4.5,4.5 0 1 1 11,6.5 A4.5,4.5 0 1 1 2,6.5 Z M9.7,9.7 L14.5,14.5",
            Self::Photo => {
                "M1.5,2.5 L14.5,2.5 L14.5,13.5 L1.5,13.5 Z M1.5,11 L5.5,7 L9,10.5 L11,8.5 \
                 L14.5,12 M10,5 A1,1 0 1 1 12,5 A1,1 0 1 1 10,5 Z"
            }
        }
    }

    /// Width of its line when it is stroked, as the expander's arrow and the chrome's
    /// glyphs; `None` when it is filled, as every geometry of the C#, the dots of More and
    /// the favourite's star.
    const fn stroke(self) -> Option<f32> {
        match self {
            Self::ChevronRight | Self::ChevronDown => Some(STROKE_WIDTH),
            Self::Lightning
            | Self::Filter
            | Self::Add
            | Self::ClosePane
            | Self::Split
            | Self::Close
            | Self::Pin
            | Self::Broadcast
            | Self::List
            | Self::Back
            | Self::Up
            | Self::Home
            | Self::Refresh
            | Self::FolderGlyph
            | Self::Page
            | Self::Link
            | Self::Info
            | Self::NewFolder
            | Self::FavoriteStar
            | Self::Admin
            | Self::Restore
            | Self::Warning
            | Self::ErrorBadge
            | Self::CommandPrompt
            | Self::Setting
            | Self::Package
            | Self::Zoom
            | Self::Photo => Some(GLYPH_STROKE_WIDTH),
            _ => None,
        }
    }
}

/// Which colour of the theme an icon takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tint {
    /// A protocol's, as the C# `Protocol*Brush`.
    Protocol(ProfileKind),
    /// The C# `InfoBrush`: a folder without a colour of its own.
    Info,
    /// The secondary text: a closed expander's arrow.
    Secondary,
    /// The text: an open expander's arrow, a button's glyph.
    Text,
    /// The C# `ErrorTextBrush`: broadcast input's glyph while it is on.
    Danger,
    /// The C# `AccentBrush`: a pinned tab's pin.
    Accent,
    /// The C# `WarningBrush`: a question that warns.
    Warning,
    /// A colour of the theme's palette: a file's, by what it holds.
    Hue(Hue),
    /// A colour of its own: a folder's.
    Own(Color),
}

/// A colour of the theme's palette, as the C# `*Color` keys a brush is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hue {
    /// `GreenColor`.
    Green,
    /// `CyanColor`.
    Cyan,
    /// `CommentColor`.
    Comment,
    /// `OrangeColor`.
    Orange,
    /// `PinkColor`.
    Pink,
    /// `YellowColor`.
    Yellow,
}

impl Hue {
    /// The colour it is in `theme`.
    fn color(self, theme: &Theme) -> Color {
        let colors = crate::themes::colors_of(theme);
        match self {
            Self::Green => colors.green,
            Self::Cyan => colors.cyan,
            Self::Comment => colors.comment,
            Self::Orange => colors.orange,
            Self::Pink => colors.pink,
            Self::Yellow => colors.yellow,
        }
    }
}

impl Tint {
    /// The colour it is in `theme`.
    fn color(self, theme: &Theme) -> Color {
        match self {
            Self::Protocol(kind) => protocol_color(theme, kind),
            Self::Info => crate::themes::colors_of(theme).cyan,
            Self::Secondary => theme.extended_palette().secondary.base.color,
            Self::Text => theme.palette().text,
            Self::Danger => theme.extended_palette().danger.base.color,
            Self::Accent => theme.palette().primary,
            Self::Warning => theme.extended_palette().warning.base.color,
            Self::Hue(hue) => hue.color(theme),
            Self::Own(color) => color,
        }
    }
}

/// The colour of `kind` in `theme`, as the C# `HeimdallThemeBridge.xaml` gives each
/// `Protocol*Brush`: RDP and FTP blue, SSH green, `WinRM` and Citrix purple, SFTP orange, VNC
/// cyan, Telnet the comment colour, Local the accent.
#[must_use]
pub fn protocol_color(theme: &Theme, kind: ProfileKind) -> Color {
    let colors = crate::themes::colors_of(theme);
    match kind {
        ProfileKind::Rdp | ProfileKind::Ftp => colors.blue,
        ProfileKind::Ssh => colors.green,
        ProfileKind::WinRm | ProfileKind::Citrix => colors.purple,
        ProfileKind::Sftp => colors.orange,
        ProfileKind::Vnc => colors.cyan,
        ProfileKind::Telnet => colors.comment,
        ProfileKind::Local => theme.palette().primary,
    }
}

/// `icon` in a square of `side`, in the colour `tint` gives it.
#[must_use]
pub fn icon<'a, Message: 'a>(icon: Icon, tint: Tint, side: f32) -> Element<'a, Message> {
    faded(icon, tint, side, 1.0)
}

/// [`icon`] at `opacity`, as a C# glyph whose button is faded.
#[must_use]
pub fn faded<'a, Message: 'a>(
    icon: Icon,
    tint: Tint,
    side: f32,
    opacity: f32,
) -> Element<'a, Message> {
    iced::widget::canvas(Drawing {
        icon,
        tint,
        opacity,
    })
    .width(side)
    .height(side)
    .into()
}

/// A square button holding `icon` in the text's colour, as the C#'s 32 by 32 toolbar
/// buttons; its style and action are its caller's.
#[must_use]
pub fn button<'a, Message: 'a>(icon: Icon) -> iced::widget::Button<'a, Message> {
    iced::widget::button(iced::widget::center(self::icon(
        icon,
        Tint::Text,
        GLYPH_SIDE,
    )))
    .width(BUTTON_SIDE)
    .height(BUTTON_SIDE)
    .padding(0.0)
}

/// What a canvas draws: an icon in a colour, at an opacity.
#[derive(Debug, Clone, Copy)]
struct Drawing {
    icon: Icon,
    tint: Tint,
    opacity: f32,
}

/// What a canvas keeps between frames: the icon drawn, and which icon in which colour, so
/// that a row given another icon or a theme changed draws it again.
#[derive(Default)]
struct Drawn {
    cache: Cache,
    key: Cell<Option<(Icon, Color)>>,
}

impl<Message> canvas::Program<Message> for Drawing {
    type State = Drawn;

    fn draw(
        &self,
        drawn: &Drawn,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let color = self.tint.color(theme).scale_alpha(self.opacity);
        let key = Some((self.icon, color));
        if drawn.key.get() != key {
            drawn.cache.clear();
            drawn.key.set(key);
        }
        // The cache draws again on its own when the size changes.
        vec![drawn.cache.draw(renderer, bounds.size(), |frame| {
            paint(frame, self.icon, color);
        })]
    }
}

/// Draws `icon` on `frame`, fitted to it, in `color`.
fn paint(frame: &mut Frame, icon: Icon, color: Color) {
    // Every icon's data parses: a test says so.
    let Ok(outline) = parse(icon.data()) else {
        return;
    };
    let inset = icon.stroke().map_or(0.0, |width| width / 2.0);
    let Some(fit) = Fit::new(outline.bounds(), frame.size(), inset) else {
        return;
    };
    let path = outline.path(&fit);
    match icon.stroke() {
        Some(width) => frame.stroke(&path, Stroke::default().with_color(color).with_width(width)),
        None => frame.fill(
            &path,
            Fill {
                style: canvas::Style::Solid(color),
                rule: outline.rule,
            },
        ),
    }
}

/// How path data is moved and scaled onto a canvas: uniformly, centred.
#[derive(Debug, Clone, Copy)]
struct Fit {
    scale: f32,
    offset: Point,
}

impl Fit {
    /// The fit of `bounds` into `size`, `inset` kept free on each side; `None` for data
    /// that covers no length.
    fn new(bounds: Rectangle, size: Size, inset: f32) -> Option<Self> {
        let room = Size::new(size.width - 2.0 * inset, size.height - 2.0 * inset);
        let across = (bounds.width > EPSILON).then(|| room.width / bounds.width);
        let down = (bounds.height > EPSILON).then(|| room.height / bounds.height);
        let scale = match (across, down) {
            (Some(across), Some(down)) => across.min(down),
            (Some(one), None) | (None, Some(one)) => one,
            (None, None) => return None,
        };
        Some(Self {
            scale,
            offset: Point::new(
                (size.width - bounds.width * scale) / 2.0 - bounds.x * scale,
                (size.height - bounds.height * scale) / 2.0 - bounds.y * scale,
            ),
        })
    }

    /// Where `point` of the data lands.
    fn map(self, point: Point) -> Point {
        Point::new(
            self.offset.x + point.x * self.scale,
            self.offset.y + point.y * self.scale,
        )
    }
}

/// A piece of an outline, in the data's coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Segment {
    /// A new figure starts here.
    Move(Point),
    /// A straight line to here.
    Line(Point),
    /// A cubic curve through two control points.
    Cubic(Point, Point, Point),
    /// A quadratic curve through one control point.
    Quadratic(Point, Point),
    /// The figure closes.
    Close,
}

/// Path data read: its figures as lines and curves, and how it is filled.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    rule: Rule,
    segments: Vec<Segment>,
}

/// Path data that does not read, and where it stops reading, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError {
    /// Where the data stops reading.
    pub at: usize,
}

impl Outline {
    /// How far it reaches, curves included.
    fn bounds(&self) -> Rectangle {
        let mut points = Vec::new();
        let mut from = Point::ORIGIN;
        for segment in &self.segments {
            match *segment {
                Segment::Move(to) | Segment::Line(to) => {
                    points.push(to);
                    from = to;
                }
                Segment::Cubic(first, second, to) => {
                    points.extend((1..=CURVE_SAMPLES).map(|step| {
                        cubic_at(
                            from,
                            first,
                            second,
                            to,
                            f32::from(step) / f32::from(CURVE_SAMPLES),
                        )
                    }));
                    from = to;
                }
                Segment::Quadratic(control, to) => {
                    points.extend((1..=CURVE_SAMPLES).map(|step| {
                        quadratic_at(
                            from,
                            control,
                            to,
                            f32::from(step) / f32::from(CURVE_SAMPLES),
                        )
                    }));
                    from = to;
                }
                Segment::Close => {}
            }
        }
        let Some(first) = points.first().copied() else {
            return Rectangle::default();
        };
        let (min, max) = points.iter().fold((first, first), |(min, max), point| {
            (
                Point::new(min.x.min(point.x), min.y.min(point.y)),
                Point::new(max.x.max(point.x), max.y.max(point.y)),
            )
        });
        Rectangle::new(min, Size::new(max.x - min.x, max.y - min.y))
    }

    /// Its figures as a path of the canvas, moved by `fit`.
    fn path(&self, fit: &Fit) -> Path {
        Path::new(|builder| {
            for segment in &self.segments {
                match *segment {
                    Segment::Move(to) => builder.move_to(fit.map(to)),
                    Segment::Line(to) => builder.line_to(fit.map(to)),
                    Segment::Cubic(first, second, to) => {
                        builder.bezier_curve_to(fit.map(first), fit.map(second), fit.map(to));
                    }
                    Segment::Quadratic(control, to) => {
                        builder.quadratic_curve_to(fit.map(control), fit.map(to));
                    }
                    Segment::Close => builder.close(),
                }
            }
        })
    }
}

/// The point at `along` (0 to 1) of a cubic curve.
fn cubic_at(from: Point, first: Point, second: Point, to: Point, along: f32) -> Point {
    let rest = 1.0 - along;
    let weights = [
        rest * rest * rest,
        3.0 * rest * rest * along,
        3.0 * rest * along * along,
        along * along * along,
    ];
    weighted(&[from, first, second, to], &weights)
}

/// The point at `along` (0 to 1) of a quadratic curve.
fn quadratic_at(from: Point, control: Point, to: Point, along: f32) -> Point {
    let rest = 1.0 - along;
    let weights = [rest * rest, 2.0 * rest * along, along * along];
    weighted(&[from, control, to], &weights)
}

/// The sum of `points`, each times its weight.
fn weighted(points: &[Point], weights: &[f32]) -> Point {
    points
        .iter()
        .zip(weights)
        .fold(Point::ORIGIN, |sum, (point, weight)| {
            Point::new(sum.x + point.x * weight, sum.y + point.y * weight)
        })
}

/// Reads path data.
///
/// # Errors
///
/// When it is not path data: a command unknown, a number missing, a figure that does not
/// start with M.
pub fn parse(data: &str) -> Result<Outline, ParseError> {
    let mut reader = Reader { data, at: 0 };
    let rule = reader.fill_rule()?;
    let mut pen = Pen::default();
    let mut command = None;
    loop {
        reader.skip_separators();
        if reader.done() {
            break;
        }
        if let Some(letter) = reader.letter() {
            command = Some(letter);
        }
        // Numbers with no letter before them repeat the last command.
        let letter = command.ok_or(ParseError { at: reader.at })?;
        pen.draw(letter, &mut reader)?;
        command = match letter {
            // Points after a move are lines, as SVG reads them.
            'M' => Some('L'),
            'm' => Some('l'),
            'Z' | 'z' => None,
            other => Some(other),
        };
    }
    Ok(Outline {
        rule,
        segments: pen.segments,
    })
}

/// Reads path data, a token at a time.
struct Reader<'a> {
    data: &'a str,
    at: usize,
}

impl Reader<'_> {
    fn rest(&self) -> &str {
        self.data.get(self.at..).unwrap_or_default()
    }

    fn done(&self) -> bool {
        self.rest().is_empty()
    }

    fn skip_separators(&mut self) {
        let rest = self.rest();
        let kept = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        self.at += rest.len() - kept.len();
    }

    /// WPF's fill rule, before the first figure: `F0` even-odd, `F1` non-zero; even-odd
    /// when it is not said.
    fn fill_rule(&mut self) -> Result<Rule, ParseError> {
        self.skip_separators();
        if !self.rest().starts_with('F') {
            return Ok(Rule::EvenOdd);
        }
        self.at += 1;
        self.skip_separators();
        Ok(if self.flag()? {
            Rule::NonZero
        } else {
            Rule::EvenOdd
        })
    }

    /// The command letter next, when one is.
    fn letter(&mut self) -> Option<char> {
        let letter = self
            .rest()
            .chars()
            .next()
            .filter(char::is_ascii_alphabetic)?;
        self.at += 1;
        Some(letter)
    }

    /// The number next.
    fn number(&mut self) -> Result<f32, ParseError> {
        self.skip_separators();
        let rest = self.rest();
        let bytes = rest.as_bytes();
        let mut end = 0;
        if matches!(bytes.first(), Some(b'+' | b'-')) {
            end += 1;
        }
        let digits = |from: usize| {
            bytes.get(from..).map_or(0, |tail| {
                tail.iter().take_while(|b| b.is_ascii_digit()).count()
            })
        };
        end += digits(end);
        if bytes.get(end) == Some(&b'.') {
            end += 1 + digits(end + 1);
        }
        if matches!(bytes.get(end), Some(b'e' | b'E')) {
            let mut exponent = end + 1;
            if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
                exponent += 1;
            }
            let count = digits(exponent);
            if count > 0 {
                end = exponent + count;
            }
        }
        let number = rest
            .get(..end)
            .and_then(|text| text.parse().ok())
            .ok_or(ParseError { at: self.at })?;
        self.at += end;
        Ok(number)
    }

    /// A point next: two numbers.
    fn point(&mut self) -> Result<Point, ParseError> {
        Ok(Point::new(self.number()?, self.number()?))
    }

    /// An arc's flag next: one digit, 0 or 1, which SVG lets touch what follows.
    fn flag(&mut self) -> Result<bool, ParseError> {
        self.skip_separators();
        let flag = match self.rest().as_bytes().first() {
            Some(b'0') => false,
            Some(b'1') => true,
            _ => return Err(ParseError { at: self.at }),
        };
        self.at += 1;
        Ok(flag)
    }
}

/// Where the drawing is while data is read, and what it drew.
#[derive(Default)]
struct Pen {
    /// The point reached.
    at: Point,
    /// Where the figure started, where a close goes back to.
    start: Point,
    /// The second control point of the last cubic curve, which S mirrors.
    control: Option<Point>,
    segments: Vec<Segment>,
}

impl Pen {
    /// Draws command `letter`, its numbers read from `reader`.
    fn draw(&mut self, letter: char, reader: &mut Reader<'_>) -> Result<(), ParseError> {
        let relative = letter.is_ascii_lowercase();
        let origin = if relative { self.at } else { Point::ORIGIN };
        let shift = |point: Point| Point::new(origin.x + point.x, origin.y + point.y);
        let mut control = None;
        match letter.to_ascii_uppercase() {
            'M' => {
                let to = shift(reader.point()?);
                self.segments.push(Segment::Move(to));
                self.start = to;
                self.at = to;
            }
            'Z' => {
                self.figure(reader)?;
                self.segments.push(Segment::Close);
                self.at = self.start;
            }
            'L' => {
                let to = shift(reader.point()?);
                self.line(reader, to)?;
            }
            'H' => {
                let x = reader.number()? + origin.x;
                self.line(reader, Point::new(x, self.at.y))?;
            }
            'V' => {
                let y = reader.number()? + origin.y;
                self.line(reader, Point::new(self.at.x, y))?;
            }
            'C' | 'S' => control = Some(self.cubic(letter, reader, origin)?),
            'Q' => {
                let (bend, to) = (shift(reader.point()?), shift(reader.point()?));
                self.figure(reader)?;
                self.segments.push(Segment::Quadratic(bend, to));
                self.at = to;
            }
            'A' => self.arc(reader, origin)?,
            _ => return Err(ParseError { at: reader.at - 1 }),
        }
        self.control = control;
        Ok(())
    }

    /// Starts a figure where the pen is when a close ended the last one; data that draws
    /// before any move does not read.
    fn figure(&mut self, reader: &Reader<'_>) -> Result<(), ParseError> {
        match self.segments.last() {
            None => Err(ParseError { at: reader.at }),
            Some(Segment::Close) => {
                self.segments.push(Segment::Move(self.at));
                Ok(())
            }
            Some(_) => Ok(()),
        }
    }

    fn line(&mut self, reader: &Reader<'_>, to: Point) -> Result<(), ParseError> {
        self.figure(reader)?;
        self.segments.push(Segment::Line(to));
        self.at = to;
        Ok(())
    }

    /// A cubic curve, C or S: S's first control point mirrors the last curve's second.
    /// Its second control point, which the next S mirrors.
    fn cubic(
        &mut self,
        letter: char,
        reader: &mut Reader<'_>,
        origin: Point,
    ) -> Result<Point, ParseError> {
        let shift = |point: Point| Point::new(origin.x + point.x, origin.y + point.y);
        let first = if letter.eq_ignore_ascii_case(&'S') {
            self.control.map_or(self.at, |last| {
                Point::new(2.0 * self.at.x - last.x, 2.0 * self.at.y - last.y)
            })
        } else {
            shift(reader.point()?)
        };
        let (second, to) = (shift(reader.point()?), shift(reader.point()?));
        self.figure(reader)?;
        self.segments.push(Segment::Cubic(first, second, to));
        self.at = to;
        Ok(second)
    }

    /// An elliptical arc, as cubic curves.
    fn arc(&mut self, reader: &mut Reader<'_>, origin: Point) -> Result<(), ParseError> {
        let radii = (reader.number()?, reader.number()?);
        let rotation = reader.number()?;
        let (large, sweep) = (reader.flag()?, reader.flag()?);
        let to = reader.point()?;
        let to = Point::new(origin.x + to.x, origin.y + to.y);
        self.figure(reader)?;
        arc_segments(
            &Arc {
                from: self.at,
                radii,
                rotation,
                large,
                sweep,
                to,
            },
            &mut self.segments,
        );
        self.at = to;
        Ok(())
    }
}

/// An arc as SVG writes it: from a point to another along an ellipse.
struct Arc {
    from: Point,
    radii: (f32, f32),
    /// The ellipse's turn, in degrees.
    rotation: f32,
    /// The longer of the two arcs.
    large: bool,
    /// The arc going clockwise.
    sweep: bool,
    to: Point,
}

/// `arc` as cubic curves pushed to `segments`, a quarter turn at most each, as SVG's
/// appendix on arcs works out its centre.
fn arc_segments(arc: &Arc, segments: &mut Vec<Segment>) {
    let (from, to) = (arc.from, arc.to);
    if (from.x - to.x).abs() < EPSILON && (from.y - to.y).abs() < EPSILON {
        return;
    }
    let (mut rx, mut ry) = (arc.radii.0.abs(), arc.radii.1.abs());
    if rx < EPSILON || ry < EPSILON {
        segments.push(Segment::Line(to));
        return;
    }
    let (sin, cos) = arc.rotation.to_radians().sin_cos();
    let (half_x, half_y) = ((from.x - to.x) / 2.0, (from.y - to.y) / 2.0);
    let x1 = cos * half_x + sin * half_y;
    let y1 = -sin * half_x + cos * half_y;
    // Radii too short to join the two points grow until they do.
    let reach = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if reach > 1.0 {
        rx *= reach.sqrt();
        ry *= reach.sqrt();
    }
    let numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let branch = if arc.large == arc.sweep { -1.0 } else { 1.0 };
    let factor = branch * (numerator / denominator).max(0.0).sqrt();
    let (cx1, cy1) = (factor * rx * y1 / ry, -factor * ry * x1 / rx);
    let centre = Point::new(
        cos * cx1 - sin * cy1 + f32::midpoint(from.x, to.x),
        sin * cx1 + cos * cy1 + f32::midpoint(from.y, to.y),
    );
    let start = angle((1.0, 0.0), ((x1 - cx1) / rx, (y1 - cy1) / ry));
    let mut turn = angle(
        ((x1 - cx1) / rx, (y1 - cy1) / ry),
        ((-x1 - cx1) / rx, (-y1 - cy1) / ry),
    );
    if !arc.sweep && turn > 0.0 {
        turn -= TAU;
    } else if arc.sweep && turn < 0.0 {
        turn += TAU;
    }
    let pieces: u8 = match turn.abs() {
        swept if swept <= FRAC_PI_2 + EPSILON => 1,
        swept if swept <= PI + EPSILON => 2,
        swept if swept <= 3.0 * FRAC_PI_2 + EPSILON => 3,
        _ => 4,
    };
    let step = turn / f32::from(pieces);
    let handle = 4.0 / 3.0 * (step / 4.0).tan();
    let on_ellipse = |across: f32, down: f32| {
        Point::new(
            centre.x + rx * cos * across - ry * sin * down,
            centre.y + rx * sin * across + ry * cos * down,
        )
    };
    for piece in 0..pieces {
        let begin = start + step * f32::from(piece);
        let (sin_begin, cos_begin) = begin.sin_cos();
        let (sin_end, cos_end) = (begin + step).sin_cos();
        let first = on_ellipse(
            cos_begin - handle * sin_begin,
            sin_begin + handle * cos_begin,
        );
        let second = on_ellipse(cos_end + handle * sin_end, sin_end - handle * cos_end);
        let end = if piece + 1 == pieces {
            to
        } else {
            on_ellipse(cos_end, sin_end)
        };
        segments.push(Segment::Cubic(first, second, end));
    }
}

/// The signed angle from `u` to `v`.
fn angle(u: (f32, f32), v: (f32, f32)) -> f32 {
    (u.0 * v.1 - u.1 * v.0).atan2(u.0 * v.0 + u.1 * v.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use heimdall_core::settings::{Accent, AppTheme};

    /// The side of the C#'s drawing box.
    const BOX: f32 = 16.0;

    fn segments(data: &str) -> Vec<Segment> {
        parse(data).expect("parses").segments
    }

    #[test]
    fn every_icon_parses_and_stays_in_the_csharp_box() {
        for icon in Icon::ALL {
            let outline = parse(icon.data()).unwrap_or_else(|error| panic!("{icon:?}: {error:?}"));
            let bounds = outline.bounds();
            assert!(
                bounds.width > 1.0 && bounds.height > 1.0,
                "{icon:?}: {bounds:?}"
            );
            assert!(
                bounds.x >= -EPSILON
                    && bounds.y >= -EPSILON
                    && bounds.x + bounds.width <= BOX + 0.01
                    && bounds.y + bounds.height <= BOX + 0.01,
                "{icon:?}: {bounds:?}"
            );
            assert_eq!(outline.rule, Rule::EvenOdd, "{icon:?}: WPF's default");
        }
    }

    #[test]
    fn the_favourites_star_is_the_bookmarks_star_filled_as_the_csharp_glyph() {
        assert_eq!(Icon::FavoriteStarFill.data(), Icon::FavoriteStar.data());
        assert!(Icon::FavoriteStarFill.stroke().is_none(), "filled");
        assert!(Icon::FavoriteStar.stroke().is_some(), "outlined");
    }

    #[test]
    fn every_protocol_has_its_own_icon() {
        for kind in ProfileKind::ALL {
            let icon = Icon::of(kind);
            assert!(icon.stroke().is_none(), "{kind:?}: filled, as the C#");
            for other in ProfileKind::ALL.into_iter().filter(|other| *other != kind) {
                assert_ne!(icon, Icon::of(other), "{kind:?} and {other:?}");
            }
        }
    }

    #[test]
    fn relative_and_shorthand_commands_read_as_their_absolute_forms() {
        assert_eq!(
            segments("m1,1 h2 v2 h-2 z"),
            segments("M1,1 L3,1 L3,3 L1,3 Z")
        );
        assert_eq!(
            segments("M1 1 H3 V3 l-2 0 Z"),
            segments("M1,1 L3,1 L3,3 L1,3 Z")
        );
        assert_eq!(
            segments("M0,0 c0,1 1,1 1,0 s1,-1 1,0"),
            segments("M0,0 C0,1 1,1 1,0 C1,-1 2,-1 2,0")
        );
        assert_eq!(segments("M0 0 q1 1 2 0"), segments("M0,0 Q1,1 2,0"));
        assert_eq!(
            segments("M0,0 1,1 2,0"),
            vec![
                Segment::Move(Point::ORIGIN),
                Segment::Line(Point::new(1.0, 1.0)),
                Segment::Line(Point::new(2.0, 0.0)),
            ],
            "points after a move are lines"
        );
        assert_eq!(
            segments("M1,1 L2,1 Z L3,3"),
            vec![
                Segment::Move(Point::new(1.0, 1.0)),
                Segment::Line(Point::new(2.0, 1.0)),
                Segment::Close,
                Segment::Move(Point::new(1.0, 1.0)),
                Segment::Line(Point::new(3.0, 3.0)),
            ],
            "after a close, a figure starts again where the last one did"
        );
        assert_eq!(
            segments("M1e1,-.5"),
            vec![Segment::Move(Point::new(10.0, -0.5))]
        );
    }

    #[test]
    fn an_arc_is_drawn_around_its_centre() {
        // The C#'s circle: two half turns of radius 7 around (8, 8).
        let outline = parse("M8,1 A7,7 0 1 1 8,15 A7,7 0 1 1 8,1 Z").expect("parses");
        let bounds = outline.bounds();
        for (said, value, expected) in [
            ("left", bounds.x, 1.0),
            ("top", bounds.y, 1.0),
            ("width", bounds.width, 14.0),
            ("height", bounds.height, 14.0),
        ] {
            assert!((value - expected).abs() < 0.05, "{said}: {value}");
        }
        // Flags may touch what follows, as SVG lets them.
        assert_eq!(
            segments("M0,0 a1,1 0 0,1 2,0"),
            segments("M0,0 A1 1 0 01 2 0")
        );
        // A radius of nothing is a line.
        assert_eq!(segments("M0,0 A0,1 0 0 1 2,0"), segments("M0,0 L2,0"));
    }

    #[test]
    fn the_wpf_fill_rule_comes_first() {
        assert_eq!(
            parse("F1 M0,0 L1,0 L1,1 Z").expect("F1").rule,
            Rule::NonZero
        );
        assert_eq!(
            parse("F0 M0,0 L1,0 L1,1 Z").expect("F0").rule,
            Rule::EvenOdd
        );
        assert_eq!(parse("M0,0 L1,0 L1,1 Z").expect("none").rule, Rule::EvenOdd);
    }

    #[test]
    fn what_is_not_path_data_does_not_read() {
        assert_eq!(parse("L1,1"), Err(ParseError { at: 4 }), "no move first");
        assert_eq!(
            parse("M0,0 X1,1"),
            Err(ParseError { at: 5 }),
            "no such command"
        );
        assert_eq!(
            parse("M0,0 L1"),
            Err(ParseError { at: 7 }),
            "a number missing"
        );
        assert_eq!(
            parse("M0,0 A1,1 0 2 1 2,0").map(|_| ()),
            Err(ParseError { at: 12 })
        );
        assert_eq!(
            parse("1,1").map(|_| ()),
            Err(ParseError { at: 0 }),
            "no command"
        );
    }

    #[test]
    fn an_icon_fits_its_square_centred() {
        let outline = parse(Icon::Rdp.data()).expect("parses");
        let fit = Fit::new(outline.bounds(), Size::new(14.0, 14.0), 0.0).expect("fits");
        assert!((fit.scale - 1.0).abs() < EPSILON, "14 units into 14 pixels");
        assert_eq!(fit.map(Point::new(1.0, 1.0)), Point::ORIGIN);
        let wide = Fit::new(
            Rectangle::new(Point::ORIGIN, Size::new(12.0, 6.0)),
            Size::new(12.0, 12.0),
            0.0,
        )
        .expect("fits");
        assert_eq!(
            wide.map(Point::ORIGIN),
            Point::new(0.0, 3.0),
            "centred down"
        );
        assert!(
            Fit::new(Rectangle::default(), Size::new(12.0, 12.0), 0.0).is_none(),
            "nothing to fit"
        );
    }

    #[test]
    fn each_protocol_takes_the_csharp_colour_of_the_theme() {
        let theme = crate::themes::theme(AppTheme::Magellan, Accent::Blue);
        let colors = crate::themes::colors(AppTheme::Magellan);
        for (kind, expected) in [
            (ProfileKind::Rdp, colors.blue),
            (ProfileKind::Ftp, colors.blue),
            (ProfileKind::Ssh, colors.green),
            (ProfileKind::WinRm, colors.purple),
            (ProfileKind::Citrix, colors.purple),
            (ProfileKind::Sftp, colors.orange),
            (ProfileKind::Vnc, colors.cyan),
            (ProfileKind::Telnet, colors.comment),
            (ProfileKind::Local, colors.blue),
        ] {
            assert_eq!(protocol_color(&theme, kind), expected, "{kind:?}");
        }
        assert_eq!(Tint::Info.color(&theme), colors.cyan, "the C# InfoBrush");
        for (hue, expected) in [
            (Hue::Green, colors.green),
            (Hue::Cyan, colors.cyan),
            (Hue::Comment, colors.comment),
            (Hue::Orange, colors.orange),
            (Hue::Pink, colors.pink),
            (Hue::Yellow, colors.yellow),
        ] {
            assert_eq!(Tint::Hue(hue).color(&theme), expected, "{hue:?}");
        }
        let red = crate::themes::theme(AppTheme::Magellan, Accent::Red);
        assert_eq!(
            protocol_color(&red, ProfileKind::Local),
            colors.red,
            "the accent"
        );
    }
}
