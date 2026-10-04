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

//! A profile following the application's RDP options opens with them, as they are when it
//! opens, as the C# `RdpProfileResolver`; one with its own keeps them.

use std::path::Path;

use heimdall_app::profile_draft::{DraftProtocol, ProfileField, ProfileToggle};
use heimdall_app::{
    App, AppConfig, ConnectAs, ConnectionEvent, Effect, Message, NetworkFailure, QuickResult,
    SettingsMessage, SystemCredentials, TabProfile, UiError,
};
use heimdall_core::profile::{
    AudioPlayback, ColorDepth, Forwards, ProfileId, RdpDefaults, RdpOptions, RdpProfile,
};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::Ending;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn profile(id: &str, follow_defaults: bool) -> RdpProfile {
    RdpProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: format!("{id}.lab"),
        port: 3389,
        username: None,
        domain: None,
        allow_tls_only: false,
        gateway: None,
        redirect_clipboard: true,
        redirect_drives: false,
        vault_entry: None,
        forwards: Forwards::default(),
        options: RdpOptions {
            color_depth: ColorDepth::Bpp32,
            audio: AudioPlayback::Off,
            ..RdpOptions::default()
        },
        follow_defaults,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([profile("following", true), profile("own", false)]);
    store.merge([heimdall_core::profile::SshProfile {
        id: ProfileId::new("web"),
        name: "web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

fn defaults(app: &mut App, color_depth: ColorDepth) {
    app.update(Message::Settings(SettingsMessage::RdpDefaults(
        RdpDefaults {
            color_depth,
            audio: AudioPlayback::Local,
            ..RdpDefaults::default()
        },
    )));
}

/// The profile the attempt of `effects` connects with.
fn asked(effects: &[Effect]) -> RdpProfile {
    match effects {
        [Effect::ConnectRdp { request, .. }] => request.profile.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_following_profile_opens_with_the_defaults_and_one_with_its_own_keeps_them() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    defaults(&mut app, ColorDepth::Bpp16);

    let effects = app.update(Message::OpenRdp(ProfileId::new("following")));
    let following = asked(&effects);
    assert_eq!(
        (following.options.color_depth, following.options.audio),
        (ColorDepth::Bpp16, AudioPlayback::Local)
    );
    let Some(TabProfile::Rdp(shown)) = app.tabs.last().map(|tab| tab.profile.clone()) else {
        panic!("no RDP tab");
    };
    assert_eq!(
        shown.options.color_depth,
        ColorDepth::Bpp16,
        "the tab draws with them"
    );

    let own = asked(&app.update(Message::OpenRdp(ProfileId::new("own"))));
    assert_eq!(
        (own.options.color_depth, own.options.audio),
        (ColorDepth::Bpp32, AudioPlayback::Off)
    );
    assert_eq!(
        app.rdp_profiles()
            .iter()
            .find(|p| p.id.as_str() == "following")
            .expect("saved")
            .options
            .color_depth,
        ColorDepth::Bpp32,
        "never written back into the profile"
    );
}

#[test]
fn a_reconnect_takes_the_defaults_as_they_are_then() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    defaults(&mut app, ColorDepth::Bpp16);
    let effects = app.update(Message::OpenRdp(ProfileId::new("following")));
    let [Effect::ConnectRdp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Ended {
            reason: Ending::Logoff,
        },
    });
    defaults(&mut app, ColorDepth::Bpp24);
    let again = asked(&app.update(Message::ReconnectTab(tab)));
    assert_eq!(again.options.color_depth, ColorDepth::Bpp24);
}

#[test]
fn a_new_rdp_profile_follows_the_defaults_and_an_edit_keeps_what_was_chosen() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::NewProfile);
    app.update(Message::ChooseProtocol(DraftProtocol::Rdp));
    for (field, value) in [(ProfileField::Name, "new"), (ProfileField::Host, "new.lab")] {
        app.update(Message::ProfileField {
            field,
            value: value.to_owned(),
        });
    }
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    let new = app
        .rdp_profiles()
        .iter()
        .find(|p| p.name == "new")
        .expect("saved");
    assert!(new.follow_defaults, "ticked for a new profile, as the C#");

    app.update(Message::EditProfile(ProfileId::new("own")));
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    assert!(
        !app.rdp_profiles()
            .iter()
            .find(|p| p.id.as_str() == "own")
            .expect("saved")
            .follow_defaults,
        "its own options, kept"
    );
    app.update(Message::EditProfile(ProfileId::new("own")));
    app.update(Message::ProfileToggle {
        toggle: ProfileToggle::FollowDefaults,
        on: true,
    });
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    assert!(
        app.rdp_profiles()
            .iter()
            .find(|p| p.id.as_str() == "own")
            .expect("saved")
            .follow_defaults
    );
}

#[test]
fn the_defaults_are_saved_with_the_settings() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    defaults(&mut app, ColorDepth::Bpp24);
    assert_eq!(app.settings().rdp_defaults.color_depth, ColorDepth::Bpp24);
    let again = self::app(dir.path());
    assert_eq!(
        again.settings().rdp_defaults.color_depth,
        ColorDepth::Bpp24,
        "read back when the application starts"
    );
}

