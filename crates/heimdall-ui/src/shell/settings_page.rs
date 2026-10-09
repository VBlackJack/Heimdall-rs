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

//! The Settings page, drawn from the rows of [`crate::settings_rows`]: a tab's cards, or,
//! while the search holds a word, the rows it finds under their tab and card; each setting
//! that differs from its default marked "Modified" with its "Reset"; and the security
//! overview at the top of the Security tab, as the C# Settings tab has them.
//!
//! The C# search walks the panel, Enter going from one match to the next; here the rows
//! found are shown together instead, which a list of rows allows, so Enter has nothing to
//! walk. Every choice applies at once, so the C# Ctrl+S, which saves the panel's pending
//! edits, has nothing to save.

use std::path::PathBuf;

use heimdall_app::credential_guard::{self, CHECK_TIME_LIMIT};
use heimdall_app::profile_draft::ProfileField;
use heimdall_app::update_check::{Failure, ReleaseTag, minutes_to_wait};
use heimdall_app::windows_hello;
use heimdall_app::{
    Dialog, Effect, Message as AppMessage, PinMessage, ProviderMessage, UpdateMessage,
    UpdateStatus, VaultHelloMessage, VaultHelloStatus, VaultStatus, search_folded,
};
use heimdall_core::profile::{RdpDefaults, RdpMode, SshMode};
use heimdall_core::settings::{
    Accent, AgentPreference, AppTheme, ColorScheme, CtrlKTerminal, CtrlVPaste, ExecutionPolicy,
    Language, MAX_SESSIONS_MAX, RDP_AUTO_RECONNECT_ATTEMPTS_MAX, RDP_AUTO_RECONNECT_ATTEMPTS_MIN,
    SSH_AUTO_RECONNECT_ATTEMPTS_MAX, SSH_AUTO_RECONNECT_ATTEMPTS_MIN, Settings, settings_path,
};
use iced::widget::scrollable::RelativeOffset;
use iced::widget::{
    Column, button, checkbox, column, container, operation, pick_list, row, text, text_input,
    tooltip,
};
use iced::{Element, Length, Task, Theme};

use super::{
    AgentChoice, CONNECT_TIMEOUTS, CtrlKChoice, CtrlVChoice, FONT_SIZE_FIELD_WIDTH, FontChoice,
    LanguageChoice, Message, PolicyChoice, SETTINGS_WIDTH, SchemeChoice, SessionField,
    SessionsChoice, SettingsMessage, SettingsTab, Shell, TimeoutChoice, settings_tabs,
};
use crate::browse::{BrowseTarget, browse_button};
use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
use crate::search_keys::SearchKeys;
use crate::settings_rows::{
    PostureKey, PostureLine, PostureState, SettingRow, SettingsCard, posture,
};
use crate::styles;
use crate::themes::{AccentChoice, ThemeChoice};
use crate::tokens::{BORDER_WIDTH, font_size, radius, spacing};

/// Width of the search box, as the C# one.
const SEARCH_WIDTH: f32 = 220.0;

/// Side of the dot beside "Modified": decoration, the word beside it being the signal, as
/// the C# marker's ellipse.
const MODIFIED_DOT_SIDE: f32 = 6.0;

/// Side of the glyph before "Reset", as the C#'s at `FontSizeSmallCaption`.
const RESET_GLYPH_SIDE: f32 = 11.0;

/// Room around "Reset", above and below then beside, as the C# marker's button.
const RESET_PADDING: [f32; 2] = [1.0, 6.0];

/// Space inside the "Modified" badge, above and below then beside, as the C# badge's.
const BADGE_PADDING: [f32; 2] = [0.0, 6.0];

/// Width of the outline of the row a "Go to setting" showed.
const HIGHLIGHT_EDGE: f32 = 2.0;

/// The mark of a line of the security overview that is as it should be, as the C# check.
const POSTURE_SAFE_MARK: &str = "\u{2713}";

/// The mark of a line that needs attention, as the C# warning sign.
const POSTURE_RISKY_MARK: &str = "!";

/// Between the items of a default made of several, as the C# words a list.
const DEFAULT_LIST_SEPARATOR: &str = ", ";

/// The version of this build's package, shown when it is no release.
const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A default SSH mode in the Settings page's list, named as the C# Settings tab names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DefaultSshModeChoice(SshMode);

impl std::fmt::Display for DefaultSshModeChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&ssh_mode_name(self.0))
    }
}

/// The name of `mode`, as the C# Settings tab and its "Apply to all" question say it.
pub(super) fn ssh_mode_name(mode: SshMode) -> String {
    match mode {
        SshMode::Embedded => fl!("ui-settings-ssh-default-mode-embedded"),
        SshMode::External => fl!("ui-settings-ssh-default-mode-external"),
    }
}

/// A default RDP mode in the Settings page's list, named as the C# RDP tab names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DefaultRdpModeChoice(RdpMode);

impl std::fmt::Display for DefaultRdpModeChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&rdp_mode_name(self.0))
    }
}

/// The name of `mode`, as the C# RDP tab and its "Apply to all" question say it
/// (`SettingsViewModel.cs:1647-1650`).
pub(super) fn rdp_mode_name(mode: RdpMode) -> String {
    match mode {
        RdpMode::Embedded => fl!("ui-settings-rdp-default-mode-embedded"),
        RdpMode::External => fl!("ui-settings-rdp-default-mode-external"),
    }
}

/// Widget identifier of the Settings page's search box.
#[must_use]
pub fn search_field_id() -> iced::widget::Id {
    iced::widget::Id::new("settings-search")
}

/// Widget identifier of the Settings page's scrolled content.
fn scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("settings-page")
}

/// A list of `options` with `selected` chosen, in the application's list style.
fn choices<'a, T>(
    options: Vec<T>,
    selected: Option<T>,
    on_select: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message>
where
    T: ToString + PartialEq + Clone + 'a,
{
    pick_list(options, selected, on_select)
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .into()
}

/// `label` above `control`, as the C# Settings tab lays out its lists and fields: a
/// caption in the secondary text.
fn labelled<'a>(label: String, control: impl Into<Element<'a, Message>>) -> Column<'a, Message> {
    column![
        text(label).size(font_size::CAPTION).style(text::secondary),
        control.into()
    ]
    .spacing(spacing::XS)
}

/// `control` with `unit` after it, as the C# writes a number's unit beside its box.
fn with_unit<'a>(
    control: impl Into<Element<'a, Message>>,
    unit: String,
) -> iced::widget::Row<'a, Message> {
    row![control.into(), text(unit)]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center)
}

/// A line under the page's bar, as the C# bar's lower edge.
fn divider<'a>() -> Element<'a, Message> {
    container(iced::widget::space())
        .width(Length::Fill)
        .height(BORDER_WIDTH)
        .style(styles::divider)
        .into()
}

/// `message` as the window sends it to the application.
fn send(message: SettingsMessage) -> Message {
    Message::App(AppMessage::Settings(message))
}

/// The heading over `card`, when it has one of its own.
fn card_heading(card: SettingsCard) -> Option<String> {
    Some(match card {
        SettingsCard::Appearance => fl!("ui-settings-appearance"),
        SettingsCard::Behavior => fl!("ui-settings-behavior"),
        SettingsCard::Updates => fl!("ui-settings-updates"),
        SettingsCard::Reachability => fl!("ui-settings-reachability"),
        SettingsCard::Terminal => fl!("ui-settings-terminal"),
        SettingsCard::SessionLogging => fl!("ui-settings-session-logging"),
        SettingsCard::Macros => fl!("ui-macros-menu"),
        SettingsCard::SshReconnect => fl!("ui-settings-ssh-auto-reconnect"),
        SettingsCard::SshSession => fl!("ui-settings-ssh-session"),
        SettingsCard::Sftp => fl!("ui-settings-sftp"),
        SettingsCard::X11 => fl!("ui-settings-x11"),
        SettingsCard::ExternalEditor => fl!("ui-settings-external-editor"),
        SettingsCard::RdpDefaults => fl!("ui-settings-rdp-defaults"),
        SettingsCard::ConnectionChecks => fl!("ui-settings-connection-checks"),
        _ => return None,
    })
}

/// What `card` says of itself at its top, when it says something.
fn card_description(card: SettingsCard) -> Option<String> {
    (card == SettingsCard::SshReconnect).then(|| fl!("ui-settings-ssh-auto-reconnect-description"))
}

/// Whether `card` is drawn in a frame of its own; a list or card drawn elsewhere has its own.
fn card_framed(card: SettingsCard) -> bool {
    !matches!(
        card,
        SettingsCard::Macros
            | SettingsCard::SshTrusted
            | SettingsCard::RdpReset
            | SettingsCard::RdpTrusted
            | SettingsCard::Gateways
            | SettingsCard::Provider
    )
}

