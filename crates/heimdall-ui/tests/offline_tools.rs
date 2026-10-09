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

//! The offline tools drawn headless, each opened in its tab, used and clicked: the subnet
//! calculator, the IP converter, the network calculator, the chmod calculator, the date and
//! time converter, the ULID generator, the crontab builder and the SSH config generator.
//!
//! Strings are the fallback language's (English): the tests never select a language.
//! Setting `HEIMDALL_SNAPSHOT_DIR` writes a PNG of each tool there, for a visual pass.

mod common;

use std::path::Path;

use heimdall_app::tools::ToolId;
use heimdall_app::{App, AppConfig, TabId};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::tools::{
    self, ChmodMessage, CrontabMessage, DateTimeMessage, IpConverterMessage, NetCalcField,
    NetCalcMessage, NetCalcMode, SshConfigMessage, SshField, SubnetMessage, ToolMessage,
    UlidMessage,
};
use iced::{Settings, Size};

/// Size of the simulated window, in logical pixels.
const WINDOW: Size = Size::new(1200.0, 760.0);

/// Environment variable naming a directory for PNG snapshots.
const SNAPSHOT_VARIABLE: &str = "HEIMDALL_SNAPSHOT_DIR";

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn settings() -> Settings {
    Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    }
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    common::simulator(settings(), WINDOW, shell.view())
}

/// Writes a PNG of the window when `HEIMDALL_SNAPSHOT_DIR` is set.
fn snapshot(shell: &Shell, name: &str) {
    let Some(dir) = std::env::var_os(SNAPSHOT_VARIABLE) else {
        return;
    };
    let path = Path::new(&dir).join(name);
    // iced names the picture after its renderer: an old one is cleared first, else it is
    // only compared against.
    let stem = name.trim_end_matches(".png");
    if let Ok(entries) = std::fs::read_dir(Path::new(&dir)) {
        for entry in entries.flatten() {
            let file = entry.file_name();
            let file = file.to_string_lossy();
            if file == name || file.starts_with(&format!("{stem}-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    simulator(shell)
        .snapshot(&shell.theme())
        .expect("drawn")
        .matches_image(path)
        .expect("written");
}

/// A window with `tool` open in its tab, and the tab.
fn opened(dir: &Path, tool: ToolId) -> (Shell, TabId) {
    let mut shell = Shell::with_app(app(dir));
    let _ = shell.update(Message::OpenTool(tool));
    let tab = shell.app().tabs[0].id;
    assert_eq!(shell.app().tabs[0].display_title(), tools::label(tool));
    (shell, tab)
}

/// Whether the window sent `wanted` among its messages.
fn sent(messages: impl Iterator<Item = Message>, wanted: impl Fn(&ToolMessage) -> bool) -> bool {
    messages
        .into_iter()
        .any(|message| matches!(&message, Message::Tool(_, tool) if wanted(tool)))
}

#[test]
fn the_subnet_calculator_breaks_a_network_down() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::SubnetCalculator);
    {
        let mut ui = simulator(&shell);
        ui.find("Enter a CIDR notation to calculate subnet details")
            .expect("empty state");
        ui.find("IP address / CIDR notation (e.g. 192.168.1.0/24)")
            .expect("label");
    }
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::Subnet(SubnetMessage::InputEdited("192.168.1.64/26".to_owned())),
    ));
    snapshot(&shell, "tools-subnet.png");
    let mut ui = simulator(&shell);
    ui.find("192.168.1.127").expect("broadcast");
    ui.find("255.255.255.192").expect("mask");
    ui.find("62").expect("hosts");
    ui.find("Wildcard Mask").expect("row");
    ui.find("192.168.1.64/26").expect("cidr");
    ui.click("Copy").expect("a copy button");
    assert!(sent(ui.into_messages(), |message| matches!(
        message,
        ToolMessage::Subnet(SubnetMessage::Copy(_))
    )));
}

