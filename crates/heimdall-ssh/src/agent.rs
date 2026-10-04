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
//! (1Password, `KeeAgent`, gpg), then the OpenSSH agent pipe and Pageant, in the order the
//! C# `SshAgentPreference` puts them, or one of them alone. Authentication reaches every one
//! and offers the keys of all, a key loaded in Pageant offered even while the OpenSSH agent
//! runs: wider than the C#, which offers the keys of the first agent holding any. Every
//! attempt is bounded: russh retries a busy Windows pipe with no limit of its own.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use russh::keys::PublicKey;
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::{AgentClient, AgentStream};

use heimdall_core::settings::AgentPreference;

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

/// Connects to the first agent `source` designates, or `None`: the one a forwarded agent
/// channel is relayed to.
pub(crate) async fn connect(source: &AgentSource) -> Option<Agent> {
    for place in places(source) {
        if let Some(agent) = bounded(reach(&place)).await {
            return Some(agent);
        }
    }
    None
}

/// Connects to every agent `source` designates that answers, in order, each within its own
/// time limit.
pub(crate) async fn connect_all(source: &AgentSource) -> Vec<Agent> {
    let mut agents = Vec::new();
    for place in places(source) {
        if let Some(agent) = bounded(reach(&place)).await {
            agents.push(agent);
        }
    }
    agents
}

/// Name of the OpenSSH agent service shipped with Windows, as the C# Heimdall names it.
const OPENSSH_AGENT_NAME: &str = "Windows OpenSSH Agent";

/// Name of `PuTTY`'s agent, as the C# Heimdall names it.
const PAGEANT_NAME: &str = "Pageant";

/// Name of the agent `SSH_AUTH_SOCK` designates on Unix.
const UNIX_AGENT_NAME: &str = "ssh-agent";

/// An agent that answered, and how many keys it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSurvey {
    /// Its name: "Pageant", "Windows OpenSSH Agent", or the socket or pipe it answers on.
    pub name: String,
    /// The public keys it holds; certificates are not counted.
    pub keys: usize,
}

/// Every agent `source` designates that answers, in the order their keys would be offered,
/// with how many keys each holds, as the C# agent chip asks them.
pub async fn survey(source: &AgentSource) -> Vec<AgentSurvey> {
    let mut found = Vec::new();
    for place in places(source) {
        if let Some(mut agent) = bounded(reach(&place)).await {
            let keys = identities(&mut agent).await.len();
            found.push(AgentSurvey {
                name: name(&place),
                keys,
            });
        }
    }
    found
}

/// What the user knows `place` as.
fn name(place: &Place) -> String {
    match place {
        Place::Path(path) if path.as_os_str() == OPENSSH_AGENT_PIPE => {
            OPENSSH_AGENT_NAME.to_owned()
        }
        Place::Path(path) => path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned(),
        Place::Environment => UNIX_AGENT_NAME.to_owned(),
        Place::Pageant => PAGEANT_NAME.to_owned(),
    }
}

async fn bounded(connecting: impl Future<Output = Option<Agent>>) -> Option<Agent> {
    tokio::time::timeout(AGENT_CONNECT_TIMEOUT, connecting)
        .await
        .ok()
        .flatten()
}

/// Where an agent may be found.
enum Place {
    /// A socket or pipe.
    Path(PathBuf),
    /// Unix: the socket `SSH_AUTH_SOCK` names.
    #[cfg_attr(windows, allow(dead_code))]
    Environment,
    /// Windows: Pageant.
    #[cfg_attr(not(windows), allow(dead_code))]
    Pageant,
}

/// The places `source` designates, in the order their keys are offered.
fn places(source: &AgentSource) -> Vec<Place> {
    match source {
        AgentSource::Disabled => Vec::new(),
        AgentSource::Path(path) => vec![Place::Path(path.clone())],
        AgentSource::Paths(paths) => paths.iter().cloned().map(Place::Path).collect(),
        AgentSource::Auto(preference) => auto_places(*preference),
    }
}

#[cfg(unix)]
fn auto_places(_preference: AgentPreference) -> Vec<Place> {
    vec![Place::Environment]
}

#[cfg(windows)]
fn auto_places(preference: AgentPreference) -> Vec<Place> {
    let auth_sock = std::env::var(AUTH_SOCK_VARIABLE).ok();
    windows_places(auth_sock.as_deref(), preference)
        .into_iter()
        .map(|place| match place {
            WindowsPlace::Pipe(pipe) => Place::Path(PathBuf::from(pipe)),
            WindowsPlace::Pageant => Place::Pageant,
        })
        .collect()
}

