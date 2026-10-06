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

use heimdall_core::import::citrix_cache::{CacheWarning, parse, scan_folder};

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
    let scan = scan_folder(dir.path());
    assert_eq!(scan.apps.len(), 3, "only the cache file's");
    assert!(matches!(
        scan.warnings.as_slice(),
        [CacheWarning::Unreadable { file, .. }] if file == "broken_cache.XML"
    ));

    let empty = tempfile::tempdir().expect("dir");
    assert_eq!(
        scan_folder(empty.path()).warnings,
        [CacheWarning::NoCacheFiles]
    );
    assert_eq!(
        scan_folder(&empty.path().join("missing")).warnings,
        [CacheWarning::FolderMissing]
    );
}
