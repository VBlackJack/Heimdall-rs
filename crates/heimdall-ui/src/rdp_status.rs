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

//! The indicators of the C# embedded RDP session header (`EmbeddedRdpView.xaml`) that the
//! built-in client can drive: the health dot, the four-step connection phase stepper, the
//! seconds elapsed while it reconnects, the letterbox hint of a fixed size that does not fill
//! the tab, and the redirections, the disabled ones behind a "+N" badge.

use std::time::{Duration, Instant};

use heimdall_app::{Phase, Retry, SessionState, Tab, TabId, UiError};
use heimdall_core::profile::{AudioPlayback, RdpProfile};
use heimdall_rdp::{Ending, Step};
use iced::widget::text::Span;
use iced::widget::{Row, button, container, rich_text, span, text, tooltip};
use iced::{Element, Font};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Side of the health dot, as the C# `HealthDotColor` border.
const DOT_SIDE: f32 = 10.0;

/// Width of a stepper segment, as the C# `SessionPhaseSegmentWidth`.
const SEGMENT_WIDTH: f32 = 14.0;

/// Height of a stepper segment, as the C# `SessionPhaseSegmentHeight`.
const SEGMENT_HEIGHT: f32 = 6.0;

/// Room between two segments, as the C# segment's right margin.
const SEGMENT_GAP: f32 = 3.0;

/// How long the letterbox hint stays, as the C# `LetterboxHintDisplayDuration`.
pub const LETTERBOX_HINT_SHOWN: Duration = Duration::from_secs(4);

/// What the health dot says of a session, as the C# `RdpHealthDotState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// Nothing open: closed by the user, or never opened.
    Idle,
    /// Connected.
    Healthy,
    /// On its way, or coming back.
    Transitional,
    /// It failed, or the server ended it.
    Faulted,
}

impl Health {
    /// The health of `tab`'s session, as the C# `RdpHealthDotPolicy` resolves it: connected
    /// is healthy, a connection on its way or a reconnection is in transition, a failure or
    /// an ending the server gave a reason for is faulted, an ending the user chose is idle.
    #[must_use]
    pub fn of(tab: &Tab) -> Self {
        if tab.retry.is_some() {
            return Self::Transitional;
        }
        match &tab.phase {
            Phase::Connected => Self::Healthy,
            Phase::Failed(UiError::Cancelled | UiError::CertificateRefused) => Self::Idle,
            Phase::Failed(_) => Self::Faulted,
            Phase::Closed { .. } => match &tab.end_reason {
                None | Some(Ending::Logoff) => Self::Idle,
                Some(_) => Self::Faulted,
            },
            _ => Self::Transitional,
        }
    }

    /// Its name, as the C# says it.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Idle => fl!("ui-rdp-health-idle"),
            Self::Healthy => fl!("ui-rdp-health-healthy"),
            Self::Transitional => fl!("ui-rdp-health-transitional"),
            Self::Faulted => fl!("ui-rdp-health-faulted"),
        }
    }

    /// The session state whose colour the dot takes, as the tree's dot.
    fn state(self) -> SessionState {
        match self {
            Self::Idle => SessionState::Ended,
            Self::Healthy => SessionState::Connected,
            Self::Transitional => SessionState::Connecting,
            Self::Faulted => SessionState::Failed,
        }
    }
}

/// How far a connection has come, as the C# `RdpConnectionPhase`, from the steps the built-in
/// client reports: Preparing until the server answers (the gateways, the connection),
/// Connecting through the security exchange, TLS and the logon, Loading once logged on, then
/// Connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectPhase {
    /// Not connecting: nothing to show.
    None,
    /// Reaching the server.
    Preparing,
    /// The server answered: security, certificate, logon.
    Connecting,
    /// Logged on: the session is being set up.
    Loading,
    /// The desktop is shown.
    Connected,
}

impl ConnectPhase {
    /// Segments of the stepper, as the C# `RdpConnectionPhasePolicy.SegmentCount`.
    pub const SEGMENTS: u32 = 4;

    /// The phase of `tab`'s connection.
    #[must_use]
    pub fn of(tab: &Tab) -> Self {
        match &tab.phase {
            Phase::Connected => Self::Connected,
            Phase::Connecting | Phase::HostKey { .. } => match tab.rdp_step {
                None => Self::Preparing,
                Some(Step::Connecting) => Self::Connecting,
                Some(Step::Loading) => Self::Loading,
            },
            _ => Self::None,
        }
    }

