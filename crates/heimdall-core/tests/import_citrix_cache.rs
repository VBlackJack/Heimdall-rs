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

//! Citrix Workspace's local cache read as the C# `CitrixCacheScanner` reads it.

use heimdall_core::import::citrix_cache::{
    CacheWarning, MAX_APPS_PER_FILE, MAX_APPS_TOTAL, MAX_CACHE_FILE_BYTES, cache_directory_valid,
    extract_citrix_id, normalize_store_prefix, parse, scan_folder, self_service_path_valid,
};

/// A cache file as Citrix Workspace writes one, in a namespace.
const CACHE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<resources xmlns="http://citrix.com/selfservice">
  <resource>
    <FriendlyName>Notepad</FriendlyName>
    <Category>Tools\Editors</Category>
    <LaunchCommandLine> -qlaunch "Notepad" -s store-abc </LaunchCommandLine>
  </resource>
  <resource>
    <FriendlyName>Excel 2024</FriendlyName>
    <Description>Spreadsheets</Description>
    <resourceType>Application</resourceType>
    <Category>COMMONAPPS - NIGHT</Category>
    <LaunchCommandLine>-qlaunch "Excel 2024" -s store-abc</LaunchCommandLine>
    <icaLaunchUrl>https://Store.Corp.Lab:443/Citrix/Store/resources/v2/abc/launch/ica</icaLaunchUrl>
  </resource>
  <resource>
    <FriendlyName>   </FriendlyName>
    <LaunchCommandLine>-qlaunch "Nameless"</LaunchCommandLine>
  </resource>
  <resource>
    <FriendlyName>Word</FriendlyName>
  </resource>
  <resource>
    <FriendlyName>Desktop</FriendlyName>
    <LaunchCommandLine>-qlaunch "Desktop"</LaunchCommandLine>
  </resource>
</resources>
"#;

#[test]
fn each_resource_with_a_name_and_a_launch_line_is_an_application() {
    let apps = parse(CACHE).expect("parsed");
    let names: Vec<&str> = apps.iter().map(|app| app.name.as_str()).collect();
    assert_eq!(names, ["Notepad", "Excel 2024", "Desktop"]);
    assert_eq!(
        apps[0].launch_line.as_str(),
        "-qlaunch \"Notepad\" -s store-abc",
        "trimmed, as the C# reads an element"
    );
}

#[test]
fn an_application_is_filed_under_citrix_and_its_category() {
    let apps = parse(CACHE).expect("parsed");
    assert_eq!(apps[0].group().as_deref(), Some("Citrix/Tools/Editors"));
    assert_eq!(
        apps[1].group().as_deref(),
        Some("Citrix/COMMONAPPS - NIGHT")
    );
    assert_eq!(apps[2].group(), None);
}

#[test]
fn the_store_is_the_scheme_and_host_of_the_first_launch_address() {
    let apps = parse(CACHE).expect("parsed");
    for app in &apps {
        assert_eq!(app.store_url.as_deref(), Some("https://store.corp.lab"));
    }
    let without = parse(
        "<resources><resource><FriendlyName>A</FriendlyName>\
         <LaunchCommandLine>-qlaunch A</LaunchCommandLine>\
         <icaLaunchUrl>not an address</icaLaunchUrl></resource></resources>",
    )
    .expect("parsed");
    assert_eq!(without[0].store_url, None);
}

#[test]
fn a_dtd_is_refused_and_its_entities_never_expanded() {
    let doctype = r#"<?xml version="1.0"?>
<!DOCTYPE resources [<!ENTITY secret SYSTEM "file:///C:/Windows/win.ini">]>
<resources><resource><FriendlyName>&secret;</FriendlyName>
<LaunchCommandLine>-qlaunch x</LaunchCommandLine></resource></resources>"#;
    assert!(parse(doctype).is_err());
}

#[test]
fn the_cache_files_of_a_folder_are_read_and_the_others_left() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("Store1_Cache.xml"), CACHE).expect("write");
    std::fs::write(dir.path().join("broken_cache.XML"), "<resources>").expect("write");
    std::fs::write(dir.path().join("other.xml"), CACHE).expect("write");
    let scan = scan_folder(dir.path(), None);
    assert_eq!(scan.apps.len(), 3, "only the cache file's");
    assert!(matches!(
        scan.warnings.as_slice(),
        [CacheWarning::Unreadable { file, .. }] if file == "broken_cache.XML"
    ));

    let empty = tempfile::tempdir().expect("dir");
    assert_eq!(
        scan_folder(empty.path(), None).warnings,
        [CacheWarning::NoCacheFiles]
    );
    assert_eq!(
        scan_folder(&empty.path().join("missing"), None).warnings,
        [CacheWarning::FolderMissing]
    );
}

