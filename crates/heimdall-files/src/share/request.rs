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

//! The request of the share server: one request line and its headers, read by hand under
//! strict limits. Only what the server acts on is kept; anything it does not understand is
//! refused rather than guessed at (no folded headers, no body, no duplicated credentials).

/// Longest request line accepted, in bytes: a longer target is refused with 414.
pub const MAX_REQUEST_LINE_BYTES: usize = 4096;

/// Longest request line and headers together, in bytes: more is refused with 431.
pub const MAX_HEAD_BYTES: usize = 16 * 1024;

/// Most header lines accepted: more is refused with 431.
pub const MAX_HEADERS: usize = 32;

/// The end of a request's head.
pub(super) const HEAD_END: &[u8] = b"\r\n\r\n";

/// The method of a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// `GET`.
    Get,
    /// `HEAD`.
    Head,
    /// Any other method, as it was written: refused with 405.
    Other(String),
}

impl Method {
    /// The method as it is logged.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Other(name) => name,
        }
    }
}

/// What the server reads from a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The method.
    pub method: Method,
    /// The request target as written: a path and perhaps a query.
    pub target: String,
    /// The value of the `Authorization` header, when there is one.
    pub authorization: Option<String>,
}

/// Why a request is refused before it is looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestError {
    /// 400: the request is not well formed, or carries a body.
    Malformed,
    /// 414: the request line is too long.
    LineTooLong,
    /// 431: the head, or the number of headers, is too large.
    HeadTooLarge,
    /// 505: the version is not HTTP/1.0 or HTTP/1.1.
    Version,
}

impl RequestError {
    /// The status code that answers it.
    #[must_use]
    pub fn status(self) -> u16 {
        match self {
            Self::Malformed => 400,
            Self::LineTooLong => 414,
            Self::HeadTooLarge => 431,
            Self::Version => 505,
        }
    }
}

/// Whether `byte` may be part of a token (a method or a header name), RFC 9110 5.6.2.
fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// Whether `text` is a non-empty token.
fn is_token(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(is_token_byte)
}

/// Reads a request from its head: the bytes before the blank line that ends it.
///
/// # Errors
///
/// The head is too long, is not a well-formed HTTP/1.x request, or announces a body.
pub fn parse_head(head: &[u8]) -> Result<Request, RequestError> {
    if head.len() > MAX_HEAD_BYTES {
        return Err(RequestError::HeadTooLarge);
    }
    let text = std::str::from_utf8(head).map_err(|_| RequestError::Malformed)?;
    let mut lines = text.split("\r\n");
    let line = lines.next().unwrap_or_default();
    if line.len() > MAX_REQUEST_LINE_BYTES {
        return Err(RequestError::LineTooLong);
    }
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(RequestError::Malformed);
    };
    if !is_token(method) || target.is_empty() || target.bytes().any(|b| b.is_ascii_control()) {
        return Err(RequestError::Malformed);
    }
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(if version.starts_with("HTTP/") {
            RequestError::Version
        } else {
            RequestError::Malformed
        });
    }
    let mut authorization = None;
    let mut count = 0_usize;
    for header in lines {
        count += 1;
        if count > MAX_HEADERS {
            return Err(RequestError::HeadTooLarge);
        }
        // A folded line continues the one before, which the server does not accept.
        if header.starts_with([' ', '\t']) {
            return Err(RequestError::Malformed);
        }
        let Some((name, value)) = header.split_once(':') else {
            return Err(RequestError::Malformed);
        };
        if !is_token(name) {
            return Err(RequestError::Malformed);
        }
        let value = value.trim_matches([' ', '\t']);
        if value.bytes().any(|b| b.is_ascii_control() && b != b'\t') {
            return Err(RequestError::Malformed);
        }
        if name.eq_ignore_ascii_case("authorization") {
            if authorization.is_some() {
                return Err(RequestError::Malformed);
            }
            authorization = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("transfer-encoding")
            || (name.eq_ignore_ascii_case("content-length") && value != "0")
        {
            // A read-only server has no use for a body, and ignoring one is how requests
            // get smuggled.
            return Err(RequestError::Malformed);
        }
    }
    Ok(Request {
        method: match method {
            "GET" => Method::Get,
            "HEAD" => Method::Head,
            other => Method::Other(other.to_owned()),
        },
        target: target.to_owned(),
        authorization,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(text: &str) -> Result<Request, RequestError> {
        parse_head(text.as_bytes())
    }

    #[test]
    fn a_get_with_a_bearer_header_is_read() {
        let request =
            head("GET /a/b?token=x HTTP/1.1\r\nHost: h\r\nAuthorization: Bearer abc").unwrap();
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.target, "/a/b?token=x");
        assert_eq!(request.authorization.as_deref(), Some("Bearer abc"));
    }

    #[test]
    fn other_methods_are_kept_for_the_405() {
        let request = head("DELETE / HTTP/1.1").unwrap();
        assert_eq!(request.method, Method::Other("DELETE".to_owned()));
        assert_eq!(request.method.name(), "DELETE");
    }

    #[test]
    fn malformed_requests_are_refused() {
        for text in [
            "GET /",
            "GET / HTTP/1.1 extra",
            "GET  / HTTP/1.1",
            "GET / FTP/1.1",
            "G(T / HTTP/1.1",
            "GET / HTTP/1.1\r\nNoColon",
            "GET / HTTP/1.1\r\n folded: x",
            "GET / HTTP/1.1\r\nBad Name: x",
            "GET /\u{7f} HTTP/1.1",
            "GET / HTTP/1.1\r\nAuthorization: a\r\nAuthorization: b",
        ] {
            assert_eq!(head(text), Err(RequestError::Malformed), "{text:?}");
        }
        assert_eq!(head("GET / HTTP/2.0"), Err(RequestError::Version));
    }

    #[test]
    fn a_body_is_refused() {
        assert_eq!(
            head("GET / HTTP/1.1\r\nContent-Length: 5"),
            Err(RequestError::Malformed)
        );
        assert_eq!(
            head("GET / HTTP/1.1\r\nTransfer-Encoding: chunked"),
            Err(RequestError::Malformed)
        );
        assert!(head("GET / HTTP/1.1\r\nContent-Length: 0").is_ok());
    }

    #[test]
    fn the_limits_are_enforced() {
        let long = format!("GET /{} HTTP/1.1", "a".repeat(MAX_REQUEST_LINE_BYTES));
        assert_eq!(head(&long), Err(RequestError::LineTooLong));
        let many = "\r\nX-n: v".repeat(MAX_HEADERS + 1);
        assert_eq!(
            head(&format!("GET / HTTP/1.1{many}")),
            Err(RequestError::HeadTooLarge)
        );
        let big = format!("GET / HTTP/1.1\r\nX: {}", "v".repeat(MAX_HEAD_BYTES));
        assert_eq!(head(&big), Err(RequestError::HeadTooLarge));
    }
}