#[test]
fn the_ip_converter_shows_every_form() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::IpConverter);
    {
        let mut ui = simulator(&shell);
        ui.find("Enter an IP address, integer, hex, or dotted binary to convert.")
            .expect("empty state");
    }
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::IpConverter(IpConverterMessage::InputEdited("3232235777".to_owned())),
    ));
    snapshot(&shell, "tools-ipconv.png");
    let mut ui = simulator(&shell);
    ui.find("192.168.1.1").expect("dotted");
    ui.find("0xC0A80101").expect("hex");
    ui.find("11000000.10101000.00000001.00000001")
        .expect("binary");
    ui.find("::ffff:c0a8:0101").expect("mapped");
    ui.find("IPv4-Mapped IPv6").expect("label");
    let _ = ui.into_messages();
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::IpConverter(IpConverterMessage::InputEdited("abc".to_owned())),
    ));
    let mut ui = simulator(&shell);
    ui.find("Invalid input. Enter a valid IPv4 address, integer, hex (0x...), or dotted binary.")
        .expect("error");
}

#[test]
fn the_network_calculator_plans_a_vlan_and_switches_modes() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::NetworkCalculator);
    {
        let mut ui = simulator(&shell);
        ui.find("CIDR ranges (one per line)")
            .expect("supernet mode");
        ui.find("Enter subnets or IP ranges to calculate.")
            .expect("empty state");
    }
    let send = |shell: &mut Shell, message| {
        let _ = shell.update(Message::Tool(tab, ToolMessage::NetCalc(message)));
    };
    send(&mut shell, NetCalcMessage::Mode(NetCalcMode::VlanPlanner));
    send(
        &mut shell,
        NetCalcMessage::Edited(NetCalcField::Hosts, "50".to_owned()),
    );
    send(
        &mut shell,
        NetCalcMessage::Edited(NetCalcField::BaseNetwork, "10.0.0.0".to_owned()),
    );
    {
        let mut ui = simulator(&shell);
        ui.click("Compute").expect("button");
        assert!(sent(ui.into_messages(), |message| matches!(
            message,
            ToolMessage::NetCalc(NetCalcMessage::Compute)
        )));
    }
    send(&mut shell, NetCalcMessage::Compute);
    snapshot(&shell, "tools-netcalc.png");
    let mut ui = simulator(&shell);
    ui.find("Hosts needed").expect("label");
    ui.find("Copy").expect("the result's copy");
}

#[test]
fn the_chmod_calculator_keeps_its_forms_in_step() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::Chmod);
    {
        let mut ui = simulator(&shell);
        ui.find("rwxr-xr-x").expect("755 at first");
        ui.find("chmod 755 filename").expect("command");
        ui.find("Common presets").expect("presets");
        ui.click("644").expect("preset");
        assert!(sent(ui.into_messages(), |message| matches!(
            message,
            ToolMessage::Chmod(ChmodMessage::Preset(_))
        )));
    }
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::Chmod(ChmodMessage::SymbolicEdited("u=rwx,g=rx,o=".to_owned())),
    ));
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::Chmod(ChmodMessage::ApplySymbolic),
    ));
    snapshot(&shell, "tools-chmod.png");
    let mut ui = simulator(&shell);
    ui.find("rwxr-x---").expect("symbolic");
    ui.find("chmod u=rwx,g=rx,o= filename").expect("command");
    ui.click("Copy octal").expect("button");
    assert!(sent(ui.into_messages(), |message| matches!(
        message,
        ToolMessage::Chmod(ChmodMessage::CopyOctal)
    )));
}

#[test]
fn the_date_time_converter_shows_every_form() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::DateTime);
    {
        let mut ui = simulator(&shell);
        ui.find("Enter a Unix timestamp or ISO 8601 date to convert.")
            .expect("empty state");
        ui.click("Now").expect("button");
        assert!(sent(ui.into_messages(), |message| matches!(
            message,
            ToolMessage::DateTime(DateTimeMessage::Now)
        )));
    }
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::DateTime(DateTimeMessage::InputEdited("1712345678".to_owned())),
    ));
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::DateTime(DateTimeMessage::Convert),
    ));
    snapshot(&shell, "tools-datetime.png");
    let mut ui = simulator(&shell);
    ui.find("Detected: Unix timestamp").expect("detected");
    ui.find("2024-04-05T19:34:38.0000000Z").expect("UTC");
    ui.find("ISO 8601 (Local)").expect("local");
    ui.find("Relative time").expect("relative");
}

