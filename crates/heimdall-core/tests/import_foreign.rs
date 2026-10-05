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

//! `MobaXterm`, `mRemoteNG` and `RDCMan` files read as the C# Heimdall reads them, then converted
//! by its document import.

use heimdall_core::import::csharp::{ImportReport, SkipReason};
use heimdall_core::import::foreign::{FileWarning, Parsed};
use heimdall_core::import::{mobaxterm, mremoteng, rdcman};
use heimdall_core::profile::ColorDepth;

/// The report, each kept session given `id-0`, `id-1`, ...
fn report(parsed: &Parsed) -> ImportReport {
    let mut next = 0;
    parsed.report(&mut || {
        next += 1;
        format!("id-{}", next - 1)
    })
}

const MOBA: &str = "\
; a comment
[Bookmarks]
SubRep=
ImgNum=42
web= #109#0%web.lab%2222%root%%C:\\keys\\web.ppk%%%%%%%1%0
bare= #109#0%bare.lab%%%
fwd= #109#0%fwd.lab%22%root%%%%%%%%%0%1
[bookmarks_1]
SubRep=Paris\\..\\Prod\\\\DMZ
ImgNum=41
dc= #91#4%dc.lab%0%admin
ftp= #130#6%ftp.lab%21%anon%0%1
tel= #98#1%tel.lab%2323%op
vnc= #128#5%vnc.lab%5901
files= #140#7%files.lab%22%me
unknown= #999#0%x.lab%1
nohost= #109#0%%22%root
Unsafe= #109#0%u.lab%22%root%%key;rm
Climb= #109#0%c.lab%22%root%%..\\..\\key
None= #109#0%n.lab%22%root%%-1
tel=#98#1%tel2.lab%23%op
[Passwords]
root@web.lab=encrypted
admin@dc.lab=encrypted
[credentials]
one=encrypted
";

/// An SSH profile in one line: name, address, user, key, then compression, agent and files.
fn ssh_line(profile: &heimdall_core::profile::SshProfile) -> String {
    format!(
        "{} {}:{} {} {} c={} a={} f={}",
        profile.name,
        profile.host,
        profile.port,
        profile.username.as_deref().unwrap_or("-"),
        profile
            .key_path
            .as_ref()
            .map_or_else(|| "-".to_owned(), |path| path.display().to_string()),
        profile.compression,
        profile.forward_agent,
        profile.sftp,
    )
}

#[test]
fn mobaxterm_ssh_sessions_are_read_with_their_folders_and_safe_key_paths() {
    let parsed = mobaxterm::parse(MOBA);
    assert_eq!(parsed.stored_credentials, 3, "both sections, any case");
    assert!(parsed.warnings.is_empty());
    let report = report(&parsed);
    let ssh: Vec<String> = report.profiles.iter().map(ssh_line).collect();
    assert_eq!(
        ssh,
        [
            r"web web.lab:2222 root C:\keys\web.ppk c=true a=false f=false",
            "bare bare.lab:22 - - c=false a=false f=false",
            "fwd fwd.lab:22 root - c=false a=true f=false",
            "files files.lab:22 me - c=false a=false f=true",
            "Unsafe u.lab:22 root - c=false a=false f=false",
            "Climb c.lab:22 root - c=false a=false f=false",
            "None n.lab:22 root - c=false a=false f=false",
        ],
        "an unknown protocol and a session without host are not sessions; unsafe key paths go"
    );
    assert_eq!(report.profiles[0].group, None);
    let files = report
        .profiles
        .iter()
        .find(|p| p.name == "files")
        .expect("files");
    assert_eq!(
        files.group.as_deref(),
        Some("Paris/Prod/DMZ"),
        "a lower-case section too"
    );
    assert_eq!(report.profiles[0].id.as_str(), "id-0");
}

#[test]
fn mobaxterm_rdp_ftp_telnet_and_vnc_sessions_are_read_by_protocol_code() {
    let report = report(&mobaxterm::parse(MOBA));
    let dc = &report.rdp[0];
    assert_eq!(
        (dc.name.as_str(), dc.port, dc.username.as_deref()),
        ("dc", 3389, Some("admin"))
    );
    let ftp = &report.ftp[0];
    assert_eq!(
        (ftp.host.as_str(), ftp.port, ftp.passive, ftp.tls),
        ("ftp.lab", 21, false, true)
    );
    let telnet: Vec<(&str, u16)> = report
        .telnet
        .iter()
        .map(|p| (p.host.as_str(), p.port))
        .collect();
    assert_eq!(
        telnet,
        [("tel2.lab", 23)],
        "a key given twice is its last value"
    );
    assert_eq!(report.vnc[0].port, 5901);
    assert!(report.skipped.is_empty());
}

