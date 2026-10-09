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

//! The RDP options of a profile's form, as the C# server dialog's RDP tabs in their order:
//! "Display & Audio" (the audio mode beside the colour depth, then how the desktop is sized,
//! the monitors spanned and the microphone), "Devices", "Performance" and "Behavior"; the RD
//! Gateway, which the C# asks on its Network tab; and the application's RDP options on the
//! settings page.
//!
//! While a profile follows the global defaults, the options they decide are greyed and out
//! of reach, their values the profile's own, as the C# greys them. What only Remote Desktop
//! Connection honours says so while the profile opens in a tab.

use heimdall_app::profile_draft::{ProfileChoice, ProfileDraft, ProfileField, ProfileToggle};
use heimdall_app::{Message as AppMessage, SettingsMessage};
use heimdall_core::profile::{
    Aspect, AudioPlayback, ColorDepth, Experience, RdpDefaults, RdpOptions, RdpSwitch, Resolution,
};
use iced::widget::{checkbox, column, container, opaque, pick_list, row, space, stack, text};
use iced::{Alignment, Color, Element, Length, Theme};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// How much of the form's background veils a list the global defaults decide, as a
/// disabled C# control is greyed.
const VEIL_OPACITY: f32 = 0.5;
/// The fewest screens that can be spanned, as the C# `RdpDisplayCapabilities` offers the
/// multi-monitor mode.
const MULTI_MONITOR_SCREENS: usize = 2;

/// A screen of this computer, as the C# `MonitorInfo` gives it to the monitors an RDP
/// profile spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor {
    /// Its place among the screens, as Remote Desktop Connection numbers them.
    pub index: u32,
    /// Its width, in pixels.
    pub width: u32,
    /// Its height, in pixels.
    pub height: u32,
    /// Whether it is the primary screen.
    pub primary: bool,
}

/// A colour depth as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DepthChoice(ColorDepth);

impl std::fmt::Display for DepthChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            ColorDepth::Bpp16 => fl!("ui-profile-color-16"),
            ColorDepth::Bpp24 => fl!("ui-profile-color-24"),
            ColorDepth::Bpp32 => fl!("ui-profile-color-32"),
        })
    }
}

/// An audio mode as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AudioChoice(AudioPlayback);

impl std::fmt::Display for AudioChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            AudioPlayback::Off => fl!("ui-profile-audio-off"),
            AudioPlayback::Local => fl!("ui-profile-audio-local"),
            AudioPlayback::OnServer => fl!("ui-profile-audio-on-server"),
        })
    }
}

/// The audio mode and colour depth lists of `options`, side by side; greyed and out of
/// reach when `locked`, the global defaults deciding them.
#[must_use]
pub fn view<'a>(options: RdpOptions, locked: bool) -> Element<'a, Message> {
    veiled(
        lists(
            options.audio,
            options.color_depth,
            |audio| Message::App(AppMessage::ProfileChoice(ProfileChoice::Audio(audio))),
            |depth| Message::App(AppMessage::ProfileChoice(ProfileChoice::ColorDepth(depth))),
        ),
        locked,
    )
}

/// `element` greyed and out of reach when `locked`, as the C# greys a control: a list has
/// no disabled state of its own, so a veil takes the clicks meant for it.
fn veiled(element: Element<'_, Message>, locked: bool) -> Element<'_, Message> {
    if !locked {
        return element;
    }
    let veil = container(space::horizontal())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(
                Color {
                    a: VEIL_OPACITY,
                    ..theme.palette().background
                }
                .into(),
            ),
            ..container::Style::default()
        });
    stack![element, opaque(veil)].width(Length::Fill).into()
}

/// Whether the form's profile follows the global defaults, which then decide the options
/// they have.
fn following(draft: &ProfileDraft) -> bool {
    draft.is_on(ProfileToggle::FollowDefaults)
}

/// Whether the profile opens in a tab, where only the built-in client's options apply: not
/// set to Remote Desktop Connection, and no RD Gateway typed, which opens it there too.
fn opens_in_tab(draft: &ProfileDraft) -> bool {
    !draft.rdp_extras.external && draft.rd_gateway.trim().is_empty()
}

