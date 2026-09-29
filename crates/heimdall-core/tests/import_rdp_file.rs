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

//! `.rdp` files read as the C# Heimdall reads them: what a profile carries, what it does not,
//! and what is refused.

use std::collections::HashSet;

use heimdall_core::import::rdp_file::{Patch, Refusal, auto_rename, parse, proposed_name};
use heimdall_core::profile::{AudioPlayback, ColorDepth, ProfileId, RdpProfile, Resolution};

fn profile_of(text: &str) -> RdpProfile {
    Patch::of(&parse(text))
        .expect("a profile")
        .new_profile(ProfileId::new("new"), "new".to_owned())
}

/// A file as mstsc saves it, trimmed.
const MSTSC: &str = "screen mode id:i:2\r\nuse multimon:i:0\r\ndesktopwidth:i:1920\r\ndesktopheight:i:1080\r\nsession bpp:i:24\r\ncompression:i:1\r\naudiomode:i:1\r\nredirectclipboard:i:0\r\nredirectdrives:i:1\r\nfull address:s:dc.lab:3390\r\nusername:s:LAB\\admin\r\ndomain:s:LAB\r\nadministrative session:i:1\r\nenablecredsspsupport:i:0\r\ndynamic resolution:i:0\r\npassword 51:b:01000000D08C9DDF\r\nkdcproxyname:s:\r\n";

#[test]
fn a_file_saved_by_mstsc_gives_its_settings_and_nothing_secret() {
    let file = parse(MSTSC);
    assert!(file.has_password, "noticed, never read");
    let profile = profile_of(MSTSC);
    assert_eq!((profile.host.as_str(), profile.port), ("dc.lab", 3390));
    assert_eq!(profile.username.as_deref(), Some(r"LAB\admin"));
    assert_eq!(profile.domain.as_deref(), Some("LAB"));
    assert!(profile.options.admin_session);
    assert_eq!(profile.options.audio, AudioPlayback::OnServer);
    assert!(!profile.redirect_clipboard);
    assert!(profile.redirect_drives);
    assert_eq!(profile.options.color_depth, ColorDepth::Bpp24);
    assert!(profile.allow_tls_only, "enablecredsspsupport 0: no NLA");
    assert!(!profile.options.dynamic_resolution);
    assert_eq!(
        profile.options.resolution,
        Resolution::FitWindow,
        "size keys not carried"
    );
    assert_eq!(
        file.not_carried,
        [
            "screen mode id",
            "use multimon",
            "desktopwidth",
            "desktopheight",
            "compression"
        ]
    );
    assert_eq!(file.unknown, 1, "kdcproxyname");
    assert!(Patch::of(&file).expect("patch").is_partial(&file));
}

#[test]
fn an_address_alone_takes_the_csharp_defaults_and_is_not_partial() {
    let file = parse("full address:s:srv.lab\n");
    let patch = Patch::of(&file).expect("patch");
    assert!(!patch.is_partial(&file));
    let profile = patch.new_profile(ProfileId::new("p"), "p".to_owned());
    assert_eq!(profile.port, 3389);
    assert!(profile.redirect_clipboard && !profile.redirect_drives && !profile.allow_tls_only);
    assert_eq!(profile.options.audio, AudioPlayback::default());
}

#[test]
fn addresses_are_read_as_the_csharp_splits_them() {
    for (address, host, port, out_of_range) in [
        ("srv", "srv", 3389, false),
        ("srv:3390", "srv", 3390, false),
        ("srv:70000", "srv", 3389, true),
        ("srv:0", "srv", 3389, true),
        ("[fe80::1]:3391", "[fe80::1]", 3391, false),
        ("[fe80::1]", "[fe80::1]", 3389, false),
        ("fe80::1", "fe80::1", 3389, false),
    ] {
        let patch = Patch::of(&parse(&format!("full address:s:{address}\n"))).expect(address);
        assert_eq!(
            (patch.host.as_str(), patch.port, patch.port_out_of_range),
            (host, port, out_of_range),
            "{address}"
        );
    }
    let alternate = Patch::of(&parse(
        "full address:s:\nalternate full address:s:alt.lab\n",
    ))
    .expect("the alternate address");
    assert_eq!(alternate.host, "alt.lab");
    assert_eq!(
        Patch::of(&parse("username:s:x\n")),
        Err(Refusal::InvalidAddress)
    );
}

#[test]
fn a_gateway_in_use_is_refused_and_a_gateway_not_in_use_is_not() {
    assert_eq!(
        Patch::of(&parse(
            "full address:s:srv\ngatewayhostname:s:gw.lab\ngatewayusagemethod:i:1\n"
        )),
        Err(Refusal::NeedsRdGateway)
    );
    for text in [
        "full address:s:srv\ngatewayhostname:s:gw.lab\ngatewayusagemethod:i:0\n",
        "full address:s:srv\ngatewayhostname:s:gw.lab\n",
        "full address:s:srv\ngatewayhostname:s:\ngatewayusagemethod:i:2\n",
    ] {
        assert!(Patch::of(&parse(text)).is_ok(), "{text}");
    }
}