#[test]
fn the_ulid_generator_makes_one_then_a_batch() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::Ulid);
    {
        let mut ui = simulator(&shell);
        ui.find("Generated ULID").expect("label");
        ui.click("Generate Batch").expect("button");
        assert!(sent(ui.into_messages(), |message| matches!(
            message,
            ToolMessage::Ulid(UlidMessage::GenerateBatch)
        )));
    }
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::Ulid(UlidMessage::CountEdited("3".to_owned())),
    ));
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::Ulid(UlidMessage::GenerateBatch),
    ));
    snapshot(&shell, "tools-ulid.png");
    let mut ui = simulator(&shell);
    ui.find("Copy all").expect("batch copy");
    ui.find("Batch Generation").expect("section");
}

#[test]
fn the_crontab_builder_applies_a_preset() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::Crontab);
    {
        let mut ui = simulator(&shell);
        ui.find("Runs every minute").expect("description");
        ui.find("Next 5 executions").expect("runs");
        ui.click("Weekdays 9am").expect("preset");
        assert!(sent(ui.into_messages(), |message| matches!(
            message,
            ToolMessage::Crontab(CrontabMessage::Preset(3))
        )));
    }
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::Crontab(CrontabMessage::Preset(3)),
    ));
    snapshot(&shell, "tools-crontab.png");
    let mut ui = simulator(&shell);
    ui.find("Runs every 1-5 at 09:00").expect("description");
    ui.find("0 9 * * 1-5").expect("the expression");
    let _ = ui.into_messages();
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::Crontab(CrontabMessage::ManualEdited("* *".to_owned())),
    ));
    let mut ui = simulator(&shell);
    ui.find("A cron expression must have exactly 5 fields separated by spaces")
        .expect("error");
}

#[test]
fn the_ssh_config_generator_writes_a_block() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::SshConfig);
    {
        let mut ui = simulator(&shell);
        ui.find("Configure options above to generate an SSH config block")
            .expect("empty state");
        ui.find("ServerAliveInterval").expect("field");
        ui.click("Generate").expect("button");
        assert!(sent(ui.into_messages(), |message| matches!(
            message,
            ToolMessage::SshConfig(SshConfigMessage::Generate)
        )));
    }
    for (field, value) in [
        (SshField::Alias, "web"),
        (SshField::HostName, "web.example.com"),
        (SshField::User, "admin"),
        (SshField::Port, "2222"),
    ] {
        let _ = shell.update(Message::Tool(
            tab,
            ToolMessage::SshConfig(SshConfigMessage::Edited(field, value.to_owned())),
        ));
    }
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::SshConfig(SshConfigMessage::Generate),
    ));
    snapshot(&shell, "tools-sshconfig.png");
    let mut ui = simulator(&shell);
    ui.find("Copy").expect("the output's copy");
    assert!(
        ui.find("Configure options above to generate an SSH config block")
            .is_err(),
        "the output replaces the empty state"
    );
}

#[test]
fn every_offline_tool_has_its_help() {
    for (tool, key) in [
        (ToolId::SubnetCalculator, "ui-tool-subnet-help"),
        (ToolId::IpConverter, "ui-tool-ipconv-help"),
        (ToolId::NetworkCalculator, "ui-tool-netcalc-help"),
        (ToolId::Chmod, "ui-tool-chmod-help"),
        (ToolId::DateTime, "ui-tool-datetime-help"),
        (ToolId::Ulid, "ui-tool-ulid-help"),
        (ToolId::Crontab, "ui-tool-crontab-help"),
        (ToolId::SshConfig, "ui-tool-sshconfig-help"),
    ] {
        let dir = tempfile::tempdir().expect("dir");
        let (mut shell, tab) = opened(dir.path(), tool);
        let _ = shell.update(Message::Tool(tab, ToolMessage::ToggleHelp));
        let help = heimdall_ui::i18n::LOADER.get(key);
        assert!(help.contains("\n\n"), "{key}: {help:?}");
        let mut ui = simulator(&shell);
        ui.find(help.as_str()).expect(key);
    }
}