    /// Segments lit, as the C# `GetLitSegmentCount`.
    #[must_use]
    pub fn lit(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Preparing => 1,
            Self::Connecting => 2,
            Self::Loading => 3,
            Self::Connected => 4,
        }
    }

    /// What the C# status line says of it; `None` when there is nothing to say.
    #[must_use]
    pub fn label(self) -> Option<String> {
        Some(match self {
            Self::None => return None,
            Self::Preparing => fl!("ui-rdp-phase-preparing"),
            Self::Connecting => fl!("ui-rdp-phase-connecting"),
            Self::Loading => fl!("ui-rdp-phase-loading"),
            Self::Connected => fl!("ui-rdp-phase-connected"),
        })
    }

    /// What the stepper says of itself, as the C# announces it: the phase and its rank.
    #[must_use]
    pub fn progress(self) -> Option<String> {
        self.label().map(|phase| {
            fl!(
                "ui-rdp-phase-progress",
                phase = phase,
                step = self.lit().to_string(),
                steps = Self::SEGMENTS.to_string()
            )
        })
    }
}

/// Whole seconds since `retry`'s session dropped, at `now`, as the C# floors them.
#[must_use]
pub fn elapsed_seconds(retry: Retry, now: Instant) -> u64 {
    now.saturating_duration_since(retry.since).as_secs()
}

/// "Ns elapsed", as the C# says it while the session reconnects.
#[must_use]
pub fn elapsed_text(retry: Retry, now: Instant) -> String {
    fl!(
        "ui-rdp-reconnect-elapsed",
        seconds = elapsed_seconds(retry, now).to_string()
    )
}

/// The health dot, its state in its tip.
#[must_use]
pub fn health_dot<'a>(health: Health) -> Element<'a, Message> {
    tooltip(
        crate::tree_view::sized_state_dot(Some(health.state()), DOT_SIDE),
        text(fl!("ui-rdp-health-tooltip", state = health.label())).size(font_size::CAPTION),
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// The phase stepper: four segments, as many lit as the phase is far; its phase and rank in
/// its tip. `None` when not connecting, as the C# collapses it.
#[must_use]
pub fn stepper<'a>(phase: ConnectPhase) -> Option<Element<'a, Message>> {
    let lit = phase.lit();
    let progress = phase.progress()?;
    let segments = (1..=ConnectPhase::SEGMENTS).map(|segment| {
        container(iced::widget::space())
            .width(SEGMENT_WIDTH)
            .height(SEGMENT_HEIGHT)
            .style(move |theme| styles::phase_segment(theme, segment <= lit))
            .into()
    });
    Some(
        tooltip(
            Row::with_children(segments)
                .spacing(SEGMENT_GAP)
                .align_y(iced::Alignment::Center),
            text(progress).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box)
        .into(),
    )
}

/// The status of `tab`'s session at `now`: its health dot, its phase stepper while it
/// connects, and the seconds elapsed while it reconnects.
#[must_use]
pub fn status<'a>(tab: &Tab, now: Instant) -> Row<'a, Message> {
    let mut status = Row::new()
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center)
        .push(health_dot(Health::of(tab)))
        .push(stepper(ConnectPhase::of(tab)));
    if let Some(retry) = tab.retry {
        status = status.push(
            text(elapsed_text(retry, now))
                .size(font_size::CAPTION)
                .font(Font {
                    style: iced::font::Style::Italic,
                    ..crate::UI_FONT
                })
                .style(text::secondary),
        );
    }
    status
}

/// Whether a desktop of fixed size `fixed`, in a tab of `shown`, leaves bars around it:
/// shown at its size and not the tab's, or `fit` to a tab of another shape. As the C#
/// `RdpRegionFrameLayout.IsLetterboxActive`.
#[must_use]
pub fn letterboxed(fixed: Option<(u16, u16)>, shown: Option<(u16, u16)>, fit: bool) -> bool {
    let (Some((width, height)), Some((shown_width, shown_height))) = (fixed, shown) else {
        return false;
    };
    if fit {
        u32::from(width) * u32::from(shown_height) != u32::from(height) * u32::from(shown_width)
    } else {
        (width, height) != (shown_width, shown_height)
    }
}

/// The letterbox hint of one tab, as the C# `LetterboxHintState`: shown once for a size and
/// mode, for [`LETTERBOX_HINT_SHOWN`], and once more only after either changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LetterboxHint {
    /// The fixed size and mode last seen.
    observed: Option<(Option<(u16, u16)>, bool)>,
    /// Shown already for them.
    shown: bool,
    /// When it was shown, while it shows.
    since: Option<Instant>,
}

impl LetterboxHint {
    /// What the tab is at `now`: its fixed size, whether fitted, whether letterboxed.
    pub fn observe(
        &mut self,
        fixed: Option<(u16, u16)>,
        fit: bool,
        letterboxed: bool,
        now: Instant,
    ) {
        if self.observed != Some((fixed, fit)) {
            self.observed = Some((fixed, fit));
            self.shown = false;
        }
        if !letterboxed {
            self.since = None;
            return;
        }
        if !self.shown {
            self.shown = true;
            self.since = Some(now);
        }
    }