#[test]
fn drives_follow_the_newer_key_and_a_named_list_is_said_widened() {
    let drives = |text: &str| {
        let file = parse(&format!("full address:s:srv\n{text}"));
        let patch = Patch::of(&file).expect("patch");
        let profile = patch.new_profile(ProfileId::new("p"), "p".to_owned());
        (profile.redirect_drives, patch.drives_widened)
    };
    assert_eq!(
        drives("drivestoredirect:s:*\nredirectdrives:i:0\n"),
        (true, false)
    );
    assert_eq!(drives("drivestoredirect:s:C:;\n"), (true, true));
    assert_eq!(drives("redirectdrives:i:1\n"), (true, false));
    assert_eq!(
        drives("drivestoredirect:s:\nredirectdrives:i:1\n"),
        (true, false)
    );
    assert_eq!(drives("drivestoredirect:s:\n"), (false, false));
    assert_eq!(drives(""), (false, false));
}

#[test]
fn nla_audio_and_depth_map_as_the_csharp_maps_them() {
    let nla = |value: &str| {
        profile_of(&format!(
            "full address:s:srv\nenablecredsspsupport:i:{value}\n"
        ))
        .allow_tls_only
    };
    assert!(!nla("1"));
    assert!(nla("0"));
    assert!(!nla("2"), "outside 0 and 1: left as it was");
    let audio = |value: &str| {
        profile_of(&format!("full address:s:srv\naudiomode:i:{value}\n"))
            .options
            .audio
    };
    assert_eq!(audio("0"), AudioPlayback::Local);
    assert_eq!(audio("1"), AudioPlayback::OnServer);
    assert_eq!(audio("2"), AudioPlayback::Off);
    let depth = |value: &str| {
        profile_of(&format!("full address:s:srv\nsession bpp:i:{value}\n"))
            .options
            .color_depth
    };
    assert_eq!(depth("15"), ColorDepth::Bpp16);
    assert_eq!(depth("24"), ColorDepth::Bpp24);
    assert_eq!(depth("32"), ColorDepth::Bpp32);
}

#[test]
fn a_value_of_the_wrong_type_is_unknown_not_read() {
    let file =
        parse("full address:i:12\nredirectclipboard:s:yes\nno colon here\nfull address:s:srv\n");
    assert_eq!(file.unknown, 2);
    assert!(Patch::of(&file).is_ok(), "the well-typed address");
}

#[test]
fn replacing_writes_only_what_the_file_names() {
    let mut existing = profile_of("full address:s:old.lab\n");
    existing.id = ProfileId::new("kept");
    existing.name = "Kept".to_owned();
    existing.vault_entry = Some("Win/Kept".to_owned());
    existing.gateway = Some(ProfileId::new("edge"));
    existing.options.resolution = Resolution::Fixed;
    existing.options.fixed_width = 1280;
    existing.domain = Some("OLD".to_owned());
    existing.redirect_clipboard = false;
    Patch::of(&parse("full address:s:new.lab:3390\nusername:s:ops\n"))
        .expect("patch")
        .apply(&mut existing);
    assert_eq!((existing.host.as_str(), existing.port), ("new.lab", 3390));
    assert_eq!(existing.username.as_deref(), Some("ops"));
    assert_eq!(existing.id.as_str(), "kept");
    assert_eq!(existing.name, "Kept");
    assert_eq!(
        existing.domain.as_deref(),
        Some("OLD"),
        "not named by the file"
    );
    assert!(!existing.redirect_clipboard, "not named by the file");
    assert_eq!(existing.vault_entry.as_deref(), Some("Win/Kept"));
    assert_eq!(
        existing.gateway.as_ref().map(ProfileId::as_str),
        Some("edge")
    );
    assert_eq!(
        (existing.options.resolution, existing.options.fixed_width),
        (Resolution::Fixed, 1280)
    );
}

#[test]
fn a_file_is_named_by_its_file_unless_generic() {
    let file = parse("full address:s:srv.lab\nalternate full address:s:alt.lab\n");
    assert_eq!(proposed_name("DC01", &file, "Imported RDP"), "DC01");
    assert_eq!(proposed_name("Default", &file, "Imported RDP"), "alt.lab");
    assert_eq!(
        proposed_name(
            "Remote Desktop Connection",
            &parse("full address:s:srv.lab\n"),
            "x"
        ),
        "srv.lab"
    );
    assert_eq!(
        proposed_name("connection", &parse(""), "Imported RDP"),
        "Imported RDP"
    );
}

#[test]
fn auto_rename_takes_the_first_free_number_from_2() {
    let taken: HashSet<String> = ["dc (imported 2)".to_owned()].into();
    let rename = |base: &str, n: u32| format!("{base} (Imported {n})");
    assert_eq!(auto_rename("DC", &taken, &rename), "DC (Imported 3)");
    assert_eq!(auto_rename("Web", &taken, &rename), "Web (Imported 2)");
}
