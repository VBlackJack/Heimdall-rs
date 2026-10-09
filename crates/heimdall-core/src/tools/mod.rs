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
//! `Matching`, `Permissions`, `Temporal`, `Hashing`, `Otp` and `Jwt`, and the computations
//! of the C# tool views: what a tool computes, apart from how its tab shows it.

pub mod base64_codec;
pub mod cron_builder;
pub mod date_time;
pub mod diff_engine;
pub mod hash_computer;
pub mod hmac_computer;
pub mod ip_address;
pub mod ip_codec;
pub mod json_codec;
pub mod jwt_parser;
pub mod network_calculator;
pub mod number_text;
pub mod posix_mode;
pub mod regex_engine;
pub mod ssh_config;
pub mod subnet_calculator;
pub mod text_case_codec;
pub mod time_zone_rules;
pub mod totp_generator;
pub mod ulid_generator;
pub mod url_codec;
pub mod uuid_generator;
