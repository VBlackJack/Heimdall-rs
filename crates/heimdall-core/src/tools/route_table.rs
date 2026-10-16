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

//! The active IPv4 routing table, as the C# `RouteTableService`.
//!
//! The C# reads `route print` and finds its sections by their English and French titles,
//! which miss on any other language. The same table comes here as data: Windows
//! `PowerShell`'s `Get-NetRoute` and `Get-NetIPAddress` as JSON, `ip -j route` on Linux.
//! The columns are `route print`'s: destination, mask, gateway (on-link when there is none),
//! the interface's address, and the metric (the route's plus the interface's, as
//! `route print` adds them).

use std::net::Ipv4Addr;

use serde::Deserialize;

use super::system_listing::{self, ListingError, one_or_many, text_or_null};

/// Lists the active IPv4 routes and the interfaces' IPv4 addresses as JSON. A table with no
/// row is not a failure; any other error is. No double quote: see
/// [`system_listing::run_powershell`].
#[cfg(windows)]
const LISTING_SCRIPT: &str = "$routeErrors = $null; $addressErrors = $null; \
     $routes = @(Get-NetRoute -AddressFamily IPv4 -PolicyStore ActiveStore \
     -ErrorAction SilentlyContinue -ErrorVariable routeErrors | \
     Select-Object DestinationPrefix,NextHop,InterfaceIndex,InterfaceAlias,RouteMetric,InterfaceMetric); \
     $addresses = @(Get-NetIPAddress -AddressFamily IPv4 -PolicyStore ActiveStore \
     -ErrorAction SilentlyContinue -ErrorVariable addressErrors | \
     Select-Object InterfaceIndex,IPAddress); \
     foreach ($failure in @($routeErrors) + @($addressErrors)) { \
     if ($failure.CategoryInfo.Category -ne 'ObjectNotFound') { throw $failure } }; \
     ConvertTo-Json -Compress -Depth 3 -InputObject @{ routes = $routes; addresses = $addresses }";

/// The next hop of a route with no gateway, in `Get-NetRoute`.
const NO_NEXT_HOP: &str = "0.0.0.0";

/// The destination of the default route in `ip -j route`.
const DEFAULT_DESTINATION: &str = "default";

/// The prefix length of a route to one host, which `ip` writes without one.
const HOST_PREFIX: u8 = 32;

/// A route's next hop.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gateway {
    /// No gateway: the destination is on the interface's link (`route print`'s `On-link`),
    /// which the view says in the user's language.
    OnLink,
    /// The gateway's address.
    Address(String),
}

/// A single route table entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteEntry {
    /// Destination network address.
    pub destination: Ipv4Addr,
    /// Subnet mask, dotted.
    pub mask: Ipv4Addr,
    /// Next hop.
    pub gateway: Gateway,
    /// The interface: its IPv4 address as `route print` says it, its name when it has none.
    pub interface: String,
    /// Route metric, the interface's included on Windows as `route print` says it.
    pub metric: u32,
}

/// `prefix` (`192.168.1.0/24`, a lone address being a /32) as its network and mask.
fn destination_and_mask(prefix: &str) -> Option<(Ipv4Addr, Ipv4Addr)> {
    let (address, length) = match prefix.split_once('/') {
        Some((address, length)) => (address, length.parse().ok()?),
        None => (prefix, HOST_PREFIX),
    };
    Some((address.parse().ok()?, system_listing::ipv4_mask(length)?))
}

/// `route print`'s order: destination, then mask, then metric.
fn sort(routes: &mut [RouteEntry]) {
    routes
        .sort_by(|a, b| (a.destination, a.mask, a.metric).cmp(&(b.destination, b.mask, b.metric)));
}

/// A row of `Get-NetRoute`.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsRoute {
    #[serde(default, deserialize_with = "text_or_null")]
    destination_prefix: String,
    #[serde(default, deserialize_with = "text_or_null")]
    next_hop: String,
    #[serde(default)]
    interface_index: u32,
    #[serde(default, deserialize_with = "text_or_null")]
    interface_alias: String,
    #[serde(default)]
    route_metric: Option<u32>,
    #[serde(default)]
    interface_metric: Option<u32>,
}

/// A row of `Get-NetIPAddress`.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsAddress {
    #[serde(default)]
    interface_index: u32,
    #[serde(rename = "IPAddress", default, deserialize_with = "text_or_null")]
    ip_address: String,
}

/// What the Windows listing script writes.
#[derive(Deserialize)]
struct WindowsListing {
    #[serde(default, deserialize_with = "one_or_many")]
    routes: Vec<WindowsRoute>,
    #[serde(default, deserialize_with = "one_or_many")]
    addresses: Vec<WindowsAddress>,
}

