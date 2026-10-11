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

//! The look for a newer release, as the C# update check, to notify only: GitHub is asked for
//! the latest release of the pinned repository, its tag is compared with the release this
//! build is, and a newer one is offered as a link to its page. Nothing is downloaded and
//! nothing is installed.
//!
//! The release this build is comes from the `HEIMDALL_RELEASE` variable, read when the
//! release build is compiled: its tag, `v2026.100901`, the leading `v` optional. A build
//! compiled without it, as every development build, never looks: it cannot tell an older
//! release from a newer one, and offering every release as newer would be wrong.
//!
//! The release page offered is built from the pinned repository and the tag, once the tag
//! is checked: the page address GitHub's answer names is never followed.
//!
//! The request goes through the proxy the environment names (`ALL_PROXY`, `HTTPS_PROXY`,
//! `HTTP_PROXY`, with `NO_PROXY`), else, on Windows, the proxy server set in the user's
//! Internet settings, its bypass list read for host names and `*` wildcards. A proxy
//! auto-configuration script (PAC) and automatic detection (WPAD) are not read: with only
//! those set, the request goes direct. The server's certificate is checked against the
//! system's certificate store, so a company's own root is trusted as the system trusts it.

use std::io;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::external_url::launchable_url;

/// The repository whose releases are looked at: pinned, never read from the settings.
pub const RELEASES_REPOSITORY: &str = "VBlackJack/Heimdall-rs";

/// GitHub's API, asked for the latest release.
const API_HOST: &str = "api.github.com";

/// GitHub's site, where a release's page is.
const PAGE_HOST: &str = "github.com";

/// The scheme of both: the request is refused over anything else.
const HTTPS: &str = "https://";

/// The answer asked for, as GitHub documents it.
const ACCEPT: &str = "application/vnd.github+json";

/// The product the request names itself as, before the version.
const USER_AGENT_PRODUCT: &str = "Heimdall-rs";

/// The longest the request may take, connection and answer together.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// The most of an answer read: a release's description is a few kilobytes.
const MAX_ANSWER_BYTES: u64 = 1024 * 1024;

/// The wait after start before the first look, so a launch is not slowed by it.
pub const START_DELAY: Duration = Duration::from_secs(30);

/// How often it is asked whether a look is due; a look is due once the interval chosen has
/// passed since the last answer.
pub const TICK: Duration = Duration::from_hours(1);

/// Seconds in an hour, for the interval chosen in hours.
pub const SECONDS_PER_HOUR: u64 = 60 * 60;

/// The variable a release build is compiled with: its release tag.
pub const RELEASE_VARIABLE: &str = "HEIMDALL_RELEASE";

/// The tag of this build's release, when it was compiled as one.
const BUILD_RELEASE: Option<&str> = option_env!("HEIMDALL_RELEASE");

/// What starts a release tag.
const TAG_PREFIX: char = 'v';

/// Between the year and the day's release number.
const TAG_SEPARATOR: char = '.';

/// Digits of the year.
const YEAR_DIGITS: usize = 4;

/// Digits of the month, the day and the release number of the day: `MMDDNN`.
const RELEASE_DIGITS: usize = 6;

/// The field of GitHub's answer read: the release's tag, and nothing else of it.
const TAG_FIELD: &str = "tag_name";

/// The most of a tag that is not ours written to the log.
const LOGGED_TAG_CHARS: usize = 64;

/// HTTP statuses read.
const STATUS_OK: u16 = 200;
const STATUS_MULTIPLE_CHOICES: u16 = 300;
const STATUS_UNAUTHORIZED: u16 = 401;
const STATUS_FORBIDDEN: u16 = 403;
const STATUS_NOT_FOUND: u16 = 404;
const STATUS_TOO_MANY_REQUESTS: u16 = 429;

/// The successful statuses.
const SUCCESS: std::ops::Range<u16> = STATUS_OK..STATUS_MULTIPLE_CHOICES;

/// The headers that say how long to wait: the standard one, then GitHub's own.
const RETRY_AFTER_HEADER: &str = "retry-after";
const RATE_LIMIT_REMAINING_HEADER: &str = "x-ratelimit-remaining";
const RATE_LIMIT_RESET_HEADER: &str = "x-ratelimit-reset";

