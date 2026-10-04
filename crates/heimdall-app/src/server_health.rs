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

//! What an SSH server says of its CPU, memory and disk, as the C# `ServerHealthMonitor`
//! asks it every 15 seconds: `top`, `free` and `df`, run on the session's own connection.
//! Their output is the server's: read for numbers only, never trusted beyond that.

use std::time::Duration;

use heimdall_ssh::Connection;
use tokio_util::sync::CancellationToken;

/// How often the server is asked, as the C#.
pub const HEALTH_INTERVAL: Duration = Duration::from_secs(15);

/// How long one command may take.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// Longest size text kept from `df`, as shown.
const MAX_SIZE_TEXT: usize = 16;

/// What runs each command: a POSIX shell reading it on its input, as the other commands
/// Heimdall runs on a server.
const SHELL: &str = "sh -s";

const CPU_COMMAND: &str = "top -b -n 1 | head -5";
const MEMORY_COMMAND: &str = "free -m | grep Mem";
const DISK_COMMAND: &str = "df -h / | tail -1";

/// A shell tab's server health panel, as the C# SSH view's.
#[derive(Debug, Default)]
pub struct HealthPane {
    /// The session's connection, held without keeping it open; none for a tab that is not
    /// an SSH shell.
    pub connection: Option<heimdall_ssh::WeakConnection>,
    /// The panel is shown, and the server asked every [`HEALTH_INTERVAL`].
    pub shown: bool,
    /// What the server last said.
    pub last: Option<ServerHealth>,
    /// The server is being asked: one question at a time.
    pub asking: bool,
}

impl HealthPane {
    /// Whether the panel can be shown: an SSH shell whose session is open.
    #[must_use]
    pub fn available(&self) -> bool {
        self.connection
            .as_ref()
            .is_some_and(|connection| connection.upgrade().is_some())
    }

    /// The connection to ask the server over now: while shown, connected and not asking.
    pub fn ask(&mut self) -> Option<Connection> {
        if !self.shown || self.asking {
            return None;
        }
        let connection = self.connection.as_ref()?.upgrade()?;
        self.asking = true;
        Some(connection)
    }
}

/// What the server said of itself.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerHealth {
    /// CPU in use, in percent.
    pub cpu_percent: f64,
    /// Memory, in MB: in all, used.
    pub memory_mb: (u64, u64),
    /// The root file system's size and use, as `df -h` writes them, and its use in percent.
    pub disk: (String, String, u8),
    /// The server answered as a Linux server does: what it said was read.
    pub supported: bool,
}

/// Asks the server over `connection` for its CPU, memory and disk, the three at once.
pub async fn collect(connection: Connection) -> ServerHealth {
    let run = |command: &'static str| {
        let connection = connection.clone();
        async move {
            match connection
                .run_command(
                    SHELL,
                    command.as_bytes(),
                    COMMAND_TIMEOUT,
                    CancellationToken::new(),
                )
                .await
            {
                Ok(end) if end.status == Some(0) => {
                    String::from_utf8_lossy(&end.stdout).into_owned()
                }
                _ => String::new(),
            }
        }
    };
    let (cpu, memory, disk) =
        tokio::join!(run(CPU_COMMAND), run(MEMORY_COMMAND), run(DISK_COMMAND));
    read(&cpu, &memory, &disk)
}

/// What `top`, `free` and `df` said, read as the C# reads it: supported when something
/// came and all that came was read.
#[must_use]
pub fn read(cpu: &str, memory: &str, disk: &str) -> ServerHealth {
    let cpu_read = (!cpu.trim().is_empty()).then(|| cpu_percent(cpu));
    let memory_read = (!memory.trim().is_empty()).then(|| memory_mb(memory));
    let disk_read = (!disk.trim().is_empty()).then(|| disk_use(disk));
    let answered = [
        cpu_read.is_some(),
        memory_read.is_some(),
        disk_read.is_some(),
    ];
    let failed = cpu_read.as_ref().is_some_and(Option::is_none)
        || memory_read.as_ref().is_some_and(Option::is_none)
        || disk_read.as_ref().is_some_and(Option::is_none);
    ServerHealth {
        cpu_percent: cpu_read.flatten().unwrap_or_default(),
        memory_mb: memory_read.flatten().unwrap_or_default(),
        disk: disk_read
            .flatten()
            .unwrap_or_else(|| ("?".to_owned(), "?".to_owned(), 0)),
        supported: answered.contains(&true) && !failed,
    }
}

/// CPU in use from `top`'s summary: 100 less its idle share, else its user share.
fn cpu_percent(top: &str) -> Option<f64> {
    let line = top
        .lines()
        .find(|line| line.to_ascii_lowercase().contains("cpu") && line.contains(':'))?;
    let (_, shares) = line.split_once(':')?;
    if let Some(idle) = share(shares, "id") {
        return Some(round(100.0 - idle).max(0.0));
    }
    share(shares, "us").map(round)
}