    /// Whether it shows at `now`.
    #[must_use]
    pub fn visible(&self, now: Instant) -> bool {
        self.since
            .is_some_and(|since| now.saturating_duration_since(since) < LETTERBOX_HINT_SHOWN)
    }
}

/// The letterbox hint's words for a fixed size of `width` by `height`.
#[must_use]
pub fn letterbox_text(width: u16, height: u16) -> String {
    fl!(
        "ui-rdp-letterbox-hint",
        width = width.to_string(),
        height = height.to_string()
    )
}

/// The letterbox hint's badge.
#[must_use]
pub fn letterbox_badge<'a>(width: u16, height: u16) -> Element<'a, Message> {
    container(
        text(letterbox_text(width, height))
            .size(font_size::CAPTION)
            .style(text::secondary)
            .wrapping(text::Wrapping::None),
    )
    .padding([3.0, 6.0])
    .style(styles::letterbox_hint)
    .into()
}

/// Whether a redirection is shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sharing {
    /// Shared.
    On,
    /// The profile turned it off.
    Off,
    /// The profile asks for it and the built-in client cannot share it yet (decision D8).
    Unsupported,
}

/// One of the C# session header's redirections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirection {
    /// Its name on the bar.
    pub name: String,
    /// Its full name, as the C# `RdpRedirectionLabel*`.
    pub label: String,
    /// Whether it is shared.
    pub sharing: Sharing,
}

impl Redirection {
    /// What its tip says, as the C# `RdpRedirectionStatus*Format`.
    #[must_use]
    pub fn status(&self) -> String {
        let name = self.label.as_str();
        match self.sharing {
            Sharing::On => fl!("ui-rdp-redirection-on", name = name),
            Sharing::Off => fl!("ui-rdp-redirection-off", name = name),
            Sharing::Unsupported => fl!("ui-rdp-redirection-unsupported", name = name),
        }
    }
}

/// The redirections of a session of `profile`, in the C# header's order: clipboard, drives,
/// printers, COM ports, smart cards, USB, audio, multi-monitor. The built-in client shares
/// the clipboard, the drives and the sound; the rest it cannot share yet.
#[must_use]
pub fn redirections(profile: &RdpProfile) -> Vec<Redirection> {
    let shared = |on: bool| if on { Sharing::On } else { Sharing::Off };
    let unsupported = |asked: bool| {
        if asked {
            Sharing::Unsupported
        } else {
            Sharing::Off
        }
    };
    let extras = &profile.extras;
    [
        (
            fl!("ui-desktop-shares-clipboard"),
            fl!("ui-desktop-shares-clipboard-tooltip"),
            shared(profile.redirect_clipboard),
        ),
        (
            fl!("ui-desktop-shares-drives"),
            fl!("ui-desktop-shares-drives-tooltip"),
            shared(profile.redirect_drives),
        ),
        (
            fl!("ui-desktop-shares-printers"),
            fl!("ui-desktop-shares-printers-tooltip"),
            unsupported(extras.redirect_printers),
        ),
        (
            fl!("ui-desktop-shares-com-ports"),
            fl!("ui-desktop-shares-com-ports-tooltip"),
            unsupported(extras.redirect_com_ports),
        ),
        (
            fl!("ui-desktop-shares-smart-cards"),
            fl!("ui-desktop-shares-smart-cards-tooltip"),
            unsupported(extras.redirect_smart_cards),
        ),
        (
            fl!("ui-desktop-shares-usb"),
            fl!("ui-desktop-shares-usb-tooltip"),
            unsupported(extras.redirect_usb),
        ),
        (
            fl!("ui-desktop-shares-audio"),
            fl!("ui-desktop-shares-audio-tooltip"),
            shared(profile.options.audio == AudioPlayback::Local),
        ),
        (
            fl!("ui-desktop-shares-multi-monitor"),
            fl!("ui-desktop-shares-multi-monitor-tooltip"),
            unsupported(extras.multi_monitor),
        ),
    ]
    .into_iter()
    .map(|(name, label, sharing)| Redirection {
        name,
        label,
        sharing,
    })
    .collect()
}

/// The redirections not shared, as the C# counts them for its "+N" badge.
#[must_use]
pub fn hidden(redirections: &[Redirection]) -> usize {
    redirections
        .iter()
        .filter(|redirection| redirection.sharing != Sharing::On)
        .count()
}