/// The Windows key of the user's Internet settings, and the values of its proxy.
#[cfg(windows)]
const INTERNET_SETTINGS_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
#[cfg(windows)]
const PROXY_ENABLE_VALUE: &str = "ProxyEnable";
#[cfg(windows)]
const PROXY_SERVER_VALUE: &str = "ProxyServer";
#[cfg(windows)]
const PROXY_OVERRIDE_VALUE: &str = "ProxyOverride";

/// The bypass entry that names every host without a dot, as Windows reads it.
const LOCAL_BYPASS: &str = "<local>";

/// The scheme a proxy server written without one is reached by.
const PROXY_SCHEME: &str = "http://";

/// The schemes of a per-protocol proxy list, the HTTPS one first.
const PROXY_SCHEMES: [&str; 2] = ["https", "http"];

/// A release, as its tag names it: `v2026.100901`, the year, then the month, the day and
/// the day's release number. Ordered by the year, then by the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReleaseTag {
    year: u16,
    release: u32,
}

impl ReleaseTag {
    /// The release `tag` names, when it is `v`, four digits, a dot and six digits, nothing
    /// before or after.
    #[must_use]
    pub fn parse(tag: &str) -> Option<Self> {
        let (year, release) = tag.strip_prefix(TAG_PREFIX)?.split_once(TAG_SEPARATOR)?;
        let digits = |part: &str, count: usize| {
            part.len() == count && part.bytes().all(|b| b.is_ascii_digit())
        };
        if !digits(year, YEAR_DIGITS) || !digits(release, RELEASE_DIGITS) {
            return None;
        }
        Some(Self {
            year: year.parse().ok()?,
            release: release.parse().ok()?,
        })
    }

    /// Its tag, as the release is published under: `v2026.100901`.
    #[must_use]
    pub fn tag(self) -> String {
        format!("{TAG_PREFIX}{self}")
    }

    /// The day it was built, read from its tag as the C# `DeriveBuildDate` reads its
    /// version (`AppVersionProvider.cs:66-99`): `2026-10-09` for `v2026.100901`; `None`
    /// when the month and day are no date.
    #[must_use]
    pub fn date(self) -> Option<String> {
        let month = self.release / MONTH_DIVISOR;
        let day = self.release / DAY_DIVISOR % DAY_MODULUS;
        let days = match month {
            FEBRUARY if is_leap_year(self.year) => FEBRUARY_LEAP_DAYS,
            FEBRUARY => FEBRUARY_DAYS,
            month if SHORT_MONTHS.contains(&month) => SHORT_MONTH_DAYS,
            JANUARY..=DECEMBER => LONG_MONTH_DAYS,
            _ => return None,
        };
        (1..=days)
            .contains(&day)
            .then(|| format!("{:04}-{month:02}-{day:02}", self.year))
    }
}

/// Where the month and the day are in a release number `MMDDNN`.
const MONTH_DIVISOR: u32 = 10_000;
const DAY_DIVISOR: u32 = 100;
const DAY_MODULUS: u32 = 100;

/// The months, as a release number writes them.
const JANUARY: u32 = 1;
const FEBRUARY: u32 = 2;
const DECEMBER: u32 = 12;

/// The months of 30 days: April, June, September and November.
const SHORT_MONTHS: [u32; 4] = [4, 6, 9, 11];

/// Days of the months.
const LONG_MONTH_DAYS: u32 = 31;
const SHORT_MONTH_DAYS: u32 = 30;
const FEBRUARY_DAYS: u32 = 28;
const FEBRUARY_LEAP_DAYS: u32 = 29;

/// The Gregorian calendar's cycles: a leap year every 4, but not every 100, but every 400.
const LEAP_CYCLE: u16 = 4;
const CENTURY: u16 = 100;
const LEAP_CENTURY_CYCLE: u16 = 400;

/// Whether `year` of the Gregorian calendar has a 29 February.
fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(LEAP_CYCLE) && !year.is_multiple_of(CENTURY))
        || year.is_multiple_of(LEAP_CENTURY_CYCLE)
}

/// The version, as the banner shows it: `2026.100901`.
impl std::fmt::Display for ReleaseTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:0year$}{TAG_SEPARATOR}{:0release$}",
            self.year,
            self.release,
            year = YEAR_DIGITS,
            release = RELEASE_DIGITS
        )
    }
}

