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

//! The properties panel docked under the session tree, as the C# one
//! (`MainWindow.xaml:1225-1398`, `MainWindow.PropertiesPanel.cs`,
//! `Controls/SessionPropertiesView.xaml`): off until asked for, from the tree's filter menu
//! or Ctrl+Shift+B; under a splitter that sets its height; read-only, readable while a
//! session tab covers the detail pane. It follows the tree: one session's properties, a
//! summary of the folder or of the sessions selected together, or a hint that nothing is.
//!
//! Never a secret: what is saved for a session is said as the kind of sign-in it allows
//! ("Username + Password"), read from [`SavedCredentials`], which holds no secret itself.

use heimdall_app::reachability::Verdict;
use heimdall_app::{
    GatewayBadge, Message as AppMessage, NO_FOLDER, ProfileKind, ProfileSummary, SavedCredentials,
    SessionState, server_text,
};
use heimdall_core::profile::{ProfileId, display_address};
use iced::widget::{
    Column, button, column, container, mouse_area, row, space, table, text, tooltip,
};
use iced::{Alignment, Element, Length, Padding, Theme};

use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Height of the panel before its splitter is ever dragged, as the C# `DefaultHeight`.
pub const DEFAULT_HEIGHT: f32 = 240.0;
/// The lowest it goes, as the C# `MinHeight`: below it the panel shows less than the
/// sign-in line and the address.
pub const MIN_HEIGHT: f32 = 120.0;
/// The highest a kept height is taken, as the C# `MaxHeight`; the sidebar's height limits
/// it further when it is laid out.
pub const MAX_HEIGHT: f32 = 2000.0;
/// What the tree keeps of the sidebar whatever the panel asks for, as the C#
/// `TreeMinHeight`.
pub const TREE_MIN_HEIGHT: f32 = 120.0;
/// Height of the splitter above the panel, as the C# `SplitterHeight`: 4 was hard to catch.
pub const SPLITTER_HEIGHT: f32 = 6.0;

/// Room around the panel's title, as the C# header's `Margin="8,4,4,0"`.
const HEADER_PADDING: Padding = Padding {
    top: 4.0,
    right: 4.0,
    bottom: 0.0,
    left: 8.0,
};
/// Room around what the panel shows, as the C# `ScrollViewer`'s `Padding="8,4,8,8"`.
const CONTENT_PADDING: Padding = Padding {
    top: 4.0,
    right: 8.0,
    bottom: 8.0,
    left: 8.0,
};
/// Room around the close mark, as the C# ghost button's `Padding="6,2"`.
const CLOSE_PADDING: [f32; 2] = [2.0, 6.0];
/// Side of the close mark, as the C# glyph at `FontSizeCaption`.
const CLOSE_SIDE: f32 = font_size::CAPTION;
/// Room under the session's name, as the C# `Margin="0,0,0,6"`.
const NAME_GAP: f32 = 6.0;
/// Room around Connect's and Edit's labels, as the C# `Padding="12,4"`.
const ACTION_PADDING: [f32; 2] = [4.0, 12.0];
/// Room above the address, as the C# `Margin="0,8,0,0"`.
const ENDPOINT_GAP: f32 = 8.0;
/// Room above the card, as the C# `Margin="0,10,0,0"`.
const CARD_GAP: f32 = 10.0;
/// Room inside the card, as the C# `Padding="12,8"`.
const CARD_PADDING: [f32; 2] = [8.0, 12.0];
/// Room between a line's label and its value, as the C# label's `Margin="0,2,12,2"`.
const LABEL_GAP: f32 = 12.0;
/// Room between two lines of the card, as the C# `Margin="0,2"` above and below each.
const LINE_GAP: f32 = 4.0;
/// Room above a summary's card, as the C# `Margin="0,8,0,0"`.
const SUMMARY_CARD_GAP: f32 = 8.0;
/// Room under a summary's title, as the C# `Margin="0,0,0,2"`.
const SUMMARY_TITLE_GAP: f32 = 2.0;
/// What joins the parts of the sign-in line, as the C# `string.Join(" + ", parts)`.
const AUTH_JOINER: &str = " + ";

/// Whether the panel is on screen: asked for, and the sidebar showing the sessions rather
/// than the tools, which have no session to describe, as the C# `IsShown`.
#[must_use]
pub fn is_shown(enabled: bool, tools_tab: bool) -> bool {
    enabled && !tools_tab
}

/// Whether Ctrl+Shift+B shows the panel rather than toggling it, as the C#
/// `ShortcutReveals`: while the Tools tab is up or the sidebar hidden the panel cannot be
/// seen, and a toggle there turned it off unseen.
#[must_use]
pub fn shortcut_reveals(tools_tab: bool, sidebar_hidden: bool) -> bool {
    tools_tab || sidebar_hidden
}

/// A kept or dragged height brought within [`MIN_HEIGHT`] and [`MAX_HEIGHT`]; one that is no
/// number is [`DEFAULT_HEIGHT`], as the C# `ClampHeight`.
#[must_use]
pub fn clamp_height(height: f32) -> f32 {
    if height.is_finite() {
        height.clamp(MIN_HEIGHT, MAX_HEIGHT)
    } else {
        DEFAULT_HEIGHT
    }
}