/// A group of the options, named as the C# names its tab.
fn group<'a>(title: String) -> Element<'a, Message> {
    crate::dialog_parts::section(title, None)
}

/// A part of a group, named as the C# labels it.
fn part<'a>(title: String) -> Element<'a, Message> {
    text(title).size(font_size::CAPTION).into()
}

/// `element`, and beside it, while the profile opens in a tab, that only Remote Desktop
/// Connection honours it.
fn external_only<'a>(draft: &ProfileDraft, element: Element<'a, Message>) -> Element<'a, Message> {
    if !opens_in_tab(draft) {
        return element;
    }
    row![
        element,
        text(fl!("ui-profile-rdp-mstsc-only"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center)
    .into()
}

/// The box of `toggle`, out of reach when `locked`.
fn toggle_box<'a>(
    draft: &ProfileDraft,
    toggle: ProfileToggle,
    label: String,
    locked: bool,
) -> Element<'a, Message> {
    checkbox(draft.is_on(toggle))
        .style(styles::checkbox)
        .label(label)
        .on_toggle_maybe(
            (!locked).then_some(move |on| Message::App(AppMessage::ProfileToggle { toggle, on })),
        )
        .into()
}

/// The box of `switch`, out of reach while the global defaults decide it, said to be
/// Remote Desktop Connection's; hardware acceleration said honoured by neither client yet.
fn switch_box<'a>(draft: &ProfileDraft, switch: RdpSwitch) -> Element<'a, Message> {
    let locked = following(draft) && switch.follows_defaults();
    let tick = checkbox(switch.is_on(&draft.rdp_extras))
        .style(styles::checkbox)
        .label(switch_label(switch))
        .on_toggle_maybe((!locked).then_some(move |on| choice(ProfileChoice::Extra(switch, on))))
        .into();
    if switch == RdpSwitch::HardwareAcceleration {
        return unsupported(tick);
    }
    external_only(draft, tick)
}