/// The name of `row`, as the page shows it.
fn row_label(row: SettingRow) -> String {
    if let Some(field) = row.session_field() {
        return field.label();
    }
    match row {
        SettingRow::Language => fl!("ui-settings-language"),
        SettingRow::Theme => fl!("ui-settings-theme"),
        SettingRow::Accent => fl!("ui-settings-accent"),
        SettingRow::CollapseTunnelsPanel => fl!("ui-settings-collapse-tunnels-panel"),
        SettingRow::PreventSleep => fl!("ui-settings-prevent-sleep"),
        SettingRow::MaxSessions => fl!("ui-settings-max-sessions"),
        SettingRow::UpdateChecks => fl!("ui-settings-updates-enabled"),
        SettingRow::UpdateVersion => fl!("ui-settings-updates-current-version"),
        SettingRow::Reachability => fl!("ui-settings-reachability-enabled"),
        SettingRow::FontSize => fl!("ui-settings-font-size"),
        SettingRow::FontFamily => fl!("ui-settings-font-family"),
        SettingRow::ColorScheme => fl!("ui-settings-color-scheme"),
        SettingRow::CtrlVPaste => fl!("ui-settings-ctrl-v"),
        SettingRow::CtrlKTerminal => fl!("ui-settings-ctrl-k"),
        SettingRow::PowerShellPolicy => fl!("ui-settings-powershell-policy"),
        SettingRow::SessionLogging => fl!("ui-settings-session-logging-record"),
        SettingRow::SessionLogDirectory => fl!("ui-settings-session-log-directory"),
        SettingRow::Macros => fl!("ui-macros-menu"),
        SettingRow::SshAutoReconnect => fl!("ui-settings-ssh-auto-reconnect-enable"),
        SettingRow::SshAutoReconnectAttempts => fl!("ui-settings-ssh-auto-reconnect-attempts"),
        SettingRow::SshAgentPreference => fl!("ui-settings-ssh-agent-preference"),
        SettingRow::SftpBrowser => fl!("ui-settings-sftp-browser-enabled"),
        SettingRow::SftpAutoOpen => fl!("ui-settings-sftp-auto-open"),
        SettingRow::SftpFollow => fl!("ui-settings-sftp-follow"),
        SettingRow::DockLocalBrowser => fl!("ui-settings-dock-local-browser"),
        SettingRow::LocalFollow => fl!("ui-settings-local-follow"),
        SettingRow::ExternalEditor => fl!("ui-settings-external-editor-path"),
        SettingRow::PuttyPath => fl!("ui-settings-putty-path"),
        SettingRow::SshDefaultMode => fl!("ui-settings-ssh-default-mode"),
        SettingRow::X11ServerPath => fl!("ui-settings-x11-server-path"),
        SettingRow::X11AutoStart => fl!("ui-settings-x11-auto-start"),
        SettingRow::HostKeys => fl!("ui-trusted-host-keys-title"),
        SettingRow::FtpsCertificates => fl!("ui-trusted-ftps-certificates-title"),
        SettingRow::VncCertificates => fl!("ui-trusted-vnc-certificates-title"),
        SettingRow::RdpDefaultMode => fl!("ui-settings-rdp-default-mode"),
        SettingRow::RdpDefaults => fl!("ui-settings-rdp-defaults"),
        SettingRow::RdpAutoReconnectAttempts => fl!("ui-settings-rdp-auto-reconnect-attempts"),
        SettingRow::RdpConnectTimeout => fl!("ui-settings-rdp-connect-timeout"),
        SettingRow::RdpResolutionPresets => fl!("ui-settings-rdp-resolution-presets"),
        SettingRow::RdpResetAll => fl!("ui-settings-rdp-reset-defaults"),
        SettingRow::Certificates => fl!("ui-trusted-certificates-title"),
        SettingRow::Gateways => fl!("ui-gateways-title"),
        SettingRow::Pin => fl!("ui-settings-pin-title"),
        SettingRow::Vault => fl!("ui-settings-vault-title"),
        SettingRow::DisconnectOnLock => fl!("ui-settings-disconnect-on-lock"),
        SettingRow::Provider => fl!("ui-settings-provider-title"),
        SettingRow::RequireCredentialGuard => fl!("ui-settings-require-credential-guard"),
        SettingRow::RequireWindowsHello => fl!("ui-settings-require-windows-hello"),
        // Numbers, named above.
        _ => String::new(),
    }
}

/// What the page says of `row` under it, or beside it as a tooltip, when it says something.
fn row_hint(row: SettingRow) -> Option<String> {
    if let Some(field) = row.session_field() {
        return field.hint();
    }
    Some(match row {
        SettingRow::CollapseTunnelsPanel => fl!("ui-settings-collapse-tunnels-panel-hint"),
        SettingRow::PreventSleep => fl!("ui-settings-prevent-sleep-hint"),
        SettingRow::Reachability => fl!("ui-settings-reachability-hint"),
        SettingRow::PowerShellPolicy => fl!("ui-settings-powershell-policy-hint"),
        SettingRow::SessionLogging => fl!("ui-settings-session-logging-warning"),
        SettingRow::SessionLogDirectory => fl!("ui-settings-session-log-directory-hint"),
        SettingRow::SshAgentPreference => fl!("ui-settings-ssh-agent-preference-hint"),
        SettingRow::ExternalEditor => fl!("ui-settings-external-editor-hint"),
        SettingRow::PuttyPath => fl!("ui-settings-putty-path-hint"),
        SettingRow::SshDefaultMode => fl!("ui-settings-ssh-default-mode-hint"),
        SettingRow::X11ServerPath => fl!("ui-settings-x11-server-path-hint"),
        SettingRow::HostKeys => fl!("ui-trusted-host-keys-hint"),
        SettingRow::FtpsCertificates => fl!("ui-trusted-ftps-certificates-hint"),
        SettingRow::VncCertificates => fl!("ui-trusted-vnc-certificates-hint"),
        SettingRow::RdpDefaultMode => fl!("ui-settings-rdp-default-mode-hint"),
        SettingRow::RdpDefaults => fl!("ui-settings-rdp-defaults-hint"),
        SettingRow::RdpResolutionPresets => fl!("ui-settings-rdp-resolution-presets-hint"),
        SettingRow::RdpResetAll => fl!("ui-settings-rdp-reset-defaults-tooltip"),
        SettingRow::Certificates => fl!("ui-trusted-certificates-hint"),
        SettingRow::Gateways => fl!("ui-gateways-description"),
        SettingRow::Vault => fl!("ui-settings-vault-explanation"),
        SettingRow::DisconnectOnLock => fl!("ui-settings-disconnect-on-lock-hint"),
        SettingRow::RequireCredentialGuard => fl!("ui-settings-require-credential-guard-hint"),
        SettingRow::RequireWindowsHello => fl!("ui-settings-require-windows-hello-hint"),
        _ => return None,
    })
}

/// The choices `row`'s list offers, as it names them: the C# search finds a list by the
/// items it holds as well.
fn row_choices(row: SettingRow) -> Vec<String> {
    match row {
        SettingRow::Language => Language::ALL
            .map(|l| LanguageChoice(l).to_string())
            .to_vec(),
        SettingRow::Theme => AppTheme::ALL.map(|t| ThemeChoice(t).to_string()).to_vec(),
        SettingRow::Accent => Accent::ALL.map(|a| AccentChoice(a).to_string()).to_vec(),
        SettingRow::ColorScheme => ColorScheme::ALL
            .map(|s| SchemeChoice(s).to_string())
            .to_vec(),
        SettingRow::CtrlVPaste => CtrlVPaste::ALL.map(|c| CtrlVChoice(c).to_string()).to_vec(),
        SettingRow::CtrlKTerminal => CtrlKTerminal::ALL
            .map(|c| CtrlKChoice(c).to_string())
            .to_vec(),
        SettingRow::PowerShellPolicy => ExecutionPolicy::ALL
            .map(|p| PolicyChoice(p).to_string())
            .to_vec(),
        SettingRow::SshAgentPreference => AgentPreference::ALL
            .map(|a| AgentChoice(a).to_string())
            .to_vec(),
        SettingRow::MaxSessions => vec![SessionsChoice(0).to_string()],
        SettingRow::UpdateVersion => vec![fl!("ui-settings-updates-check-now")],
        // The modes, and the button beside them, which the C# search finds as well.
        SettingRow::SshDefaultMode => SshMode::ALL
            .map(|mode| DefaultSshModeChoice(mode).to_string())
            .into_iter()
            .chain([fl!("ui-settings-apply-mode-to-all")])
            .collect(),
        SettingRow::RdpDefaultMode => RdpMode::ALL
            .map(|mode| DefaultRdpModeChoice(mode).to_string())
            .into_iter()
            .chain([fl!("ui-settings-apply-mode-to-all")])
            .collect(),
        SettingRow::RdpDefaults => rdp_switches()
            .into_iter()
            .map(|(label, _)| label)
            .chain([fl!("ui-profile-audio"), fl!("ui-profile-color-depth")])
            .collect(),
        SettingRow::Provider => vec![
            fl!("ui-settings-provider-enabled"),
            fl!("ui-settings-provider-timeout"),
            fl!("ui-settings-provider-preset"),
            fl!("ui-settings-provider-command"),
            fl!("ui-settings-provider-username"),
            fl!("ui-settings-provider-database"),
            fl!("ui-settings-provider-key-file"),
            fl!("ui-settings-provider-first-line"),
        ],
        _ => Vec::new(),
    }
}