/// The height kept when the window opens: the one left, else [`DEFAULT_HEIGHT`].
#[must_use]
pub fn kept_height(left: Option<f32>) -> f32 {
    left.map_or(DEFAULT_HEIGHT, clamp_height)
}

/// The panel's height in a sidebar of `available` height, the tree keeping at least
/// [`TREE_MIN_HEIGHT`], as the C# `FitHeight`: a height kept from a taller window would push
/// the tree off a shorter one. Never below [`MIN_HEIGHT`].
#[must_use]
pub fn fit_height(saved: f32, available: f32) -> f32 {
    let height = clamp_height(saved);
    if !available.is_finite() || available <= 0.0 {
        return height;
    }
    let room = available - TREE_MIN_HEIGHT - SPLITTER_HEIGHT;
    height.min(room).max(MIN_HEIGHT)
}

/// The height a drag of the splitter leaves: the one it started from, `start`, taller by
/// how far the pointer went up from `anchor` to `at`, held where [`fit_height`] would hold
/// it in a sidebar of `available` height.
#[must_use]
pub fn dragged_height(start: f32, anchor: f32, at: f32, available: f32) -> f32 {
    fit_height(start + (anchor - at), available)
}

/// What the panel describes, as the C# panel picks it: the sessions selected together, else
/// the session selected, else the folder pressed last, else nothing. A press on a folder
/// unselects the sessions and a press on a session the folder, so whichever was touched last
/// is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// One session.
    Session(ProfileId),
    /// Several sessions selected together.
    Selection(Vec<ProfileId>),
    /// A folder, by its path; [`NO_FOLDER`] for the sessions in none.
    Folder(String),
    /// Nothing selected.
    Nothing,
}

/// What the panel describes, from the tree's `selection` (empty unless two or more), its
/// session `selected` and its folder `folder`.
#[must_use]
pub fn subject(
    selection: Vec<ProfileId>,
    selected: Option<&ProfileId>,
    folder: Option<&str>,
) -> Subject {
    if selection.len() > 1 {
        return Subject::Selection(selection);
    }
    match (selected, folder) {
        (Some(id), _) => Subject::Session(id.clone()),
        (None, Some(folder)) => Subject::Folder(folder.to_owned()),
        (None, None) => Subject::Nothing,
    }
}

/// A way a session signs in, as the C# `BuildAuthSummary` lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthPart {
    /// An account is named.
    Username,
    /// A private key is set.
    SshKey,
    /// A password is saved.
    Password,
    /// Neither a key nor a password: whatever agent runs signs.
    SshAgent,
    /// The account running Heimdall: a `WinRM` profile naming none.
    CurrentUser,
    /// The external credential provider is asked at connect.
    CredentialProvider,
}

impl AuthPart {
    /// Its name, in the window's language.
    fn label(self) -> String {
        match self {
            Self::Username => fl!("ui-properties-auth-username"),
            Self::SshKey => fl!("ui-properties-auth-ssh-key"),
            Self::Password => fl!("ui-properties-auth-password"),
            Self::SshAgent => fl!("ui-properties-auth-ssh-agent"),
            Self::CurrentUser => fl!("ui-properties-auth-current-user"),
            Self::CredentialProvider => fl!("ui-properties-auth-provider"),
        }
    }
}

/// How `profile` signs in, `saved` what is kept for it and `provider` whether the credential
/// provider is asked, as the C# `BuildAuthSummary`: `None` for a protocol with no sign-in to
/// describe (Telnet, a local program, Citrix); empty when nothing is kept, which reads "No
/// saved credentials".
#[must_use]
pub fn auth_parts(
    profile: &ProfileSummary,
    saved: &SavedCredentials,
    provider: bool,
) -> Option<Vec<AuthPart>> {
    let named = profile
        .username
        .as_deref()
        .is_some_and(|user| !user.trim().is_empty());
    let mut parts = Vec::new();
    if named {
        parts.push(AuthPart::Username);
    }
    match profile.kind {
        ProfileKind::Ssh | ProfileKind::Sftp => {
            if saved.key_file.is_some() {
                parts.push(AuthPart::SshKey);
            }
            if saved.password {
                parts.push(AuthPart::Password);
            }
            // With neither, the connection signs through whatever agent runs: "no saved
            // credentials" hid the one method used, as the C# says.
            if saved.key_file.is_none() && !saved.password {
                parts.push(AuthPart::SshAgent);
            }
        }
        // As the C# `WinRmIdentityMode.CurrentUser`: an account named none is this one.
        ProfileKind::WinRm if !named => return Some(vec![AuthPart::CurrentUser]),
        ProfileKind::Rdp | ProfileKind::Ftp | ProfileKind::WinRm => {
            if saved.password {
                parts.push(AuthPart::Password);
            }
        }
        // A VNC server asks a password alone, whatever account a profile names.
        ProfileKind::Vnc => {
            parts.clear();
            if saved.password {
                parts.push(AuthPart::Password);
            }
        }
        // A web console signs in inside the browser: nothing here to describe.
        ProfileKind::Telnet
        | ProfileKind::Local
        | ProfileKind::Citrix
        | ProfileKind::Web
        | ProfileKind::Tool => {
            return None;
        }
    }
    // Asked only for a password not saved, as the C# `GetCredentialTarget` and the connect
    // path, which answers from the saved one first.
    if provider && !saved.password {
        parts.push(AuthPart::CredentialProvider);
    }
    Some(parts)
}

