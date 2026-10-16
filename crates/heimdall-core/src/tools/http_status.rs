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

//! HTTP status codes reference database and search engine.

/// Category classification of an HTTP status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HttpStatusCategory {
    /// 1xx Informational responses
    Informational,
    /// 2xx Successful responses
    Success,
    /// 3xx Redirection messages
    Redirection,
    /// 4xx Client error responses
    ClientError,
    /// 5xx Server error responses
    ServerError,
}

impl HttpStatusCategory {
    /// Derives the category from the numerical status code.
    #[must_use]
    pub const fn from_code(code: u16) -> Self {
        match code {
            100..=199 => Self::Informational,
            200..=299 => Self::Success,
            300..=399 => Self::Redirection,
            400..=499 => Self::ClientError,
            _ => Self::ServerError,
        }
    }
}

/// An entry in the HTTP status codes reference table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HttpStatusEntry {
    /// Numeric code (e.g. 200, 404).
    pub code: u16,
    /// Canonical reason phrase / title (e.g. "OK", "Not Found").
    pub name: &'static str,
    /// Response category.
    pub category: HttpStatusCategory,
}

/// Complete catalog of standard HTTP status codes.
pub static ALL_STATUS_CODES: &[HttpStatusEntry] = &[
    // 1xx Informational
    HttpStatusEntry {
        code: 100,
        name: "Continue",
        category: HttpStatusCategory::Informational,
    },
    HttpStatusEntry {
        code: 101,
        name: "Switching Protocols",
        category: HttpStatusCategory::Informational,
    },
    HttpStatusEntry {
        code: 102,
        name: "Processing",
        category: HttpStatusCategory::Informational,
    },
    HttpStatusEntry {
        code: 103,
        name: "Early Hints",
        category: HttpStatusCategory::Informational,
    },
    // 2xx Success
    HttpStatusEntry {
        code: 200,
        name: "OK",
        category: HttpStatusCategory::Success,
    },
    HttpStatusEntry {
        code: 201,
        name: "Created",
        category: HttpStatusCategory::Success,
    },
    HttpStatusEntry {
        code: 202,
        name: "Accepted",
        category: HttpStatusCategory::Success,
    },
    HttpStatusEntry {
        code: 203,
        name: "Non-Authoritative Information",
        category: HttpStatusCategory::Success,
    },
    HttpStatusEntry {
        code: 204,
        name: "No Content",
        category: HttpStatusCategory::Success,
    },
    HttpStatusEntry {
        code: 205,
        name: "Reset Content",
        category: HttpStatusCategory::Success,
    },
    HttpStatusEntry {
        code: 206,
        name: "Partial Content",
        category: HttpStatusCategory::Success,
    },
    HttpStatusEntry {
        code: 207,
        name: "Multi-Status",
        category: HttpStatusCategory::Success,
    },
    HttpStatusEntry {
        code: 208,
        name: "Already Reported",
        category: HttpStatusCategory::Success,
    },
    HttpStatusEntry {
        code: 226,
        name: "IM Used",
        category: HttpStatusCategory::Success,
    },
    // 3xx Redirection
    HttpStatusEntry {
        code: 300,
        name: "Multiple Choices",
        category: HttpStatusCategory::Redirection,
    },
    HttpStatusEntry {
        code: 301,
        name: "Moved Permanently",
        category: HttpStatusCategory::Redirection,
    },
    HttpStatusEntry {
        code: 302,
        name: "Found",
        category: HttpStatusCategory::Redirection,
    },
    HttpStatusEntry {
        code: 303,
        name: "See Other",
        category: HttpStatusCategory::Redirection,
    },
    HttpStatusEntry {
        code: 304,
        name: "Not Modified",
        category: HttpStatusCategory::Redirection,
    },
    HttpStatusEntry {
        code: 305,
        name: "Use Proxy",
        category: HttpStatusCategory::Redirection,
    },
    HttpStatusEntry {
        code: 307,
        name: "Temporary Redirect",
        category: HttpStatusCategory::Redirection,
    },
    HttpStatusEntry {
        code: 308,
        name: "Permanent Redirect",
        category: HttpStatusCategory::Redirection,
    },
    // 4xx Client Error
    HttpStatusEntry {
        code: 400,
        name: "Bad Request",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 401,
        name: "Unauthorized",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 402,
        name: "Payment Required",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 403,
        name: "Forbidden",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 404,
        name: "Not Found",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 405,
        name: "Method Not Allowed",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 406,
        name: "Not Acceptable",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 407,
        name: "Proxy Authentication Required",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 408,
        name: "Request Timeout",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 409,
        name: "Conflict",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 410,
        name: "Gone",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 411,
        name: "Length Required",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 412,
        name: "Precondition Failed",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 413,
        name: "Content Too Large",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 414,
        name: "URI Too Long",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 415,
        name: "Unsupported Media Type",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 416,
        name: "Range Not Satisfiable",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 417,
        name: "Expectation Failed",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 418,
        name: "I'm a Teapot",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 421,
        name: "Misdirected Request",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 422,
        name: "Unprocessable Content",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 423,
        name: "Locked",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 424,
        name: "Failed Dependency",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 425,
        name: "Too Early",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 426,
        name: "Upgrade Required",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 428,
        name: "Precondition Required",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 429,
        name: "Too Many Requests",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 431,
        name: "Request Header Fields Too Large",
        category: HttpStatusCategory::ClientError,
    },
    HttpStatusEntry {
        code: 451,
        name: "Unavailable For Legal Reasons",
        category: HttpStatusCategory::ClientError,
    },
    // 5xx Server Error
    HttpStatusEntry {
        code: 500,
        name: "Internal Server Error",
        category: HttpStatusCategory::ServerError,
    },
    HttpStatusEntry {
        code: 501,
        name: "Not Implemented",
        category: HttpStatusCategory::ServerError,
    },
    HttpStatusEntry {
        code: 502,
        name: "Bad Gateway",
        category: HttpStatusCategory::ServerError,
    },
    HttpStatusEntry {
        code: 503,
        name: "Service Unavailable",
        category: HttpStatusCategory::ServerError,
    },
    HttpStatusEntry {
        code: 504,
        name: "Gateway Timeout",
        category: HttpStatusCategory::ServerError,
    },
    HttpStatusEntry {
        code: 505,
        name: "HTTP Version Not Supported",
        category: HttpStatusCategory::ServerError,
    },
    HttpStatusEntry {
        code: 506,
        name: "Variant Also Negotiates",
        category: HttpStatusCategory::ServerError,
    },
    HttpStatusEntry {
        code: 507,
        name: "Insufficient Storage",
        category: HttpStatusCategory::ServerError,
    },
    HttpStatusEntry {
        code: 508,
        name: "Loop Detected",
        category: HttpStatusCategory::ServerError,
    },
    HttpStatusEntry {
        code: 510,
        name: "Not Extended",
        category: HttpStatusCategory::ServerError,
    },
    HttpStatusEntry {
        code: 511,
        name: "Network Authentication Required",
        category: HttpStatusCategory::ServerError,
    },
];

