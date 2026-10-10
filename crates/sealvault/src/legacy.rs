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

//! Broken cryptography that a protocol still mandates, each piece fenced to that one use.
//!
//! Nothing here protects anything by today's standards. Each function exists only because
//! a specification Heimdall must speak leaves no other choice, does exactly what that
//! specification says and nothing more, and exports no general primitive: there is no DES
//! cipher to reach for here, only the one exchange that requires it.
//!
//! ```
//! // The VNC response is reachable...
//! let _ = sealvault::legacy::vnc_des::response(b"password", &[0; 16]);
//! ```
//!
//! ```compile_fail
//! // ...but not the cipher underneath it.
//! use sealvault::legacy::vnc_des::Des;
//! ```

pub mod vnc_des;