/// The sign-in line, as the C# says it.
fn auth_text(parts: &[AuthPart]) -> String {
    if parts.is_empty() {
        return fl!("ui-properties-auth-none-saved");
    }
    parts
        .iter()
        .map(|part| part.label())
        .collect::<Vec<_>>()
        .join(AUTH_JOINER)
}

/// A line of the session's card, in the C# order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// How it signs in.
    Auth,
    /// Its account.
    Username,
    /// Its private key, by its file's name: SSH and SFTP only.
    SshKey,
    /// The gateway it goes through.
    Gateway,
    /// Its folder.
    Folder,
    /// Where an imported session came from.
    Origin,
    /// Its environment.
    Environment,
    /// Its tags.
    Tags,
    /// Marked as a favorite.
    Favorite,
    /// The latency of the last check that it answers.
    Latency,
}

impl Field {
    /// Its label, in the window's language.
    fn label(self) -> String {
        match self {
            Self::Auth => fl!("ui-properties-auth"),
            Self::Username => fl!("ui-detail-username"),
            Self::SshKey => fl!("ui-properties-ssh-key"),
            Self::Gateway => fl!("ui-detail-gateway"),
            Self::Folder => fl!("ui-detail-folder"),
            Self::Origin => fl!("ui-properties-origin"),
            Self::Environment => fl!("ui-detail-environment"),
            Self::Tags => fl!("ui-detail-tags"),
            Self::Favorite => fl!("ui-detail-favorite"),
            Self::Latency => fl!("ui-properties-latency"),
        }
    }

    /// Whether a click on its value copies it, as the C# copy buttons: neither the sign-in
    /// line nor the origin, which are said, not values; nor the star.
    #[must_use]
    pub fn copies(self) -> bool {
        !matches!(self, Self::Auth | Self::Origin | Self::Favorite)
    }

    /// Whether its value is cut on one line, as the C# `TextTrimming`, rather than wrapped.
    fn one_line(self) -> bool {
        matches!(self, Self::Username | Self::Environment | Self::Latency)
    }
}

/// A line of the session's card: its field, what it says, and whether in the warning colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Which.
    pub field: Field,
    /// What it says; empty for the favorite's star.
    pub value: String,
    /// Said in the warning colour: a gateway that is gone.
    pub warning: bool,
}

impl Line {
    /// A line saying `value` in the window's colour.
    fn plain(field: Field, value: String) -> Self {
        Self {
            field,
            value,
            warning: false,
        }
    }
}

/// What the card of a session is drawn from.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// The session.
    pub profile: &'a ProfileSummary,
    /// What is kept for it, once read.
    pub saved: Option<&'a SavedCredentials>,
    /// The credential provider is asked when it connects.
    pub provider: bool,
    /// What the background check last found of it.
    pub reachability: Option<&'a Verdict>,
    /// The tree's rows show the gateway they go through.
    pub gateway_badge: bool,
}

/// The lines of the session's card, in the C# order, only those with something to say.
/// The C# project and last-opened lines are left out: this application keeps neither.
#[must_use]
pub fn lines(facts: Facts<'_>) -> Vec<Line> {
    let Facts {
        profile,
        saved,
        provider,
        reachability,
        gateway_badge,
    } = facts;
    let mut lines = Vec::new();
    if let Some(parts) = saved.and_then(|saved| auth_parts(profile, saved, provider)) {
        lines.push(Line::plain(Field::Auth, auth_text(&parts)));
    }
    if let Some(user) = profile
        .username
        .as_deref()
        .filter(|user| !user.trim().is_empty())
    {
        lines.push(Line::plain(Field::Username, server_text(user)));
    }
    if matches!(profile.kind, ProfileKind::Ssh | ProfileKind::Sftp)
        && let Some(key) = saved.and_then(|saved| saved.key_file.as_deref())
    {
        lines.push(Line::plain(Field::SshKey, server_text(key)));
    }
    // As the C# `IsGatewayBadgeVisible`: hidden with the tree's badge, but a gateway gone
    // is always said.
    match &profile.gateway {
        Some(GatewayBadge::Via(name)) if gateway_badge => {
            lines.push(Line::plain(Field::Gateway, server_text(name)));
        }
        Some(GatewayBadge::Missing) => lines.push(Line {
            field: Field::Gateway,
            value: fl!("ui-tree-gateway-missing"),
            warning: true,
        }),
        _ => {}
    }
    if let Some(group) = profile
        .group
        .as_deref()
        .filter(|group| !group.trim().is_empty())
    {
        lines.push(Line::plain(Field::Folder, server_text(group)));
    }
    if let Some(origin) = profile.metadata.origin {
        lines.push(Line::plain(
            Field::Origin,
            crate::texts::origin_name(origin),
        ));
    }
    if let Some(environment) = profile.metadata.environment {
        lines.push(Line::plain(
            Field::Environment,
            crate::texts::environment_name(Some(environment)),
        ));
    }
    if !profile.metadata.tags.trim().is_empty() {
        lines.push(Line::plain(
            Field::Tags,
            server_text(&profile.metadata.tags),
        ));
    }
    if profile.favorite {
        lines.push(Line::plain(Field::Favorite, String::new()));
    }
    // Only a check that answered, as the C# `LatencyText`: a figure from before a failure
    // would read as current.
    if let Some(Verdict::Up(millis)) = reachability {
        lines.push(Line::plain(
            Field::Latency,
            fl!("ui-properties-latency-value", millis = (*millis)),
        ));
    }
    lines
}