/// Every text the search reads for `row`: its name, what is said of it, its choices.
fn row_texts(row: SettingRow) -> Vec<String> {
    let mut texts = vec![row_label(row)];
    texts.extend(row_hint(row));
    texts.extend(row_choices(row));
    texts
}

/// How a box of the RDP options is read.
type RdpSwitch = fn(&RdpDefaults) -> bool;

/// The boxes of the RDP options, as the RDP tab names them, with how each is read.
fn rdp_switches() -> [(String, RdpSwitch); 16] {
    [
        (fl!("ui-profile-resolution-dynamic"), |d| {
            d.dynamic_resolution
        }),
        (fl!("ui-settings-rdp-multi-monitor"), |d| d.multi_monitor),
        (fl!("ui-settings-rdp-audio-capture"), |d| d.microphone),
        (fl!("ui-profile-toggle-clipboard"), |d| d.redirect_clipboard),
        (fl!("ui-profile-toggle-drives"), |d| d.redirect_drives),
        (fl!("ui-settings-rdp-redirect-printers"), |d| {
            d.redirect_printers
        }),
        (fl!("ui-settings-rdp-redirect-com-ports"), |d| {
            d.redirect_com_ports
        }),
        (fl!("ui-settings-rdp-redirect-smart-cards"), |d| {
            d.redirect_smart_cards
        }),
        (fl!("ui-settings-rdp-redirect-webcam"), |d| {
            d.redirect_webcam
        }),
        (fl!("ui-settings-rdp-redirect-usb"), |d| d.redirect_usb),
        (fl!("ui-settings-rdp-bitmap-cache"), |d| d.bitmap_caching),
        (fl!("ui-settings-rdp-compression"), |d| d.compression),
        (fl!("ui-settings-rdp-hardware-acceleration"), |d| {
            d.hardware_acceleration
        }),
        (fl!("ui-settings-rdp-auto-reconnect"), |d| d.auto_reconnect),
        (fl!("ui-profile-toggle-nla"), |d| d.nla),
        (fl!("ui-settings-rdp-strict-server-auth"), |d| {
            d.strict_server_authentication
        }),
    ]
}

/// `on` as the C# words a box's value.
fn on_off(on: bool) -> String {
    if on {
        fl!("ui-settings-value-on")
    } else {
        fl!("ui-settings-value-off")
    }
}

/// `value`, or the word for nothing when it is blank.
fn or_empty(value: &str) -> String {
    if value.trim().is_empty() {
        fl!("ui-settings-value-empty")
    } else {
        value.to_owned()
    }
}

/// The RDP options `current` holds otherwise than `defaults`, each with its default: the
/// card has one marker for every option, where the C# has one each.
fn rdp_changes(current: &RdpDefaults, defaults: &RdpDefaults) -> String {
    let mut changes: Vec<String> = rdp_switches()
        .into_iter()
        .filter(|(_, read)| read(current) != read(defaults))
        .map(|(label, read)| {
            let value = on_off(read(defaults));
            fl!(
                "ui-settings-posture-line",
                label = label.as_str(),
                state = value.as_str()
            )
        })
        .collect();
    if current.audio != defaults.audio {
        changes.push(fl!("ui-profile-audio"));
    }
    if current.color_depth != defaults.color_depth {
        changes.push(fl!("ui-profile-color-depth"));
    }
    changes.join(DEFAULT_LIST_SEPARATOR)
}

/// What the settings say of Windows Hello for the vault, in the C# words.
fn vault_hello_status(status: VaultHelloStatus) -> String {
    match status {
        VaultHelloStatus::Enabled => fl!("ui-settings-vault-hello-status-enabled"),
        VaultHelloStatus::Available => fl!("ui-settings-vault-hello-status-available"),
        VaultHelloStatus::Unavailable => fl!("ui-settings-vault-hello-status-unavailable"),
        VaultHelloStatus::Enrolling => fl!("ui-settings-vault-hello-status-enrolling"),
        VaultHelloStatus::UnlockRequired => {
            fl!("ui-settings-vault-hello-status-unlock-required")
        }
    }
}

/// What the settings say of Credential Guard while it is required, in the C# words: why
/// it is not available when that is known, so that a computer where `PowerShell` is kept
/// from starting can be told apart.
fn credential_guard_text(status: &credential_guard::Status) -> String {
    match status {
        credential_guard::Status::Active => fl!("ui-settings-credential-guard-enabled"),
        credential_guard::Status::Inactive => fl!("ui-settings-credential-guard-disabled"),
        credential_guard::Status::Indeterminate(failure) => {
            let reason = credential_guard_failure(failure);
            fl!(
                "ui-settings-credential-guard-disabled-reason",
                reason = reason.as_str()
            )
        }
    }
}

/// Why Credential Guard's state could not be found, worded.
fn credential_guard_failure(failure: &credential_guard::Failure) -> String {
    use credential_guard::Failure;
    match failure {
        Failure::NotWindows => fl!("ui-settings-credential-guard-reason-not-windows"),
        Failure::NoSystemFolder => fl!("ui-settings-credential-guard-reason-no-system-folder"),
        Failure::NotStarted(detail) => fl!(
            "ui-settings-credential-guard-reason-not-started",
            detail = detail.trim()
        ),
        Failure::TimedOut => fl!(
            "ui-settings-credential-guard-reason-timed-out",
            seconds = CHECK_TIME_LIMIT.as_secs()
        ),
        Failure::Exited(Some(code)) => {
            let code = *code;
            fl!("ui-settings-credential-guard-reason-exited", code = code)
        }
        Failure::Exited(None) => fl!("ui-settings-credential-guard-reason-stopped"),
        Failure::NoInstance => fl!("ui-settings-credential-guard-reason-no-instance"),
        Failure::Unread(detail) => fl!(
            "ui-settings-credential-guard-reason-unread",
            detail = detail.trim()
        ),
        Failure::NoValue => fl!("ui-settings-credential-guard-reason-no-value"),
        Failure::InvalidValue => fl!("ui-settings-credential-guard-reason-invalid-value"),
    }
}

/// The name of a line of the security overview.
fn posture_label(key: PostureKey) -> String {
    match key {
        PostureKey::RdpNla => fl!("ui-settings-posture-label-rdp-nla"),
        PostureKey::RdpStrictServerAuthentication => {
            fl!("ui-settings-posture-label-rdp-strict-server-auth")
        }
        PostureKey::SessionTranscripts => fl!("ui-settings-posture-label-transcripts"),
        PostureKey::PowerShellExecutionPolicy => fl!("ui-settings-posture-label-ps-policy"),
        PostureKey::Vault => fl!("ui-settings-posture-label-vault"),
        PostureKey::AutoLock => fl!("ui-settings-posture-label-auto-lock"),
        PostureKey::DisconnectOnLock => fl!("ui-settings-posture-label-disconnect-on-lock"),
        PostureKey::CredentialGuard => fl!("ui-settings-posture-label-credential-guard"),
        PostureKey::WindowsHelloOnConnect => fl!("ui-settings-posture-label-windows-hello"),
        PostureKey::UpdateChecks => fl!("ui-settings-posture-label-update-checks"),
    }
}

/// The state of a line, worded.
fn posture_state(state: PostureState) -> String {
    match state {
        PostureState::On => fl!("ui-settings-value-on"),
        PostureState::Off => fl!("ui-settings-value-off"),
        PostureState::Enabled => fl!("ui-settings-posture-state-enabled"),
        PostureState::Disabled => fl!("ui-settings-posture-state-disabled"),
        PostureState::Policy(policy) => PolicyChoice(policy).to_string(),
        PostureState::AfterMinutes(minutes) => {
            fl!("ui-settings-posture-state-after-minutes", minutes = minutes)
        }
        PostureState::Never => fl!("ui-settings-posture-state-never"),
        PostureState::RequiresVault => fl!("ui-settings-posture-state-requires-vault"),
        PostureState::Required => fl!("ui-settings-posture-state-required"),
        PostureState::NotRequired => fl!("ui-settings-posture-state-not-required"),
    }
}

/// Why a line needs attention; only a choice that can be risky has a reason.
fn posture_warning(key: PostureKey) -> Option<String> {
    match key {
        PostureKey::RdpNla => Some(fl!("ui-settings-posture-warning-rdp-nla")),
        PostureKey::SessionTranscripts => Some(fl!("ui-settings-posture-warning-transcripts")),
        PostureKey::PowerShellExecutionPolicy => Some(fl!("ui-settings-posture-warning-ps-policy")),
        PostureKey::AutoLock => Some(fl!("ui-settings-posture-warning-auto-lock")),
        PostureKey::UpdateChecks => Some(fl!("ui-settings-posture-warning-update-checks")),
        PostureKey::RdpStrictServerAuthentication
        | PostureKey::Vault
        | PostureKey::DisconnectOnLock
        | PostureKey::CredentialGuard
        | PostureKey::WindowsHelloOnConnect => None,
    }
}