/// The release a build compiled with `variable` as [`RELEASE_VARIABLE`] is: its tag, with
/// or without the leading `v`.
#[must_use]
pub fn build_release(variable: Option<&str>) -> Option<ReleaseTag> {
    let value = variable?.trim();
    ReleaseTag::parse(value).or_else(|| ReleaseTag::parse(&format!("{TAG_PREFIX}{value}")))
}

/// The release this build is; `None` for a development build.
#[must_use]
pub fn running_release() -> Option<ReleaseTag> {
    build_release(BUILD_RELEASE)
}

/// Where GitHub says the latest release of [`RELEASES_REPOSITORY`].
#[must_use]
pub fn latest_release_url() -> String {
    format!("{HTTPS}{API_HOST}/repos/{RELEASES_REPOSITORY}/releases/latest")
}

/// The page of release `tag`, built from the pinned repository: what "View release" opens.
#[must_use]
pub fn release_page(tag: ReleaseTag) -> Option<String> {
    launchable_url(&format!(
        "{HTTPS}{PAGE_HOST}/{RELEASES_REPOSITORY}/releases/tag/{}",
        tag.tag()
    ))
}

/// What GitHub answered, as far as the check reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    /// The HTTP status.
    pub status: u16,
    /// The `Retry-After` header, when there is one.
    pub retry_after: Option<String>,
    /// The `X-RateLimit-Remaining` header: what is left of the quota.
    pub remaining: Option<String>,
    /// The `X-RateLimit-Reset` header: when the quota comes back, in seconds since 1970.
    pub reset: Option<String>,
    /// The body.
    pub body: String,
}

/// Why a look got no answer, as the C# `UpdateCheckFailure` reads it: by what the user
/// would do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The server could not be reached: no route, no name, no answer.
    NetworkUnreachable,
    /// The connection could not be secured: a proxy that intercepts it, a root missing, a
    /// clock far out.
    SecureChannel,
    /// A proxy would not open the tunnel.
    ProxyRefused,
    /// The quota of this network is spent: looked at again at the next interval.
    RateLimited {
        /// How long the server asked to wait, when it said.
        retry_after: Option<Duration>,
    },
    /// The request was refused for another reason.
    AccessDenied,
    /// The server failed on its side.
    SourceUnavailable,
    /// The answer could not be read.
    MalformedResponse,
    /// No answer in [`REQUEST_TIMEOUT`].
    TimedOut,
}

/// What a look found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The latest release.
    Latest(ReleaseTag),
    /// No release this build can compare with: none published yet, or a latest whose tag is
    /// not of the release scheme. Nothing newer, then.
    NoRelease,
    /// No answer.
    Failed(Failure),
}

impl Outcome {
    /// Whether the look counts as made, and the next waits for the interval: an answer, or
    /// a spent quota, which looking again sooner would only keep spent.
    #[must_use]
    pub fn answered(self) -> bool {
        !matches!(
            self,
            Self::Failed(
                Failure::NetworkUnreachable
                    | Failure::SecureChannel
                    | Failure::ProxyRefused
                    | Failure::AccessDenied
                    | Failure::SourceUnavailable
                    | Failure::MalformedResponse
                    | Failure::TimedOut
            )
        )
    }
}

/// Where the latest release is asked: GitHub, or a stand-in in the tests.
pub trait ReleaseSource {
    /// GitHub's answer to a request of `url`.
    ///
    /// # Errors
    ///
    /// No answer came, and why.
    fn latest(&self, url: &str) -> Result<Answer, Failure>;
}

/// Asks `source` for the latest release and reads its answer, `now` being the time a quota's
/// reset is counted from.
pub fn check(source: &impl ReleaseSource, now: SystemTime) -> Outcome {
    match source.latest(&latest_release_url()) {
        Ok(answer) => classify(&answer, now),
        Err(failure) => Outcome::Failed(failure),
    }
}