/// Where `profile` connects, as the C# `Endpoint`: `host:port`; none for a protocol that
/// reaches no server.
#[must_use]
pub fn endpoint(profile: &ProfileSummary) -> Option<String> {
    let (host, port) = profile.endpoint.as_ref()?;
    (!host.trim().is_empty()).then(|| display_address(&server_text(host), *port))
}

/// A folder's or a selection's summary, as the C# `PropertiesSummaryViewModel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// The folder's name, or the selection's size said.
    pub title: String,
    /// The folder's full path; none for a selection or the sessions in no folder.
    pub path: Option<String>,
    /// How many sessions it holds.
    pub sessions: usize,
    /// How many of them have a session connected.
    pub connected: usize,
    /// How many use each protocol, the most used first, ties by name.
    pub protocols: Vec<(ProfileKind, usize)>,
}

/// The summary of `profiles`, each with the state of its session; one listed twice counts
/// once.
#[must_use]
pub fn summary(
    title: String,
    path: Option<String>,
    profiles: impl IntoIterator<Item = (ProfileSummary, Option<SessionState>)>,
) -> Summary {
    let mut seen: Vec<ProfileId> = Vec::new();
    let mut connected = 0;
    let mut protocols: Vec<(ProfileKind, usize)> = Vec::new();
    for (profile, state) in profiles {
        if seen.contains(&profile.id) {
            continue;
        }
        seen.push(profile.id.clone());
        if state == Some(SessionState::Connected) {
            connected += 1;
        }
        match protocols.iter_mut().find(|(kind, _)| *kind == profile.kind) {
            Some((_, count)) => *count += 1,
            None => protocols.push((profile.kind, 1)),
        }
    }
    // Ties by name, so the order does not move when the set is drawn again.
    protocols.sort_by(|(a, a_count), (b, b_count)| {
        b_count.cmp(a_count).then_with(|| a.label().cmp(b.label()))
    });
    Summary {
        title,
        path,
        sessions: seen.len(),
        connected,
        protocols,
    }
}

/// The title and path of folder `path`'s summary: its own name and its full path; "(No
/// Folder)" and no path for the sessions in none.
#[must_use]
pub fn folder_title(path: &str) -> (String, Option<String>) {
    if path == NO_FOLDER {
        return (fl!("ui-sidebar-group-none"), None);
    }
    (
        server_text(&heimdall_core::folder::name(path)),
        Some(server_text(&heimdall_core::folder::normal(path))),
    )
}

/// What the panel shows, read off the application by the window.
#[derive(Debug, Clone)]
pub enum Shown {
    /// One session's properties.
    Session {
        /// The session.
        profile: ProfileSummary,
        /// What is kept for it, once read.
        saved: Option<SavedCredentials>,
        /// The credential provider is asked when it connects.
        provider: bool,
        /// What the background check last found of it.
        reachability: Option<Verdict>,
        /// The tree's rows show the gateway they go through.
        gateway_badge: bool,
        /// The state of its session's tab, when one is open.
        state: Option<SessionState>,
        /// It can be edited.
        editable: bool,
    },
    /// A folder's or a selection's summary.
    Summary(Summary),
    /// Nothing selected.
    Nothing,
}

/// The splitter above the panel, dragged to set its height, as the C# `GridSplitter`: the
/// card colour at rest, the accent under the pointer. A press says the height the panel has
/// and the height of the room it shares with the tree.
pub fn splitter<'a>(height: f32, available: f32) -> Element<'a, Message> {
    let handle = |lit: bool| {
        container(space().height(SPLITTER_HEIGHT))
            .width(Length::Fill)
            .style(move |theme: &Theme| container::Style {
                background: Some(if lit {
                    theme.extended_palette().primary.base.color.into()
                } else {
                    styles::card_color(theme).into()
                }),
                ..container::Style::default()
            })
    };
    mouse_area(iced::widget::hover(handle(false), handle(true)))
        .on_press(Message::PropertiesDragStart { height, available })
        .interaction(iced::mouse::Interaction::ResizingVertically)
        .into()
}