/// The hardware acceleration box, and under it that it is not supported yet: the C# sets
/// the `MsTscAx` control's `EnableHardwareMode` (`RdpActiveXHost.cs:2017-2042`), which the
/// built-in client has no counterpart of, and writes nothing of it in the `.rdp` file it
/// gives Remote Desktop Connection (`RdpFileGenerator.cs`), which documents no key for it.
/// The box is kept, its value saved and carried to and from the C# as the fields of
/// decision D8 are.
fn unsupported(tick: Element<'_, Message>) -> Element<'_, Message> {
    column![
        tick,
        text(fl!("ui-profile-rdp-hardware-acceleration-unsupported"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    ]
    .spacing(spacing::XS)
    .into()
}

/// The C# server dialog's label of `switch`, the RDP settings' where the words are the same.
fn switch_label(switch: RdpSwitch) -> String {
    match switch {
        RdpSwitch::Printers => fl!("ui-settings-rdp-redirect-printers"),
        RdpSwitch::ComPorts => fl!("ui-settings-rdp-redirect-com-ports"),
        RdpSwitch::SmartCards => fl!("ui-settings-rdp-redirect-smart-cards"),
        RdpSwitch::Webcam => fl!("ui-settings-rdp-redirect-webcam"),
        RdpSwitch::Usb => fl!("ui-settings-rdp-redirect-usb"),
        RdpSwitch::Microphone => fl!("ui-profile-rdp-microphone"),
        RdpSwitch::BitmapCaching => fl!("ui-profile-rdp-bitmap-cache"),
        RdpSwitch::Compression => fl!("ui-profile-rdp-compression"),
        RdpSwitch::HardwareAcceleration => fl!("ui-profile-rdp-hardware-acceleration"),
        RdpSwitch::DisableUdp => fl!("ui-profile-rdp-disable-udp"),
        RdpSwitch::FullScreen => fl!("ui-profile-rdp-full-screen"),
    }
}

/// Whether the RDP form draws the box of `toggle` in one of its groups, rather than the
/// form's list of boxes.
#[must_use]
pub fn draws(toggle: ProfileToggle) -> bool {
    matches!(
        toggle,
        ProfileToggle::RedirectClipboard
            | ProfileToggle::RedirectDrives
            | ProfileToggle::AntiIdle
            | ProfileToggle::AutoReconnect
            | ProfileToggle::Nla
            | ProfileToggle::StrictServerAuthentication
            | ProfileToggle::AdminSession
    )
}

/// The C# "Display & Audio" tab of `draft`: the audio mode and colour depth, then the
/// resolution card, its fields drawn by `field`, with the monitors among `monitors`.
pub fn display_audio<'a>(
    draft: &'a ProfileDraft,
    monitors: &[Monitor],
    field: impl Fn(ProfileField) -> Element<'a, Message>,
) -> Element<'a, Message> {
    column![
        group(fl!("ui-profile-rdp-tab-display-audio")),
        view(draft.rdp_options, following(draft)),
        resolution(draft, monitors, field),
    ]
    .spacing(spacing::SM)
    .into()
}

/// The C# "Devices" tab of `draft`: what this computer shares with the server.
#[must_use]
pub fn devices<'a>(draft: &ProfileDraft) -> Element<'a, Message> {
    let locked = following(draft);
    let mut tab = column![
        group(fl!("ui-profile-rdp-tab-devices")),
        toggle_box(
            draft,
            ProfileToggle::RedirectClipboard,
            fl!("ui-profile-toggle-clipboard"),
            locked
        ),
        toggle_box(
            draft,
            ProfileToggle::RedirectDrives,
            fl!("ui-profile-toggle-drives"),
            locked
        ),
    ]
    .spacing(spacing::SM);
    for switch in [
        RdpSwitch::Printers,
        RdpSwitch::ComPorts,
        RdpSwitch::SmartCards,
        RdpSwitch::Webcam,
        RdpSwitch::Usb,
    ] {
        tab = tab.push(switch_box(draft, switch));
    }
    tab.into()
}

/// The C# "Performance" tab of `draft`: the connection's boxes, the visual experience, then
/// UDP.
#[must_use]
pub fn performance<'a>(draft: &ProfileDraft) -> Element<'a, Message> {
    let locked = following(draft);
    column![
        group(fl!("ui-profile-rdp-tab-performance")),
        part(fl!("ui-profile-rdp-connection-section")),
        toggle_box(
            draft,
            ProfileToggle::AntiIdle,
            fl!("ui-profile-toggle-anti-idle"),
            false
        ),
        switch_box(draft, RdpSwitch::BitmapCaching),
        switch_box(draft, RdpSwitch::Compression),
        switch_box(draft, RdpSwitch::HardwareAcceleration),
        toggle_box(
            draft,
            ProfileToggle::AutoReconnect,
            fl!("ui-profile-toggle-auto-reconnect"),
            locked
        ),
        experience(draft.rdp_options),
        switch_box(draft, RdpSwitch::DisableUdp),
    ]
    .spacing(spacing::SM)
    .into()
}

/// The C# "Behavior" tab of `draft`: the security boxes, strict server authentication only
/// with Network Level Authentication, then the administrative session and full screen.
#[must_use]
pub fn behavior<'a>(draft: &ProfileDraft) -> Element<'a, Message> {
    let locked = following(draft);
    column![
        group(fl!("ui-profile-rdp-tab-behavior")),
        part(fl!("ui-profile-rdp-security-section")),
        toggle_box(
            draft,
            ProfileToggle::Nla,
            fl!("ui-profile-toggle-nla"),
            locked
        ),
        toggle_box(
            draft,
            ProfileToggle::StrictServerAuthentication,
            fl!("ui-profile-toggle-strict-server-auth"),
            locked || !draft.is_on(ProfileToggle::Nla),
        ),
        toggle_box(
            draft,
            ProfileToggle::AdminSession,
            fl!("ui-profile-toggle-admin"),
            false
        ),
        switch_box(draft, RdpSwitch::FullScreen),
    ]
    .spacing(spacing::SM)
    .into()
}

/// The C# "RD Gateway server" of `draft`: its `field`, the C# hint, and, while the profile
/// would open in a tab, that a gateway opens it in Remote Desktop Connection.
pub fn rd_gateway<'a>(draft: &ProfileDraft, field: Element<'a, Message>) -> Element<'a, Message> {
    let mut card = column![
        field,
        text(fl!("ui-profile-rd-gateway-hint")).size(font_size::CAPTION),
    ]
    .spacing(spacing::XS);
    if !draft.rdp_extras.external {
        card = card.push(
            text(fl!("ui-profile-rd-gateway-mstsc"))
                .size(font_size::CAPTION)
                .style(text::secondary),
        );
    }
    card.into()
}