/// What `answer` says, `now` being the time a quota's reset is counted from: a release, none
/// (404, as no release is published yet), or why not. One line is logged when it says
/// nothing usable.
#[must_use]
pub fn classify(answer: &Answer, now: SystemTime) -> Outcome {
    if SUCCESS.contains(&answer.status) {
        return latest_of(&answer.body);
    }
    if answer.status == STATUS_NOT_FOUND {
        return Outcome::NoRelease;
    }
    let failure = status_failure(answer, now);
    log::warn!(
        "update check: HTTP {} from GitHub, read as {failure:?}",
        answer.status
    );
    Outcome::Failed(failure)
}

/// The release a successful `body` names.
fn latest_of(body: &str) -> Outcome {
    let tag = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|answer| answer.get(TAG_FIELD)?.as_str().map(str::to_owned));
    let Some(tag) = tag else {
        log::warn!("update check: GitHub's answer holds no release tag");
        return Outcome::Failed(Failure::MalformedResponse);
    };
    if let Some(release) = ReleaseTag::parse(&tag) {
        return Outcome::Latest(release);
    }
    // A tag of another scheme, a test release, is nothing this build can be older than.
    let shown: String = tag.chars().take(LOGGED_TAG_CHARS).collect();
    log::warn!("update check: the latest release tag {shown:?} is not of the vYYYY.MMDDNN scheme");
    Outcome::NoRelease
}

/// The failure a status other than success or 404 means, as the C# classifies it. GitHub
/// answers a spent quota with 403 more often than with 429, and a 403 is otherwise a plain
/// refusal: a quota at 0, or a `Retry-After`, tells the two apart.
fn status_failure(answer: &Answer, now: SystemTime) -> Failure {
    let rate_limited = || Failure::RateLimited {
        retry_after: retry_after(answer, now),
    };
    match answer.status {
        STATUS_TOO_MANY_REQUESTS => rate_limited(),
        STATUS_FORBIDDEN if quota_spent(answer) || answer.retry_after.is_some() => rate_limited(),
        STATUS_FORBIDDEN | STATUS_UNAUTHORIZED => Failure::AccessDenied,
        _ => Failure::SourceUnavailable,
    }
}

/// Whether the answer says the quota is spent.
fn quota_spent(answer: &Answer) -> bool {
    answer
        .remaining
        .as_deref()
        .and_then(|remaining| remaining.trim().parse::<u64>().ok())
        == Some(0)
}

/// How long the server asked to wait: `Retry-After` in seconds, else until the quota's
/// reset; never a wait already over.
fn retry_after(answer: &Answer, now: SystemTime) -> Option<Duration> {
    let seconds = |header: &Option<String>| header.as_deref()?.trim().parse::<u64>().ok();
    if let Some(wait) = seconds(&answer.retry_after) {
        return (wait > 0).then(|| Duration::from_secs(wait));
    }
    let reset = SystemTime::UNIX_EPOCH + Duration::from_secs(seconds(&answer.reset)?);
    reset
        .duration_since(now)
        .ok()
        .filter(|wait| !wait.is_zero())
}

/// Whole minutes to wait for `wait`, at least one: rounded up, so the user is not told to
/// come back before the quota does.
#[must_use]
pub fn minutes_to_wait(wait: Duration) -> u64 {
    wait.as_secs().div_ceil(60).max(1)
}

/// The proxy of the user's Windows Internet settings for `host`: `enabled` as
/// `ProxyEnable`, `server` as `ProxyServer`, one address or one per protocol
/// (`http=a:80;https=b:443`), `bypass` as `ProxyOverride`, entries separated by `;`, `*`
/// matching any run of characters and `<local>` every host without a dot. `None` when no
/// proxy is set, or `host` bypasses it.
#[must_use]
pub fn system_proxy_uri(enabled: bool, server: &str, bypass: &str, host: &str) -> Option<String> {
    if !enabled || bypassed(bypass, host) {
        return None;
    }
    let chosen = if server.contains('=') {
        let entries = || {
            server
                .split(';')
                .filter_map(|entry| entry.split_once('='))
                .map(|(scheme, address)| (scheme.trim(), address.trim()))
        };
        PROXY_SCHEMES.iter().find_map(|wanted| {
            entries()
                .find(|(scheme, _)| scheme.eq_ignore_ascii_case(wanted))
                .map(|(_, address)| address)
        })?
    } else {
        server.trim()
    };
    if chosen.is_empty() {
        return None;
    }
    Some(if chosen.contains("://") {
        chosen.to_owned()
    } else {
        format!("{PROXY_SCHEME}{chosen}")
    })
}