/// What the last "Check now" found, worded as the C# Settings page words it.
fn update_status_text(status: UpdateStatus) -> String {
    match status {
        UpdateStatus::Checking => fl!("ui-settings-updates-checking"),
        UpdateStatus::UpToDate => fl!("ui-settings-updates-up-to-date"),
        UpdateStatus::Available(release) => {
            fl!(
                "ui-settings-updates-available",
                version = release.to_string()
            )
        }
        UpdateStatus::NeedsRelease => fl!("ui-settings-updates-unknown-version"),
        UpdateStatus::Failed(failure) => update_failure_text(failure),
    }
}

/// Why a look got no answer, by what the user would do about it, as the C# says it.
fn update_failure_text(failure: Failure) -> String {
    match failure {
        Failure::NetworkUnreachable => fl!("ui-settings-updates-failed-network"),
        Failure::SecureChannel => fl!("ui-settings-updates-failed-secure-channel"),
        Failure::ProxyRefused => fl!("ui-settings-updates-failed-proxy"),
        // A wait is said only when the server said it, never guessed.
        Failure::RateLimited { retry_after } => match retry_after.map(minutes_to_wait) {
            None => fl!("ui-settings-updates-failed-rate-limited"),
            Some(1) => fl!("ui-settings-updates-failed-rate-limited-minute"),
            Some(minutes) => fl!(
                "ui-settings-updates-failed-rate-limited-minutes",
                minutes = minutes
            ),
        },
        Failure::AccessDenied => fl!("ui-settings-updates-failed-access-denied"),
        Failure::SourceUnavailable => fl!("ui-settings-updates-failed-unavailable"),
        Failure::MalformedResponse => fl!("ui-settings-updates-failed-malformed"),
        Failure::TimedOut => fl!("ui-settings-updates-failed-timed-out"),
    }
}

/// The check mark of what is as it should be, or the warning sign of what needs attention:
/// the colour is never the only signal.
fn posture_mark<'a>(risky: bool) -> Element<'a, Message> {
    let style: fn(&Theme) -> text::Style = if risky { text::warning } else { text::success };
    text(if risky {
        POSTURE_RISKY_MARK
    } else {
        POSTURE_SAFE_MARK
    })
    .style(style)
    .into()
}

/// A line of the security overview: its choice and state, why it needs attention when it
/// does, and then the way to the setting, as the C# line.
fn posture_line<'a>(line: PostureLine) -> Element<'a, Message> {
    let label = posture_label(line.key);
    let state = posture_state(line.state);
    let mut said = column![text(fl!(
        "ui-settings-posture-line",
        label = label.as_str(),
        state = state.as_str()
    ))];
    let warning = posture_warning(line.key).filter(|_| line.risky);
    let risky = warning.is_some();
    if let Some(warning) = warning {
        said = said.push(text(warning).size(font_size::CAPTION).style(text::warning));
    }
    let mut shown = row![posture_mark(risky), said]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
    if risky {
        shown = shown.push(iced::widget::space::horizontal()).push(
            button(text(fl!("ui-settings-posture-go-to")).size(font_size::CAPTION))
                .style(styles::secondary)
                .on_press(Message::GoToSetting(line.target)),
        );
    }
    shown.into()
}

