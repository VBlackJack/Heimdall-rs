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

//! The workspace lock beyond Ctrl+L, as the C# `WorkspaceLockService`: the idle auto-lock,
//! the sessions closed on lock when the settings ask, and the auto-reconnect waiting while
//! the workspace is locked.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use heimdall_app::split::{Axis, Placement, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, FloatMessage, Message, Phase,
    SettingsMessage, SystemCredentials, TabId, UiError, VaultStatus, open_vault,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;

const MASTER: &str = "correct horse battery staple";

/// Seconds in a minute.
const SECONDS_PER_MINUTE: u64 = 60;

/// The threshold the tests set, in minutes.
const THRESHOLD: u32 = 5;

#[derive(Debug)]
struct NullSink;

impl heimdall_app::InputSink for NullSink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: heimdall_ssh::TerminalSize) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

/// A profile of server `id`, with no account: its shell gets no SFTP pane docked by itself.
fn profile(id: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
    }
}

/// The application over `dir`, with profiles `a`, `b` and `c`.
fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge(["a", "b", "c"].map(profile));
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

/// Types `master` into the vault dialog shown and runs its job, as the UI would; what the
/// result asked for.
async fn submit(app: &mut App, master: &str, confirm: bool) -> Vec<Effect> {
    let effects = app.update(Message::SubmitVault {
        password: Secret::new(master.to_owned()),
        new: None,
        confirm: confirm.then(|| Secret::new(master.to_owned())),
    });
    let (ticket, result) = match <[Effect; 1]>::try_from(effects) {
        Ok(
            [
                Effect::OpenVault {
                    path,
                    password,
                    job,
                    ticket,
                },
            ],
        ) => (ticket, open_vault(path, password, job).await),
        other => panic!("expected OpenVault, got {other:?}"),
    };
    app.update(Message::VaultOpened(ticket, result))
}

/// The application with a master password set, the vault open.
async fn unlocked(dir: &Path) -> App {
    let mut app = app(dir);
    app.update(Message::ShowVault);
    submit(&mut app, MASTER, true).await;
    assert_eq!(app.vault_status(), VaultStatus::Open);
    app
}

fn set(app: &mut App, message: SettingsMessage) {
    app.update(Message::Settings(message));
}

fn minutes(count: u32) -> Duration {
    Duration::from_secs(u64::from(count) * SECONDS_PER_MINUTE)
}

/// A shell of profile `id`, connected.
fn live(app: &mut App, id: &str) -> (TabId, AttemptId) {
    let effects = app.update(Message::OpenProfile(ProfileId::new(id)));
    let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    });
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
    (tab, attempt)
}

#[tokio::test]
async fn idle_below_the_threshold_does_not_lock_and_at_it_locks() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = unlocked(dir.path()).await;
    set(&mut app, SettingsMessage::AutoLockIdleMinutes(THRESHOLD));
    assert!(app.watches_idle());

    let below = minutes(THRESHOLD).saturating_sub(Duration::from_millis(1));
    app.update(Message::Idle(below));
    assert!(!app.is_locked(), "{:?}", app.dialog);
    assert_eq!(app.vault_status(), VaultStatus::Open);

    app.update(Message::Idle(minutes(THRESHOLD)));
    assert!(app.is_locked());
    assert_eq!(app.vault_status(), VaultStatus::Locked);
    assert!(!app.watches_idle(), "nothing to watch while locked");
}

#[tokio::test]
async fn a_threshold_of_zero_never_locks() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = unlocked(dir.path()).await;
    assert_eq!(app.settings().auto_lock_idle_minutes, 0, "off by default");
    assert!(!app.watches_idle());
    app.update(Message::Idle(Duration::from_secs(u64::from(u32::MAX))));
    assert!(!app.is_locked(), "{:?}", app.dialog);
    assert_eq!(app.vault_status(), VaultStatus::Open);
}

#[test]
fn without_a_master_password_idle_locks_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    set(&mut app, SettingsMessage::AutoLockIdleMinutes(THRESHOLD));
    assert!(!app.watches_idle());
    app.update(Message::Idle(minutes(THRESHOLD)));
    assert!(!app.is_locked());
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
}

#[tokio::test]
async fn a_locked_workspace_stays_locked_whatever_the_idle_time() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = unlocked(dir.path()).await;
    set(&mut app, SettingsMessage::AutoLockIdleMinutes(THRESHOLD));
    app.update(Message::LockVault);
    let shown = app.dialog.clone();
    for idle in [Duration::ZERO, minutes(THRESHOLD), minutes(THRESHOLD * 2)] {
        app.update(Message::Idle(idle));
        assert!(app.is_locked());
        assert_eq!(app.dialog, shown, "the lock screen as it was");
    }

    // Unlocked, it is watched again.
    submit(&mut app, MASTER, false).await;
    assert!(!app.is_locked(), "{:?}", app.dialog);
    assert!(app.watches_idle());
}

