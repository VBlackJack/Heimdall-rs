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
    AudioPlayback, ColorDepth, Forwards, ProfileId, RdpDefaults, RdpExtras, RdpOptions, RdpProfile,
    RdpSwitch,
};
use heimdall_core::settings::{SETTINGS_FILE_NAME, Settings};
use heimdall_core::store::ProfileStore;

/// A profile with options of its own, all different from the defaults.
fn own(follow_defaults: bool) -> RdpProfile {
    RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
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
            redirect_printers: false,
            redirect_com_ports: false,
            redirect_smart_cards: false,
            redirect_webcam: false,
            redirect_usb: false,
            nla: true,
            strict_server_authentication: false,
            color_depth: ColorDepth::Bpp32,
            audio: AudioPlayback::Off,
            microphone: false,
            dynamic_resolution: true,
            multi_monitor: false,
            bitmap_caching: true,
            compression: true,
            hardware_acceleration: false,
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
        ..RdpDefaults::default()
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
            redirect_usb: true,
            strict_server_authentication: true,
            compression: false,
            ..RdpDefaults::default()
        },
        ..Settings::default()
    };
    settings.save(&path).expect("save");
    assert_eq!(Settings::load(&path).expect("load"), settings);
}

#[test]
fn each_csharp_default_reaches_a_following_profile_and_one_with_its_own_keeps_them() {
    // Every default the opposite of its factory value, so each one shows.
    let factory = RdpDefaults::default();
    let defaults = RdpDefaults {
        redirect_printers: !factory.redirect_printers,
        redirect_com_ports: !factory.redirect_com_ports,
        redirect_smart_cards: !factory.redirect_smart_cards,
        redirect_webcam: !factory.redirect_webcam,
        redirect_usb: !factory.redirect_usb,
        strict_server_authentication: !factory.strict_server_authentication,
        microphone: !factory.microphone,
        multi_monitor: !factory.multi_monitor,
        bitmap_caching: !factory.bitmap_caching,
        compression: !factory.compression,
        hardware_acceleration: !factory.hardware_acceleration,
        ..factory
    };
    let extras = own(true).effective(&defaults).extras;
    assert_eq!(
        [
            extras.redirect_printers,
            extras.redirect_com_ports,
            extras.redirect_smart_cards,
            extras.redirect_webcam,
            extras.redirect_usb,
            extras.strict_server_authentication,
            extras.microphone,
            extras.multi_monitor,
            extras.bitmap_caching,
            extras.compression,
            extras.hardware_acceleration,
        ],
        [
            defaults.redirect_printers,
            defaults.redirect_com_ports,
            defaults.redirect_smart_cards,
            defaults.redirect_webcam,
            defaults.redirect_usb,
            defaults.strict_server_authentication,
            defaults.microphone,
            defaults.multi_monitor,
            defaults.bitmap_caching,
            defaults.compression,
            defaults.hardware_acceleration,
        ],
        "every RdpDefault* the C# resolver reads"
    );
    // What the defaults do not cover stays the profile's.
    let mut kept = own(true);
    kept.extras.rd_gateway = Some("rdg.lab".to_owned());
    kept.extras.disable_udp = true;
    kept.extras.full_screen = true;
    kept.extras.monitors = vec![1];
    let effective = kept.clone().effective(&defaults);
    assert_eq!(
        (
            effective.extras.rd_gateway,
            effective.extras.disable_udp,
            effective.extras.full_screen,
            effective.extras.monitors,
        ),
        (Some("rdg.lab".to_owned()), true, true, vec![1])
    );
    // A profile with its own options keeps every one of them.
    let mut own_choice = own(false);
    own_choice.extras.strict_server_authentication = true;
    own_choice.extras.compression = false;
    assert_eq!(own_choice.clone().effective(&defaults), own_choice);
}

/// Every box of the profile form's RDP extras.
const SWITCHES: [RdpSwitch; 11] = [
    RdpSwitch::Printers,
    RdpSwitch::ComPorts,
    RdpSwitch::SmartCards,
    RdpSwitch::Webcam,
    RdpSwitch::Usb,
    RdpSwitch::Microphone,
    RdpSwitch::BitmapCaching,
    RdpSwitch::Compression,
    RdpSwitch::HardwareAcceleration,
    RdpSwitch::DisableUdp,
    RdpSwitch::FullScreen,
];

#[test]
fn each_extras_box_reads_and_writes_its_own_option_and_says_whether_the_defaults_take_it() {
    for switch in SWITCHES {
        for on in [true, false] {
            let mut extras = RdpExtras::default();
            let before = extras.clone();
            switch.set(&mut extras, on);
            assert_eq!(switch.is_on(&extras), on, "{switch:?}");
            // The others untouched: each box is one option.
            for other in SWITCHES.iter().filter(|other| **other != switch) {
                assert_eq!(other.is_on(&extras), other.is_on(&before), "{switch:?}");
            }
            // A box the defaults take is the defaults' whatever the profile says; one
            // they do not have stays the profile's.
            let mut following = own(true);
            following.extras = extras;
            let effective = following.effective(&RdpDefaults::default()).extras;
            let defaults_value = {
                let mut factory = own(true);
                switch.set(&mut factory.extras, !on);
                switch.is_on(&factory.effective(&RdpDefaults::default()).extras)
            };
            if switch.follows_defaults() {
                assert_eq!(switch.is_on(&effective), defaults_value, "{switch:?}");
            } else {
                assert_eq!(switch.is_on(&effective), on, "{switch:?}");
            }
        }
    }
}

#[test]
fn a_settings_file_written_before_the_new_defaults_reads_as_it_did() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    // The [rdp] section as it was written with the first seven defaults alone.
    std::fs::write(
        &path,
        "version = 1\n\n[rdp]\nredirect_clipboard = false\nredirect_drives = true\nnla = false\n\
         color_depth = 16\naudio = \"local\"\ndynamic_resolution = false\nauto_reconnect = false\n",
    )
    .expect("write");
    let read = Settings::load(&path).expect("load").rdp_defaults;
    assert_eq!(
        read,
        RdpDefaults {
            redirect_clipboard: false,
            redirect_drives: true,
            nla: false,
            color_depth: ColorDepth::Bpp16,
            audio: AudioPlayback::Local,
            dynamic_resolution: false,
            auto_reconnect: false,
            ..RdpDefaults::default()
        },
        "its seven kept, the others at their C# defaults"
    );
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
