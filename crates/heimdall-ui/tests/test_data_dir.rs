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

//! The test data folder of a test build: its argument, and the title that says it. Its own
//! process: the folder, once set, is the process's.

#[cfg(feature = "test-data-dir")]
mod with_the_feature {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use heimdall_core::settings::Language;
    use heimdall_ui::test_data::{ArgumentError, folder_from, init, titled};

    fn arguments(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_folder_is_read_from_its_argument_and_nothing_else_is_taken() {
        let folder = std::env::temp_dir().join("heimdall-scratch");
        let text = folder.to_string_lossy().into_owned();
        assert_eq!(
            folder_from(arguments(&["--data-dir", &text])),
            Ok(folder.clone())
        );
        assert_eq!(
            folder_from(arguments(&[&format!("--data-dir={text}")])),
            Ok(folder.clone())
        );
        assert_eq!(folder_from(arguments(&[])), Err(ArgumentError::Missing));
        assert_eq!(
            folder_from(arguments(&["--data-dir"])),
            Err(ArgumentError::NoValue)
        );
        assert_eq!(
            folder_from(arguments(&["--data-dir="])),
            Err(ArgumentError::NoValue)
        );
        assert_eq!(
            folder_from(arguments(&["--data-dir", &text, "--data-dir", &text])),
            Err(ArgumentError::Repeated)
        );
        assert_eq!(
            folder_from(arguments(&["--data-dir", &text, "--verbose"])),
            Err(ArgumentError::Unknown(OsString::from("--verbose")))
        );
        assert_eq!(
            folder_from(arguments(&["--data-directory", &text])),
            Err(ArgumentError::Unknown(OsString::from("--data-directory")))
        );
    }

    #[test]
    fn every_title_says_the_test_data_folder() {
        heimdall_ui::i18n::apply(Some(Language::English));
        let scratch = tempfile::tempdir().expect("scratch folder");
        let root: PathBuf = scratch.path().join("winsight");
        assert!(matches!(
            init(arguments(&["--data-dir", "relative"])),
            Err(ArgumentError::Refused(_))
        ));
        init(arguments(&["--data-dir", &root.to_string_lossy()])).expect("accepted");
        assert_eq!(
            titled("Heimdall".to_owned()),
            format!("Heimdall [test data: {}]", root.display())
        );
        assert_eq!(heimdall_core::paths::test_data_dir(), Some(root.as_path()));
    }
}

#[cfg(not(feature = "test-data-dir"))]
#[test]
fn without_the_feature_no_argument_is_read() {
    // The only reading of the command line is in the program's start, behind the feature,
    // and the module that parses it is compiled with it alone: a build without the feature
    // takes no `--data-dir`, whatever is passed.
    const GATE: &str = "#[cfg(feature = \"test-data-dir\")]";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let main = std::fs::read_to_string(root.join("main.rs")).expect("main.rs");
    let read = main
        .find("args_os")
        .expect("the start reads the command line");
    let gate = main.find(GATE).expect("behind the feature");
    assert!(gate < read);
    assert_eq!(main.matches("args").count(), 1, "one reading only");
    let lib = std::fs::read_to_string(root.join("lib.rs")).expect("lib.rs");
    assert!(lib.contains(&format!("{GATE}\npub mod test_data;")));
}