/// The C# monitors of `draft` while it spans them: the note, the saved ones not connected
/// said kept, then each of `monitors` to tick when there are several, as the C# picker.
fn spanned<'a>(draft: &ProfileDraft, monitors: &[Monitor]) -> Option<Element<'a, Message>> {
    if !draft.spans_monitors() {
        return None;
    }
    let mut card = column![text(fl!("ui-profile-rdp-multi-monitor-note")).size(font_size::CAPTION)]
        .spacing(spacing::SM);
    let saved = &draft.rdp_extras.monitors;
    if saved
        .iter()
        .any(|index| !monitors.iter().any(|monitor| monitor.index == *index))
    {
        card =
            card.push(text(fl!("ui-profile-rdp-monitors-offline-kept")).size(font_size::CAPTION));
    }
    if monitors.len() >= MULTI_MONITOR_SCREENS {
        card = card
            .push(text(fl!("ui-profile-rdp-monitors-title")).size(font_size::CAPTION))
            .push(text(fl!("ui-profile-rdp-monitors-caption")).size(font_size::CAPTION));
        for monitor in monitors {
            let index = monitor.index;
            card = card.push(
                checkbox(saved.contains(&index))
                    .style(styles::checkbox)
                    .label(monitor_label(monitor))
                    .on_toggle(move |on| choice(ProfileChoice::Monitor(index, on))),
            );
        }
    }
    Some(card.into())
}

/// A monitor as the C# picker names it: its number from 1 and its size, then whether it is
/// the primary one, then whether it stands upright.
fn monitor_label(monitor: &Monitor) -> String {
    let mut label = fl!(
        "ui-profile-rdp-monitor",
        number = monitor.index.saturating_add(1),
        width = monitor.width,
        height = monitor.height
    );
    if monitor.primary {
        label = fl!("ui-profile-rdp-monitor-primary", monitor = label);
    }
    if monitor.width > 0 && monitor.height > 0 && monitor.width < monitor.height {
        label = fl!("ui-profile-rdp-monitor-vertical", monitor = label);
    }
    label
}

/// The C# "Enable multi-monitor mode" of `draft`: offered with several `monitors`, or to
/// turn it off; out of reach while the global defaults decide it.
fn multi_monitor<'a>(draft: &ProfileDraft, monitors: &[Monitor]) -> Element<'a, Message> {
    let spans = draft.spans_monitors();
    let offered = (monitors.len() >= MULTI_MONITOR_SCREENS || spans) && !following(draft);
    external_only(
        draft,
        checkbox(spans)
            .style(styles::checkbox)
            .label(fl!("ui-profile-rdp-multi-monitor"))
            .on_toggle_maybe(offered.then_some(|on| choice(ProfileChoice::MultiMonitor(on))))
            .into(),
    )
}