/// The redirections on the bar of `tab`'s session: those shared; those not shared too once
/// `expanded`, struck through, else the "+N" badge that shows them.
#[must_use]
pub fn redirection_controls<'a>(
    tab: TabId,
    profile: &RdpProfile,
    expanded: bool,
) -> Vec<Element<'a, Message>> {
    let all = redirections(profile);
    let count = hidden(&all);
    let mut controls: Vec<Element<'a, Message>> = all
        .into_iter()
        .filter(|redirection| expanded || redirection.sharing == Sharing::On)
        .map(|redirection| {
            let tip = redirection.status();
            let name: Element<'a, Message> = if redirection.sharing == Sharing::On {
                text(redirection.name)
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .into()
            } else {
                let spans: [Span<'a, (), Font>; 1] = [span(redirection.name).strikethrough(true)];
                rich_text(spans)
                    .size(font_size::CAPTION)
                    .style(text::secondary)
                    .into()
            };
            tooltip(
                name,
                text(tip).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
            .into()
        })
        .collect();
    if !expanded && count > 0 {
        controls.push(
            tooltip(
                button(
                    text(fl!("ui-rdp-redirections-more", count = count.to_string()))
                        .size(font_size::CAPTION),
                )
                .style(styles::subtle)
                .on_press(Message::ShowDisabledRedirections(tab)),
                text(fl!("ui-rdp-redirections-more-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
            .into(),
        );
    }
    controls
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed size smaller than the tab.
    const SMALL: (u16, u16) = (1024, 768);
    /// The tab's size.
    const TAB: (u16, u16) = (1600, 900);

    #[test]
    fn the_stepper_lights_one_segment_per_phase_as_the_csharp() {
        assert_eq!(ConnectPhase::None.lit(), 0);
        assert_eq!(ConnectPhase::Preparing.lit(), 1);
        assert_eq!(ConnectPhase::Connecting.lit(), 2);
        assert_eq!(ConnectPhase::Loading.lit(), 3);
        assert_eq!(ConnectPhase::Connected.lit(), ConnectPhase::SEGMENTS);
        assert_eq!(ConnectPhase::None.progress(), None);
        assert_eq!(
            ConnectPhase::Connecting.progress().as_deref(),
            Some("Connecting... (2 of 4)")
        );
        assert_eq!(
            ConnectPhase::Loading.progress().as_deref(),
            Some("Loading the remote desktop session... (3 of 4)")
        );
    }

    #[test]
    fn the_elapsed_counter_counts_whole_seconds_from_the_drop() {
        let since = Instant::now();
        let retry = Retry {
            attempt: 2,
            max: 20,
            due: since + Duration::from_secs(20),
            since,
        };
        assert_eq!(elapsed_seconds(retry, since), 0);
        assert_eq!(
            elapsed_seconds(retry, since + Duration::from_millis(12_900)),
            12
        );
        assert_eq!(
            elapsed_text(retry, since + Duration::from_secs(7)),
            "7s elapsed"
        );
    }

    #[test]
    fn a_fixed_size_other_than_the_tab_s_is_letterboxed() {
        assert!(letterboxed(Some(SMALL), Some(TAB), false), "smaller");
        assert!(
            letterboxed(Some((1920, 1080)), Some(SMALL), false),
            "larger, shown at its size"
        );
        assert!(
            letterboxed(Some((1920, 1200)), Some(TAB), true),
            "fitted to a tab of another shape"
        );
        assert!(
            !letterboxed(Some((3200, 1800)), Some(TAB), true),
            "fitted to a tab of its shape"
        );
        assert!(!letterboxed(Some(TAB), Some(TAB), false), "the tab's size");
        assert!(!letterboxed(None, Some(TAB), false), "following the tab");
        assert_eq!(
            letterbox_text(1024, 768),
            "Fixed 1024x768 - resize the window or change resolution to fill."
        );
    }

    #[test]
    fn the_letterbox_hint_shows_once_per_size_for_four_seconds() {
        let start = Instant::now();
        let mut hint = LetterboxHint::default();
        hint.observe(Some(SMALL), false, true, start);
        assert!(hint.visible(start));
        assert!(hint.visible(start + Duration::from_secs(3)));
        assert!(!hint.visible(start + LETTERBOX_HINT_SHOWN));
        // Not shown again for the same size and mode, letterboxed again or not.
        let later = start + Duration::from_secs(10);
        hint.observe(Some(SMALL), false, false, later);
        hint.observe(Some(SMALL), false, true, later);
        assert!(!hint.visible(later));
        // Another size: once more.
        hint.observe(Some((800, 600)), false, true, later);
        assert!(hint.visible(later));
        // No longer letterboxed: gone at once.
        hint.observe(Some((800, 600)), false, false, later);
        assert!(!hint.visible(later));
    }
}
