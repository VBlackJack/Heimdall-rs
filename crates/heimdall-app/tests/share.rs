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

//! "Share folder" through the application: the toggle, the dialog, a share started and heard
//! from, its URL and command copied, and its end by the user or by its time.

use std::path::Path;
use std::time::Duration;

use heimdall_app::share::{
    BindChoice, ListingLabels, ShareEvent, ShareField, ShareNotice, ShareUpdate, StopReason,
    share_events,
};
use heimdall_app::{App, AppConfig, Dialog, Effect, Message, Notice, ShareMessage, ShareStatus};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio_stream::{Stream, StreamExt as _};

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let store = ProfileStore::open(&profiles_file).expect("store");
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn labels() -> ListingLabels {
    ListingLabels {
        index_of: "Index of".to_owned(),
        name: "Name".to_owned(),
        size: "Size".to_owned(),
    }
}

fn share(app: &mut App, message: ShareMessage) -> Vec<Effect> {
    app.update(Message::Share(message))
}

/// Opens the dialog, fills it for `folder` on a free port, and confirms; the effect starting
/// the share is returned.
fn start_effect(
    app: &mut App,
    folder: &Path,
    minutes: &str,
) -> (u64, heimdall_app::share::ShareRequest) {
    let effects = share(app, ShareMessage::Toggle);
    assert!(matches!(effects.as_slice(), [Effect::ListShareAddresses]));
    share(app, ShareMessage::FolderChosen(folder.to_path_buf()));
    share(
        app,
        ShareMessage::Field {
            field: ShareField::Port,
            value: "0".to_owned(),
        },
    );
    share(
        app,
        ShareMessage::Field {
            field: ShareField::Minutes,
            value: minutes.to_owned(),
        },
    );
    let mut effects = app.update(Message::ConfirmDialog);
    assert_eq!(effects.len(), 1, "{effects:?}");
    let Some(Effect::StartShare { run, request }) = effects.pop() else {
        panic!("no share started");
    };
    (run, *request)
}

async fn next(updates: &mut (impl Stream<Item = ShareUpdate> + Unpin)) -> ShareUpdate {
    tokio::time::timeout(Duration::from_secs(10), updates.next())
        .await
        .expect("an update in time")
        .expect("an update")
}

#[test]
fn the_toggle_opens_a_dialog_that_listens_on_the_loopback() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    let effects = share(&mut app, ShareMessage::Toggle);
    assert!(matches!(effects.as_slice(), [Effect::ListShareAddresses]));
    let Some(Dialog::ShareFolder(form)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(form.bind, BindChoice::Loopback);
    assert_eq!(form.port, "8080");
    assert_eq!(form.minutes, "30");
    assert!(app.share_status().is_none());
}

#[test]
fn nothing_starts_until_a_folder_and_valid_numbers_are_given() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    share(&mut app, ShareMessage::Toggle);
    assert!(app.update(Message::ConfirmDialog).is_empty(), "no folder");
    share(
        &mut app,
        ShareMessage::FolderChosen(dir.path().to_path_buf()),
    );
    share(
        &mut app,
        ShareMessage::Field {
            field: ShareField::Minutes,
            value: "0".to_owned(),
        },
    );
    assert!(app.update(Message::ConfirmDialog).is_empty(), "no time");
    assert!(matches!(app.dialog, Some(Dialog::ShareFolder(_))));
    assert!(app.share_status().is_none());
}

#[test]
fn the_chosen_address_is_kept_by_the_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    share(&mut app, ShareMessage::Toggle);
    let lan = BindChoice::Interface("192.168.1.10".parse().unwrap());
    share(&mut app, ShareMessage::Bind(lan));
    let Some(Dialog::ShareFolder(form)) = &app.dialog else {
        panic!();
    };
    assert_eq!(form.bind, lan);
}