/// A cache as Citrix Cloud / Workspace writes it, with the subscription state of each
/// resource (C# v2026.102301 fixtures): subscribed, unsubscribed, and legacy without one.
const STATEFUL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<resources>
  <resource>
    <FriendlyName>Subscribed app</FriendlyName>
    <ServerState>Subscribed</ServerState>
    <LaunchCommandLine>-qlaunch "Subscribed app" -CitrixID "store@@Subscribed app" -s store</LaunchCommandLine>
  </resource>
  <resource>
    <FriendlyName>Lowercase subscribed</FriendlyName>
    <ServerState> subscribed </ServerState>
    <LaunchCommandLine>-qlaunch x -CitrixID store@@Lower</LaunchCommandLine>
  </resource>
  <resource>
    <FriendlyName>Catalog only</FriendlyName>
    <ServerState>Unsubscribed</ServerState>
    <LaunchCommandLine>-qlaunch "Catalog only" -CitrixID "store@@Catalog only"</LaunchCommandLine>
  </resource>
  <resource>
    <FriendlyName>Legacy app</FriendlyName>
    <LaunchCommandLine>-qlaunch "Legacy app"</LaunchCommandLine>
  </resource>
  <resource>
    <FriendlyName>Blank state is legacy</FriendlyName>
    <ServerState>   </ServerState>
    <LaunchCommandLine>-qlaunch "Blank"</LaunchCommandLine>
  </resource>
</resources>"#;

#[test]
fn only_subscribed_applications_are_imported_and_legacy_caches_import_as_before() {
    let apps = parse(STATEFUL).expect("parsed");
    let names: Vec<&str> = apps.iter().map(|app| app.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Subscribed app",
            "Lowercase subscribed",
            "Legacy app",
            "Blank state is legacy"
        ]
    );
}

#[test]
fn the_citrix_identifier_is_read_from_the_launch_line() {
    let apps = parse(STATEFUL).expect("parsed");
    assert_eq!(
        apps[0].citrix_id.as_deref(),
        Some("store@@Subscribed app"),
        "quoted"
    );
    assert_eq!(apps[1].citrix_id.as_deref(), Some("store@@Lower"), "bare");
    assert_eq!(apps[2].citrix_id, None, "none in a legacy line");

    assert_eq!(
        extract_citrix_id("-citrixid abc -s x").as_deref(),
        Some("abc")
    );
    assert_eq!(extract_citrix_id("-CitrixID"), None);
    assert_eq!(extract_citrix_id("-CitrixID -s x"), None);
    assert_eq!(extract_citrix_id("-CitrixID \"  \""), None);
    assert_eq!(extract_citrix_id("-CitrixID \"unterminated"), None);
    assert_eq!(
        extract_citrix_id("x-CitrixID a -CitrixIDs b -CitrixID c").as_deref(),
        Some("c"),
        "a whole word only"
    );
}

#[test]
fn the_store_prefix_keeps_the_files_that_start_with_it() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("Corp_Store_Cache.xml"), CACHE).expect("write");
    std::fs::write(dir.path().join("corp2_Cache.xml"), STATEFUL).expect("write");
    std::fs::write(dir.path().join("Other_Cache.xml"), CACHE).expect("write");
    let corp = scan_folder(dir.path(), Some("Corp"));
    assert_eq!(corp.apps.len(), 3 + 4, "both Corp files, without case");
    let exact = scan_folder(dir.path(), Some("corp_"));
    assert_eq!(exact.apps.len(), 3);
    let none = scan_folder(dir.path(), Some("Nothing"));
    assert!(none.apps.is_empty());
    assert_eq!(none.warnings, [CacheWarning::NoCacheFiles]);
    assert_eq!(scan_folder(dir.path(), None).apps.len(), 3 + 4 + 3);
}

#[test]
fn a_store_prefix_is_safe_file_name_characters_only() {
    assert_eq!(
        normalize_store_prefix("  Corp-1_a.b "),
        Some("Corp-1_a.b".to_owned())
    );
    assert_eq!(
        normalize_store_prefix("Mon Magasin"),
        Some("Mon Magasin".to_owned())
    );
    let too_long = "x".repeat(65);
    for refused in [
        "",
        "   ",
        "a/b",
        r"a\b",
        r"..\x",
        "a*",
        "a?",
        "a:b",
        "a\u{0}b",
        "a\nb",
        "a\u{200B}b",
        "a\u{202E}b",
        "a\"b",
        "a<b",
        too_long.as_str(),
    ] {
        assert_eq!(normalize_store_prefix(refused), None, "{refused:?}");
    }
}