#[test]
fn a_mobaxterm_ini_without_bookmarks_gives_nothing() {
    let parsed = mobaxterm::parse("[Misc]\nweb= #109#0%web.lab%22%root\n");
    assert!(parsed.servers.is_empty());
    assert_eq!(parsed.stored_credentials, 0);
}

const MREMOTENG: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<mrng:Connections xmlns:mrng="http://mremoteng.org" Name="Connections" ConfVersion="2.6">
  <Node Name="Paris" Type="Container">
    <Node Name="Prod" Type="Container">
      <Node Name="dc" Type="Connection" Protocol="RDP" Hostname="dc.lab" Port="3390"
            Username="admin" Domain="CORP" Colors="Colors256" Resolution="SmartSize"
            RedirectClipboard="False" RDGatewayUsageMethod="Never" RDGatewayHostname="gw.lab" />
      <Node Name="web" Type="Connection" Protocol="SSH2" Hostname="web.lab" Port="22"
            Username="root" Domain="CORP" />
    </Node>
  </Node>
  <Node Name="plain" Hostname="plain.lab" />
  <Node Name="clip" Hostname="clip.lab" RedirectClipboard="False" />
  <Node Name="router" Type="Connection" Protocol="Rlogin" Hostname="r.lab" Username="op" />
  <Node Name="site" Type="Connection" Protocol="HTTPS" Hostname="site.lab" />
  <Node Name="behind" Type="Connection" Protocol="RDP" Hostname="b.lab" RDGatewayUsageMethod="Always" RDGatewayHostname="gw.lab" />
  <Node Type="Connection" Protocol="VNC" Hostname="vnc.lab" Port="5905" />
  <Node Type="Connection" Protocol="SSH2" />
</mrng:Connections>
"#;

#[test]
fn mremoteng_nodes_are_read_with_their_containers_as_folders() {
    let parsed = mremoteng::parse(MREMOTENG);
    assert!(parsed.warnings.is_empty());
    let report = report(&parsed);

    let dc = &report.rdp[0];
    assert_eq!(
        (
            dc.name.as_str(),
            dc.host.as_str(),
            dc.port,
            dc.group.as_deref(),
            dc.username.as_deref(),
            dc.domain.as_deref()
        ),
        (
            "dc",
            "dc.lab",
            3390,
            Some("Paris/Prod"),
            Some("admin"),
            Some("CORP")
        )
    );
    assert_eq!(dc.options.color_depth, ColorDepth::nearest(16));
    assert!(dc.options.dynamic_resolution);
    assert!(!dc.redirect_clipboard, "its own setting, not the default");
    assert_eq!(
        (report.rdp[1].name.as_str(), report.rdp[1].port),
        ("plain", 3389),
        "no protocol and no type: an RDP connection, as mRemoteNG's default"
    );
    assert!(
        !report.rdp[2].redirect_clipboard,
        "a redirection alone is the profile's own setting, not the default"
    );

    let web = &report.profiles[0];
    assert_eq!(
        (web.group.as_deref(), web.username.as_deref()),
        (Some("Paris/Prod"), Some("root@CORP"))
    );
    assert_eq!(
        (report.telnet[0].host.as_str(), report.telnet[0].port),
        ("r.lab", 23),
        "Rlogin as Telnet, as the C#"
    );
    assert_eq!(
        (report.vnc[0].name.as_str(), report.vnc[0].port),
        ("vnc.lab", 5905),
        "no name: the host names it"
    );
    let skipped: Vec<(&str, &SkipReason)> = report
        .skipped
        .iter()
        .map(|s| (s.name.as_str(), &s.reason))
        .collect();
    assert_eq!(
        skipped,
        [("site", &SkipReason::NotSsh("HTTPS".to_owned()))],
        "a protocol Heimdall lacks is said, not made RDP; a node with neither name nor host is none"
    );
    let gateways: Vec<(&str, Option<&str>)> = report
        .rdp
        .iter()
        .map(|profile| (profile.name.as_str(), profile.extras.rd_gateway()))
        .filter(|(_, gateway)| gateway.is_some())
        .collect();
    assert_eq!(
        gateways,
        [("behind", Some("gw.lab"))],
        "a gateway always used is kept; one never used is none"
    );
}