/// Returns all status code entries in the reference catalog.
#[must_use]
pub fn all_entries() -> &'static [HttpStatusEntry] {
    ALL_STATUS_CODES
}

/// Filters the status code catalog by query string.
#[must_use]
pub fn filter_entries(filter: &str) -> Vec<&'static HttpStatusEntry> {
    let trimmed = filter.trim();
    if trimmed.is_empty() {
        return ALL_STATUS_CODES.iter().collect();
    }

    let filter_lower = trimmed.to_lowercase();
    ALL_STATUS_CODES
        .iter()
        .filter(|entry| {
            entry.code.to_string().contains(&filter_lower)
                || entry.name.to_lowercase().contains(&filter_lower)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_entries_count() {
        let entries = all_entries();
        assert!(entries.len() >= 40);
        assert_eq!(entries[0].code, 100);
    }

    #[test]
    fn test_filter_by_code() {
        let matches = filter_entries("404");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "Not Found");
    }

    #[test]
    fn test_filter_by_name() {
        let matches = filter_entries("teapot");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].code, 418);
    }

    #[test]
    fn test_category_ranges() {
        assert_eq!(
            HttpStatusCategory::from_code(100),
            HttpStatusCategory::Informational
        );
        assert_eq!(
            HttpStatusCategory::from_code(200),
            HttpStatusCategory::Success
        );
        assert_eq!(
            HttpStatusCategory::from_code(301),
            HttpStatusCategory::Redirection
        );
        assert_eq!(
            HttpStatusCategory::from_code(404),
            HttpStatusCategory::ClientError
        );
        assert_eq!(
            HttpStatusCategory::from_code(500),
            HttpStatusCategory::ServerError
        );
    }
}