#[test]
fn the_cache_directory_and_the_launcher_must_be_local_absolute_paths() {
    for good in [r"C:\Cache\SelfService", "d:/cache", "/var/cache/citrix"] {
        assert!(cache_directory_valid(good), "{good}");
    }
    for bad in [
        "",
        r"relative\folder",
        "Cache",
        r"\\server\share\Citrix",
        "//server/share",
        r"C:\Cache\..\Windows",
        "C:\\Cache\u{200B}",
        "C:\\Ca\u{7}che",
        r"%LOCALAPPDATA%\Citrix",
    ] {
        assert!(!cache_directory_valid(bad), "{bad:?}");
    }

    for good in [
        r"C:\Program Files\Citrix\SelfService.exe",
        "c:/citrix/selfservice.EXE",
        "/opt/citrix/SelfService.exe",
    ] {
        assert!(self_service_path_valid(good), "{good}");
    }
    for bad in [
        "",
        "SelfService.exe",
        r"C:\Citrix\other.exe",
        r"C:\Citrix\SelfService.exe.bat",
        r"C:\Citrix\SelfService.exe\",
        r"C:\Citrix",
        r"\\server\share\SelfService.exe",
        r"C:\Citrix\..\SelfService.exe",
        "C:\\Cit\u{FEFF}rix\\SelfService.exe",
    ] {
        assert!(!self_service_path_valid(bad), "{bad:?}");
    }
}

#[test]
fn an_oversized_cache_file_is_refused_unread() {
    let dir = tempfile::tempdir().expect("dir");
    let file = std::fs::File::create(dir.path().join("Big_Cache.xml")).expect("file");
    file.set_len(MAX_CACHE_FILE_BYTES + 1).expect("sparse");
    let scan = scan_folder(dir.path(), None);
    assert!(scan.apps.is_empty());
    assert!(matches!(
        scan.warnings.as_slice(),
        [CacheWarning::Unreadable { file, .. }] if file == "Big_Cache.xml"
    ));
}

#[test]
fn the_applications_of_a_file_are_bounded() {
    use std::fmt::Write as _;
    let mut text = String::from("<resources>");
    for n in 0..(MAX_APPS_PER_FILE + 5) {
        write!(
            text,
            "<resource><FriendlyName>A{n}</FriendlyName><LaunchCommandLine>x</LaunchCommandLine></resource>"
        )
        .expect("write");
    }
    text.push_str("</resources>");
    let apps = parse(&text).expect("parsed");
    assert_eq!(apps.len(), MAX_APPS_PER_FILE);
}

#[test]
fn any_path_starting_with_two_separators_is_refused() {
    for prefix in [r"\\", "//", r"/\", r"\/"] {
        let dir = format!("{prefix}server{prefix}share");
        assert!(!cache_directory_valid(&dir), "{dir}");
        let exe = format!("{prefix}server{prefix}share{prefix}SelfService.exe");
        assert!(!self_service_path_valid(&exe), "{exe}");
    }
    assert!(!cache_directory_valid(r"\\?\C:\Cache"));
}

/// `count` applications named after `prefix` with identifiers `prefix` + number.
fn many(prefix: &str, count: usize) -> String {
    use std::fmt::Write as _;
    let mut text = String::from("<resources>");
    for n in 0..count {
        write!(
            text,
            "<resource><FriendlyName>{prefix}{n}</FriendlyName><LaunchCommandLine>-CitrixID {prefix}{n}</LaunchCommandLine></resource>"
        )
        .expect("write");
    }
    text.push_str("</resources>");
    text
}

#[test]
fn a_truncated_file_and_the_total_cap_are_said() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(
        dir.path().join("a_Cache.xml"),
        many("a", MAX_APPS_PER_FILE + 1),
    )
    .expect("write");
    let scan = scan_folder(dir.path(), None);
    assert_eq!(scan.apps.len(), MAX_APPS_PER_FILE);
    assert_eq!(
        scan.warnings,
        [CacheWarning::FileTruncated {
            file: "a_Cache.xml".to_owned()
        }]
    );

    // Three full files exceed the total.
    let dir = tempfile::tempdir().expect("dir");
    for name in ["a", "b", "c"] {
        std::fs::write(
            dir.path().join(format!("{name}_Cache.xml")),
            many(name, MAX_APPS_PER_FILE),
        )
        .expect("write");
    }
    let scan = scan_folder(dir.path(), None);
    assert_eq!(scan.apps.len(), MAX_APPS_TOTAL);
    assert_eq!(scan.warnings, [CacheWarning::TotalCapReached]);
}

#[test]
fn a_repeated_citrix_id_keeps_the_first_and_is_said() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("a_Cache.xml"), many("x", 3)).expect("write");
    std::fs::write(dir.path().join("b_Cache.xml"), many("x", 2)).expect("write");
    let scan = scan_folder(dir.path(), None);
    assert_eq!(scan.apps.len(), 3);
    assert_eq!(scan.warnings, [CacheWarning::DuplicateIds { count: 2 }]);
}