/// The panel, `height` high: its title and its close mark, then what it shows, scrolled.
pub fn view<'a>(shown: Shown, height: f32) -> Element<'a, Message> {
    let close = styles::tip(
        button(icons::icon(Icon::Close, Tint::Secondary, CLOSE_SIDE))
            .style(styles::subtle)
            .padding(CLOSE_PADDING)
            .on_press(Message::App(AppMessage::Filter(
                heimdall_app::FilterMessage::PropertiesPanel,
            ))),
        text(fl!("ui-properties-hide-tooltip")).size(font_size::CAPTION),
        tooltip::Position::Top,
    )
    .padding(styles::TOOLTIP_PADDING)
    .style(styles::tooltip);
    let header = container(
        row![
            text(fl!("ui-properties-title"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD)
                .style(text::secondary)
                .width(Length::Fill),
            close,
        ]
        .align_y(Alignment::Center),
    )
    .padding(HEADER_PADDING);
    let content: Element<'a, Message> = match shown {
        Shown::Session {
            profile,
            saved,
            provider,
            reachability,
            gateway_badge,
            state,
            editable,
        } => session(
            Facts {
                profile: &profile,
                saved: saved.as_ref(),
                provider,
                reachability: reachability.as_ref(),
                gateway_badge,
            },
            state,
            editable,
        ),
        Shown::Summary(summary) => summary_view(summary),
        Shown::Nothing => text(fl!("ui-properties-empty"))
            .size(font_size::CAPTION)
            .style(text::secondary)
            .into(),
    };
    column![
        styles::edge_line(),
        header,
        styles::scroll(
            container(content)
                .padding(CONTENT_PADDING)
                .width(Length::Fill)
        )
        .height(Length::Fill),
    ]
    .height(height)
    .width(Length::Fill)
    .into()
}

/// One session: its name, Connect and Edit, its protocol and state, its address, then its
/// card, as the C# panel and its `SessionPropertiesView`.
fn session<'a>(
    facts: Facts<'_>,
    state: Option<SessionState>,
    editable: bool,
) -> Element<'a, Message> {
    let profile = facts.profile;
    let name = server_text(&profile.name);
    let id = profile.id.clone();
    let mut panel = Column::new()
        .push(
            styles::tip(
                crate::ellipsis::text(name.clone())
                    .size(font_size::BODY_LARGE)
                    .font(styles::SEMIBOLD)
                    .width(Length::Fill),
                text(name).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .padding(styles::TOOLTIP_PADDING)
            .style(styles::tooltip),
        )
        .push(space().height(NAME_GAP))
        // The detail pane has these too, and hides as soon as a tab is open: the panel's
        // own way to act on what it shows, as the C# says.
        .push(
            row![
                button(text(fl!("ui-detail-connect")).size(font_size::CAPTION))
                    .style(styles::primary)
                    .padding(ACTION_PADDING)
                    .on_press(Message::App(AppMessage::ConnectProfile(id.clone()))),
                button(text(fl!("ui-detail-edit")).size(font_size::CAPTION))
                    .style(styles::secondary)
                    .padding(ACTION_PADDING)
                    .on_press_maybe(editable.then(|| Message::App(AppMessage::EditProfile(id)))),
            ]
            .spacing(spacing::SM)
            .wrap(),
        )
        .push(space().height(spacing::SM))
        .push(
            row![
                crate::detail_view::pill(profile.kind),
                crate::detail_view::status(state)
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
        );
    if let Some(endpoint) = endpoint(profile) {
        panel = panel.push(space().height(ENDPOINT_GAP)).push(copy_value(
            text_value(endpoint.clone(), true)
                .size(font_size::BODY_LARGE)
                .style(text::secondary),
            endpoint,
        ));
    }
    let lines = lines(facts);
    if !lines.is_empty() {
        panel = panel.push(space().height(CARD_GAP)).push(card(&lines));
    }
    panel.width(Length::Fill).into()
}

/// What a value says, cut on one line when `one_line`, else wrapped.
fn text_value<'a>(said: String, one_line: bool) -> ValueText<'a> {
    if one_line {
        ValueText::Cut(crate::ellipsis::text(said).width(Length::Fill))
    } else {
        ValueText::Wrapped(text(said))
    }
}

/// A value's text: cut on one line, or wrapped.
enum ValueText<'a> {
    /// Cut with an ellipsis where its room ends.
    Cut(crate::ellipsis::Ellipsis<'a>),
    /// Wrapped over as many lines as it takes.
    Wrapped(iced::widget::Text<'a>),
}

impl ValueText<'_> {
    fn size(self, size: f32) -> Self {
        match self {
            Self::Cut(said) => Self::Cut(said.size(size)),
            Self::Wrapped(said) => Self::Wrapped(said.size(size)),
        }
    }

    fn style(self, style: fn(&Theme) -> text::Style) -> Self {
        match self {
            Self::Cut(said) => Self::Cut(said.style(style)),
            Self::Wrapped(said) => Self::Wrapped(said.style(style)),
        }
    }
}

impl<'a> From<ValueText<'a>> for Element<'a, Message> {
    fn from(value: ValueText<'a>) -> Self {
        match value {
            ValueText::Cut(said) => said.into(),
            ValueText::Wrapped(said) => said.into(),
        }
    }
}

/// `shown`, a value that copies `value` at a click, as the C# `SpvCopyValueStyle`: the
/// text it was, in the accent under the pointer, its whole value and how to copy it said on
/// hover, for a value cut at the sidebar's narrowest.
fn copy_value<'a>(shown: impl Into<Element<'a, Message>>, value: String) -> Element<'a, Message> {
    styles::tip(
        button(shown)
            .style(copy_style)
            .padding(0.0)
            .width(Length::Fill)
            .on_press(Message::App(AppMessage::CopyValue(value.clone()))),
        column![
            text(value).size(font_size::CAPTION),
            text(fl!("ui-properties-copy-hint"))
                .size(font_size::CAPTION)
                .style(text::secondary),
        ],
        tooltip::Position::Bottom,
    )
    .padding(styles::TOOLTIP_PADDING)
    .style(styles::tooltip)
    .into()
}

