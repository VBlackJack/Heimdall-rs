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

//! Architecture diagram editor, templates, and diagramming engine.
//!
//! Provides templates for network topologies, cloud architectures, DMZ firewalls,
//! and server maps in Mermaid, ASCII, and Graphviz formats.

use std::path::PathBuf;

/// Diagram representation format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagramFormat {
    /// Mermaid diagram syntax.
    #[default]
    Mermaid,
    /// Graphviz DOT syntax.
    Dot,
    /// PlantUML syntax.
    PlantUml,
    /// ASCII box drawing syntax.
    Ascii,
}

impl DiagramFormat {
    /// File extension standard for the format.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mermaid => "mmd",
            Self::Dot => "dot",
            Self::PlantUml => "puml",
            Self::Ascii => "txt",
        }
    }

    /// Display label for the format.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Mermaid => "Mermaid",
            Self::Dot => "Graphviz DOT",
            Self::PlantUml => "PlantUML",
            Self::Ascii => "ASCII",
        }
    }
}

/// Predefined architectural diagram template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramTemplate {
    /// Template unique key identifier.
    pub id: &'static str,
    /// Human-readable title of the template.
    pub title: &'static str,
    /// Target format.
    pub format: DiagramFormat,
    /// Diagram source definition.
    pub content: &'static str,
}

/// Built-in diagram templates catalog.
pub const DIAGRAM_TEMPLATES: &[DiagramTemplate] = &[
    DiagramTemplate {
        id: "network-perimeter",
        title: "Network Perimeter & DMZ",
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
        id: "cloud-vpc",
        title: "Cloud VPC & Dual Subnets",
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
        id: "ascii-topology",
        title: "ASCII Enterprise Topology",
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
        id: "active-directory",
        title: "Active Directory Topology",
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

/// A diagram document state holding name, format, content, and file binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramDocument {
    /// Document file or display name.
    pub name: String,
    /// Diagram source format.
    pub format: DiagramFormat,
    /// Raw diagram code text.
    pub content: String,
    /// Bound file path on disk (if saved).
    pub file_path: Option<PathBuf>,
    /// Unsaved modifications indicator.
    pub dirty: bool,
}

impl DiagramDocument {
    /// Creates a new empty diagram document with the specified template.
    #[must_use]
    pub fn from_template(template: &DiagramTemplate) -> Self {
        Self {
            name: template.title.to_owned(),
            format: template.format,
            content: template.content.to_owned(),
            file_path: None,
            dirty: false,
        }
    }

    /// Sets new content and marks document as modified.
    pub fn update_content(&mut self, new_content: String) {
        if self.content != new_content {
            self.content = new_content;
            self.dirty = true;
        }
    }

    /// Updates diagram source text.
    pub fn update_source(&mut self, new_source: String) {
        self.update_content(new_source);
    }

    /// Clears diagram content and marks as dirty.
    pub fn clear(&mut self) {
        self.content.clear();
        self.dirty = true;
    }
}

impl Default for DiagramDocument {
    fn default() -> Self {
        Self::from_template(&DIAGRAM_TEMPLATES[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_templates_catalog() {
        assert!(DIAGRAM_TEMPLATES.len() >= 4);
        let first = &DIAGRAM_TEMPLATES[0];
        assert_eq!(first.id, "network-perimeter");
        assert!(first.content.contains("graph TD"));
    }

    #[test]
    fn test_document_dirty_tracking() {
        let mut doc = DiagramDocument::default();
        assert!(!doc.dirty);

        doc.update_content("flowchart TD\nA-->B".to_owned());
        assert!(doc.dirty);
        assert_eq!(doc.content, "flowchart TD\nA-->B");
    }
}
