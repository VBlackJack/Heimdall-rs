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

//! A tab's own window, as the C# `FloatingSessionWindow`: a header naming the session, its
//! state and its route, with Reattach at its right, above the session as its tab shows it.
//!
//! The window opens at the C# size, centred, and its place is not kept. Its close button
//! is the application's to answer: the tab goes back to the strip, then closes as any tab.
//! It has none of the main window's shortcuts, as the C# window has no input bindings;
//! behind the lock screen it shows a veil and takes nothing.
//!
//! What its session sends is let through by name, each message naming its tab: a message
//! added later, or one the main window would apply to its own tab shown, is dropped.

use heimdall_app::{FloatId, FloatMessage, Message as AppMessage, QuestionId, SessionState, Tab};
use iced::widget::{button, center, column, container, opaque, row, text};
use iced::{Element, Length, Size, event, keyboard, window};

use crate::i18n::fl;
use crate::shell::Message;

/// The size the window opens at, in logical pixels, as the C# one.
pub const WINDOW_SIZE: Size = Size::new(1024.0, 768.0);

/// The smallest the window is made, in logical pixels, as the C# one.
pub const MIN_WINDOW_SIZE: Size = Size::new(400.0, 300.0);

/// Height of the header, in logical pixels, as the C# one.
const HEADER_HEIGHT: f32 = 36.0;

/// Room at the header's ends, in logical pixels, as the C# one's margin.
const HEADER_PADDING: [f32; 2] = [0.0, 8.0];

/// Gap between the header's parts, in logical pixels.
const HEADER_SPACING: f32 = 6.0;

/// Size of the session's name in the header, as the C# body text.
const TITLE_SIZE: f32 = 15.0;

/// Size of the state and the protocol in the header, as the C# caption.
const CAPTION_SIZE: f32 = 12.0;

/// Size of the route in the header, as the C# small caption.
const ROUTE_SIZE: f32 = 11.0;

/// Room inside the Reattach button, as the C# one's padding.
const BUTTON_PADDING: [f32; 2] = [4.0, 12.0];

/// Between the session's name and its state, as the C# header writes it.
const TITLE_STATE_SEPARATOR: &str = " - ";

/// Size of the veil's text.
const VEIL_TEXT_SIZE: f32 = 20.0;

/// How a tab's own window opens: at the C# size, centred, its close button the
/// application's to answer.
#[must_use]
pub fn settings() -> window::Settings {
    window::Settings {
        size: WINDOW_SIZE,
        min_size: Some(MIN_WINDOW_SIZE),
        position: window::Position::Centered,
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

/// What a tab's own window reports of itself; its session's widgets report the rest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatEvent {
    /// Its close button.
    CloseRequested,
    /// It gained the focus, or lost it.
    Focused(bool),
    /// Its screen draws this many physical pixels per logical one.
    Rescaled(f32),
    /// Shift, Ctrl, Alt or the logo key pressed or released over it.
    Modifiers(keyboard::Modifiers),
}

/// The event of a tab's own window it reports; none of the main window's shortcuts.
#[expect(
    clippy::needless_pass_by_value,
    reason = "the signature `event::listen_with` takes"
)]
pub(crate) fn window_event(
    event: iced::Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<FloatEvent> {
    match event {
        iced::Event::Window(window::Event::CloseRequested) => Some(FloatEvent::CloseRequested),
        iced::Event::Window(window::Event::Focused) => Some(FloatEvent::Focused(true)),
        iced::Event::Window(window::Event::Unfocused) => Some(FloatEvent::Focused(false)),
        iced::Event::Window(window::Event::Rescaled(scale)) => Some(FloatEvent::Rescaled(scale)),
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(FloatEvent::Modifiers(modifiers))
        }
        _ => None,
    }
}

/// An event of a window other than `main`, as a tab's own window's; none of the main
/// window, nor any while no main window is named, as in tests, which open none.
pub(crate) fn from_floating(
    (main, (window, event)): (Option<window::Id>, (window::Id, FloatEvent)),
) -> Option<Message> {
    main.is_some_and(|main| main != window)
        .then_some(Message::Float(window, event))
}