/// A value that copies itself: nothing behind it, in the accent under the pointer.
fn copy_style(theme: &Theme, status: button::Status) -> button::Style {
    let style = styles::bare(theme, status);
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            text_color: theme.extended_palette().primary.base.color,
            ..style
        },
        button::Status::Active | button::Status::Disabled => style,
    }
}

/// The card of `lines`: labels in a column as wide as the widest, each value beside its
/// own, as the C# grid's rows: a value wrapped over two lines pushes the next line down.
fn card<'a>(lines: &[Line]) -> Element<'a, Message> {
    let labels = table::column(space(), |line: Line| {
        container(
            text(line.field.label())
                .size(font_size::CAPTION)
                .style(text::secondary),
        )
        .padding(Padding {
            right: LABEL_GAP,
            ..Padding::ZERO
        })
    });
    // Each value as wide as its column: a text wrapped is measured in it.
    let values = table::column(space(), |line: Line| {
        container(line_value(&line)).width(Length::Fill)
    })
    .width(Length::Fill);
    container(
        table([labels, values], lines.iter().cloned())
            .padding_x(0.0)
            .padding_y(LINE_GAP / 2.0)
            .separator(0.0),
    )
    .padding(CARD_PADDING)
    .width(Length::Fill)
    .style(crate::detail_view::card_style)
    .into()
}

/// What `line` says: its text, a copy of it at a click when it is a value, or the star.
fn line_value<'a>(line: &Line) -> Element<'a, Message> {
    if line.field == Field::Favorite {
        return icons::icon(Icon::FavoriteStarFill, Tint::Warning, font_size::CAPTION);
    }
    let said = text_value(line.value.clone(), line.field.one_line())
        .size(font_size::CAPTION)
        .style(if line.warning {
            text::warning
        } else {
            text::default
        });
    // A gateway gone says so in the window's words: nothing to paste elsewhere.
    if line.field.copies() && !line.warning {
        copy_value(said, line.value.clone())
    } else {
        said.into()
    }
}