#[tokio::test]
async fn a_share_runs_is_copied_and_is_stopped_by_the_toggle() {
    let dir = tempfile::tempdir().unwrap();
    let shared = tempfile::tempdir().unwrap();
    std::fs::write(shared.path().join("hello.txt"), "hello").unwrap();
    let mut app = app(dir.path());
    let (run, request) = start_effect(&mut app, shared.path(), "5");
    assert_eq!(request.bind, BindChoice::Loopback);
    assert_eq!(request.lifetime, Duration::from_mins(5));
    assert_eq!(app.share_status(), Some(ShareStatus::Starting));
    assert!(app.dialog.is_none());

    let mut updates = Box::pin(share_events(request, labels()));
    let started = next(&mut updates).await;
    let ShareUpdate::Started(info) = &started else {
        panic!("{started:?}");
    };
    assert!(info.addr.ip().is_loopback());
    share(
        &mut app,
        ShareMessage::Update {
            run,
            update: Some(started.clone()),
        },
    );
    assert!(matches!(
        app.share_status(),
        Some(ShareStatus::Running { .. })
    ));
    assert!(matches!(app.dialog, Some(Dialog::ShareStarted)));
    assert!(matches!(
        app.notice(),
        Some(Notice::Share(ShareNotice::Started(_)))
    ));

    // The URL and the command carry the token, and the status line never does.
    let url = match share(&mut app, ShareMessage::CopyUrl).as_slice() {
        [Effect::WriteClipboard(url)] => url.clone(),
        other => panic!("{other:?}"),
    };
    assert!(url.starts_with(&format!("http://{}/?token=", info.addr)));
    let token = url.rsplit('=').next().unwrap().to_owned();
    assert_eq!(token.len(), 64);
    let command = match share(&mut app, ShareMessage::CopyCommand).as_slice() {
        [Effect::WriteClipboard(command)] => command.clone(),
        other => panic!("{other:?}"),
    };
    assert!(command.contains(&format!("Authorization: Bearer {token}")));
    assert!(
        !command.contains(&format!("?token={token}")),
        "the token is in the header"
    );
    assert!(!format!("{:?}", app.notice()).contains(&token));

    // The share answers with the token from the URL.
    let mut stream = TcpStream::connect(info.addr).await.unwrap();
    stream
        .write_all(format!("GET /hello.txt?token={token} HTTP/1.1\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).await.unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"));
    assert!(reply.ends_with("hello"));
    let request_seen = next(&mut updates).await;
    assert!(matches!(
        request_seen,
        ShareUpdate::Event(ShareEvent::Request(_))
    ));
    share(
        &mut app,
        ShareMessage::Update {
            run,
            update: Some(request_seen),
        },
    );
    assert_eq!(app.share_requests(), 1);

    // The toggle stops it: at once for the bar, and the server follows.
    share(&mut app, ShareMessage::Toggle);
    assert!(app.share_status().is_none());
    assert!(app.dialog.is_none());
    assert_eq!(app.notice(), Some(&Notice::Share(ShareNotice::Stopped)));
    let ended = next(&mut updates).await;
    assert!(matches!(
        ended,
        ShareUpdate::Event(ShareEvent::Stopped(StopReason::Stopped))
    ));
    assert!(updates.next().await.is_none());
    assert!(TcpStream::connect(info.addr).await.is_err());
    // What it says later is not heard.
    share(
        &mut app,
        ShareMessage::Update {
            run,
            update: Some(ended),
        },
    );
    assert!(app.share_status().is_none());
}

#[tokio::test]
async fn a_share_whose_time_is_over_is_gone_from_the_bar() {
    let dir = tempfile::tempdir().unwrap();
    let shared = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    let (run, mut request) = start_effect(&mut app, shared.path(), "1");
    // A shorter life than the dialog allows, to wait for it.
    request.lifetime = Duration::from_millis(300);
    let mut updates = Box::pin(share_events(request, labels()));
    while let Some(update) = updates.next().await {
        share(
            &mut app,
            ShareMessage::Update {
                run,
                update: Some(update),
            },
        );
    }
    share(&mut app, ShareMessage::Update { run, update: None });
    assert!(app.share_status().is_none());
    assert_eq!(app.notice(), Some(&Notice::Share(ShareNotice::Expired)));
}

#[tokio::test]
async fn a_share_that_cannot_listen_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    let missing = dir.path().join("none");
    let (run, request) = start_effect(&mut app, &missing, "5");
    let mut updates = Box::pin(share_events(request, labels()));
    let failed = next(&mut updates).await;
    assert!(matches!(failed, ShareUpdate::Failed(_)));
    share(
        &mut app,
        ShareMessage::Update {
            run,
            update: Some(failed),
        },
    );
    assert!(app.share_status().is_none());
    assert!(matches!(
        app.notice(),
        Some(Notice::Share(ShareNotice::Failed(_)))
    ));
}

#[test]
fn the_application_ending_stops_the_share() {
    let dir = tempfile::tempdir().unwrap();
    let shared = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    let (_run, request) = start_effect(&mut app, shared.path(), "5");
    let cancel = request.cancel.clone();
    assert!(!cancel.is_cancelled());
    // Exit is asked for, and nothing live or unsaved holds it back.
    let effects = app.update(Message::WindowCloseRequested);
    assert!(
        effects.iter().any(|effect| matches!(effect, Effect::Exit)),
        "{effects:?}"
    );
    assert!(cancel.is_cancelled());
    assert!(app.share_status().is_none());
}