#[test]
fn a_session_opened_without_a_saved_profile_takes_the_defaults() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    defaults(&mut app, ColorDepth::Bpp16);
    let quick = asked(&app.update(Message::QuickConnect(QuickResult::Rdp {
        host: "ts.lab".to_owned(),
    })));
    assert_eq!(
        quick.options.color_depth,
        ColorDepth::Bpp16,
        "quick connect"
    );
    let other = asked(&app.update(Message::ConnectAs {
        id: ProfileId::new("web"),
        protocol: ConnectAs::Rdp,
    }));
    assert_eq!(
        other.options.color_depth,
        ColorDepth::Bpp16,
        "connect as RDP"
    );
}

#[test]
fn editing_a_following_profile_keeps_it_following() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::EditProfile(ProfileId::new("following")));
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    assert!(
        app.rdp_profiles()
            .iter()
            .find(|p| p.id.as_str() == "following")
            .expect("saved")
            .follow_defaults
    );
}

#[test]
fn a_desktop_back_by_itself_takes_the_defaults_as_they_are_then() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    defaults(&mut app, ColorDepth::Bpp16);
    let effects = app.update(Message::OpenRdp(ProfileId::new("following")));
    let [Effect::ConnectRdp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::RdpReady {
            framebuffer: heimdall_rdp::Framebuffer::new(64, 48),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    });
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::Network {
            failure: NetworkFailure::Reset,
            detail: "reset".to_owned(),
        }),
    });
    defaults(&mut app, ColorDepth::Bpp24);
    let again = asked(&app.update(Message::AutoReconnect { tab, attempt }));
    assert_eq!(again.options.color_depth, ColorDepth::Bpp24);
    let Some(TabProfile::Rdp(shown)) = app.tab(tab).map(|tab| tab.profile.clone()) else {
        panic!("no RDP tab");
    };
    assert_eq!(
        shown.options.color_depth,
        ColorDepth::Bpp24,
        "the tab draws with them"
    );
}

#[test]
fn the_resolution_presets_are_kept_within_the_limits_and_reset_with_the_rdp_settings_once_agreed() {
    use heimdall_app::Dialog;
    use heimdall_core::profile::RESOLUTION_PRESETS;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Settings(SettingsMessage::RdpResolutionPresets(
        vec![(1366, 768), (99, 99)],
    )));
    assert_eq!(
        app.settings().rdp_resolution_presets,
        RESOLUTION_PRESETS,
        "one out of the limits: refused whole"
    );
    app.update(Message::Settings(SettingsMessage::RdpResolutionPresets(
        vec![(1366, 768)],
    )));
    assert_eq!(app.settings().resolution_presets(), [(1366, 768)]);
    defaults(&mut app, ColorDepth::Bpp16);

    app.update(Message::Settings(SettingsMessage::ResetRdpDefaults));
    assert!(
        matches!(app.dialog, Some(Dialog::ConfirmResetRdpDefaults)),
        "asked first, as the C#"
    );
    app.update(Message::DismissDialog);
    assert_eq!(app.settings().resolution_presets(), [(1366, 768)], "kept");

    app.update(Message::Settings(SettingsMessage::ResetRdpDefaults));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.settings().rdp_resolution_presets, RESOLUTION_PRESETS);
    assert_eq!(app.settings().rdp_defaults, RdpDefaults::default());
    // Saved, as every setting.
    let read = heimdall_core::settings::Settings::load(
        &dir.path().join(heimdall_core::settings::SETTINGS_FILE_NAME),
    )
    .expect("load");
    assert_eq!(read.rdp_resolution_presets, RESOLUTION_PRESETS);
}
