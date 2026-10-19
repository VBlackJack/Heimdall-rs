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

//! What a folder gives the sessions put in it, as the C# `GroupDefaultsDto` does: a gateway,
//! a login, a key, a port and an environment, each inherited from the nearest folder that
//! sets it (`GroupDefaultsDto.Resolve`).
//!
//! Unlike the C#, which fills a session at every connection, they are filled once, when a
//! session is created in the folder or moved into it, and then belong to the session.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::folder;
use crate::metadata::Environment;
use crate::profile::ProfileId;

/// The defaults one folder sets itself; a field left `None` is not set by it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderDefaults {
    /// The SSH gateway its sessions go through.
    pub gateway: Option<ProfileId>,
    /// The account its SSH sessions log in with.
    pub username: Option<String>,
    /// The key file its SSH sessions use.
    pub key_path: Option<PathBuf>,
    /// The port its SSH sessions connect to.
    pub port: Option<u16>,
    /// The environment its sessions belong to.
    pub environment: Option<Environment>,
}

impl FolderDefaults {
    /// Whether it sets nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// A field a folder can give a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InheritedField {
    /// The SSH gateway.
    Gateway,
    /// The account.
    Username,
    /// The key file.
    KeyPath,
    /// The port.
    Port,
    /// The environment.
    Environment,
}

/// A value and the folder it comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inherited<T> {
    /// The value.
    pub value: T,
    /// The folder that sets it, normalised.
    pub folder: String,
}

/// The defaults in force for a folder: each field from the nearest folder that sets it, as
/// `GroupDefaultsDto.Resolve` merges them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedDefaults {
    /// The SSH gateway.
    pub gateway: Option<Inherited<ProfileId>>,
    /// The account.
    pub username: Option<Inherited<String>>,
    /// The key file.
    pub key_path: Option<Inherited<PathBuf>>,
    /// The port.
    pub port: Option<Inherited<u16>>,
    /// The environment.
    pub environment: Option<Inherited<Environment>>,
}

impl ResolvedDefaults {
    /// Resolves the defaults of `path` from `own`, which gives a folder's own defaults by
    /// normalised path: the deepest folder that sets a field wins, one that leaves it unset
    /// never blanks what an ancestor gave (`??=` in the C#).
    pub(crate) fn resolve<'a>(
        path: &str,
        own: impl Fn(&str) -> Option<&'a FolderDefaults>,
    ) -> Self {
        let mut result = Self::default();
        let mut current = folder::normal(path);
        while !current.is_empty() {
            if let Some(defaults) = own(&current) {
                fill(&mut result.gateway, defaults.gateway.as_ref(), &current);
                fill(&mut result.username, defaults.username.as_ref(), &current);
                fill(&mut result.key_path, defaults.key_path.as_ref(), &current);
                fill(&mut result.port, defaults.port.as_ref(), &current);
                fill(
                    &mut result.environment,
                    defaults.environment.as_ref(),
                    &current,
                );
            }
            current = folder::parent(&current);
        }
        result
    }

    /// Whether no folder gives anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.gateway.is_none()
            && self.username.is_none()
            && self.key_path.is_none()
            && self.port.is_none()
            && self.environment.is_none()
    }
}

fn fill<T: Clone>(target: &mut Option<Inherited<T>>, source: Option<&T>, folder: &str) {
    if target.is_none()
        && let Some(value) = source
    {
        *target = Some(Inherited {
            value: value.clone(),
            folder: folder.to_owned(),
        });
    }
}

/// A folder's defaults as the profile file keeps them: words and numbers, read leniently.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct DefaultsEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gateway: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
}

impl DefaultsEntry {
    /// What it says that is known: a blank text, a port of 0 and an environment not known
    /// are left out.
    pub(crate) fn read(&self) -> FolderDefaults {
        FolderDefaults {
            gateway: self
                .gateway
                .as_deref()
                .filter(|id| !id.trim().is_empty())
                .map(|id| ProfileId::new(id.to_owned())),
            username: self.username.clone().filter(|name| !name.is_empty()),
            key_path: self
                .key_path
                .clone()
                .filter(|path| !path.as_os_str().is_empty()),
            port: self.port.filter(|port| *port != 0),
            environment: self.environment.as_deref().and_then(Environment::named),
        }
    }

    pub(crate) fn of(defaults: &FolderDefaults) -> Self {
        Self {
            gateway: defaults.gateway.as_ref().map(|id| id.as_str().to_owned()),
            username: defaults.username.clone(),
            key_path: defaults.key_path.clone(),
            port: defaults.port,
            environment: defaults.environment.map(|e| e.name().to_owned()),
        }
    }
}