impl Shell {
    /// The Settings page: its heading with the search, then the tab chosen, or what the
    /// search finds.
    pub(super) fn settings_page(&self) -> Element<'_, Message> {
        let query = self.settings_search.trim();
        let body = if query.is_empty() {
            self.settings_tab_page(self.settings_tab)
        } else {
            self.search_results(query)
        };
        // The tab shown joins the card under it, as the C#'s; its cards in the middle.
        let content = container(
            container(body.spacing(spacing::LG).width(Length::Fill)).max_width(SETTINGS_WIDTH),
        )
        .center_x(Length::Fill)
        .padding(spacing::LG)
        .style(styles::strip);
        styles::scroll(
            column![
                self.settings_header(),
                divider(),
                column![settings_tabs(self.settings_tab), content],
            ]
            .spacing(spacing::SM)
            .padding(spacing::MD),
        )
        .id(scroll_id())
        .into()
    }

    /// What the Settings page holds typed and not applied yet, forgotten: every setting was
    /// put back to its default, and what was typed over the old values goes with them.
    pub(super) fn forget_typed_settings(&mut self) {
        self.log_directory = None;
        self.editor_typed = None;
        self.tool_paths_typed = Default::default();
        self.font_size_typed = None;
        self.session_typed = Default::default();
    }

    /// "Reset defaults" at the left, as the C# bar at the top of its Settings tab
    /// (`MainWindow.xaml:2406-2408`), asked first; "Find modified settings" and the search
    /// at its right. Its Save and Undo are left out, every choice applying at once.
    fn settings_header(&self) -> Element<'_, Message> {
        let field = SearchKeys::escape_only(
            text_input(
                &fl!("ui-settings-search-placeholder"),
                &self.settings_search,
            )
            .style(styles::text_input)
            .id(search_field_id())
            .on_input(Message::SettingsSearch)
            .width(SEARCH_WIDTH),
            // A first Escape empties it; empty, Escape takes the keyboard from it.
            (!self.settings_search.is_empty()).then(|| Message::SettingsSearch(String::new())),
        );
        let mut header = row![
            button(text(fl!("ui-settings-reset-all")))
                .style(styles::secondary)
                .on_press(send(SettingsMessage::ResetAllSettings)),
            iced::widget::space::horizontal(),
            tooltip(
                button(text(fl!("ui-settings-find-modified")))
                    .style(styles::subtle)
                    .on_press(Message::FindModifiedSettings),
                text(fl!("ui-settings-find-modified-hint")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
            tooltip(
                field,
                text(fl!("ui-settings-search-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        if !self.settings_search.is_empty() {
            header = header.push(
                tooltip(
                    button(text(fl!("ui-tree-search-clear-button")))
                        .style(styles::secondary)
                        .on_press(Message::SettingsSearch(String::new())),
                    text(fl!("ui-settings-search-clear")).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box),
            );
        }
        header.into()
    }

    /// The cards of `tab`; the Security tab's under its security overview, as the C#.
    fn settings_tab_page(&self, tab: SettingsTab) -> Column<'_, Message> {
        let mut page = Column::new();
        if tab == SettingsTab::Security {
            page = page.push(self.posture_card());
        }
        for card in SettingsCard::ALL
            .into_iter()
            .filter(|card| card.tab() == tab)
        {
            page = page.push(self.card_view(card, &card.rows()));
        }
        page
    }

    /// The rows `query` finds, under the name of their tab and the heading of their card,
    /// with how many there are, or the C# words for none.
    fn search_results(&self, query: &str) -> Column<'_, Message> {
        let found = self.settings_found(query);
        let count = if found.is_empty() {
            fl!("ui-settings-search-no-results")
        } else {
            fl!("ui-settings-search-results", count = found.len())
        };
        let mut results = column![text(count).size(font_size::CAPTION).style(text::secondary)];
        for tab in SettingsTab::ALL {
            let mut cards = Column::new().spacing(spacing::SM);
            let mut any = false;
            for card in SettingsCard::ALL
                .into_iter()
                .filter(|card| card.tab() == tab)
            {
                let rows: Vec<SettingRow> = card
                    .rows()
                    .into_iter()
                    .filter(|row| found.contains(row))
                    .collect();
                if !rows.is_empty() {
                    any = true;
                    cards = cards.push(self.card_view(card, &rows));
                }
            }
            if any {
                results = results
                    .push(text(tab.label()).size(font_size::TITLE))
                    .push(cards);
            }
        }
        results
    }

    /// The rows `query` finds, in the page's order: those whose name, what is said of it or
    /// its choices hold it, or whose card's heading does, whatever the case and the accents,
    /// as the C# search; and, as the C# finds a marker shown, those marked "Modified" when
    /// that word holds it.
    #[must_use]
    pub fn settings_found(&self, query: &str) -> Vec<SettingRow> {
        let wanted = search_folded(query);
        if wanted.is_empty() {
            return Vec::new();
        }
        let holds = |said: &String| search_folded(said).contains(&wanted);
        let modified = search_folded(&fl!("ui-settings-modified-badge")).contains(&wanted);
        let settings = self.app.settings();
        SettingRow::ALL
            .into_iter()
            .filter(|row| {
                let card = row.card();
                card_heading(card).iter().any(holds)
                    || card_description(card).iter().any(holds)
                    || row_texts(*row).iter().any(holds)
                    || (modified && row.is_modified(settings))
            })
            .collect()
    }

    /// `rows` of `card` under its heading, in its card as the C#'s.
    fn card_view(&self, card: SettingsCard, rows: &[SettingRow]) -> Element<'_, Message> {
        let mut body = Column::new().spacing(spacing::SM);
        if let Some(description) = card_description(card) {
            body = body.push(text(description).size(font_size::CAPTION));
        }
        for &row in rows {
            body = body.push(self.setting_row(row));
        }
        if matches!(card, SettingsCard::SshTrusted | SettingsCard::RdpTrusted)
            && let Some(unreadable) = crate::trusted_keys_view::unreadable(self.app.trusted_keys())
        {
            // What could not be read, said once under the lists.
            body = body.push(unreadable);
        }
        // Its heading semi-bold at its top, as the C# card's section title.
        let mut shown = Column::new().spacing(spacing::MD);
        if let Some(heading) = card_heading(card) {
            shown = shown.push(
                text(heading)
                    .size(font_size::SUBTITLE)
                    .font(styles::SEMIBOLD),
            );
        }
        let shown = shown.push(body);
        if card_framed(card) {
            container(shown)
                .padding(spacing::LG)
                .width(Length::Fill)
                .style(styles::card)
                .into()
        } else {
            shown.into()
        }
    }

    /// `row`, with its "Modified" marker when its value is not the default, outlined when a
    /// "Go to setting" showed it.
    fn setting_row(&self, row: SettingRow) -> Element<'_, Message> {
        let mut shown = column![self.row_body(row)].spacing(spacing::SM);
        if row.is_modified(self.app.settings()) {
            shown = shown.push(self.marker(row));
        }
        if self.settings_highlight != Some(row) {
            return shown.into();
        }
        container(shown)
            .padding(spacing::SM)
            .style(|theme: &Theme| container::Style {
                border: iced::Border {
                    color: theme.extended_palette().primary.strong.color,
                    width: HIGHLIGHT_EDGE,
                    radius: radius::SM.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    /// The C# marker of a setting modified, as `SettingDefaultMarker`: "Modified" beside a
    /// dot in a badge outlined in the accent, its default said on hover, and "Reset" after
    /// its glyph, which puts it back.
    fn marker(&self, row: SettingRow) -> Element<'_, Message> {
        let value = self.default_text(row);
        let badge = container(
            row![
                container(iced::widget::space())
                    .width(MODIFIED_DOT_SIDE)
                    .height(MODIFIED_DOT_SIDE)
                    .style(styles::accent_dot),
                text(fl!("ui-settings-modified-badge"))
                    .size(font_size::SMALL_CAPTION)
                    .style(text::secondary),
            ]
            .spacing(spacing::XS)
            .align_y(iced::Alignment::Center),
        )
        .padding(BADGE_PADDING)
        .style(styles::badge);
        row![
            tooltip(
                badge,
                text(fl!(
                    "ui-settings-modified-from-default",
                    value = value.as_str()
                ))
                .size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
            tooltip(
                button(
                    row![
                        icons::icon(Icon::Restore, Tint::Text, RESET_GLYPH_SIDE),
                        text(fl!("ui-settings-reset-to-default")).size(font_size::SMALL_CAPTION),
                    ]
                    .spacing(spacing::XS)
                    .align_y(iced::Alignment::Center),
                )
                .padding(RESET_PADDING)
                .style(styles::subtle)
                .on_press(Message::ResetSetting(row)),
                text(fl!(
                    "ui-settings-reset-to-default-tooltip",
                    value = value.as_str()
                ))
                .size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// The default of `row`, worded as its list or box words its values.
    fn default_text(&self, row: SettingRow) -> String {
        let defaults = Settings::default();
        if let Some(field) = row.session_field() {
            return field.value(&defaults).to_string();
        }
        if let Some(on) = row.flag(&defaults) {
            return on_off(on);
        }
        match row {
            SettingRow::Theme => ThemeChoice(defaults.theme).to_string(),
            SettingRow::Accent => AccentChoice(defaults.accent).to_string(),
            SettingRow::MaxSessions => SessionsChoice(defaults.max_sessions).to_string(),
            SettingRow::FontSize => defaults.terminal_font_size.to_string(),
            SettingRow::FontFamily => defaults.terminal_font_family,
            SettingRow::ColorScheme => SchemeChoice(defaults.color_scheme).to_string(),
            SettingRow::CtrlVPaste => CtrlVChoice(defaults.ctrl_v_paste).to_string(),
            SettingRow::CtrlKTerminal => CtrlKChoice(defaults.ctrl_k_terminal).to_string(),
            SettingRow::PowerShellPolicy => {
                PolicyChoice(defaults.powershell_execution_policy).to_string()
            }
            SettingRow::SessionLogDirectory => or_empty(&defaults.session_log_directory),
            SettingRow::SshAutoReconnectAttempts => {
                defaults.ssh_auto_reconnect_attempts.to_string()
            }
            SettingRow::SshAgentPreference => {
                AgentChoice(defaults.ssh_agent_preference).to_string()
            }
            SettingRow::ExternalEditor => or_empty(&defaults.external_editor),
            SettingRow::PuttyPath => or_empty(&defaults.putty_path),
            SettingRow::SshDefaultMode => {
                DefaultSshModeChoice(defaults.ssh_default_mode).to_string()
            }
            SettingRow::X11ServerPath => or_empty(&defaults.x11_server_path),
            SettingRow::RdpDefaultMode => {
                DefaultRdpModeChoice(defaults.rdp_default_mode).to_string()
            }
            SettingRow::RdpDefaults => {
                rdp_changes(&self.app.settings().rdp_defaults, &defaults.rdp_defaults)
            }
            SettingRow::RdpAutoReconnectAttempts => {
                defaults.rdp_auto_reconnect_attempts.to_string()
            }
            SettingRow::RdpConnectTimeout => {
                TimeoutChoice(defaults.rdp_connect_timeout).to_string()
            }
            SettingRow::RdpResolutionPresets => defaults
                .resolution_presets()
                .iter()
                .map(|(width, height)| format!("{width}x{height}"))
                .collect::<Vec<_>>()
                .join(DEFAULT_LIST_SEPARATOR),
            _ => String::new(),
        }
    }

    /// Whether a master password is set.
    fn vault_set(&self) -> bool {
        self.app.vault_status() != VaultStatus::Missing
    }

    /// Whether `row` can be changed now: a workspace lock setting only with a master
    /// password set, the Windows Hello grace only with Windows Hello required, as the C#
    /// panel enables them.
    fn row_available(&self, row: SettingRow) -> bool {
        (!row.needs_vault() || self.vault_set()) && row.number_enabled(self.app.settings())
    }

    /// What `row` shows: its box, list or field, with what is said of it.
    fn row_body(&self, row: SettingRow) -> Element<'_, Message> {
        if let Some(field) = row.session_field() {
            let number = self.number_row(field, self.row_available(row));
            // Shown disabled with the reason rather than hidden, as the C#: hidden, nothing
            // says the lock exists nor what turns it on.
            if row == SettingRow::AutoLock && !self.vault_set() {
                return column![
                    text(fl!("ui-settings-auto-lock-requires-vault")).size(font_size::CAPTION),
                    number
                ]
                .spacing(spacing::SM)
                .into();
            }
            return number;
        }
        if row.flag(self.app.settings()).is_some() {
            return self.toggle_row(row);
        }
        match row {
            SettingRow::FontSize => self.font_size_row(),
            SettingRow::FontFamily => self.font_family_row(),
            SettingRow::SessionLogDirectory | SettingRow::ExternalEditor => self.path_row(row),
            SettingRow::PuttyPath | SettingRow::X11ServerPath => self.tool_path_row(row),
            SettingRow::SshDefaultMode => self.ssh_default_mode_row(),
            SettingRow::RdpDefaultMode => self.rdp_default_mode_row(),
            SettingRow::HostKeys
            | SettingRow::FtpsCertificates
            | SettingRow::VncCertificates
            | SettingRow::Certificates
            | SettingRow::Macros
            | SettingRow::Gateways
            | SettingRow::RdpDefaults
            | SettingRow::RdpResolutionPresets
            | SettingRow::RdpResetAll => self.list_card_row(row),
            SettingRow::Pin | SettingRow::Vault | SettingRow::Provider => self.security_row(row),
            SettingRow::UpdateVersion => self.update_version_row(),
            _ => self.choice_row(row),
        }
    }

    /// The version running, as the C# Updates card shows it, then "Check now" and what it
    /// found; the button waits while a look runs. The release skipped from the banner is
    /// said under them with "Offer it again", as the C# `SkippedVersionPanel`
    /// (`MainWindow.xaml:2618-2626`).
    fn update_version_row(&self) -> Element<'_, Message> {
        let version = self.app.running_release().map_or_else(
            || {
                fl!(
                    "ui-settings-updates-development-build",
                    version = PACKAGE_VERSION
                )
            },
            |release| release.to_string(),
        );
        let status = self.app.update_status();
        let mut check = button(text(fl!("ui-settings-updates-check-now"))).style(styles::secondary);
        if status != Some(UpdateStatus::Checking) {
            check = check.on_press(Message::App(AppMessage::Update(UpdateMessage::CheckNow)));
        }
        let mut body = column![
            text(row_label(SettingRow::UpdateVersion))
                .size(font_size::CAPTION)
                .style(text::secondary),
            text(version),
            check,
        ]
        .spacing(spacing::SM);
        if let Some(status) = status {
            let style: fn(&Theme) -> text::Style = if matches!(status, UpdateStatus::Failed(_)) {
                text::warning
            } else {
                text::secondary
            };
            body = body.push(
                text(update_status_text(status))
                    .size(font_size::CAPTION)
                    .style(style),
            );
        }
        if let Some(skipped) = self
            .app
            .settings()
            .update_check
            .skipped
            .as_deref()
            .filter(|skipped| !skipped.trim().is_empty())
        {
            // Shown as the banner shows a release: `2026.100901`, without its tag's "v".
            let skipped = skipped.trim();
            let version = ReleaseTag::parse(skipped)
                .map_or_else(|| skipped.to_owned(), |release| release.to_string());
            body = body.push(
                row![
                    text(fl!(
                        "ui-settings-updates-skipped-version",
                        version = version.as_str()
                    ))
                    .style(text::secondary),
                    button(text(fl!("ui-settings-updates-clear-skipped")))
                        .style(styles::secondary)
                        .on_press(Message::App(AppMessage::Update(
                            UpdateMessage::ClearSkipped
                        ))),
                ]
                .spacing(spacing::MD)
                .align_y(iced::Alignment::Center),
            );
        }
        body.into()
    }

    /// A box ticked on or off, what is said of it under it; one hanging from another box is
    /// greyed while that box is off, a workspace lock one while no master password is set.
    fn toggle_row(&self, row: SettingRow) -> Element<'_, Message> {
        let settings = self.app.settings();
        let mut tick = checkbox(row.flag(settings).unwrap_or_default())
            .style(styles::checkbox)
            .label(row_label(row));
        if row.toggle_enabled(settings)
            && self.row_available(row)
            && let (Some(ticked), Some(unticked)) =
                (row.toggled(settings, true), row.toggled(settings, false))
        {
            tick =
                tick.on_toggle(move |on| send(if on { ticked.clone() } else { unticked.clone() }));
        }
        let mut body = column![tick].spacing(spacing::SM);
        if let Some(hint) = row_hint(row) {
            body = body.push(text(hint).size(font_size::CAPTION));
        }
        // Required, whether Credential Guard runs, as the C# status beside the box; nothing
        // until the check answers.
        if row == SettingRow::RequireCredentialGuard
            && settings.require_credential_guard
            && let Some(status) = self.app.credential_guard_status()
        {
            let style: fn(&Theme) -> text::Style = if status.is_active() {
                text::secondary
            } else {
                text::warning
            };
            body = body.push(
                text(credential_guard_text(&status))
                    .size(font_size::CAPTION)
                    .style(style),
            );
        }
        // Where Windows Hello does not exist, why the box is greyed, or what it does on.
        if row == SettingRow::RequireWindowsHello && !windows_hello::SUPPORTED {
            body = body.push(
                text(fl!("ui-settings-require-windows-hello-windows-only"))
                    .size(font_size::CAPTION)
                    .style(text::danger),
            );
        }
        body.into()
    }

    /// A list of `row`'s choices, what is said of it under it.
    fn choice_row(&self, row: SettingRow) -> Element<'_, Message> {
        let settings = self.app.settings();
        let label = row_label(row);
        let line = match row {
            SettingRow::Language => labelled(
                label,
                choices(
                    Language::ALL.map(LanguageChoice).to_vec(),
                    Some(LanguageChoice(
                        settings.language.unwrap_or_else(crate::i18n::current),
                    )),
                    |LanguageChoice(language)| Message::LanguageChosen(language),
                ),
            ),
            SettingRow::Theme => labelled(
                label,
                choices(
                    AppTheme::ALL.map(ThemeChoice).to_vec(),
                    Some(ThemeChoice(settings.theme)),
                    |ThemeChoice(theme)| send(SettingsMessage::Theme(theme)),
                ),
            ),
            SettingRow::Accent => labelled(
                label,
                choices(
                    Accent::ALL.map(AccentChoice).to_vec(),
                    Some(AccentChoice(settings.accent)),
                    |AccentChoice(accent)| send(SettingsMessage::Accent(accent)),
                ),
            ),
            SettingRow::MaxSessions => labelled(
                label,
                choices(
                    (0..=MAX_SESSIONS_MAX)
                        .map(SessionsChoice)
                        .collect::<Vec<_>>(),
                    Some(SessionsChoice(settings.max_sessions)),
                    |SessionsChoice(max)| send(SettingsMessage::MaxSessions(max)),
                ),
            ),
            SettingRow::ColorScheme => labelled(
                label,
                choices(
                    ColorScheme::ALL.map(SchemeChoice).to_vec(),
                    Some(SchemeChoice(settings.color_scheme)),
                    |SchemeChoice(scheme)| send(SettingsMessage::ColorScheme(scheme)),
                ),
            ),
            SettingRow::CtrlVPaste => labelled(
                label,
                choices(
                    CtrlVPaste::ALL.map(CtrlVChoice).to_vec(),
                    Some(CtrlVChoice(settings.ctrl_v_paste)),
                    |CtrlVChoice(choice)| send(SettingsMessage::CtrlVPaste(choice)),
                ),
            ),
            SettingRow::CtrlKTerminal => labelled(
                label,
                choices(
                    CtrlKTerminal::ALL.map(CtrlKChoice).to_vec(),
                    Some(CtrlKChoice(settings.ctrl_k_terminal)),
                    |CtrlKChoice(choice)| send(SettingsMessage::CtrlKTerminal(choice)),
                ),
            ),
            SettingRow::PowerShellPolicy => labelled(
                label,
                choices(
                    ExecutionPolicy::ALL.map(PolicyChoice).to_vec(),
                    Some(PolicyChoice(settings.powershell_execution_policy)),
                    |PolicyChoice(policy)| send(SettingsMessage::PowerShellExecutionPolicy(policy)),
                ),
            ),
            SettingRow::SshAgentPreference => labelled(
                label,
                choices(
                    AgentPreference::ALL.map(AgentChoice).to_vec(),
                    Some(AgentChoice(settings.ssh_agent_preference)),
                    |AgentChoice(preference)| send(SettingsMessage::SshAgentPreference(preference)),
                ),
            ),
            _ => return self.attempts_row(row),
        };
        let mut body = column![line].spacing(spacing::SM);
        if let Some(hint) = row_hint(row) {
            body = body.push(text(hint).size(font_size::CAPTION));
        }
        body.into()
    }

    /// The auto-reconnect attempts and the RDP logon watchdog: a list of the numbers their
    /// C# range allows.
    fn attempts_row(&self, row: SettingRow) -> Element<'_, Message> {
        let settings = self.app.settings();
        let label = row_label(row);
        let line = match row {
            SettingRow::SshAutoReconnectAttempts => labelled(
                label,
                pick_list(
                    (SSH_AUTO_RECONNECT_ATTEMPTS_MIN..=SSH_AUTO_RECONNECT_ATTEMPTS_MAX)
                        .collect::<Vec<u32>>(),
                    Some(settings.ssh_auto_reconnect_attempts),
                    |attempts| send(SettingsMessage::SshAutoReconnectAttempts(attempts)),
                )
                .style(styles::pick_list)
                .menu_style(styles::menu),
            ),
            SettingRow::RdpAutoReconnectAttempts => labelled(
                label,
                pick_list(
                    (RDP_AUTO_RECONNECT_ATTEMPTS_MIN..=RDP_AUTO_RECONNECT_ATTEMPTS_MAX)
                        .collect::<Vec<u32>>(),
                    Some(settings.rdp_auto_reconnect_attempts),
                    |attempts| send(SettingsMessage::RdpAutoReconnectAttempts(attempts)),
                )
                .style(styles::pick_list)
                .menu_style(styles::menu),
            ),
            // As the C# watchdog: off, or a choice of the seconds its range allows.
            _ => labelled(
                label,
                pick_list(
                    CONNECT_TIMEOUTS
                        .into_iter()
                        .map(TimeoutChoice)
                        .collect::<Vec<_>>(),
                    Some(TimeoutChoice(settings.rdp_connect_timeout)),
                    |TimeoutChoice(seconds)| send(SettingsMessage::RdpConnectTimeout(seconds)),
                )
                .style(styles::pick_list)
                .menu_style(styles::menu),
            ),
        };
        line.into()
    }

    /// A number typed and applied with Enter, its unit after it, what is said of it under
    /// it, and its rule while what is typed is out of its range; greyed when not `enabled`.
    fn number_row(&self, field: SessionField, enabled: bool) -> Element<'_, Message> {
        let shown = field.value(self.app.settings()).to_string();
        let typed = self.session_typed[field.index()].clone().unwrap_or(shown);
        let refused = self.session_typed[field.index()].is_some()
            && !self
                .typed_session(field)
                .is_some_and(|value| field.accepted(value));
        let mut input = text_input("", &typed)
            .style(styles::text_input)
            .width(FONT_SIZE_FIELD_WIDTH);
        if enabled {
            input = input
                .on_input(move |typed| Message::SessionFieldEdited(field, typed))
                .on_submit(Message::SessionFieldApply(field));
        }
        let line = match field.unit() {
            Some(unit) => labelled(field.label(), with_unit(input, unit)),
            None => labelled(field.label(), input),
        };
        let mut body = column![line].spacing(spacing::SM);
        if let Some(hint) = field.hint() {
            body = body.push(text(hint).size(font_size::CAPTION));
        }
        if refused {
            body = body.push(
                text(field.refusal())
                    .size(font_size::CAPTION)
                    .style(text::danger),
            );
        }
        body.into()
    }

    /// The terminals' font size, applied with Enter; one out of the range stays typed, the
    /// C# message under it.
    fn font_size_row(&self) -> Element<'_, Message> {
        let shown = self.app.settings().terminal_font_size.to_string();
        let typed = self.font_size_typed.clone().unwrap_or(shown);
        let refused = self.font_size_typed.is_some()
            && !self
                .typed_font_size()
                .is_some_and(heimdall_core::settings::terminal_font_size_accepted);
        let mut body = column![labelled(
            row_label(SettingRow::FontSize),
            with_unit(
                text_input("", &typed)
                    .style(styles::text_input)
                    .width(FONT_SIZE_FIELD_WIDTH)
                    .on_input(Message::FontSizeEdited)
                    .on_submit(Message::FontSizeApply),
                fl!("ui-settings-font-size-unit"),
            ),
        ),]
        .spacing(spacing::SM);
        if refused {
            body = body.push(
                text(fl!(
                    "ui-settings-font-size-refused",
                    min = heimdall_core::settings::TERMINAL_FONT_SIZE_MIN,
                    max = heimdall_core::settings::TERMINAL_FONT_SIZE_MAX
                ))
                .size(font_size::CAPTION)
                .style(text::danger),
            );
        }
        body.into()
    }

    /// The family of the terminals' text, as the C# box beside the size: the families this
    /// computer can draw, and the one chosen when it cannot, then said drawn in the embedded
    /// one.
    fn font_family_row(&self) -> Element<'_, Message> {
        let chosen = &self.app.settings().terminal_font_family;
        let mut offered: Vec<FontChoice> =
            crate::terminal_view::font::available(crate::terminal_view::font::installed)
                .into_iter()
                .map(|family| FontChoice(family.to_owned()))
                .collect();
        let selected = offered
            .iter()
            .find(|offer| offer.0.eq_ignore_ascii_case(chosen))
            .cloned()
            .unwrap_or_else(|| FontChoice(chosen.clone()));
        if !offered.contains(&selected) {
            offered.push(selected.clone());
        }
        let mut body = column![labelled(
            row_label(SettingRow::FontFamily),
            pick_list(offered, Some(selected), |FontChoice(family)| {
                send(SettingsMessage::TerminalFontFamily(family))
            })
            .style(styles::pick_list)
            .menu_style(styles::menu),
        )]
        .spacing(spacing::SM);
        if !self.terminal_font().is(chosen) {
            body = body.push(
                text(fl!(
                    "ui-settings-font-family-missing",
                    family = chosen.as_str(),
                    fallback = crate::terminal_view::FONT_FAMILY
                ))
                .size(font_size::CAPTION)
                .style(text::danger),
            );
        }
        body.into()
    }

    /// The transcripts' folder or the external editor, typed and applied with Enter,
    /// "Browse..." beside it, what is said of it under it.
    fn path_row(&self, row: SettingRow) -> Element<'_, Message> {
        let settings = self.app.settings();
        let target = if row == SettingRow::SessionLogDirectory {
            BrowseTarget::SessionLogDirectory
        } else {
            BrowseTarget::ExternalEditor
        };
        let field = if row == SettingRow::SessionLogDirectory {
            let typed = self
                .log_directory
                .as_deref()
                .unwrap_or(&settings.session_log_directory);
            text_input(
                heimdall_core::settings::DEFAULT_SESSION_LOG_DIRECTORY,
                typed,
            )
            .style(styles::text_input)
            .on_input(Message::LogDirectoryEdited)
            .on_submit(Message::LogDirectoryApply)
        } else {
            let typed = self
                .editor_typed
                .as_deref()
                .unwrap_or(&settings.external_editor);
            text_input("", typed)
                .style(styles::text_input)
                .on_input(Message::EditorEdited)
                .on_submit(Message::EditorApply)
        };
        let field = row![field, browse_button(target)]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center);
        let mut body = column![labelled(row_label(row), field)].spacing(spacing::SM);
        if let Some(hint) = row_hint(row) {
            body = body.push(text(hint).size(font_size::CAPTION));
        }
        body.into()
    }

    /// The dialog of a "Browse..." button, opening where the path set now is.
    pub(super) fn browse(&self, target: BrowseTarget) -> Task<Message> {
        let settings = self.app.settings();
        let current = match target {
            BrowseTarget::Tool(path) => PathBuf::from(
                self.tool_paths_typed[path.index()]
                    .as_deref()
                    .unwrap_or_else(|| path.value(settings))
                    .trim(),
            ),
            BrowseTarget::ExternalEditor => PathBuf::from(
                self.editor_typed
                    .as_deref()
                    .unwrap_or(&settings.external_editor)
                    .trim(),
            ),
            // Where the transcripts go, a relative folder read beside the settings.
            BrowseTarget::SessionLogDirectory => {
                settings.session_log_folder(&settings_path(self.app.profiles_file()))
            }
            BrowseTarget::ProviderDatabase => {
                PathBuf::from(settings.credential_provider.database.trim())
            }
            BrowseTarget::ProviderKeyFile => {
                PathBuf::from(settings.credential_provider.key_file.trim())
            }
            BrowseTarget::GatewayKey => match &self.app.dialog {
                Some(Dialog::EditGateway { draft, .. }) => {
                    PathBuf::from(draft.value(ProfileField::KeyPath).trim())
                }
                _ => PathBuf::new(),
            },
        };
        crate::browse::pick(target, target.start(&current), self.main_window)
    }

    /// The path picked in a "Browse..." dialog, put in its field and applied as Enter
    /// applies what is typed there; what was typed and not applied goes.
    pub(super) fn browsed(&mut self, target: BrowseTarget, path: String) -> Vec<Effect> {
        let message = match target {
            BrowseTarget::Tool(tool) => {
                self.tool_paths_typed[tool.index()] = None;
                AppMessage::Settings(tool.applied(path))
            }
            BrowseTarget::ExternalEditor => {
                self.editor_typed = None;
                AppMessage::Settings(SettingsMessage::ExternalEditor(path))
            }
            BrowseTarget::SessionLogDirectory => {
                self.log_directory = None;
                AppMessage::Settings(SettingsMessage::SessionLogDirectory(path))
            }
            BrowseTarget::ProviderDatabase => {
                AppMessage::CredentialProvider(ProviderMessage::Database(path))
            }
            BrowseTarget::ProviderKeyFile => {
                AppMessage::CredentialProvider(ProviderMessage::KeyFile(path))
            }
            BrowseTarget::GatewayKey => AppMessage::GatewayField {
                field: ProfileField::KeyPath,
                value: path,
            },
        };
        self.app.update(message)
    }

    /// A program's path, `PuTTY` or the X server, typed and applied with Enter, "Browse..."
    /// beside it, what is said of it under it.
    fn tool_path_row(&self, row: SettingRow) -> Element<'_, Message> {
        let Some(path) = row.tool_path() else {
            return column![].into();
        };
        let typed = self.tool_paths_typed[path.index()]
            .as_deref()
            .unwrap_or_else(|| path.value(self.app.settings()));
        let field = text_input("", typed)
            .style(styles::text_input)
            .on_input(move |typed| Message::ToolPathEdited(path, typed))
            .on_submit(Message::ToolPathApply(path));
        let field = row![field, browse_button(BrowseTarget::Tool(path))]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center);
        let mut body = column![labelled(row_label(row), field)].spacing(spacing::SM);
        if let Some(hint) = row_hint(row) {
            body = body.push(text(hint).size(font_size::CAPTION));
        }
        body.into()
    }

    /// The default SSH mode's list, and "Apply to all saved sessions" beside it, as the C#
    /// row; what is said of it under them.
    fn ssh_default_mode_row(&self) -> Element<'_, Message> {
        let modes = pick_list(
            SshMode::ALL.map(DefaultSshModeChoice).to_vec(),
            Some(DefaultSshModeChoice(self.app.settings().ssh_default_mode)),
            |DefaultSshModeChoice(mode)| send(SettingsMessage::SshDefaultMode(mode)),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu);
        let apply = tooltip(
            button(text(fl!("ui-settings-apply-mode-to-all")).size(font_size::CAPTION))
                .style(styles::secondary)
                .on_press(send(SettingsMessage::ApplySshModeToAll)),
            text(fl!("ui-settings-apply-mode-to-all-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box);
        column![
            labelled(
                row_label(SettingRow::SshDefaultMode),
                row![modes, apply]
                    .spacing(spacing::SM)
                    .align_y(iced::Alignment::Center),
            ),
            text(fl!("ui-settings-ssh-default-mode-hint")).size(font_size::CAPTION),
        ]
        .spacing(spacing::SM)
        .into()
    }

    /// The default RDP mode's list, and "Apply to all saved sessions" beside it, as the C#
    /// row (`MainWindow.xaml:3356-3375`); what is said of it under them.
    fn rdp_default_mode_row(&self) -> Element<'_, Message> {
        let modes = pick_list(
            RdpMode::ALL.map(DefaultRdpModeChoice).to_vec(),
            Some(DefaultRdpModeChoice(self.app.settings().rdp_default_mode)),
            |DefaultRdpModeChoice(mode)| send(SettingsMessage::RdpDefaultMode(mode)),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu);
        let apply = tooltip(
            button(text(fl!("ui-settings-apply-mode-to-all")).size(font_size::CAPTION))
                .style(styles::secondary)
                .on_press(send(SettingsMessage::ApplyRdpModeToAll)),
            text(fl!("ui-settings-apply-mode-to-all-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box);
        column![
            labelled(
                row_label(SettingRow::RdpDefaultMode),
                row![modes, apply]
                    .spacing(spacing::SM)
                    .align_y(iced::Alignment::Center),
            ),
            text(fl!("ui-settings-rdp-default-mode-hint")).size(font_size::CAPTION),
        ]
        .spacing(spacing::SM)
        .into()
    }

    /// The program's path typed, or applied.
    pub(super) fn tool_path_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::ToolPathEdited(path, typed) => {
                self.tool_paths_typed[path.index()] = Some(typed);
                Vec::new()
            }
            Message::ToolPathApply(path) => match self.tool_paths_typed[path.index()].take() {
                Some(typed) => self.app.update(AppMessage::Settings(path.applied(typed))),
                None => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// A list or a card drawn by its own view: the trusted keys, the macros, the gateways,
    /// the RDP options and presets, and "Reset RDP defaults".
    fn list_card_row(&self, row: SettingRow) -> Element<'_, Message> {
        let keys = self.app.trusted_keys();
        match row {
            SettingRow::HostKeys => container(crate::trusted_keys_view::host_keys(
                keys,
                &self.host_key_search,
            ))
            .max_width(SETTINGS_WIDTH)
            .into(),
            SettingRow::FtpsCertificates => container(crate::trusted_keys_view::ftps_certificates(
                keys,
                &self.ftps_certificate_search,
            ))
            .max_width(SETTINGS_WIDTH)
            .into(),
            SettingRow::VncCertificates => container(crate::trusted_keys_view::vnc_certificates(
                keys,
                &self.vnc_certificate_search,
            ))
            .max_width(SETTINGS_WIDTH)
            .into(),
            SettingRow::Certificates => container(crate::trusted_keys_view::certificates(
                keys,
                &self.certificate_search,
            ))
            .max_width(SETTINGS_WIDTH)
            .into(),
            SettingRow::Macros => crate::macros_view::card(self.app.macros()),
            SettingRow::Gateways => crate::gateways_view::view(
                self.app.gateway_overview(),
                self.app.gateways(),
                &self.gateway_reassign,
            )
            .into(),
            SettingRow::RdpDefaults => {
                crate::rdp_options::defaults(self.app.settings().rdp_defaults)
            }
            SettingRow::RdpResolutionPresets => self.presets.view(),
            _ => tooltip(
                button(text(fl!("ui-settings-rdp-reset-defaults")))
                    .style(styles::secondary)
                    .on_press(send(SettingsMessage::ResetRdpDefaults)),
                text(fl!("ui-settings-rdp-reset-defaults-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
            .into(),
        }
    }

    /// The application PIN, the master password and the external credential provider, as
    /// the C# Security tab's cards.
    fn security_row(&self, row: SettingRow) -> Element<'_, Message> {
        match row {
            SettingRow::Pin => column![
                text(row_label(row)).size(font_size::SUBTITLE),
                labelled(
                    if self.app.settings().pin.is_some() {
                        fl!("ui-settings-pin-enabled")
                    } else {
                        fl!("ui-settings-pin-disabled")
                    },
                    button(text(fl!("ui-settings-pin-configure")))
                        .style(styles::secondary)
                        .on_press(Message::App(AppMessage::Pin(PinMessage::Configure))),
                ),
            ]
            .spacing(spacing::SM)
            .into(),
            SettingRow::Vault => self.vault_row(),
            _ => container(crate::provider_view::card(&self.app, &self.provider_unlock))
                .max_width(SETTINGS_WIDTH)
                .into(),
        }
    }

    /// The master password: on or off, and what can be done with it.
    fn vault_row(&self) -> Element<'_, Message> {
        let enabled = self.app.vault_status() != VaultStatus::Missing;
        let mut actions = row![
            text(if enabled {
                fl!("ui-settings-vault-enabled")
            } else {
                fl!("ui-settings-vault-disabled")
            }),
            iced::widget::space::horizontal(),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        if enabled {
            actions = actions
                .push(
                    button(text(fl!("ui-settings-vault-change")))
                        .style(styles::secondary)
                        .on_press(Message::App(AppMessage::ChangeMasterPassword)),
                )
                .push(
                    button(text(fl!("ui-settings-vault-disable")))
                        .style(styles::secondary)
                        .on_press(Message::App(AppMessage::DisableMasterPassword)),
                );
        } else {
            actions = actions.push(
                button(text(fl!("ui-settings-vault-enable")))
                    .style(styles::primary)
                    .on_press(Message::App(AppMessage::ShowVault)),
            );
        }
        let mut body = column![
            text(row_label(SettingRow::Vault)).size(font_size::SUBTITLE),
            text(fl!("ui-settings-vault-explanation")).size(font_size::CAPTION),
            actions,
        ]
        .spacing(spacing::SM);
        // Windows Hello unlocking the vault, only with one, as the C#
        // `VaultHelloSectionVisible`.
        if enabled {
            body = body.push(self.vault_hello_row());
        }
        body.into()
    }

    /// Windows Hello unlocking the vault: what stands, and enabling or disabling it, as the
    /// C# settings card's buttons and status line.
    fn vault_hello_row(&self) -> Element<'_, Message> {
        let card = self.app.vault_hello_card();
        let mut line = row![].spacing(spacing::SM).align_y(iced::Alignment::Center);
        if let Some(status) = card.status {
            line = line.push(text(vault_hello_status(status)).size(font_size::CAPTION));
        }
        line = line.push(iced::widget::space::horizontal());
        let hello = |message| Message::App(AppMessage::VaultHello(message));
        if card.can_enable {
            line = line.push(
                button(text(fl!("ui-settings-vault-hello-enable")))
                    .style(styles::secondary)
                    .on_press(hello(VaultHelloMessage::Enable)),
            );
        }
        if card.can_disable {
            line = line.push(
                button(text(fl!("ui-settings-vault-hello-disable")))
                    .style(styles::secondary)
                    .on_press(hello(VaultHelloMessage::Disable)),
            );
        }
        line.into()
    }

    /// The C# security overview: the security-relevant choices as they stand, how many need
    /// attention, and for each that does, why and the way to it.
    fn posture_card(&self) -> Element<'_, Message> {
        let lines = posture(
            self.app.settings(),
            self.app.vault_status() != VaultStatus::Missing,
        );
        let risky = lines.iter().filter(|line| line.risky).count();
        let summary = if risky == 0 {
            fl!("ui-settings-posture-summary-none")
        } else {
            fl!("ui-settings-posture-summary", count = risky)
        };
        let mut card = column![
            text(fl!("ui-settings-posture-title"))
                .size(font_size::SUBTITLE)
                .font(styles::SEMIBOLD),
            text(fl!("ui-settings-posture-description")).size(font_size::CAPTION),
            row![posture_mark(risky > 0), text(summary)]
                .spacing(spacing::SM)
                .align_y(iced::Alignment::Center),
        ]
        .spacing(spacing::SM);
        for line in lines {
            card = card.push(posture_line(line));
        }
        container(card)
            .padding(spacing::LG)
            .width(Length::Fill)
            .style(styles::card)
            .into()
    }

    /// The search typed, "Find modified settings", or a "Go to setting" followed.
    pub(super) fn settings_search_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SettingsSearch(typed) => {
                self.settings_search = typed;
                self.settings_highlight = None;
                Task::none()
            }
            Message::FindModifiedSettings => {
                // As the C#: the marker's word searched, which finds the rows marked.
                self.settings_search = fl!("ui-settings-modified-badge");
                self.settings_highlight = None;
                let field = search_field_id();
                operation::focus(field.clone()).chain(operation::move_cursor_to_end(field))
            }
            Message::GoToSetting(row) => {
                self.settings_tab = row.tab();
                self.settings_search.clear();
                self.settings_highlight = Some(row);
                operation::snap_to(scroll_id(), RelativeOffset::START)
            }
            _ => Task::none(),
        }
    }

    /// `row`'s "Reset": what is typed in its field dropped, then its default put back
    /// through the message its own choice sends.
    pub(super) fn reset_setting(&mut self, row: SettingRow) -> Vec<Effect> {
        if let Some(field) = row.session_field() {
            self.session_typed[field.index()] = None;
        }
        if let Some(path) = row.tool_path() {
            self.tool_paths_typed[path.index()] = None;
        }
        match row {
            SettingRow::FontSize => self.font_size_typed = None,
            SettingRow::SessionLogDirectory => self.log_directory = None,
            SettingRow::ExternalEditor => self.editor_typed = None,
            _ => {}
        }
        match row.reset(self.app.settings()) {
            Some(message) => self.app.update(AppMessage::Settings(message)),
            None => Vec::new(),
        }
    }
}
