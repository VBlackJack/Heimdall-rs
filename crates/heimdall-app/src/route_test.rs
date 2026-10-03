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

//! Running the gateway dialog's "Test route": the route walked by `heimdall-ssh`, each step
//! handed on as it ends, then the end.

use heimdall_core::profile::SshProfile;
use heimdall_ssh::{ConnectOptions, HopSecrets, Step, diagnose_route};
use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::sync::CancellationToken;

/// A "Test route" to run.
pub struct RouteTestRequest {
    /// The gateways, nearest first; the dialog's own, as typed, last.
    pub hops: Vec<SshProfile>,
    /// What each signs in with, by place on the route.
    pub secrets: Vec<HopSecrets>,
    /// The destination reached through the last gateway, if any.
    pub destination: Option<(String, u16)>,
    /// SSH options.
    pub options: ConnectOptions,
    /// Stops it.
    pub cancel: CancellationToken,
}

impl std::fmt::Debug for RouteTestRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The secrets are not shown.
        f.debug_struct("RouteTestRequest")
            .field("hops", &self.hops.len())
            .field("destination", &self.destination.is_some())
            .finish_non_exhaustive()
    }
}

/// Runs `request`: each step as it ends, then `None` at the end. Called when its task runs,
/// on the runtime: the test is spawned here.
pub fn route_test_events(request: RouteTestRequest) -> impl Stream<Item = Option<Step>> + Send {
    let (sender, receiver) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let steps = sender.clone();
        diagnose_route(
            &request.hops,
            &request.secrets,
            request.destination,
            &request.options,
            request.cancel,
            move |step| {
                let _ = steps.send(Some(step));
            },
        )
        .await;
        let _ = sender.send(None);
    });
    UnboundedReceiverStream::new(receiver)
}