/// Whether `host` is in the bypass list `list`, whatever the case.
fn bypassed(list: &str, host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    list.split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .any(|entry| {
            if entry.eq_ignore_ascii_case(LOCAL_BYPASS) {
                !host.contains('.')
            } else {
                wildcard_matches(&entry.to_ascii_lowercase(), &host)
            }
        })
}

/// Whether `text` is `pattern`, each `*` of it standing for any run of characters.
fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        return rest.is_empty();
    };
    for part in middle {
        let Some(at) = rest.find(part) else {
            return false;
        };
        rest = &rest[at + part.len()..];
    }
    rest.ends_with(last)
}

/// What an I/O error carries: a handshake refused by rustls among others.
type IoCause = dyn std::error::Error + Send + Sync;

/// What a transport `error` means for the user, as the C# reads its exceptions.
#[must_use]
pub fn transport_failure(error: &ureq::Error) -> Failure {
    match error {
        ureq::Error::Timeout(_) => Failure::TimedOut,
        ureq::Error::Io(error) if error.kind() == io::ErrorKind::TimedOut => Failure::TimedOut,
        ureq::Error::Io(error)
            if error
                .get_ref()
                .is_some_and(IoCause::is::<tokio_rustls::rustls::Error>) =>
        {
            Failure::SecureChannel
        }
        ureq::Error::Io(_) | ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => {
            Failure::NetworkUnreachable
        }
        ureq::Error::Rustls(_) | ureq::Error::Tls(_) | ureq::Error::TlsRequired => {
            Failure::SecureChannel
        }
        ureq::Error::ConnectProxyFailed(_) | ureq::Error::InvalidProxyUrl => Failure::ProxyRefused,
        _ => Failure::MalformedResponse,
    }
}

/// GitHub, asked over HTTPS with the system's certificates and proxy.
pub struct GitHubSource {
    agent: ureq::Agent,
}

impl GitHubSource {
    /// The client: reads the system's certificates and proxy, so it is made off the UI
    /// thread, once per look.
    #[must_use]
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .http_status_as_error(false)
            .timeout_global(Some(REQUEST_TIMEOUT))
            .user_agent(format!(
                "{USER_AGENT_PRODUCT}/{}",
                env!("CARGO_PKG_VERSION")
            ))
            .accept(ACCEPT)
            .proxy(proxy())
            .tls_config(tls())
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

impl Default for GitHubSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ReleaseSource for GitHubSource {
    fn latest(&self, url: &str) -> Result<Answer, Failure> {
        let failed = |error: ureq::Error| {
            let failure = transport_failure(&error);
            log::warn!("update check: {error}, read as {failure:?}");
            failure
        };
        let response = self.agent.get(url).call().map_err(failed)?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let status = response.status().as_u16();
        let retry_after = header(RETRY_AFTER_HEADER);
        let remaining = header(RATE_LIMIT_REMAINING_HEADER);
        let reset = header(RATE_LIMIT_RESET_HEADER);
        let body = response
            .into_body()
            .with_config()
            .limit(MAX_ANSWER_BYTES)
            .read_to_string()
            .map_err(failed)?;
        Ok(Answer {
            status,
            retry_after,
            remaining,
            reset,
            body,
        })
    }
}

/// TLS on rustls with ring, chosen here as the rest of the application uses it, trusting the
/// system's certificate store, a company's own roots included.
fn tls() -> ureq::tls::TlsConfig {
    let loaded = rustls_native_certs::load_native_certs();
    for error in &loaded.errors {
        log::debug!("update check: a system certificate could not be read: {error}");
    }
    let roots: Vec<ureq::tls::Certificate<'static>> = loaded
        .certs
        .iter()
        .map(|der| ureq::tls::Certificate::from_der(der.as_ref()).to_owned())
        .collect();
    ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .root_certs(ureq::tls::RootCerts::new_with_certs(&roots))
        .unversioned_rustls_crypto_provider(Arc::new(
            tokio_rustls::rustls::crypto::ring::default_provider(),
        ))
        .build()
}

/// The proxy the environment names, else the user's Windows one.
fn proxy() -> Option<ureq::Proxy> {
    ureq::Proxy::try_from_env().or_else(system_proxy)
}

/// The proxy of the user's Windows Internet settings, for GitHub's API.
#[cfg(windows)]
fn system_proxy() -> Option<ureq::Proxy> {
    use windows_registry::CURRENT_USER;

    let key = CURRENT_USER.open(INTERNET_SETTINGS_KEY).ok()?;
    let enabled = key.get_u32(PROXY_ENABLE_VALUE).is_ok_and(|on| on != 0);
    let server = key.get_string(PROXY_SERVER_VALUE).unwrap_or_default();
    let bypass = key.get_string(PROXY_OVERRIDE_VALUE).unwrap_or_default();
    let uri = system_proxy_uri(enabled, &server, &bypass, API_HOST)?;
    ureq::Proxy::new(&uri)
        .inspect_err(|error| log::warn!("update check: the system proxy is not usable: {error}"))
        .ok()
}

/// No system proxy is read elsewhere than on Windows: the environment's is.
#[cfg(not(windows))]
fn system_proxy() -> Option<ureq::Proxy> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_v_four_digits_a_dot_and_six_digits() {
        let tag = ReleaseTag::parse("v2026.100901").expect("a tag");
        assert_eq!(tag.to_string(), "2026.100901");
        assert_eq!(tag.tag(), "v2026.100901");
        for refused in [
            "",
            "v",
            "2026.100901",
            "V2026.100901",
            "v2026.10091",
            "v2026.1009011",
            "v226.100901",
            "v2026-100901",
            "v2026.100901-rc1",
            " v2026.100901",
            "v2026.+10090",
            "v1.0.0",
            "v\u{FF12}026.100901",
        ] {
            assert_eq!(ReleaseTag::parse(refused), None, "{refused:?}");
        }
    }