/// Whether `message`, sent by the session drawn in the window of `tab`, may reach the
/// application: it names `tab` itself, or a question `tab` asks, or opens a dialog of the
/// main window that names nothing shown. Anything else is dropped, a message added later
/// included: it could act on the main window's tab shown.
#[must_use]
pub fn floating_message_allowed(message: &Message, tab: &Tab) -> bool {
    match message {
        // Its terminal, its desktop and its session bar, its cards: each names its tab.
        Message::App(
            AppMessage::Key { tab: named, .. }
            | AppMessage::Pointer { tab: named, .. }
            | AppMessage::Resize { tab: named, .. }
            | AppMessage::Copy(named)
            | AppMessage::ClipboardText { tab: named, .. }
            | AppMessage::ScrollHistory { tab: named, .. }
            | AppMessage::DesktopInput { tab: named, .. }
            | AppMessage::DesktopResize { tab: named, .. }
            | AppMessage::DesktopShown { tab: named, .. }
            | AppMessage::SendKeys { tab: named, .. }
            | AppMessage::VncQuality { tab: named, .. }
            | AppMessage::SendClipboard(named)
            | AppMessage::SaveRemoteFiles(named)
            | AppMessage::CancelSave(named)
            | AppMessage::StopAntiIdle(named)
            | AppMessage::DisconnectDesktop(named)
            | AppMessage::RequestCloseTab(named)
            | AppMessage::ReconnectTab(named)
            | AppMessage::ForgetServer(named)
            | AppMessage::CancelAutoReconnect(named)
            | AppMessage::CopyHostKeyFingerprint(named)
            | AppMessage::HostKeyDecision { tab: named, .. }
            | AppMessage::HostKeyTrustOnce(named),
        )
        | Message::Submit(named)
        | Message::Decline(named)
        | Message::DesktopFit { tab: named, .. }
        | Message::CopyError(named)
        | Message::CopyAnonymousError(named) => *named == tab.id,
        // What is typed into its question, which only it asks.
        Message::Field { question, .. } | Message::FocusField { question, .. } => tab
            .prompts
            .iter()
            .any(|prompt| prompt.question == *question),
        // The main window's dialogs, naming a profile or nothing: the keyboard's help from
        // the session bar, the profile form from the failure card.
        Message::App(AppMessage::ShowShortcuts | AppMessage::EditProfile(_)) => true,
        _ => false,
    }
}

/// What the window keeps of a tab's own window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloatingWindow {
    /// The application's name for it.
    pub key: FloatId,
    /// Physical pixels per logical one on its screen: its desktop is drawn one of its
    /// pixels per physical one.
    pub scale: f32,
    /// The question of its tab whose first field was last given the focus.
    pub question: Option<QuestionId>,
}

/// What a tab's own window says of its session, in its header.
#[derive(Debug, Clone)]
pub struct Header {
    /// The window.
    pub key: FloatId,
    /// The session's state, as its dot shows it.
    pub state: SessionState,
    /// Its protocol, as its tab names it.
    pub kind: String,
    /// Its name.
    pub title: String,
    /// The gateways it goes through, named; `None` when it goes straight.
    pub route: Option<String>,
}

/// The state of a session, as the C# header words it.
#[must_use]
pub fn state_text(state: SessionState) -> String {
    match state {
        SessionState::Connected => fl!("ui-status-connected-short"),
        SessionState::Connecting => fl!("ui-status-connecting"),
        SessionState::Reconnecting => fl!("ui-status-reconnecting"),
        SessionState::Ended => fl!("ui-status-disconnected"),
        SessionState::Failed => fl!("ui-status-error"),
    }
}

/// The header of a tab's own window, as the C# one: the protocol and the name, the state,
/// the route, and Reattach at the right.
fn header<'a>(header: Header) -> Element<'a, Message> {
    let mut named = row![
        crate::tree_view::state_dot(Some(header.state)),
        text(header.kind).size(CAPTION_SIZE),
        text(header.title).size(TITLE_SIZE),
        text(TITLE_STATE_SEPARATOR)
            .size(CAPTION_SIZE)
            .style(text::secondary),
        text(state_text(header.state)).size(CAPTION_SIZE),
    ]
    .spacing(HEADER_SPACING)
    .align_y(iced::Alignment::Center);
    if let Some(route) = header.route {
        named = named.push(text(route).size(ROUTE_SIZE).style(text::secondary));
    }
    let reattach = button(text(fl!("ui-detach-reattach")).size(CAPTION_SIZE))
        .style(button::secondary)
        .padding(BUTTON_PADDING)
        .on_press(Message::App(AppMessage::Float(FloatMessage::Reattach(
            header.key,
        ))));
    container(
        row![named, iced::widget::space::horizontal(), reattach].align_y(iced::Alignment::Center),
    )
    .padding(HEADER_PADDING)
    .height(HEADER_HEIGHT)
    .width(Length::Fill)
    .align_y(iced::alignment::Vertical::Center)
    .style(container::rounded_box)
    .into()
}

/// A tab's own window: its header above `body`, the session as its tab shows it.
#[must_use]
pub fn view(heading: Header, body: Element<'_, Message>) -> Element<'_, Message> {
    column![
        header(heading),
        container(body).width(Length::Fill).height(Length::Fill)
    ]
    .into()
}

/// A tab's own window behind the lock screen: a veil over all of it, taking every click,
/// and nothing of the session drawn, so no field of it takes what is typed.
#[must_use]
pub fn veil<'a>() -> Element<'a, Message> {
    opaque(
        container(center(
            text(fl!("ui-vault-locked-title")).size(VEIL_TEXT_SIZE),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme: &iced::Theme| container::Style {
            background: Some(theme.palette().background.into()),
            ..container::Style::default()
        }),
    )
}
