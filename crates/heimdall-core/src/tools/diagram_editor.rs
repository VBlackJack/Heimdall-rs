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

//! The diagram editor's text: a diagram's source in Mermaid, Graphviz DOT, PlantUML or
//! ASCII art, typed or started from a template, opened from and saved to a text file.
//!
//! This is not the C# `DIAGRAM` tool, which embeds the draw.io editor
//! (`Views/Tools/DiagramEditorView.xaml.cs`) and draws on a canvas: here the diagram is its
//! source text, drawn by whatever reads that format.

use std::path::Path;

/// The largest file opened, so a file picked by mistake cannot fill memory.
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// The language a diagram's source is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagramFormat {
    /// Mermaid.
    #[default]
    Mermaid,
    /// Graphviz DOT.
    Dot,
    /// PlantUML.
    PlantUml,
    /// Plain text drawn with ASCII characters.
    Ascii,
}

impl DiagramFormat {
    /// Every format.
    pub const ALL: [Self; 4] = [Self::Mermaid, Self::Dot, Self::PlantUml, Self::Ascii];

    /// The extension of a file in this format.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mermaid => "mmd",
            Self::Dot => "dot",
            Self::PlantUml => "puml",
            Self::Ascii => "txt",
        }
    }

    /// The format a file's extension names; ASCII for any other.
    #[must_use]
    pub fn of_path(path: &Path) -> Self {
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        match extension.as_str() {
            "mmd" | "mermaid" => Self::Mermaid,
            "dot" | "gv" => Self::Dot,
            "puml" | "plantuml" => Self::PlantUml,
            _ => Self::Ascii,
        }
    }
}

/// Which template: its name is the window's to write in its language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    /// A perimeter with its DMZ.
    Perimeter,
    /// A cloud VPC with a public and a private subnet.
    CloudVpc,
    /// A network topology in ASCII art.
    AsciiTopology,
    /// An Active Directory forest.
    ActiveDirectory,
}

/// A diagram to start from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramTemplate {
    /// Which it is.
    pub kind: TemplateKind,
    /// The language of its source.
    pub format: DiagramFormat,
    /// Its source.
    pub content: &'static str,
}

/// The templates, in the order they are offered.
pub const DIAGRAM_TEMPLATES: &[DiagramTemplate] = &[
    DiagramTemplate {
        kind: TemplateKind::Perimeter,
        format: DiagramFormat::Mermaid,
        content: r#"graph TD
    Internet([Internet]) --> FW1[External Firewall]
    FW1 --> DMZ[DMZ Subnet 192.168.1.0/24]
    subgraph DMZ_Cluster [DMZ Zone]
        DMZ --> Web1[Web Server 01]
        DMZ --> Web2[Web Server 02]
        DMZ --> Bastion[SSH Bastion Host]
    end
    Web1 --> FW2[Internal Firewall]
    Web2 --> FW2
    Bastion --> FW2
    FW2 --> Core[Internal LAN 10.0.0.0/16]
    subgraph LAN_Zone [Internal Network]
        Core --> AppSrv[App Servers]
        Core --> DB[(PostgreSQL Cluster)]
        Core --> AD[Active Directory DC]
    end"#,
    },
    DiagramTemplate {
        kind: TemplateKind::CloudVpc,
        format: DiagramFormat::Mermaid,
        content: r#"flowchart LR
    User([Clients]) --> ALB[Application Load Balancer]
    subgraph VPC [VPC 10.100.0.0/16]
        subgraph PublicSubnet [Public Subnet - 10.100.1.0/24]
            ALB
            NAT[NAT Gateway]
        end
        subgraph PrivateSubnet [Private Subnet - 10.100.2.0/24]
            Node1[API Worker 1]
            Node2[API Worker 2]
            DB[(RDS Database)]
        end
    end
    ALB --> Node1
    ALB --> Node2
    Node1 --> DB
    Node2 --> DB
    Node1 -.-> NAT
    Node2 -.-> NAT"#,
    },
    DiagramTemplate {
        kind: TemplateKind::AsciiTopology,
        format: DiagramFormat::Ascii,
        content: r#"+---------------------------------------------------------------+
|                        [ INTERNET ]                           |
+------------------------------+--------------------------------+
                               |
                        [ FIREWALL 01 ]
                               |
            +------------------+------------------+
            |                                     |
    [ DMZ 192.168.1.0/24 ]               [ VPN GATEWAY ]
    +-- Web Server 01                     +-- Remote Clients
    +-- Web Server 02
            |
    [ INTERNAL FIREWALL ]
            |
    [ CORE SWITCH 10.0.0.1 ]
            +-- App Server Cluster (10.0.10.0/24)
            +-- Database Cluster   (10.0.20.0/24)
            +-- Admin Workstations (10.0.30.0/24)
+---------------------------------------------------------------+"#,
    },
    DiagramTemplate {
        kind: TemplateKind::ActiveDirectory,
        format: DiagramFormat::Mermaid,
        content: r#"graph TD
    Forest([corp.internal Forest Root])
    Forest --> DC1[Primary DC - PDC/FSMO]
    Forest --> DC2[Replica DC - Site HQ]
    Forest --> DC3[Branch DC - Site Remote]
    DC1 <-->|AD Replication| DC2
    DC1 <-->|AD Replication| DC3
    DC1 --> DNS1[DNS / Kerberos]
    DC2 --> DNS2[DNS / Kerberos]
    DC3 --> DNS3[DNS / Kerberos]"#,
    },
];

/// The template of kind `kind`.
#[must_use]
pub fn template(kind: TemplateKind) -> &'static DiagramTemplate {
    DIAGRAM_TEMPLATES
        .iter()
        .find(|template| template.kind == kind)
        .unwrap_or(&DIAGRAM_TEMPLATES[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_one_template_with_a_source() {
        for kind in [
            TemplateKind::Perimeter,
            TemplateKind::CloudVpc,
            TemplateKind::AsciiTopology,
            TemplateKind::ActiveDirectory,
        ] {
            let found = template(kind);
            assert_eq!(found.kind, kind);
            assert!(!found.content.trim().is_empty());
            assert_eq!(
                DIAGRAM_TEMPLATES.iter().filter(|t| t.kind == kind).count(),
                1
            );
        }
        assert_eq!(
            template(TemplateKind::AsciiTopology).format,
            DiagramFormat::Ascii
        );
    }

    #[test]
    fn a_file_extension_names_its_format() {
        for format in DiagramFormat::ALL {
            let name = format!("diagram.{}", format.extension());
            assert_eq!(DiagramFormat::of_path(Path::new(&name)), format);
        }
        assert_eq!(
            DiagramFormat::of_path(Path::new("x.GV")),
            DiagramFormat::Dot
        );
        assert_eq!(DiagramFormat::of_path(Path::new("x")), DiagramFormat::Ascii);
    }
}