/// The routes of the JSON the Windows listing script writes, in `route print`'s order.
///
/// # Errors
///
/// The JSON is not the listing's.
pub fn parse_windows_listing(json: &str) -> Result<Vec<RouteEntry>, ListingError> {
    let listing: WindowsListing = system_listing::parse_json(json)?;
    let mut routes: Vec<RouteEntry> = listing
        .routes
        .into_iter()
        .filter_map(|route| {
            let (destination, mask) = destination_and_mask(&route.destination_prefix)?;
            let interface = listing
                .addresses
                .iter()
                .find(|address| address.interface_index == route.interface_index)
                .map_or(route.interface_alias, |address| address.ip_address.clone());
            Some(RouteEntry {
                destination,
                mask,
                gateway: if route.next_hop.is_empty() || route.next_hop == NO_NEXT_HOP {
                    Gateway::OnLink
                } else {
                    Gateway::Address(route.next_hop)
                },
                interface,
                metric: route
                    .route_metric
                    .unwrap_or_default()
                    .saturating_add(route.interface_metric.unwrap_or_default()),
            })
        })
        .collect();
    sort(&mut routes);
    Ok(routes)
}

/// A route `ip -j route` lists.
#[derive(Deserialize)]
struct IpRoute {
    #[serde(default)]
    dst: String,
    #[serde(default)]
    gateway: Option<String>,
    #[serde(default)]
    dev: String,
    #[serde(default)]
    prefsrc: Option<String>,
    #[serde(default)]
    metric: Option<u32>,
}

/// The routes of `ip -j -4 route show table main`, in `route print`'s order: the default
/// route as 0.0.0.0/0, the interface as its source address when the route names one, else
/// its name.
///
/// # Errors
///
/// The JSON is not what `ip` writes.
pub fn parse_ip_route_json(json: &str) -> Result<Vec<RouteEntry>, ListingError> {
    let listed: Vec<IpRoute> = system_listing::parse_json(json)?;
    let mut routes: Vec<RouteEntry> = listed
        .into_iter()
        .filter_map(|route| {
            let (destination, mask) = if route.dst == DEFAULT_DESTINATION {
                (Ipv4Addr::UNSPECIFIED, Ipv4Addr::UNSPECIFIED)
            } else {
                destination_and_mask(&route.dst)?
            };
            Some(RouteEntry {
                destination,
                mask,
                gateway: route.gateway.map_or(Gateway::OnLink, Gateway::Address),
                interface: route.prefsrc.unwrap_or(route.dev),
                metric: route.metric.unwrap_or_default(),
            })
        })
        .collect();
    sort(&mut routes);
    Ok(routes)
}

/// Lists the system's active IPv4 routes.
///
/// # Errors
///
/// The system has no way to list them, or the listing could not be started, did not end in
/// time, failed, or wrote something else than asked.
pub fn run_route_command() -> Result<Vec<RouteEntry>, ListingError> {
    #[cfg(windows)]
    {
        let json = system_listing::run_powershell(LISTING_SCRIPT, system_listing::LIST_TIME_LIMIT)?;
        parse_windows_listing(&json)
    }
    #[cfg(target_os = "linux")]
    {
        let ip = system_listing::find_program(&system_listing::IP_CANDIDATES)
            .ok_or(ListingError::NotAvailable)?;
        let json = system_listing::run_linux(
            &ip,
            &["-j", "-4", "route", "show", "table", "main"],
            system_listing::LIST_TIME_LIMIT,
        )?;
        parse_ip_route_json(&json)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(ListingError::NotAvailable)
    }
}