/// A folder's or a selection's summary: its title, its path, then its counts in a card, as
/// the C# `Mw_PropertiesPanelSummary`.
fn summary_view<'a>(summary: Summary) -> Element<'a, Message> {
    let mut panel = Column::new().push(
        crate::ellipsis::text(summary.title)
            .size(font_size::BODY_LARGE)
            .font(styles::SEMIBOLD)
            .width(Length::Fill),
    );
    if let Some(path) = summary.path {
        panel = panel
            .push(space().height(SUMMARY_TITLE_GAP))
            .push(text(path).size(font_size::CAPTION).style(text::secondary));
    }
    let mut labels = Column::new().spacing(LINE_GAP).push(
        text(fl!("ui-properties-summary-sessions"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    );
    let mut values = Column::new()
        .spacing(LINE_GAP)
        .width(Length::Fill)
        .push(text(summary.sessions.to_string()).size(font_size::CAPTION));
    labels = labels.push(
        text(fl!("ui-properties-summary-connected"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    );
    values = values.push(text(summary.connected.to_string()).size(font_size::CAPTION));
    // One line per protocol, the most used first.
    for (kind, count) in summary.protocols {
        labels = labels.push(
            text(kind.label())
                .size(font_size::CAPTION)
                .style(text::secondary),
        );
        values = values.push(text(count.to_string()).size(font_size::CAPTION));
    }
    panel
        .push(space().height(SUMMARY_CARD_GAP))
        .push(
            container(row![labels, values].spacing(LABEL_GAP))
                .padding(CARD_PADDING)
                .width(Length::Fill)
                .style(crate::detail_view::card_style),
        )
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use heimdall_core::metadata::{Environment, ProfileMetadata, ProfileOrigin};

    use super::*;

    fn profile(kind: ProfileKind) -> ProfileSummary {
        ProfileSummary {
            id: ProfileId::new("p"),
            name: "web".to_owned(),
            group: None,
            kind,
            endpoint: Some(("web.lab".to_owned(), 22)),
            username: Some("admin".to_owned()),
            gateway: None,
            favorite: false,
            metadata: ProfileMetadata::default(),
        }
    }

    fn saved(password: bool, key: Option<&str>) -> SavedCredentials {
        SavedCredentials {
            password,
            key_file: key.map(str::to_owned),
            passphrase: key.is_some() && password,
        }
    }

    fn facts<'a>(profile: &'a ProfileSummary, saved: Option<&'a SavedCredentials>) -> Facts<'a> {
        Facts {
            profile,
            saved,
            provider: false,
            reachability: None,
            gateway_badge: true,
        }
    }

    fn fields(lines: &[Line]) -> Vec<Field> {
        lines.iter().map(|line| line.field).collect()
    }

    #[test]
    fn the_panel_is_shown_when_asked_and_the_sessions_tab_is_up() {
        assert!(is_shown(true, false));
        assert!(!is_shown(true, true), "the tools have no session");
        assert!(!is_shown(false, false));
        assert!(shortcut_reveals(true, false));
        assert!(shortcut_reveals(false, true));
        assert!(!shortcut_reveals(false, false), "seen: the key toggles it");
    }

    #[test]
    fn a_height_is_held_in_range_and_leaves_the_tree_its_room() {
        assert!((clamp_height(10.0) - MIN_HEIGHT).abs() < f32::EPSILON);
        assert!((clamp_height(5000.0) - MAX_HEIGHT).abs() < f32::EPSILON);
        assert!((clamp_height(f32::NAN) - DEFAULT_HEIGHT).abs() < f32::EPSILON);
        assert!((kept_height(None) - DEFAULT_HEIGHT).abs() < f32::EPSILON);
        assert!((kept_height(Some(300.0)) - 300.0).abs() < f32::EPSILON);
        assert!(
            (fit_height(300.0, 0.0) - 300.0).abs() < f32::EPSILON,
            "no room known yet"
        );
        let room = 500.0 - TREE_MIN_HEIGHT - SPLITTER_HEIGHT;
        assert!(
            (fit_height(900.0, 500.0) - room).abs() < f32::EPSILON,
            "kept from a taller window"
        );
        assert!(
            (fit_height(900.0, 200.0) - MIN_HEIGHT).abs() < f32::EPSILON,
            "never below its least"
        );
        assert!(
            (dragged_height(240.0, 400.0, 380.0, 800.0) - 260.0).abs() < f32::EPSILON,
            "up 20: 20 taller"
        );
        assert!(
            (dragged_height(240.0, 400.0, 0.0, 500.0) - room).abs() < f32::EPSILON,
            "never over the tree's room"
        );
    }

    #[test]
    fn the_panel_follows_the_selection_then_the_session_then_the_folder() {
        let (a, b) = (ProfileId::new("a"), ProfileId::new("b"));
        assert_eq!(
            subject(vec![a.clone(), b.clone()], Some(&b), Some("Prod")),
            Subject::Selection(vec![a.clone(), b])
        );
        assert_eq!(
            subject(Vec::new(), Some(&a), Some("Prod")),
            Subject::Session(a)
        );
        assert_eq!(
            subject(Vec::new(), None, Some("Prod")),
            Subject::Folder("Prod".to_owned())
        );
        assert_eq!(subject(Vec::new(), None, None), Subject::Nothing);
    }

    #[test]
    fn each_protocol_says_how_it_signs_in_as_the_csharp_summary() {
        use AuthPart::{CredentialProvider, CurrentUser, Password, SshAgent, SshKey, Username};
        let none = saved(false, None);
        let both = saved(true, Some("id_ed25519"));
        let case = |kind: ProfileKind, saved: &SavedCredentials, provider: bool| {
            auth_parts(&profile(kind), saved, provider)
        };
        assert_eq!(
            case(ProfileKind::Ssh, &none, false),
            Some(vec![Username, SshAgent])
        );
        assert_eq!(
            case(ProfileKind::Sftp, &both, true),
            Some(vec![Username, SshKey, Password]),
            "a password saved: the provider is not asked"
        );
        assert_eq!(
            case(ProfileKind::Sftp, &saved(false, Some("id_ed25519")), true),
            Some(vec![Username, SshKey, CredentialProvider])
        );
        assert_eq!(
            case(ProfileKind::Rdp, &both, false),
            Some(vec![Username, Password])
        );
        assert_eq!(case(ProfileKind::Ftp, &none, false), Some(vec![Username]));
        assert_eq!(case(ProfileKind::Vnc, &both, false), Some(vec![Password]));
        assert_eq!(
            case(ProfileKind::Rdp, &none, true),
            Some(vec![Username, CredentialProvider])
        );
        assert_eq!(
            case(ProfileKind::WinRm, &both, false),
            Some(vec![Username, Password])
        );
        let current = ProfileSummary {
            username: None,
            ..profile(ProfileKind::WinRm)
        };
        assert_eq!(
            auth_parts(&current, &both, true),
            Some(vec![CurrentUser]),
            "the account running Heimdall, nothing else asked"
        );
        let vnc = auth_parts(&profile(ProfileKind::Vnc), &none, false).expect("vnc");
        assert!(vnc.is_empty());
        assert_eq!(auth_text(&vnc), "No saved credentials");
        for kind in [ProfileKind::Telnet, ProfileKind::Local, ProfileKind::Citrix] {
            assert_eq!(case(kind, &both, true), None, "{kind:?}: no sign-in");
        }
        assert_eq!(
            auth_text(&[Username, SshKey, Password]),
            "Username + SSH key + Password"
        );
    }

    #[test]
    fn the_card_lists_its_lines_in_the_csharp_order_and_only_those_known() {
        let mut full = profile(ProfileKind::Ssh);
        full.group = Some("Prod/Web".to_owned());
        full.gateway = Some(GatewayBadge::Via("edge".to_owned()));
        full.favorite = true;
        full.metadata = ProfileMetadata {
            environment: Some(Environment::Production),
            tags: "linux web".to_owned(),
            origin: Some(ProfileOrigin::Putty),
            ..ProfileMetadata::default()
        };
        let kept = saved(true, Some("id_ed25519"));
        let up = Verdict::Up(12);
        let lines = lines(Facts {
            reachability: Some(&up),
            ..facts(&full, Some(&kept))
        });
        assert_eq!(
            fields(&lines),
            [
                Field::Auth,
                Field::Username,
                Field::SshKey,
                Field::Gateway,
                Field::Folder,
                Field::Origin,
                Field::Environment,
                Field::Tags,
                Field::Favorite,
                Field::Latency,
            ]
        );
        assert_eq!(lines[2].value, "id_ed25519", "the key by its file's name");
        assert_eq!(lines[9].value, "12 ms");

        let bare = ProfileSummary {
            username: None,
            ..profile(ProfileKind::Telnet)
        };
        assert!(
            super::lines(facts(&bare, Some(&kept))).is_empty(),
            "a Telnet session with nothing more says nothing more"
        );
        let rdp = profile(ProfileKind::Rdp);
        assert!(
            !fields(&super::lines(facts(&rdp, Some(&kept)))).contains(&Field::SshKey),
            "a key is SSH's and SFTP's only"
        );
        assert!(
            !fields(&super::lines(facts(&rdp, None))).contains(&Field::Auth),
            "not said before it is read"
        );
    }

    #[test]
    fn a_gateway_follows_the_tree_badge_but_a_gateway_gone_is_always_said() {
        let mut via = profile(ProfileKind::Ssh);
        via.gateway = Some(GatewayBadge::Via("edge".to_owned()));
        let hidden = Facts {
            gateway_badge: false,
            ..facts(&via, None)
        };
        assert!(!fields(&lines(hidden)).contains(&Field::Gateway));
        let mut gone = via.clone();
        gone.gateway = Some(GatewayBadge::Missing);
        let said = lines(Facts {
            gateway_badge: false,
            ..facts(&gone, None)
        });
        assert_eq!(said[1].field, Field::Gateway);
        assert!(said[1].warning);
    }

    #[test]
    fn a_latency_is_said_only_from_a_check_that_answered() {
        let ssh = profile(ProfileKind::Ssh);
        for verdict in [
            Verdict::Checking,
            Verdict::Down(heimdall_app::reachability::DownReason::Timeout),
        ] {
            let found = lines(Facts {
                reachability: Some(&verdict),
                ..facts(&ssh, None)
            });
            assert!(!fields(&found).contains(&Field::Latency), "{verdict:?}");
        }
    }

    #[test]
    fn no_line_ever_says_a_secret_only_whether_one_is_saved() {
        // What is kept is said by kind: the values say names, never a password, a
        // passphrase or a PIN, which the panel is never given.
        let ssh = profile(ProfileKind::Ssh);
        let kept = saved(true, Some("id_ed25519"));
        let said: Vec<String> = lines(facts(&ssh, Some(&kept)))
            .into_iter()
            .map(|line| line.value)
            .collect();
        assert_eq!(said[0], "Username + SSH key + Password");
        assert!(
            said.iter().all(|value| !value.contains("passphrase")),
            "{said:?}"
        );
        assert!(!Field::Auth.copies(), "the sign-in line copies nothing");
        assert!(Field::Username.copies());
    }

    #[test]
    fn an_endpoint_is_host_and_port_with_an_ipv6_address_in_brackets() {
        assert_eq!(
            endpoint(&profile(ProfileKind::Ssh)).as_deref(),
            Some("web.lab:22")
        );
        let six = ProfileSummary {
            endpoint: Some(("fe80::1".to_owned(), 3389)),
            ..profile(ProfileKind::Rdp)
        };
        assert_eq!(endpoint(&six).as_deref(), Some("[fe80::1]:3389"));
        let local = ProfileSummary {
            endpoint: None,
            ..profile(ProfileKind::Local)
        };
        assert_eq!(endpoint(&local), None);
    }

    #[test]
    fn a_summary_counts_each_session_once_and_its_protocols_most_used_first() {
        let with = |id: &str, kind: ProfileKind| ProfileSummary {
            id: ProfileId::new(id),
            ..profile(kind)
        };
        let summary = summary(
            "Prod".to_owned(),
            Some("Prod".to_owned()),
            [
                (with("a", ProfileKind::Rdp), None),
                (with("b", ProfileKind::Ssh), Some(SessionState::Connected)),
                (with("c", ProfileKind::Ssh), Some(SessionState::Failed)),
                (with("b", ProfileKind::Ssh), Some(SessionState::Connected)),
                (with("d", ProfileKind::Vnc), None),
            ],
        );
        assert_eq!(summary.sessions, 4, "b listed twice counts once");
        assert_eq!(summary.connected, 1);
        assert_eq!(
            summary.protocols,
            [
                (ProfileKind::Ssh, 2),
                (ProfileKind::Rdp, 1),
                (ProfileKind::Vnc, 1)
            ],
            "the most used first, ties by name"
        );
        assert_eq!(
            folder_title("Prod/Web"),
            ("Web".to_owned(), Some("Prod/Web".to_owned()))
        );
        assert_eq!(folder_title(NO_FOLDER), ("(No Folder)".to_owned(), None));
    }
}