#[test]
fn an_mremoteng_file_encrypted_whole_a_dtd_and_broken_xml_are_said() {
    let parsed = mremoteng::parse(
        r#"<Connections FullFileEncryption="true"><Node Name="a" Hostname="a" /></Connections>"#,
    );
    assert_eq!(parsed.warnings, [FileWarning::FullyEncrypted]);
    assert!(parsed.servers.is_empty());

    let dtd = r#"<?xml version="1.0"?><!DOCTYPE c [<!ENTITY x "boom">]><Connections><Node Name="&x;" Hostname="h" /></Connections>"#;
    let parsed = mremoteng::parse(dtd);
    assert!(matches!(
        parsed.warnings.as_slice(),
        [FileWarning::Unreadable(_)]
    ));
    assert!(parsed.servers.is_empty());

    let parsed = mremoteng::parse("<Connections><Node");
    assert!(matches!(
        parsed.warnings.as_slice(),
        [FileWarning::Unreadable(_)]
    ));
}

const RDCMAN_27: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<RDCMan programVersion="2.7" schemaVersion="3">
  <file>
    <properties><name>Estate</name></properties>
    <group>
      <properties><name>Paris</name></properties>
      <logonCredentials inherit="None">
        <profileName scope="Local">Custom</profileName>
        <userName>admin</userName>
        <password>encrypted</password>
        <domain>CORP</domain>
      </logonCredentials>
      <connectionSettings inherit="None"><port>3391</port></connectionSettings>
      <server>
        <properties><displayName>Domain controller</displayName><name>dc.lab</name></properties>
      </server>
      <server>
        <properties><name>files.lab</name></properties>
        <gatewaySettings inherit="None"><enabled>False</enabled><hostName>gw.lab</hostName></gatewaySettings>
      </server>
      <server>
        <properties><name>far.lab</name></properties>
        <gatewaySettings inherit="None"><enabled>True</enabled><hostName>gw.lab</hostName></gatewaySettings>
      </server>
    </group>
  </file>
</RDCMan>
"#;

/// An RDP profile in one line: name, address, folder, user, domain.
fn rdp_line(profile: &heimdall_core::profile::RdpProfile) -> String {
    format!(
        "{} {}:{} {} {} {}",
        profile.name,
        profile.host,
        profile.port,
        profile.group.as_deref().unwrap_or("-"),
        profile.username.as_deref().unwrap_or("-"),
        profile.domain.as_deref().unwrap_or("-"),
    )
}

#[test]
fn an_rdcman_27_file_is_read_under_its_properties_where_the_csharp_finds_nothing() {
    let parsed = rdcman::parse(RDCMAN_27);
    let report = report(&parsed);
    let rdp: Vec<String> = report.rdp.iter().map(rdp_line).collect();
    assert_eq!(
        rdp,
        [
            "Domain controller dc.lab:3391 Estate/Paris admin CORP",
            "files.lab files.lab:3391 Estate/Paris admin CORP",
            "far.lab far.lab:3391 Estate/Paris admin CORP",
        ],
        "the account and port are the group's"
    );
    let gateways: Vec<Option<&str>> = report
        .rdp
        .iter()
        .map(|profile| profile.extras.rd_gateway())
        .collect();
    assert_eq!(
        gateways,
        [None, None, Some("gw.lab")],
        "a gateway switched off is none; one switched on is kept"
    );
    assert!(report.skipped.is_empty());
}

#[test]
fn an_rdcman_22_file_names_its_groups_and_servers_directly() {
    let parsed = rdcman::parse(
        "<file><name>Old</name><group><name>Lab</name>\
         <server><name>a.lab</name></server>\
         <server><displayName>B</displayName><name>b.lab</name>\
         <logonCredentials><userName>me</userName></logonCredentials>\
         <connectionSettings><port>0</port></connectionSettings></server>\
         <server><displayName> </displayName></server>\
         </group></file>",
    );
    let report = report(&parsed);
    let rdp: Vec<String> = report.rdp.iter().map(rdp_line).collect();
    assert_eq!(
        rdp,
        ["a.lab a.lab:3389 Old/Lab - -", "B b.lab:3389 Old/Lab me -"],
        "a server without a name is none; port 0 is the default"
    );
    assert!(matches!(
        rdcman::parse("not xml").warnings.as_slice(),
        [FileWarning::Unreadable(_)]
    ));
}

#[test]
fn the_ids_given_are_the_callers_one_per_kept_session() {
    let parsed = mremoteng::parse(MREMOTENG);
    let ids: Vec<String> = {
        let report = report(&parsed);
        report
            .rdp
            .iter()
            .map(|p| p.id.as_str().to_owned())
            .chain(report.profiles.iter().map(|p| p.id.as_str().to_owned()))
            .collect()
    };
    assert_eq!(ids.len(), 5, "the one behind its RD Gateway too");
    let unique: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), 5);
    assert!(ids.iter().all(|id| id.starts_with("id-")));
}