/// The audio mode and colour depth lists, side by side, each choice sent as it says.
fn lists<'a>(
    audio: AudioPlayback,
    depth: ColorDepth,
    on_audio: impl Fn(AudioPlayback) -> Message + 'a,
    on_depth: impl Fn(ColorDepth) -> Message + 'a,
) -> Element<'a, Message> {
    let audio = column![
        text(fl!("ui-profile-audio")).size(font_size::CAPTION),
        pick_list(
            AudioPlayback::ALL.map(AudioChoice),
            Some(AudioChoice(audio)),
            move |choice: AudioChoice| on_audio(choice.0),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .width(Length::Fill),
    ]
    .spacing(spacing::XS)
    .width(Length::Fill);
    let depth = column![
        text(fl!("ui-profile-color-depth")).size(font_size::CAPTION),
        pick_list(
            ColorDepth::ALL.map(DepthChoice),
            Some(DepthChoice(depth)),
            move |choice: DepthChoice| on_depth(choice.0),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .width(Length::Fill),
    ]
    .spacing(spacing::XS)
    .width(Length::Fill);
    row![audio, depth].spacing(spacing::SM).into()
}

/// The C# "Visual experience" card of `options`: one box per performance flag, each change
/// taken at once.
#[must_use]
pub fn experience<'a>(options: RdpOptions) -> Element<'a, Message> {
    let mut card =
        column![text(fl!("ui-profile-experience")).size(font_size::CAPTION)].spacing(spacing::SM);
    for experience in Experience::ALL {
        card = card.push(
            checkbox(options.has(experience))
                .style(styles::checkbox)
                .label(experience_label(experience))
                .on_toggle(move |on| choice(ProfileChoice::Experience(experience, on))),
        );
    }
    card.into()
}

/// The C# label of `experience`'s box.
fn experience_label(experience: Experience) -> String {
    match experience {
        Experience::DisableWallpaper => fl!("ui-profile-experience-no-wallpaper"),
        Experience::DisableThemes => fl!("ui-profile-experience-no-themes"),
        Experience::DisableAnimations => fl!("ui-profile-experience-no-animations"),
        Experience::DisableDrag => fl!("ui-profile-experience-no-drag"),
        Experience::DisableCursorShadow => fl!("ui-profile-experience-no-cursor-shadow"),
        Experience::EnableFontSmoothing => fl!("ui-profile-experience-font-smoothing"),
        Experience::EnableComposition => fl!("ui-profile-experience-composition"),
    }
}

/// A box of the RDP settings: its label, what it shows and what it changes.
struct Switch {
    /// Its label, in the C# words.
    label: String,
    /// Whether it is ticked.
    get: fn(&RdpDefaults) -> bool,
    /// Ticks or clears it.
    set: fn(&mut RdpDefaults, bool),
    /// Offered only while Network Level Authentication is on, as the C# page greys strict
    /// server authentication out without it.
    needs_nla: bool,
    /// Honoured by neither client yet, which is said under it: hardware acceleration.
    unsupported: bool,
}

impl Switch {
    fn new(label: String, get: fn(&RdpDefaults) -> bool, set: fn(&mut RdpDefaults, bool)) -> Self {
        Self {
            label,
            get,
            set,
            needs_nla: false,
            unsupported: false,
        }
    }
}

/// The boxes of the RDP settings, in the C# page's order: display, sound, redirections,
/// performance, then security.
fn switches() -> [Switch; 16] {
    [
        Switch::new(
            fl!("ui-profile-resolution-dynamic"),
            |d| d.dynamic_resolution,
            |d, on| d.dynamic_resolution = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-multi-monitor"),
            |d| d.multi_monitor,
            |d, on| d.multi_monitor = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-audio-capture"),
            |d| d.microphone,
            |d, on| d.microphone = on,
        ),
        Switch::new(
            fl!("ui-profile-toggle-clipboard"),
            |d| d.redirect_clipboard,
            |d, on| d.redirect_clipboard = on,
        ),
        Switch::new(
            fl!("ui-profile-toggle-drives"),
            |d| d.redirect_drives,
            |d, on| d.redirect_drives = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-redirect-printers"),
            |d| d.redirect_printers,
            |d, on| d.redirect_printers = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-redirect-com-ports"),
            |d| d.redirect_com_ports,
            |d, on| d.redirect_com_ports = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-redirect-smart-cards"),
            |d| d.redirect_smart_cards,
            |d, on| d.redirect_smart_cards = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-redirect-webcam"),
            |d| d.redirect_webcam,
            |d, on| d.redirect_webcam = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-redirect-usb"),
            |d| d.redirect_usb,
            |d, on| d.redirect_usb = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-bitmap-cache"),
            |d| d.bitmap_caching,
            |d, on| d.bitmap_caching = on,
        ),
        Switch::new(
            fl!("ui-settings-rdp-compression"),
            |d| d.compression,
            |d, on| d.compression = on,
        ),
        Switch {
            unsupported: true,
            ..Switch::new(
                fl!("ui-settings-rdp-hardware-acceleration"),
                |d| d.hardware_acceleration,
                |d, on| d.hardware_acceleration = on,
            )
        },
        Switch::new(
            fl!("ui-settings-rdp-auto-reconnect"),
            |d| d.auto_reconnect,
            |d, on| d.auto_reconnect = on,
        ),
        Switch::new(fl!("ui-profile-toggle-nla"), |d| d.nla, |d, on| d.nla = on),
        Switch {
            needs_nla: true,
            ..Switch::new(
                fl!("ui-settings-rdp-strict-server-auth"),
                |d| d.strict_server_authentication,
                |d, on| d.strict_server_authentication = on,
            )
        },
    ]
}

/// The application's RDP options, as the C# RDP settings: what a profile following them
/// takes, in the C# page's order, each change applied at once.
#[must_use]
pub fn defaults<'a>(defaults: RdpDefaults) -> Element<'a, Message> {
    let send = |defaults: RdpDefaults| {
        Message::App(AppMessage::Settings(SettingsMessage::RdpDefaults(defaults)))
    };
    let mut page = column![
        text(fl!("ui-settings-rdp-defaults-hint")).size(font_size::CAPTION),
        lists(
            defaults.audio,
            defaults.color_depth,
            move |audio| send(RdpDefaults { audio, ..defaults }),
            move |color_depth| send(RdpDefaults {
                color_depth,
                ..defaults
            }),
        ),
    ]
    .spacing(spacing::SM);
    for switch in switches() {
        let mut tick = checkbox((switch.get)(&defaults))
            .style(styles::checkbox)
            .label(switch.label);
        if !switch.needs_nla || defaults.nla {
            let set = switch.set;
            tick = tick.on_toggle(move |on| {
                let mut changed = defaults;
                set(&mut changed, on);
                send(changed)
            });
        }
        page = page.push(if switch.unsupported {
            unsupported(tick.into())
        } else {
            tick.into()
        });
    }
    page.into()
}

/// A resolution mode as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolutionChoice(Resolution);

impl std::fmt::Display for ResolutionChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            Resolution::FitWindow => fl!("ui-profile-resolution-fit-window"),
            Resolution::Fixed => fl!("ui-profile-resolution-fixed"),
            Resolution::SmartSizing => fl!("ui-profile-resolution-smart-sizing"),
            Resolution::MultiMonitor => fl!("ui-profile-resolution-multi-monitor"),
            Resolution::Auto => fl!("ui-profile-resolution-auto"),
        })
    }
}