/// `entries` as CSV under `headers` (destination, mask, gateway, interface, metric), an
/// on-link gateway as `on_link` words it: every cell quoted and a formula defused.
#[must_use]
pub fn build_route_csv(entries: &[RouteEntry], headers: &[String], on_link: &str) -> String {
    system_listing::csv_document(
        headers,
        entries.iter().map(|route| {
            vec![
                route.destination.to_string(),
                route.mask.to_string(),
                match &route.gateway {
                    Gateway::OnLink => on_link.to_owned(),
                    Gateway::Address(address) => address.clone(),
                },
                route.interface.clone(),
                route.metric.to_string(),
            ]
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As the Windows listing script writes it: the interfaces' addresses beside the
    /// routes, an interface without an IPv4 address named instead.
    const WINDOWS_FIXTURE: &str = r#"{"routes":[
        {"DestinationPrefix":"255.255.255.255/32","NextHop":"0.0.0.0","InterfaceIndex":1,
         "InterfaceAlias":"Loopback Pseudo-Interface 1","RouteMetric":256,"InterfaceMetric":75},
        {"DestinationPrefix":"0.0.0.0/0","NextHop":"192.168.1.254","InterfaceIndex":42,
         "InterfaceAlias":"Wi-Fi 4","RouteMetric":0,"InterfaceMetric":25},
        {"DestinationPrefix":"192.168.1.0/24","NextHop":"0.0.0.0","InterfaceIndex":42,
         "InterfaceAlias":"Wi-Fi 4","RouteMetric":256,"InterfaceMetric":25},
        {"DestinationPrefix":"224.0.0.0/4","NextHop":"0.0.0.0","InterfaceIndex":28,
         "InterfaceAlias":"Connexion réseau Bluetooth 4","RouteMetric":256,"InterfaceMetric":null}],
      "addresses":[{"InterfaceIndex":1,"IPAddress":"127.0.0.1"},{"InterfaceIndex":42,"IPAddress":"192.168.1.66"}]}"#;

    fn row(route: &RouteEntry) -> (String, String, Gateway, &str, u32) {
        (
            route.destination.to_string(),
            route.mask.to_string(),
            route.gateway.clone(),
            route.interface.as_str(),
            route.metric,
        )
    }

    #[test]
    fn windows_routes_read_as_route_print_shows_them_whatever_the_language() {
        let routes = parse_windows_listing(WINDOWS_FIXTURE).expect("parses");
        let rows: Vec<_> = routes.iter().map(row).collect();
        assert_eq!(
            rows,
            [
                (
                    "0.0.0.0".to_owned(),
                    "0.0.0.0".to_owned(),
                    Gateway::Address("192.168.1.254".to_owned()),
                    "192.168.1.66",
                    25
                ),
                (
                    "192.168.1.0".to_owned(),
                    "255.255.255.0".to_owned(),
                    Gateway::OnLink,
                    "192.168.1.66",
                    281
                ),
                (
                    "224.0.0.0".to_owned(),
                    "240.0.0.0".to_owned(),
                    Gateway::OnLink,
                    "Connexion réseau Bluetooth 4",
                    256
                ),
                (
                    "255.255.255.255".to_owned(),
                    "255.255.255.255".to_owned(),
                    Gateway::OnLink,
                    "127.0.0.1",
                    331
                ),
            ]
        );
    }

    #[test]
    fn an_empty_windows_table_is_no_route_and_garbage_is_a_failure() {
        assert_eq!(
            parse_windows_listing(r#"{"routes":null,"addresses":null}"#),
            Ok(vec![])
        );
        assert!(matches!(
            parse_windows_listing("Get-NetRoute : Accès refusé"),
            Err(ListingError::Unreadable(_))
        ));
    }

    #[test]
    fn linux_routes_read_from_ip_json() {
        let json = r#"[{"dst":"default","gateway":"172.30.80.1","dev":"eth0","protocol":"kernel","flags":[]},
            {"dst":"172.30.80.0/20","dev":"eth0","protocol":"kernel","scope":"link","prefsrc":"172.30.86.176","flags":[]},
            {"dst":"10.8.0.1","dev":"tun0","metric":50,"flags":[]}]"#;
        let routes = parse_ip_route_json(json).expect("parses");
        let rows: Vec<_> = routes.iter().map(row).collect();
        assert_eq!(
            rows,
            [
                (
                    "0.0.0.0".to_owned(),
                    "0.0.0.0".to_owned(),
                    Gateway::Address("172.30.80.1".to_owned()),
                    "eth0",
                    0
                ),
                (
                    "10.8.0.1".to_owned(),
                    "255.255.255.255".to_owned(),
                    Gateway::OnLink,
                    "tun0",
                    50
                ),
                (
                    "172.30.80.0".to_owned(),
                    "255.255.240.0".to_owned(),
                    Gateway::OnLink,
                    "172.30.86.176",
                    0
                ),
            ]
        );
    }

    #[test]
    fn the_csv_quotes_every_cell_and_says_on_link_in_the_given_words() {
        let routes = parse_windows_listing(WINDOWS_FIXTURE).expect("parses");
        let headers: Vec<String> = ["D", "M", "G", "I", "Metric"].map(str::to_owned).to_vec();
        let csv = build_route_csv(&routes[..2], &headers, "Sur le lien");
        assert_eq!(
            csv,
            "\"D\",\"M\",\"G\",\"I\",\"Metric\"\n\
             \"0.0.0.0\",\"0.0.0.0\",\"192.168.1.254\",\"192.168.1.66\",\"25\"\n\
             \"192.168.1.0\",\"255.255.255.0\",\"Sur le lien\",\"192.168.1.66\",\"281\"\n"
        );
    }
}
