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

//! The lookups of the DNS and DNSBATCH tools, as the C# `DnsLookupService` and
//! `DnsBatchResolverService`: the platform resolver for addresses, the system's `nslookup`
//! by its full path for any other record, each bounded in time.

use std::time::{Duration, Instant};

use heimdall_core::tools::dns_batch::{
    self, DnsBatchFailure, DnsBatchItemResult, failed_item, item_from_addresses,
};
use heimdall_core::tools::dns_lookup::{
    self, DnsLookupError, DnsLookupRequest, DnsLookupResult, addresses_of_type,
};

use super::{ProgramFailure, ResolveFailure, resolve, run_program, shown_lines};

/// The milliseconds gone since `started`, as a whole number.
fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// `request` looked up, within [`dns_lookup::LOOKUP_TIMEOUT_MS`].
///
/// # Errors
///
/// [`DnsLookupError`].
pub async fn lookup(request: DnsLookupRequest) -> Result<DnsLookupResult, DnsLookupError> {
    let started = Instant::now();
    let timeout = Duration::from_millis(dns_lookup::LOOKUP_TIMEOUT_MS);
    let records = if dns_lookup::uses_system_resolver(&request) {
        match resolve(&request.host, timeout).await {
            Ok(addresses) => addresses_of_type(addresses, request.record_type),
            // An answer without an address of either family: no record.
            Err(ResolveFailure::NoAddress) => Vec::new(),
            Err(ResolveFailure::TimedOut) => return Err(DnsLookupError::TimedOut),
            Err(ResolveFailure::Failed(why)) => return Err(DnsLookupError::Failed(why)),
        }
    } else {
        let program = dns_lookup::nslookup_program().ok_or(DnsLookupError::NoProgram)?;
        let arguments = dns_lookup::nslookup_arguments(&request);
        match run_program(&program, &arguments, timeout).await {
            Ok(run) => dns_lookup::nslookup_answer(&run.stdout, &run.stderr)
                .iter()
                .flat_map(|line| shown_lines(line))
                .collect(),
            Err(ProgramFailure::TimedOut) => return Err(DnsLookupError::TimedOut),
            Err(ProgramFailure::Failed(why)) => return Err(DnsLookupError::Failed(why)),
        }
    };
    Ok(DnsLookupResult {
        host: request.host,
        record_type: request.record_type,
        server: request.server,
        records,
        elapsed_ms: elapsed_ms(started),
    })
}

/// `hostname` of a batch resolved, within [`dns_batch::RESOLVE_TIMEOUT_MS`].
pub async fn resolve_batch_item(hostname: String) -> DnsBatchItemResult {
    let started = Instant::now();
    let timeout = Duration::from_millis(dns_batch::RESOLVE_TIMEOUT_MS);
    match resolve(&hostname, timeout).await {
        Ok(addresses) => item_from_addresses(&hostname, addresses, elapsed_ms(started)),
        Err(failure) => {
            let failure = match failure {
                ResolveFailure::TimedOut => DnsBatchFailure::TimedOut,
                ResolveFailure::NoAddress => DnsBatchFailure::NoAddress,
                ResolveFailure::Failed(why) => DnsBatchFailure::Failed(why),
            };
            failed_item(&hostname, failure, elapsed_ms(started))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use heimdall_core::tools::dns_batch::DnsBatchItemStatus;
    use heimdall_core::tools::dns_lookup::{DnsRecordType, DnsServerPreset, validate_request};

    #[tokio::test]
    async fn an_address_resolves_to_itself_in_a_batch() {
        let item = resolve_batch_item("2001:db8::1".to_owned()).await;
        assert_eq!(item.status, DnsBatchItemStatus::Resolved);
        assert_eq!(item.ipv6, ["2001:db8::1"]);
        assert!(item.ipv4.is_empty());
    }

    #[tokio::test]
    async fn a_name_that_does_not_resolve_fails_its_row() {
        let item = resolve_batch_item("nothing.invalid".to_owned()).await;
        assert_eq!(item.status, DnsBatchItemStatus::Failed);
        assert!(item.error.is_some());
    }

    #[tokio::test]
    async fn an_address_query_to_the_system_resolver_keeps_its_family() {
        let request = validate_request(
            "127.0.0.1",
            DnsRecordType::A,
            DnsServerPreset::DefaultSystem,
        )
        .expect("valid");
        let result = lookup(request).await.expect("answered");
        assert_eq!(result.records, ["127.0.0.1"]);
        let request = validate_request(
            "127.0.0.1",
            DnsRecordType::Aaaa,
            DnsServerPreset::DefaultSystem,
        )
        .expect("valid");
        assert!(lookup(request).await.expect("answered").records.is_empty());
    }
}