/// A proportion as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AspectChoice(Aspect);

impl std::fmt::Display for AspectChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0.ratio() {
            None => fl!("ui-profile-aspect-stretch"),
            Some((wide, high)) => fl!("ui-profile-aspect-ratio-choice", wide = wide, high = high),
        })
    }
}

/// A session mode as the list names it: in a tab, or in Remote Desktop Connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModeChoice(bool);

impl std::fmt::Display for ModeChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&if self.0 {
            fl!("ui-profile-rdp-mode-external")
        } else {
            fl!("ui-profile-rdp-mode-embedded")
        })
    }
}

/// The C# "Session mode" list of `draft`: in a tab, or in Remote Desktop Connection, which
/// is then explained.
fn session_mode<'a>(draft: &ProfileDraft) -> Element<'a, Message> {
    let external = draft.rdp_extras.external;
    let mut mode = column![
        text(fl!("ui-profile-rdp-session-mode")).size(font_size::CAPTION),
        pick_list(
            [ModeChoice(false), ModeChoice(true)],
            Some(ModeChoice(external)),
            |picked: ModeChoice| choice(ProfileChoice::External(picked.0)),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .width(Length::Fill),
    ]
    .spacing(spacing::XS);
    if external {
        mode = mode.push(text(fl!("ui-profile-rdp-mode-external-desc")).size(font_size::CAPTION));
    }
    mode.into()
}

/// The common sizes the C# dialog offers for a fixed desktop.
const PRESETS: [(u16, u16); 5] = [
    (1280, 720),
    (1366, 768),
    (1920, 1080),
    (2560, 1440),
    (3840, 2160),
];

/// A common size as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PresetChoice(u16, u16);

impl std::fmt::Display for PresetChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&fl!(
            "ui-profile-resolution-preset",
            width = self.0,
            height = self.1
        ))
    }
}