/// A Windows agent, by where it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum WindowsPlace {
    /// A named pipe.
    Pipe(String),
    /// Pageant.
    Pageant,
}

/// The Windows agents to reach, in order, given the value of `SSH_AUTH_SOCK` and the
/// preference: the pipe `SSH_AUTH_SOCK` names first, the user having chosen it, then the
/// OpenSSH agent and Pageant as the C# `SshAgentRegistry` orders them; "only" keeps one.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_places(
    auth_sock: Option<&str>,
    preference: AgentPreference,
) -> Vec<WindowsPlace> {
    let openssh = WindowsPlace::Pipe(OPENSSH_AGENT_PIPE.to_owned());
    match preference {
        AgentPreference::OpenSshOnly => vec![openssh],
        AgentPreference::PageantOnly => vec![WindowsPlace::Pageant],
        AgentPreference::OpenSshFirst | AgentPreference::PageantFirst => {
            let mut places: Vec<WindowsPlace> = candidate_pipes(auth_sock)
                .into_iter()
                .filter(|pipe| pipe != OPENSSH_AGENT_PIPE)
                .map(WindowsPlace::Pipe)
                .collect();
            if preference == AgentPreference::PageantFirst {
                places.extend([WindowsPlace::Pageant, openssh]);
            } else {
                places.extend([openssh, WindowsPlace::Pageant]);
            }
            places
        }
    }
}

async fn reach(place: &Place) -> Option<Agent> {
    match place {
        Place::Path(path) => connect_path(path).await,
        #[cfg(unix)]
        Place::Environment => connect_environment().await,
        #[cfg(windows)]
        Place::Pageant => connect_pageant().await,
        // A place of the other platform: never listed here.
        #[cfg(unix)]
        Place::Pageant => None,
        #[cfg(windows)]
        Place::Environment => None,
    }
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
async fn connect_environment() -> Option<Agent> {
    AgentClient::connect_env()
        .await
        .ok()
        .map(AgentClient::dynamic)
}

#[cfg(windows)]
async fn connect_pageant() -> Option<Agent> {
    AgentClient::connect_pageant()
        .await
        .ok()
        .map(AgentClient::dynamic)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use heimdall_core::settings::AgentPreference;

    use super::{OPENSSH_AGENT_PIPE, Place, WindowsPlace, candidate_pipes, name, windows_places};

    #[test]
    fn the_preference_orders_the_openssh_agent_and_pageant_or_keeps_one_as_the_csharp() {
        let custom = r"\\.\pipe\openssh-ssh-agent-1password";
        let pipe = |pipe: &str| WindowsPlace::Pipe(pipe.to_owned());
        assert_eq!(
            windows_places(Some(custom), AgentPreference::OpenSshFirst),
            [
                pipe(custom),
                pipe(OPENSSH_AGENT_PIPE),
                WindowsPlace::Pageant
            ]
        );
        assert_eq!(
            windows_places(Some(custom), AgentPreference::PageantFirst),
            [
                pipe(custom),
                WindowsPlace::Pageant,
                pipe(OPENSSH_AGENT_PIPE)
            ],
            "the pipe chosen in SSH_AUTH_SOCK stays first"
        );
        assert_eq!(
            windows_places(Some(OPENSSH_AGENT_PIPE), AgentPreference::PageantFirst),
            [WindowsPlace::Pageant, pipe(OPENSSH_AGENT_PIPE)],
            "the OpenSSH agent named in SSH_AUTH_SOCK takes its place in the order"
        );
        assert_eq!(
            windows_places(Some(custom), AgentPreference::OpenSshOnly),
            [pipe(OPENSSH_AGENT_PIPE)]
        );
        assert_eq!(
            windows_places(None, AgentPreference::PageantOnly),
            [WindowsPlace::Pageant]
        );
    }

    #[test]
    fn the_agents_are_named_as_the_csharp_names_them() {
        assert_eq!(
            name(&Place::Path(PathBuf::from(OPENSSH_AGENT_PIPE))),
            "Windows OpenSSH Agent"
        );
        assert_eq!(name(&Place::Pageant), "Pageant");
        assert_eq!(name(&Place::Environment), "ssh-agent");
        assert_eq!(
            name(&Place::Path(PathBuf::from("/run/user/1000/agent.sock"))),
            "agent.sock"
        );
    }

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
