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

//! The application's RDP options, which a profile following them takes, as the C#
//! `RdpDefault*` settings and `RdpUseGlobalDefaults`.

use heimdall_core::export;
use heimdall_core::profile::{
    AudioPlayback, ColorDepth, Forwards, ProfileId, RdpDefaults, RdpOptions, RdpProfile,
};
use heimdall_core::settings::{SETTINGS_FILE_NAME, Settings};
use heimdall_core::store::ProfileStore;

/// A profile with options of its own, all different from the defaults.
fn own(follow_defaults: bool) -> RdpProfile {
    RdpProfile {
        id: ProfileId::new("dc"),
        name: "dc".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: None,
        domain: None,
        allow_tls_only: true,
        gateway: None,
        redirect_clipboard: false,
        redirect_drives: true,
        vault_entry: None,
        forwards: Forwards::default(),
        options: RdpOptions {
            color_depth: ColorDepth::Bpp16,
            audio: AudioPlayback::Local,
            dynamic_resolution: false,
            admin_session: true,
            ..RdpOptions::default()
        },
        follow_defaults,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }
}

#[test]
fn the_csharp_defaults_are_the_application_ones_until_changed() {
    assert_eq!(
        RdpDefaults::default(),
        RdpDefaults {
            redirect_clipboard: true,
            redirect_drives: false,
            nla: true,
            color_depth: ColorDepth::Bpp32,
            audio: AudioPlayback::Off,
            dynamic_resolution: true,
            auto_reconnect: true,
        }
    );
    assert_eq!(Settings::default().rdp_defaults, RdpDefaults::default());
}

#[test]
fn a_following_profile_takes_the_defaults_and_keeps_what_they_do_not_cover() {
    let defaults = RdpDefaults {
        redirect_clipboard: true,
        redirect_drives: false,
        nla: false,
        color_depth: ColorDepth::Bpp24,
        audio: AudioPlayback::OnServer,
        dynamic_resolution: true,
        auto_reconnect: true,
    };
    let effective = own(true).effective(&defaults);
    assert_eq!(
        (
            effective.redirect_clipboard,
            effective.redirect_drives,
            effective.allow_tls_only,
            effective.options.color_depth,
            effective.options.audio,
            effective.options.dynamic_resolution,
        ),
        (
            true,
            false,
            true,
            ColorDepth::Bpp24,
            AudioPlayback::OnServer,
            true
        ),
        "every option the defaults cover"
    );
    assert!(effective.options.admin_session, "not a default: its own");
    assert!(effective.follow_defaults, "still following");

    assert_eq!(
        own(false).effective(&defaults),
        own(false),
        "a profile with its own options keeps them"
    );
}

#[test]
fn following_is_saved_only_when_on_and_a_profile_saved_before_keeps_its_own() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("profiles.toml");
    let mut store = ProfileStore::open(&path).expect("store");
    let mut following = own(true);
    following.id = ProfileId::new("following");
    store.merge_rdp([own(false), following]);
    store.save().expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert_eq!(
        text.matches("follow_defaults").count(),
        1,
        "written once, for the one following:\n{text}"
    );
    let read = ProfileStore::open(&path).expect("read");
    let flags: Vec<bool> = read
        .rdp_profiles()
        .iter()
        .map(|profile| profile.follow_defaults)
        .collect();
    assert_eq!(flags, [false, true]);
}

#[test]
fn the_defaults_are_written_in_the_settings_and_read_back() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    std::fs::write(&path, "version = 1\n").expect("write");
    assert_eq!(
        Settings::load(&path).expect("load").rdp_defaults,
        RdpDefaults::default(),
        "absent: the C# defaults"
    );
    let settings = Settings {
        rdp_defaults: RdpDefaults {
            redirect_clipboard: false,
            redirect_drives: true,
            nla: false,
            color_depth: ColorDepth::Bpp16,
            audio: AudioPlayback::Local,
            dynamic_resolution: false,
            auto_reconnect: true,
        },
        ..Settings::default()
    };
    settings.save(&path).expect("save");
    assert_eq!(Settings::load(&path).expect("load"), settings);
}

#[test]
fn a_following_profile_is_exported_with_the_options_in_effect() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge_rdp([own(true)]);
    let defaults = RdpDefaults {
        color_depth: ColorDepth::Bpp24,
        ..RdpDefaults::default()
    };
    let document = export::csharp(&store, &|_| String::new(), &defaults);
    let value: serde_json::Value = serde_json::from_str(&document).expect("json");
    let server = &value["servers"][0];
    assert_eq!(server["rdpColorDepth"], 24, "{server}");
    assert_eq!(server["rdpRedirectClipboard"], true);
    assert_eq!(server["rdpRedirectDrives"], false);
    assert_eq!(
        server["rdpUseGlobalDefaults"], false,
        "the file means the same whatever the reader's defaults"
    );
}
