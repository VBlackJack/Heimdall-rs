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

//! Compile-time guard: a connection must be able to run on any runtime thread.
//!
//! The UI starts connections with `tokio::spawn`, which needs a `Send` future. The other
//! tests await `connect` in place, which does not check that; this file fails to compile if
//! the future stops being `Send`, as it once did (an SSH agent borrowed across an await).

mod common;

use std::sync::Arc;

use common::ScriptedPrompter;
use heimdall_ssh::{ConnectOptions, connect};
use tokio_util::sync::CancellationToken;

fn assert_send<T: Send>(_value: &T) {}

#[test]
fn the_connect_future_is_send() {
    let dir = tempfile::tempdir().expect("temp dir");
    let profile = common::profile(22, Some("ed25519-openssh"));
    let options = ConnectOptions::new(dir.path().join("known_hosts"));
    let future = connect(
        &profile,
        &options,
        Arc::new(ScriptedPrompter::default()),
        CancellationToken::new(),
    );
    assert_send(&future);
}