fn choice(choice: ProfileChoice) -> Message {
    Message::App(AppMessage::ProfileChoice(choice))
}

/// The C# "Resolution profile" card of `draft`: the mode, and in the fixed mode its common
/// sizes, the width and height drawn by `field`, and whether it is scaled; then the
/// monitors spanned among `monitors` and the multi-monitor box; then the C# "Display" with
/// whether the desktop follows the tab, and "Audio" with the microphone.
pub fn resolution<'a>(
    draft: &'a ProfileDraft,
    monitors: &[Monitor],
    field: impl Fn(ProfileField) -> Element<'a, Message>,
) -> Element<'a, Message> {
    let options = draft.rdp_options;
    let locked = following(draft);
    let mut card = column![
        text(fl!("ui-profile-resolution-title")),
        text(fl!("ui-profile-resolution-desc")).size(font_size::CAPTION),
        text(fl!("ui-profile-resolution-mode")).size(font_size::CAPTION),
        pick_list(
            Resolution::ALL.map(ResolutionChoice),
            Some(ResolutionChoice(options.resolution)),
            |picked: ResolutionChoice| choice(ProfileChoice::Resolution(picked.0)),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .width(Length::Fill),
    ]
    .spacing(spacing::XS);
    if options.resolution == Resolution::Auto {
        card = card.push(text(fl!("ui-profile-resolution-auto-desc")).size(font_size::CAPTION));
    }
    card = card
        .push(session_mode(draft))
        .push(text(fl!("ui-profile-aspect-ratio")).size(font_size::CAPTION))
        .push(
            pick_list(
                Aspect::ALL.map(AspectChoice),
                Some(AspectChoice(options.aspect)),
                |picked: AspectChoice| choice(ProfileChoice::Aspect(picked.0)),
            )
            .style(styles::pick_list)
            .menu_style(styles::menu)
            .width(Length::Fill),
        );
    if draft.shows(ProfileField::FixedWidth) {
        // The size typed, when it is one of the list's.
        let typed = (
            draft.fixed_width.trim().parse(),
            draft.fixed_height.trim().parse(),
        );
        let current = match typed {
            (Ok(width), Ok(height)) => PRESETS
                .contains(&(width, height))
                .then_some(PresetChoice(width, height)),
            _ => None,
        };
        card = card
            .push(text(fl!("ui-profile-resolution-presets")).size(font_size::CAPTION))
            .push(
                pick_list(
                    PRESETS.map(|(width, height)| PresetChoice(width, height)),
                    current,
                    |PresetChoice(width, height)| choice(ProfileChoice::Preset(width, height)),
                )
                .style(styles::pick_list)
                .menu_style(styles::menu)
                .placeholder(fl!("ui-profile-resolution-custom"))
                .width(Length::Fill),
            )
            .push(
                row![
                    field(ProfileField::FixedWidth),
                    field(ProfileField::FixedHeight)
                ]
                .spacing(spacing::SM),
            )
            .push(
                checkbox(options.scale_fixed)
                    .style(styles::checkbox)
                    .label(fl!("ui-profile-resolution-scale-fixed"))
                    .on_toggle(|on| choice(ProfileChoice::ScaleFixed(on))),
            );
    }
    if let Some(spanned) = spanned(draft, monitors) {
        card = card.push(spanned);
    }
    card.push(multi_monitor(draft, monitors))
        .push(part(fl!("ui-profile-rdp-display-section")))
        .push(
            checkbox(options.dynamic_resolution)
                .style(styles::checkbox)
                .label(fl!("ui-profile-resolution-dynamic"))
                .on_toggle_maybe(
                    (!locked).then_some(|on| choice(ProfileChoice::DynamicResolution(on))),
                ),
        )
        // The C# "Dynamic resize delay (ms)", where the desktop follows the tab.
        .push(
            draft
                .shows(ProfileField::ResizeDelay)
                .then(|| field(ProfileField::ResizeDelay)),
        )
        .push(part(fl!("ui-profile-rdp-audio-section")))
        .push(switch_box(draft, RdpSwitch::Microphone))
        .into()
}
