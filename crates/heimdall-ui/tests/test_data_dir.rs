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

    #[cfg(unix)]
    #[test]
    fn a_folder_that_is_not_text_is_refused_never_converted() {
        use std::os::unix::ffi::OsStringExt as _;

        let joined = OsString::from_vec(b"--data-dir=/tmp/\xff".to_vec());
        assert_eq!(
            folder_from([joined.clone()]),
            Err(ArgumentError::NotText(joined))
        );
        let other = OsString::from_vec(b"--\xff".to_vec());
        assert_eq!(
            folder_from([other.clone()]),
            Err(ArgumentError::Unknown(other))
        );
        // Given as its own word, the folder is taken as the system gives it.
        let folder = OsString::from_vec(b"/tmp/\xff".to_vec());
        assert_eq!(
            folder_from([OsString::from("--data-dir"), folder.clone()]),
            Ok(PathBuf::from(folder))
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
mod without_the_feature {
    use std::ffi::OsString;

    use heimdall_ui::data_root::{ArgumentError, folder_from, titled};

    fn arguments(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_data_dir_option_is_read_and_other_arguments_are_ignored() {
        // A normal build reads `--data-dir` (the alternate-instance data root), as the C#
        // `DataDirectoryArgument`: last occurrence wins, any other argument is ignored.
        let folder = std::env::temp_dir().join("heimdall-demo");
        let text = folder.to_string_lossy().into_owned();
        assert_eq!(
            folder_from(arguments(&["--data-dir", &text])),
            Ok(Some(folder.clone()))
        );
        assert_eq!(
            folder_from(arguments(&[&format!("--data-dir={text}")])),
            Ok(Some(folder.clone()))
        );
        assert_eq!(
            folder_from(arguments(&[])),
            Ok(None),
            "absent: the user's own folders"
        );
        assert_eq!(
            folder_from(arguments(&["--verbose"])),
            Ok(None),
            "an unknown argument is ignored"
        );
        assert_eq!(
            folder_from(arguments(&["--data-dir"])),
            Err(ArgumentError::NoValue)
        );
        assert_eq!(
            folder_from(arguments(&["--data-dir="])),
            Err(ArgumentError::NoValue)
        );
        let other = std::env::temp_dir().join("other");
        assert_eq!(
            folder_from(arguments(&[
                "--data-dir",
                &text,
                "--data-dir",
                &other.to_string_lossy()
            ])),
            Ok(Some(other)),
            "the last occurrence wins"
        );
        // The name in any case, as the C# compares it.
        assert_eq!(
            folder_from(arguments(&["--DATA-DIR", &text])),
            Ok(Some(folder.clone()))
        );
        assert_eq!(
            folder_from(arguments(&[&format!("--Data-Dir={text}")])),
            Ok(Some(folder.clone()))
        );
        // Spaces and quotes trimmed, as the C# trims them: a shortcut's `"C:\My Demo\"`
        // arrives with a quote at its end.
        assert_eq!(
            folder_from(arguments(&["--data-dir", &format!(" {text}\" ")])),
            Ok(Some(folder.clone()))
        );
        assert_eq!(
            folder_from(arguments(&["--data-dir", "\" \""])),
            Err(ArgumentError::NoValue)
        );
        // A name that only starts like the option is refused, never ignored: the build would
        // otherwise open the user's own profile.
        for misspelled in ["--data-directory", "--data-dirs=x", "--DATA-DIRX"] {
            assert_eq!(
                folder_from(arguments(&[misspelled])),
                Err(ArgumentError::Unknown(OsString::from(misspelled))),
                "{misspelled}"
            );
        }
        // A form that is not text is refused, never guessed.
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt as _;
            use std::path::PathBuf;
            let joined = OsString::from_vec(b"--data-dir=/tmp/\xff".to_vec());
            assert_eq!(
                folder_from([joined.clone()]),
                Err(ArgumentError::NotText(joined))
            );
            // Given as its own word, the folder is taken as the system gives it.
            let folder = OsString::from_vec(b"/tmp/\xff".to_vec());
            assert_eq!(
                folder_from([OsString::from("--data-dir"), folder.clone()]),
                Ok(Some(PathBuf::from(folder)))
            );
        }
    }

    #[test]
    fn without_a_data_dir_the_title_is_unchanged() {
        // No `--data-dir` was set in this process, so the title carries no data folder.
        assert_eq!(titled("Heimdall".to_owned()), "Heimdall");
    }

    #[test]
    fn the_start_reads_the_data_dir_option_without_the_feature() {
        // The normal build's start parses `--data-dir` through the data_root module, which is
        // compiled only without the feature; the test-data sandbox module replaces it under
        // the feature.
        const SANDBOX_GATE: &str = "#[cfg(feature = \"test-data-dir\")]";
        const NORMAL_GATE: &str = "#[cfg(not(feature = \"test-data-dir\"))]";
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let lib = std::fs::read_to_string(src.join("lib.rs")).expect("lib.rs");
        assert!(lib.contains(&format!("{SANDBOX_GATE}\npub mod test_data;")));
        assert!(lib.contains(&format!("{NORMAL_GATE}\npub mod data_root;")));
        let main = std::fs::read_to_string(src.join("main.rs")).expect("main.rs");
        assert!(
            main.contains("data_root::init"),
            "the normal start reads the data root option"
        );
    }
}
