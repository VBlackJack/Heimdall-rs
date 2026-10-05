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

//! Domain model shared by every Heimdall crate: server profiles, settings, paths and the
//! credential vault.

pub mod credential_provider;
pub mod credentials;
pub mod export;
pub mod files_state;
pub mod folder;
pub mod import;
pub mod lockout;
pub mod metadata;
pub mod paths;
pub mod pin;
pub mod post_connect;
pub mod profile;
pub mod settings;
pub mod store;
pub mod utc;
pub mod winrm;
pub mod winrm_diagnostic;
