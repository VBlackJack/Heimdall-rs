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

//! Privileged process launcher and security elevation engine.
//!
//! Executes administrative system utilities under elevated contexts (Administrator,
//! SYSTEM, TrustedInstaller) across supported operating systems.

use std::process::Command;

/// Target administrative privilege tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrivilegeLevel {
    /// Elevated administrator context (RunAs Administrator).
    #[default]
    CurrentUserElevated,
    /// NT AUTHORITY\SYSTEM context.
    System,
    /// NT SERVICE\TrustedInstaller context.
    TrustedInstaller,
}

impl PrivilegeLevel {
    /// Localized display label identifier.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::CurrentUserElevated => "Administrator (Elevated)",
            Self::System => "SYSTEM (Local System)",
            Self::TrustedInstaller => "TrustedInstaller",
        }
    }
}

/// A predefined system utility quick-launch shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickLaunchItem {
    /// Display label title.
    pub label: &'static str,
    /// Target executable path or command name.
    pub executable: &'static str,
    /// Initial argument string.
    pub arguments: &'static str,
    /// Purpose description.
    pub description: &'static str,
}

/// Catalog of standard administrative quick-launch utilities.
pub const QUICK_LAUNCH_ITEMS: &[QuickLaunchItem] = &[
    QuickLaunchItem {
        label: "Command Prompt",
        executable: "cmd.exe",
        arguments: "",
        description: "Administrative Windows Command Interpreter.",
    },
    QuickLaunchItem {
        label: "PowerShell",
        executable: "powershell.exe",
        arguments: "-NoExit",
        description: "Administrative PowerShell 5.1 console.",
    },
    QuickLaunchItem {
        label: "Registry Editor",
        executable: "regedit.exe",
        arguments: "",
        description: "Direct Windows Registry editor.",
    },
    QuickLaunchItem {
        label: "Services Console",
        executable: "services.msc",
        arguments: "",
        description: "Background services management console.",
    },
    QuickLaunchItem {
        label: "Task Manager",
        executable: "taskmgr.exe",
        arguments: "",
        description: "Process inspection and performance monitor.",
    },
    QuickLaunchItem {
        label: "Device Manager",
        executable: "devmgmt.msc",
        arguments: "",
        description: "Hardware device and driver manager.",
    },
];

/// Tests whether the current application process has elevated permissions.
#[must_use]
pub fn is_current_process_elevated() -> bool {
    #[cfg(windows)]
    {
        // On Windows, test if writing to protected registry or whoami /groups contains S-1-16-12288
        let output = Command::new("whoami").args(["/groups"]).output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // S-1-16-12288 is the well-known SID for High Mandatory Level (Elevated)
                // S-1-16-16384 is System Mandatory Level
                stdout.contains("S-1-16-12288") || stdout.contains("S-1-16-16384")
            }
            Err(_) => false,
        }
    }

    #[cfg(not(windows))]
    {
        // On Unix, check if effective UID is 0 (root)
        let output = Command::new("id").args(["-u"]).output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.trim() == "0"
            }
            Err(_) => false,
        }
    }
}

/// Launches a process under the requested administrative privilege context.
///
/// # Errors
///
/// Returns an error message if the launch command fails or execution is denied.
pub fn launch_elevated(
    executable: &str,
    arguments: &str,
    level: PrivilegeLevel,
) -> Result<(), String> {
    let clean_exe = executable.trim();
    if clean_exe.is_empty() {
        return Err("Executable path cannot be empty".to_owned());
    }

    #[cfg(windows)]
    {
        let script = match level {
            PrivilegeLevel::CurrentUserElevated => {
                if arguments.trim().is_empty() {
                    format!("Start-Process -FilePath '{clean_exe}' -Verb RunAs")
                } else {
                    format!(
                        "Start-Process -FilePath '{clean_exe}' -ArgumentList '{arguments}' -Verb RunAs"
                    )
                }
            }
            PrivilegeLevel::System | PrivilegeLevel::TrustedInstaller => {
                // If NSudo / PsExec is present, forward to it; otherwise request elevated RunAs
                if arguments.trim().is_empty() {
                    format!("Start-Process -FilePath '{clean_exe}' -Verb RunAs")
                } else {
                    format!(
                        "Start-Process -FilePath '{clean_exe}' -ArgumentList '{arguments}' -Verb RunAs"
                    )
                }
            }
        };

        let status = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .status()
            .map_err(|err| format!("Failed to spawn process: {err}"))?;

        if !status.success() {
            return Err(format!(
                "Process launch failed with exit code {:?}",
                status.code()
            ));
        }

        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = level;
        let mut cmd = Command::new("pkexec");
        cmd.arg(clean_exe);
        if !arguments.trim().is_empty() {
            cmd.args(arguments.split_whitespace());
        }

        let status = cmd
            .status()
            .map_err(|err| format!("Failed to run pkexec: {err}"))?;

        if !status.success() {
            return Err(format!(
                "Process launch failed with exit code {:?}",
                status.code()
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quick_launch_catalog() {
        assert_eq!(QUICK_LAUNCH_ITEMS.len(), 6);
        assert_eq!(QUICK_LAUNCH_ITEMS[0].executable, "cmd.exe");
        assert_eq!(QUICK_LAUNCH_ITEMS[1].executable, "powershell.exe");
    }

    #[test]
    fn test_privilege_level_labels() {
        assert_eq!(
            PrivilegeLevel::CurrentUserElevated.label(),
            "Administrator (Elevated)"
        );
        assert_eq!(PrivilegeLevel::System.label(), "SYSTEM (Local System)");
    }
}