/// The number written just before `name` in `top`'s shares ("95.5 id", "95,5 id"), as the
/// C# reads it: digits, a decimal point or comma, digits.
fn share(shares: &str, name: &str) -> Option<f64> {
    shares.match_indices(name).find_map(|(at, _)| {
        let after = shares[at + name.len()..].chars().next();
        if after.is_some_and(char::is_alphabetic) {
            return None;
        }
        let before = shares[..at].trim_end();
        let start = before
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_digit() || *c == '.' || *c == ',')
            .last()
            .map(|(index, _)| index)?;
        before[start..]
            .trim_start_matches([',', '.'])
            .replace(',', ".")
            .parse::<f64>()
            .ok()
    })
}

fn round(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// Memory in all and used, in MB, from `free -m`'s "Mem:" line.
fn memory_mb(free: &str) -> Option<(u64, u64)> {
    let line = free
        .lines()
        .find(|line| line.trim_start().starts_with("Mem:"))?;
    let mut numbers = line
        .trim_start()
        .strip_prefix("Mem:")?
        .split_whitespace()
        .map(str::parse::<u64>);
    let total = numbers.next()?.ok()?;
    let used = numbers.next()?.ok()?;
    Some((total, used))
}

/// The root file system's size, use and use in percent from `df -h`'s line.
fn disk_use(df: &str) -> Option<(String, String, u8)> {
    let line = df.lines().rev().find(|line| !line.trim().is_empty())?;
    let fields: Vec<&str> = line.split_whitespace().collect();
    let at = fields.iter().position(|field| {
        field
            .strip_suffix('%')
            .is_some_and(|number| number.parse::<u8>().is_ok())
    })?;
    if at < 3 {
        return None;
    }
    let percent = fields[at].strip_suffix('%')?.parse::<u8>().ok()?.min(100);
    let shown = |text: &str| -> String {
        crate::text::server_text(text)
            .chars()
            .take(MAX_SIZE_TEXT)
            .collect()
    };
    Some((shown(fields[at - 3]), shown(fields[at - 2]), percent))
}

#[cfg(test)]
mod tests {
    use super::{cpu_percent, disk_use, memory_mb, read};

    #[test]
    fn cpu_in_use_is_read_from_tops_summary_as_the_csharp_reads_it() {
        let top = "top - 10:00:01 up 3 days,  2 users,  load average: 0.10, 0.20, 0.30\n\
                   Tasks: 120 total,   1 running\n\
                   %Cpu(s):  3.1 us,  1.2 sy,  0.0 ni, 95.5 id,  0.2 wa,  0.0 hi,  0.0 si\n";
        assert_eq!(cpu_percent(top), Some(4.5));
        let comma = "%Cpu(s):  3,1 us,  1,2 sy,  0,0 ni, 95,5 id,  0,2 wa\n";
        assert_eq!(cpu_percent(comma), Some(4.5), "a decimal comma");
        let busybox = "CPU:  12.0 us,  3.0 sy\n";
        assert_eq!(
            cpu_percent(busybox),
            Some(12.0),
            "no idle share: the user's"
        );
        assert_eq!(cpu_percent("nothing like top\n"), None);
    }

    #[test]
    fn memory_and_disk_are_read_from_free_and_df() {
        assert_eq!(
            memory_mb("Mem:           7821        2345        3456          12\n"),
            Some((7821, 2345))
        );
        assert_eq!(memory_mb("Swap: 0 0 0\n"), None);
        assert_eq!(
            disk_use("/dev/sda1        20G  5.2G   14G  28% /\n"),
            Some(("20G".to_owned(), "5.2G".to_owned(), 28))
        );
        assert_eq!(disk_use("garbage\n"), None);
    }

    #[test]
    fn a_server_that_answers_as_linux_does_is_read_and_another_is_unsupported() {
        let health = read(
            "%Cpu(s): 10.0 us, 5.0 sy, 0.0 ni, 85.0 id\n",
            "Mem: 1000 250 750\n",
            "/dev/root 10G 4G 6G 40% /\n",
        );
        assert!(health.supported);
        assert!((health.cpu_percent - 15.0).abs() < 1e-9);
        assert_eq!(health.memory_mb, (1000, 250));
        assert_eq!(health.disk, ("10G".to_owned(), "4G".to_owned(), 40));

        assert!(!read("", "", "").supported, "nothing came");
        assert!(
            !read("'top' is not recognized\n", "", "").supported,
            "what came could not be read"
        );
        let partly = read("", "Mem: 1000 250 750\n", "");
        assert!(partly.supported, "the C# shows what it could read");
    }

    #[test]
    fn what_the_server_writes_for_its_sizes_is_cleaned_and_cut() {
        let hostile = "/dev/x \u{1b}[31mBIG\u{1b}[0m 1G 2G 50% /\n";
        let (total, _, percent) = disk_use(hostile).expect("read");
        assert!(!total.contains('\u{1b}'), "{total:?}");
        assert_eq!(percent, 50);
        let long = format!("/dev/x {} 1G 2G 50% /\n", "9".repeat(64));
        assert!(disk_use(&long).expect("read").0.chars().count() <= 16);
    }
}