    #[test]
    fn releases_are_ordered_by_year_then_by_day_and_number() {
        let tag = |text: &str| ReleaseTag::parse(text).expect(text);
        assert!(tag("v2026.100902") > tag("v2026.100901"), "the day's next");
        assert!(tag("v2026.101001") > tag("v2026.100999"), "the next day");
        assert!(tag("v2026.110101") > tag("v2026.103199"), "the next month");
        assert!(tag("v2027.010101") > tag("v2026.123199"), "the next year");
        assert_eq!(tag("v2026.100901"), tag("v2026.100901"));
    }

    #[test]
    fn a_build_names_its_release_with_or_without_the_v_and_a_development_build_none() {
        let expected = ReleaseTag::parse("v2026.100901");
        assert_eq!(build_release(Some("v2026.100901")), expected);
        assert_eq!(build_release(Some(" 2026.100901 ")), expected);
        assert_eq!(build_release(Some("0.1.0")), None);
        assert_eq!(build_release(Some("")), None);
        assert_eq!(build_release(None), None);
    }

    #[test]
    fn the_release_page_is_built_from_the_pinned_repository_and_the_tag() {
        let tag = ReleaseTag::parse("v2026.100901").expect("a tag");
        assert_eq!(
            release_page(tag).as_deref(),
            Some("https://github.com/VBlackJack/Heimdall-rs/releases/tag/v2026.100901")
        );
        assert_eq!(
            latest_release_url(),
            "https://api.github.com/repos/VBlackJack/Heimdall-rs/releases/latest"
        );
    }

    /// A successful answer naming `tag`, and a page of its own.
    fn released(tag: &str) -> Answer {
        Answer {
            status: STATUS_OK,
            body: format!(
                r#"{{"tag_name":"{tag}","html_url":"https://evil.example/download","name":"x"}}"#
            ),
            ..Answer::default()
        }
    }

    #[test]
    fn only_the_tag_of_a_success_is_read_and_its_page_is_ignored() {
        let now = SystemTime::now();
        let latest = classify(&released("v2026.100901"), now);
        assert_eq!(
            latest,
            Outcome::Latest(ReleaseTag::parse("v2026.100901").expect("a tag"))
        );
        assert_eq!(classify(&released("v1.0.0"), now), Outcome::NoRelease);
        for body in ["", "not json", r#"{"name":"x"}"#, r#"{"tag_name":7}"#] {
            let answer = Answer {
                status: STATUS_OK,
                body: body.to_owned(),
                ..Answer::default()
            };
            assert_eq!(
                classify(&answer, now),
                Outcome::Failed(Failure::MalformedResponse),
                "{body}"
            );
        }
    }

