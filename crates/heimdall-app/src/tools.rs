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

//! The built-in tools, as the C# `ToolRegistry` (`Services/ToolRegistry.cs:55-124`) lists
//! them: one entry per tool, its C# identifier, its category, the words that find it and
//! whether it reaches a host. Only the tools ported are listed: a tool appears in the
//! sidebar, on the Tools page and in a tab once it has its engine and its view.
//!
//! Adding a tool: a [`ToolId`] variant, its place in [`ToolId::ALL`] (the C# order) and its
//! [`ToolId::descriptor`] arm here; its label, description, icon and view in the window
//! crate, whose matches over [`ToolId`] say where.

/// The categories of the tools, in the C# `ToolCategory` order (`Enums.cs:108-115`): the
/// order the sidebar and the Tools page group them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolCategory {
    /// Network tools.
    Network,
    /// Security tools.
    Security,
    /// Encoding and format tools.
    Encoding,
    /// System tools.
    System,
    /// Tools of this computer and other programs.
    External,
}

impl ToolCategory {
    /// Every category, in order.
    pub const ALL: [Self; 5] = [
        Self::Network,
        Self::Security,
        Self::Encoding,
        Self::System,
        Self::External,
    ];

    /// The name a category is kept under in the settings, folded or not.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Security => "security",
            Self::Encoding => "encoding",
            Self::System => "system",
            Self::External => "external",
        }
    }
}

/// A group of the sidebar's Tools tab: the tools pinned first, then each category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolGroup {
    /// The tools pinned, as the C# "Favorites" category.
    Favorites,
    /// A category's tools.
    Category(ToolCategory),
}

/// The name the pinned tools' group is kept under in the settings, folded or not.
pub const FAVORITES_GROUP_KEY: &str = "favorites";

impl ToolGroup {
    /// The name it is kept under in the settings, folded or not.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Favorites => FAVORITES_GROUP_KEY,
            Self::Category(category) => category.key(),
        }
    }
}

/// A built-in tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolId {
    /// The hash generator, the C# `HASH`.
    Hash,
    /// The HMAC generator, the C# `HMAC`.
    Hmac,
    /// The password generator, the C# `PASSWORD`.
    Password,
    /// The SSH key generator, the C# `SSHKEY`.
    SshKey,
    /// The certificate generator, the C# `CERTGEN`.
    CertGen,
    /// The JWT parser, the C# `JWT`.
    Jwt,
    /// The TOTP generator, the C# `TOTP`.
    Totp,
    /// The password audit, the C# `PWDAUDIT`.
    PwdAudit,
    /// The Base64 encoder and decoder, the C# `BASE64`.
    Base64,
    /// The URL encoder and decoder, the C# `URLENC`.
    UrlEncoder,
    /// The JSON formatter, the C# `JSON`.
    JsonFormatter,
    /// The regular expression tester, the C# `REGEX`.
    RegexTester,
    /// The text comparison, the C# `DIFF`.
    TextDiff,
    /// The text case converter, the C# `TEXTCASE`.
    TextCase,
    /// The UUID generator, the C# `UUID`.
    Uuid,
}

/// What the registry says of a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolDescriptor {
    /// The C# identifier: what the favourites keep, as the C# `FavoriteToolIds`.
    pub code: &'static str,
    /// Its category.
    pub category: ToolCategory,
    /// The words that find it besides its name, as the C# `CommandPrefixes`.
    pub prefixes: &'static [&'static str],
    /// Whether it reaches a host, the selected session's inherited when it opens, as the
    /// C# `IsNetworkTool`: such a tool opens in a new tab each time.
    pub network: bool,
}

impl ToolId {
    /// Every tool ported, in the C# registry's order.
    pub const ALL: [Self; 15] = [
        Self::Hash,
        Self::Hmac,
        Self::Password,
        Self::SshKey,
        Self::CertGen,
        Self::Jwt,
        Self::Totp,
        Self::PwdAudit,
        Self::Base64,
        Self::UrlEncoder,
        Self::JsonFormatter,
        Self::RegexTester,
        Self::TextDiff,
        Self::TextCase,
        Self::Uuid,
    ];