#[test]
fn an_auto_lock_threshold_out_of_the_csharp_range_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let max = heimdall_core::settings::AUTO_LOCK_IDLE_MINUTES_MAX;
    set(&mut app, SettingsMessage::AutoLockIdleMinutes(max));
    assert_eq!(app.settings().auto_lock_idle_minutes, max);
    set(&mut app, SettingsMessage::AutoLockIdleMinutes(max + 1));
    assert_eq!(app.settings().auto_lock_idle_minutes, max, "left as it was");
    set(&mut app, SettingsMessage::DisconnectOnLock(true));
    assert!(app.settings().disconnect_on_lock);
}

/// Shells `a` and `b` split together, and `c` detached to a window of its own.
fn split_and_detached(app: &mut App) -> (TabId, TabId, TabId) {
    let (a, _) = live(app, "a");
    let (b, _) = live(app, "b");
    let (c, _) = live(app, "c");
    app.update(Message::Split(SplitMessage::Merge {
        host: a,
        tab: b,
        axis: Axis::SideBySide,
        placement: Placement::Second,
    }));
    assert_eq!(app.panes_of(a), [a, b]);
    app.update(Message::Float(FloatMessage::Detach(c)));
    assert!(app.is_floating(c));
    (a, b, c)
}

#[tokio::test]
async fn disconnect_on_lock_closes_every_session_split_and_detached_ones_included() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = unlocked(dir.path()).await;
    set(&mut app, SettingsMessage::DisconnectOnLock(true));
    let (_, _, c) = split_and_detached(&mut app);
    let window = app.floating_of(c).expect("window");

    let effects = app.update(Message::LockVault);
    assert!(app.is_locked(), "{:?}", app.dialog);
    assert!(app.tabs.is_empty(), "{:?}", app.tabs);
    assert!(app.floating().is_empty());
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::CloseWindow(key) if *key == window)),
        "the detached tab's window closes: {effects:?}"
    );
}

#[tokio::test]
async fn disconnect_on_lock_applies_to_the_idle_lock_too() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = unlocked(dir.path()).await;
    set(&mut app, SettingsMessage::AutoLockIdleMinutes(THRESHOLD));
    set(&mut app, SettingsMessage::DisconnectOnLock(true));
    live(&mut app, "a");
    app.update(Message::Idle(minutes(THRESHOLD)));
    assert!(app.is_locked());
    assert!(app.tabs.is_empty(), "{:?}", app.tabs);
}

#[tokio::test]
async fn by_default_the_sessions_survive_behind_the_lock() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = unlocked(dir.path()).await;
    assert!(!app.settings().disconnect_on_lock, "off by default");
    let (a, b, c) = split_and_detached(&mut app);

    let effects = app.update(Message::LockVault);
    assert!(app.is_locked());
    assert!(effects.is_empty(), "{effects:?}");
    for tab in [a, b, c] {
        assert_eq!(app.tab(tab).expect("kept").phase, Phase::Connected);
    }
    assert!(app.is_floating(c), "its window stays");
}

#[tokio::test]
async fn an_auto_reconnect_due_while_locked_waits_for_the_unlock() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = unlocked(dir.path()).await;
    set(&mut app, SettingsMessage::SshAutoReconnect(true));
    let (tab, attempt) = live(&mut app, "a");
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::ConnectionLost),
    });
    assert!(
        matches!(effects.as_slice(), [Effect::RetryAt { .. }]),
        "{effects:?}"
    );

    app.update(Message::LockVault);
    let effects = app.update(Message::AutoReconnect { tab, attempt });
    assert!(effects.is_empty(), "deferred: {effects:?}");
    assert!(
        matches!(app.tab(tab).expect("kept").phase, Phase::Failed(_)),
        "still waiting"
    );

    // A wrong master password leaves it waiting.
    let effects = submit(&mut app, "not the master", false).await;
    assert!(effects.is_empty(), "{effects:?}");
    assert!(app.is_locked());

    let effects = submit(&mut app, MASTER, false).await;
    assert!(!app.is_locked(), "{:?}", app.dialog);
    let [Effect::Connect { tab: again, .. }] = effects.as_slice() else {
        panic!("the reconnect attempted at the unlock: {effects:?}");
    };
    assert_ne!(*again, tab, "opened again in its place");
    assert!(app.tab(tab).is_none());

    // Attempted once only.
    app.update(Message::LockVault);
    assert!(submit(&mut app, MASTER, false).await.is_empty());
}
