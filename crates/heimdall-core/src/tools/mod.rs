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

//! The engines of the built-in tools, as the C# `Heimdall.Core` `Codecs`, `Identifiers`,
//! `Matching`, `Permissions`, `Temporal`, `Hashing`, `Otp`, `Jwt` and `Certificates`, and the
//! computations of the C# tool views, key and password tools among them: what a tool
//! computes, apart from how its tab shows it.

#![allow(clippy::all, clippy::pedantic)]

pub mod arp_monitor;
pub mod banner_grab;
pub mod base64_codec;
pub mod cert_inspector;
pub mod certificate_generator;
pub mod command_activity;
pub mod command_library;
pub mod cron_builder;
pub mod cron_job;
pub mod csv_cell;
pub mod cve_lookup;
pub mod date_time;
pub mod default_creds;
pub mod diagram_editor;
pub mod diff_engine;
pub mod dns_batch;
pub mod dns_lookup;
pub mod dns_security;
pub mod firewall_tester;
pub mod hash_computer;
pub mod hmac_computer;
pub mod hosts_editor;
pub mod http_headers;
pub mod http_status;
pub mod ip_address;
pub mod ip_codec;
pub mod json_codec;
pub mod jwt_parser;
pub mod legacy_tls;
pub mod log_viewer;
pub mod mac_vendor;
pub mod net_host;
pub mod net_input;
pub mod network_calculator;
pub mod network_cartography;
pub mod network_fingerprint;
pub mod network_history;
pub mod network_interfaces;
pub mod network_scanner;
pub mod notes_storage;
pub mod number_text;
pub mod open_ports;
pub mod password_audit;
pub mod password_generator;
pub mod password_presets;
pub mod password_rules;
pub mod password_wordlists;
pub mod ping;
pub mod pkcs8_pem;
pub mod port_scanner;
pub mod posix_mode;
pub mod private_file;
pub mod probe_pool;
pub mod regex_engine;
pub mod route_table;
pub mod runbooks;
pub mod secnumcloud;
pub mod secure_random;
pub mod service_status;
pub mod smb;
pub mod snmp_walker;
pub mod ssh_config;
pub mod ssh_key_audit;
pub mod ssh_key_generator;
pub mod subnet_calculator;
pub mod system_listing;
pub mod system_program;
pub mod tcp_ping;
pub mod tcp_traceroute;
pub mod text_case_codec;
pub mod time_zone_rules;
pub mod tls_audit;
pub mod totp_generator;
pub mod ulid_generator;
pub mod url_codec;
pub mod user_commands;
pub mod uuid_generator;
pub mod wake_on_lan;
pub mod whois;
pub mod wifi_scanner;