    #[test]
    fn statuses_are_read_as_the_csharp_reads_them_and_404_is_no_release() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let answer = |status: u16, retry_after: Option<&str>, remaining: Option<&str>| Answer {
            status,
            retry_after: retry_after.map(str::to_owned),
            remaining: remaining.map(str::to_owned),
            reset: Some("1000600".to_owned()),
            body: String::new(),
        };
        let in_ten_minutes = Failure::RateLimited {
            retry_after: Some(Duration::from_mins(10)),
        };
        let cases = [
            (answer(404, None, None), Outcome::NoRelease),
            (answer(429, None, None), Outcome::Failed(in_ten_minutes)),
            (
                answer(403, None, Some("0")),
                Outcome::Failed(in_ten_minutes),
            ),
            (
                answer(403, Some("90"), Some("12")),
                Outcome::Failed(Failure::RateLimited {
                    retry_after: Some(Duration::from_secs(90)),
                }),
            ),
            (
                answer(403, None, Some("12")),
                Outcome::Failed(Failure::AccessDenied),
            ),
            (
                answer(401, None, None),
                Outcome::Failed(Failure::AccessDenied),
            ),
            (
                answer(500, None, None),
                Outcome::Failed(Failure::SourceUnavailable),
            ),
            (
                answer(451, None, None),
                Outcome::Failed(Failure::SourceUnavailable),
            ),
        ];
        for (answer, expected) in cases {
            assert_eq!(classify(&answer, now), expected, "{answer:?}");
        }
        // A reset already past is no wait.
        let past = Answer {
            status: STATUS_TOO_MANY_REQUESTS,
            reset: Some("10".to_owned()),
            ..Answer::default()
        };
        assert_eq!(
            classify(&past, now),
            Outcome::Failed(Failure::RateLimited { retry_after: None })
        );
    }

    #[test]
    fn an_answer_or_a_spent_quota_counts_as_a_look_any_other_failure_does_not() {
        assert!(Outcome::NoRelease.answered());
        assert!(Outcome::Latest(ReleaseTag::parse("v2026.100901").expect("a tag")).answered());
        assert!(Outcome::Failed(Failure::RateLimited { retry_after: None }).answered());
        for failure in [
            Failure::NetworkUnreachable,
            Failure::SecureChannel,
            Failure::ProxyRefused,
            Failure::AccessDenied,
            Failure::SourceUnavailable,
            Failure::MalformedResponse,
            Failure::TimedOut,
        ] {
            assert!(!Outcome::Failed(failure).answered(), "{failure:?}");
        }
    }

    #[test]
    fn a_wait_is_said_in_whole_minutes_rounded_up() {
        assert_eq!(minutes_to_wait(Duration::from_secs(1)), 1);
        assert_eq!(minutes_to_wait(Duration::from_mins(1)), 1);
        assert_eq!(minutes_to_wait(Duration::from_secs(61)), 2);
        assert_eq!(minutes_to_wait(Duration::from_secs(400)), 7);
    }

    /// A source that answers as told, and remembers what it was asked.
    struct Scripted(Result<Answer, Failure>, std::cell::RefCell<Vec<String>>);

    impl ReleaseSource for Scripted {
        fn latest(&self, url: &str) -> Result<Answer, Failure> {
            self.1.borrow_mut().push(url.to_owned());
            self.0.clone()
        }
    }

    #[test]
    fn a_check_asks_the_pinned_repository_and_passes_a_transport_failure_on() {
        let source = Scripted(Ok(released("v2026.100901")), std::cell::RefCell::default());
        assert!(matches!(
            check(&source, SystemTime::now()),
            Outcome::Latest(_)
        ));
        assert_eq!(*source.1.borrow(), [latest_release_url()]);
        let source = Scripted(Err(Failure::TimedOut), std::cell::RefCell::default());
        assert_eq!(
            check(&source, SystemTime::now()),
            Outcome::Failed(Failure::TimedOut)
        );
    }

    #[test]
    fn transport_errors_are_read_by_what_the_user_would_do() {
        let tls = io::Error::new(
            io::ErrorKind::InvalidData,
            tokio_rustls::rustls::Error::General("handshake".to_owned()),
        );
        let cases = [
            (ureq::Error::HostNotFound, Failure::NetworkUnreachable),
            (ureq::Error::ConnectionFailed, Failure::NetworkUnreachable),
            (
                ureq::Error::Io(io::Error::from(io::ErrorKind::ConnectionRefused)),
                Failure::NetworkUnreachable,
            ),
            (
                ureq::Error::Io(io::Error::from(io::ErrorKind::TimedOut)),
                Failure::TimedOut,
            ),
            (ureq::Error::Io(tls), Failure::SecureChannel),
            (ureq::Error::Tls("no roots"), Failure::SecureChannel),
            (
                ureq::Error::ConnectProxyFailed("407".to_owned()),
                Failure::ProxyRefused,
            ),
            (ureq::Error::BodyExceedsLimit(1), Failure::MalformedResponse),
        ];
        for (error, expected) in cases {
            assert_eq!(transport_failure(&error), expected, "{error}");
        }
    }

    /// The update check names its TLS provider, ring, rather than relying on a process-wide
    /// default that nothing installs: the handshake runs, and a certificate no system trusts
    /// is refused as a secure channel failure, not a panic.
    #[test]
    fn the_update_check_handshakes_on_ring_without_a_process_default() {
        use std::io::Read as _;
        use std::net::TcpListener;
        use tokio_rustls::rustls::crypto::CryptoProvider;
        use tokio_rustls::rustls::crypto::ring::default_provider;
        use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
        use tokio_rustls::rustls::{ServerConfig, ServerConnection, StreamOwned};

        assert!(
            CryptoProvider::get_default().is_none(),
            "nothing installs a process-wide provider"
        );
        let issued =
            rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("cert");
        let certificate = CertificateDer::from(issued.cert.der().to_vec());
        let key =
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(issued.signing_key.serialize_der()));
        let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_no_client_auth()
            .with_single_cert(vec![certificate], key)
            .expect("server config");
        let config = Arc::new(config);
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let connection = ServerConnection::new(config).expect("connection");
            let mut tls = StreamOwned::new(connection, stream);
            // Drives the handshake until the client refuses the certificate.
            let mut byte = [0_u8; 1];
            let _ = tls.read(&mut byte);
        });

        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .proxy(None)
                .timeout_global(Some(REQUEST_TIMEOUT))
                .tls_config(tls())
                .build(),
        );
        let error = agent
            .get(format!("https://127.0.0.1:{port}/"))
            .call()
            .expect_err("a certificate no system trusts");
        assert_eq!(transport_failure(&error), Failure::SecureChannel, "{error}");
        server.join().expect("server");
    }

    #[test]
    fn the_windows_proxy_is_read_from_its_settings_and_its_bypass_list() {
        let host = "api.github.com";
        assert_eq!(
            system_proxy_uri(true, "proxy.corp:8080", "", host).as_deref(),
            Some("http://proxy.corp:8080")
        );
        assert_eq!(system_proxy_uri(false, "proxy.corp:8080", "", host), None);
        assert_eq!(system_proxy_uri(true, "", "", host), None);
        assert_eq!(
            system_proxy_uri(true, "http=web:80;https=secure:443;ftp=f:21", "", host).as_deref(),
            Some("http://secure:443"),
            "the HTTPS one first"
        );
        assert_eq!(
            system_proxy_uri(true, "HTTP=web:80", "", host).as_deref(),
            Some("http://web:80")
        );
        assert_eq!(system_proxy_uri(true, "socks=s:1080", "", host), None);
        assert_eq!(
            system_proxy_uri(true, "http://proxy.corp:3128", "", host).as_deref(),
            Some("http://proxy.corp:3128")
        );
        for bypass in [
            "*.github.com",
            "<local>; API.GitHub.com",
            "*github*",
            "*",
            "api.*.com",
        ] {
            assert_eq!(
                system_proxy_uri(true, "proxy:8080", bypass, host),
                None,
                "{bypass}"
            );
        }
        for kept in [
            "<local>",
            "github.com",
            "*.corp;10.*",
            "api.github.co",
            "x*api.github.com",
        ] {
            assert!(
                system_proxy_uri(true, "proxy:8080", kept, host).is_some(),
                "{kept}"
            );
        }
    }
}
