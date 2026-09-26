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

//! Reaching a running SSH agent.
//!
//! Unix: the socket named by `SSH_AUTH_SOCK`. Windows: `SSH_AUTH_SOCK` when it names a pipe
//! (1Password, `KeeAgent`, gpg), then the OpenSSH agent pipe, then Pageant. Every attempt is
//! bounded: russh retries a busy Windows pipe with no limit of its own.

use std::path::Path;
use std::time::Duration;

use russh::keys::PublicKey;
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::{AgentClient, AgentStream};

use crate::options::AgentSource;

/// Time allowed to reach an agent.
const AGENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Environment variable naming the agent socket or pipe.
#[cfg_attr(not(windows), allow(dead_code))]
const AUTH_SOCK_VARIABLE: &str = "SSH_AUTH_SOCK";

/// Prefix of a Windows named pipe path.
#[cfg_attr(not(windows), allow(dead_code))]
const PIPE_PREFIX: &str = r"\\.\pipe\";

/// Pipe of the OpenSSH agent service shipped with Windows.
#[cfg_attr(not(windows), allow(dead_code))]
const OPENSSH_AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

/// An agent connection, whatever transport it uses.
pub(crate) type Agent = AgentClient<Box<dyn AgentStream + Send + Unpin + 'static>>;

/// Connects to the agent `source` designates, or `None`.
pub(crate) async fn connect(source: &AgentSource) -> Option<Agent> {
    let connecting = async {
        match source {
            AgentSource::Disabled => None,
            AgentSource::Auto => connect_any().await,
            AgentSource::Path(path) => connect_path(path).await,
        }
    };
    tokio::time::timeout(AGENT_CONNECT_TIMEOUT, connecting)
        .await
        .ok()
        .flatten()
}

#[cfg(unix)]
async fn connect_path(path: &Path) -> Option<Agent> {
    AgentClient::connect_uds(path)
        .await
        .ok()
        .map(AgentClient::dynamic)
}

#[cfg(windows)]
async fn connect_path(path: &Path) -> Option<Agent> {
    AgentClient::connect_named_pipe(path.as_os_str())
        .await
        .ok()
        .map(AgentClient::dynamic)
}

/// Public keys the agent holds; certificates are left out.
pub(crate) async fn identities(agent: &mut Agent) -> Vec<PublicKey> {
    agent
        .request_identities()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|identity| match identity {
            AgentIdentity::PublicKey { key, .. } => Some(key),
            AgentIdentity::Certificate { .. } => None,
        })
        .collect()
}

/// Windows pipes to try, in order, given the value of `SSH_AUTH_SOCK`.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn candidate_pipes(auth_sock: Option<&str>) -> Vec<String> {
    let mut pipes: Vec<String> = auth_sock
        .filter(|value| value.starts_with(PIPE_PREFIX))
        .map(str::to_owned)
        .into_iter()
        .collect();
    if !pipes.iter().any(|pipe| pipe == OPENSSH_AGENT_PIPE) {
        pipes.push(OPENSSH_AGENT_PIPE.to_owned());
    }
    pipes
}

#[cfg(unix)]
async fn connect_any() -> Option<Agent> {
    AgentClient::connect_env()
        .await
        .ok()
        .map(AgentClient::dynamic)
}

#[cfg(windows)]
async fn connect_any() -> Option<Agent> {
    let auth_sock = std::env::var(AUTH_SOCK_VARIABLE).ok();
    for pipe in candidate_pipes(auth_sock.as_deref()) {
        if let Ok(client) = AgentClient::connect_named_pipe(&pipe).await {
            return Some(client.dynamic());
        }
    }
    AgentClient::connect_pageant()
        .await
        .ok()
        .map(AgentClient::dynamic)
}

#[cfg(test)]
mod tests {
    use super::{OPENSSH_AGENT_PIPE, candidate_pipes};

    #[test]
    fn a_pipe_in_ssh_auth_sock_is_tried_first() {
        let custom = r"\\.\pipe\openssh-ssh-agent-1password";
        assert_eq!(
            candidate_pipes(Some(custom)),
            vec![custom.to_owned(), OPENSSH_AGENT_PIPE.to_owned()]
        );
    }

    #[test]
    fn a_unix_socket_path_is_not_a_pipe() {
        assert_eq!(
            candidate_pipes(Some("/tmp/ssh-agent.sock")),
            vec![OPENSSH_AGENT_PIPE.to_owned()]
        );
        assert_eq!(candidate_pipes(None), vec![OPENSSH_AGENT_PIPE.to_owned()]);
    }

    #[test]
    fn the_openssh_pipe_is_not_listed_twice() {
        assert_eq!(
            candidate_pipes(Some(OPENSSH_AGENT_PIPE)),
            vec![OPENSSH_AGENT_PIPE.to_owned()]
        );
    }
}