    /// Its entry, as the C# registry's (`ToolRegistry.cs:77-104`).
    #[must_use]
    pub const fn descriptor(self) -> ToolDescriptor {
        match self {
            Self::Hash => ToolDescriptor {
                code: "HASH",
                category: ToolCategory::Security,
                prefixes: &["hash"],
                network: false,
            },
            Self::Hmac => ToolDescriptor {
                code: "HMAC",
                category: ToolCategory::Security,
                prefixes: &["hmac"],
                network: false,
            },
            Self::Password => ToolDescriptor {
                code: "PASSWORD",
                category: ToolCategory::Security,
                prefixes: &["password", "pwgen"],
                network: false,
            },
            Self::SshKey => ToolDescriptor {
                code: "SSHKEY",
                category: ToolCategory::Security,
                prefixes: &["sshkey", "keygen"],
                network: false,
            },
            Self::CertGen => ToolDescriptor {
                code: "CERTGEN",
                category: ToolCategory::Security,
                prefixes: &["certgen", "certificate", "openssl"],
                network: false,
            },
            Self::PwdAudit => ToolDescriptor {
                code: "PWDAUDIT",
                category: ToolCategory::Security,
                prefixes: &["pwdaudit", "password-audit", "passcheck"],
                network: false,
            },
            Self::Jwt => ToolDescriptor {
                code: "JWT",
                category: ToolCategory::Security,
                prefixes: &["jwt"],
                network: false,
            },
            Self::Totp => ToolDescriptor {
                code: "TOTP",
                category: ToolCategory::Security,
                prefixes: &["totp", "otp", "2fa"],
                network: false,
            },
            Self::Base64 => ToolDescriptor {
                code: "BASE64",
                category: ToolCategory::Encoding,
                prefixes: &["base64"],
                network: false,
            },
            Self::UrlEncoder => ToolDescriptor {
                code: "URLENC",
                category: ToolCategory::Encoding,
                prefixes: &["url", "urlencode"],
                network: false,
            },
            Self::JsonFormatter => ToolDescriptor {
                code: "JSON",
                category: ToolCategory::Encoding,
                prefixes: &["json"],
                network: false,
            },
            Self::RegexTester => ToolDescriptor {
                code: "REGEX",
                category: ToolCategory::Encoding,
                prefixes: &["regex"],
                network: false,
            },
            Self::TextDiff => ToolDescriptor {
                code: "DIFF",
                category: ToolCategory::Encoding,
                prefixes: &["diff"],
                network: false,
            },
            Self::TextCase => ToolDescriptor {
                code: "TEXTCASE",
                category: ToolCategory::Encoding,
                prefixes: &["case", "textcase"],
                network: false,
            },
            Self::Uuid => ToolDescriptor {
                code: "UUID",
                category: ToolCategory::System,
                prefixes: &["uuid", "guid"],
                network: false,
            },
        }
    }

    /// The C# identifier.
    #[must_use]
    pub const fn code(self) -> &'static str {
        self.descriptor().code
    }

    /// Its category.
    #[must_use]
    pub const fn category(self) -> ToolCategory {
        self.descriptor().category
    }

    /// The words that find it besides its name.
    #[must_use]
    pub const fn prefixes(self) -> &'static [&'static str] {
        self.descriptor().prefixes
    }

    /// Whether it reaches a host.
    #[must_use]
    pub const fn is_network(self) -> bool {
        self.descriptor().network
    }

    /// The tool of C# identifier `code`, whatever its case, as the C# registry looks it up;
    /// the C# `TOOL:` prefix is taken off first.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        let code = code.trim();
        let code = code
            .get(..TOOL_PREFIX.len())
            .filter(|prefix| prefix.eq_ignore_ascii_case(TOOL_PREFIX))
            .map_or(code, |_| &code[TOOL_PREFIX.len()..]);
        Self::ALL
            .into_iter()
            .find(|tool| tool.code().eq_ignore_ascii_case(code))
    }
}

/// The prefix the C# writes before a tool's identifier in a connection type
/// (`ConnectionTypeCatalog.ToolPrefix`).
const TOOL_PREFIX: &str = "TOOL:";

/// Tools remembered as used lately, as the C# `RecentToolList.MaxEntries`.
pub const MAX_RECENT_TOOLS: usize = 5;

/// The tools used lately, the newest first, as the C# `RecentToolList`: for this run only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecentTools(Vec<ToolId>);

impl RecentTools {
    /// Records `tool` as the one used last: moved to the front if it was there, the oldest
    /// let go past [`MAX_RECENT_TOOLS`].
    pub fn track(&mut self, tool: ToolId) {
        self.0.retain(|known| *known != tool);
        self.0.insert(0, tool);
        self.0.truncate(MAX_RECENT_TOOLS);
    }

    /// The tools, the newest first.
    #[must_use]
    pub fn ids(&self) -> &[ToolId] {
        &self.0
    }
}

/// What pinning or unpinning a tool makes of the favourites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteToggle {
    /// The favourites after it, the caller's list untouched.
    pub favorites: Vec<String>,
    /// Whether the tool became a favourite.
    pub added: bool,
}

/// `favorites` with `tool` added at the end if absent, or removed if present, as the C#
/// `FavoriteToolSet.Toggle` (`FavoriteToolSet.cs:50-75`): membership whatever the case, the
/// identifier stored upper-cased, every spelling of it removed, the others' order kept.
#[must_use]
pub fn toggle_favorite(favorites: &[String], tool: ToolId) -> FavoriteToggle {
    let code = tool.code().to_ascii_uppercase();
    let mut kept = Vec::with_capacity(favorites.len() + 1);
    let mut removed = false;
    for favorite in favorites {
        if favorite.eq_ignore_ascii_case(&code) {
            removed = true;
        } else {
            kept.push(favorite.clone());
        }
    }
    if !removed {
        kept.push(code);
    }
    FavoriteToggle {
        favorites: kept,
        added: !removed,
    }
}

/// The tab the sidebar shows, as the C# segmented "Sessions | Tools".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTab {
    /// The sessions' tree.
    #[default]
    Sessions,
    /// The tools, by category.
    Tools,
}

impl SidebarTab {
    /// The other one, as Ctrl+Shift+T flips them.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Sessions => Self::Tools,
            Self::Tools => Self::Sessions,
        }
    }
}
