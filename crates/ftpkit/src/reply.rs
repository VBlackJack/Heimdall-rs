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

//! Server replies (RFC 959 section 4.2): a three-digit code, and a line, or several lines
//! between `123-` and `123 `.

use tokio::io::{AsyncBufRead, AsyncBufReadExt as _};

/// Longest reply line read, in bytes: beyond, the server is not speaking FTP.
pub const MAX_LINE: usize = 8 * 1024;

/// Most lines one reply may hold.
pub const MAX_LINES: usize = 1024;

/// A server reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// The code, 100 to 599.
    pub code: u16,
    /// The text of each line, code and separator removed, as the server sent it.
    pub lines: Vec<String>,
}

impl Reply {
    /// The first digit: 1 preliminary, 2 done, 3 more needed, 4 failed for now, 5 failed.
    #[must_use]
    pub fn class(&self) -> u16 {
        self.code / 100
    }

    /// The text, lines joined by a space.
    #[must_use]
    pub fn text(&self) -> String {
        self.lines.join(" ")
    }
}

/// Why no reply could be read.
#[derive(Debug, thiserror::Error)]
pub enum ReplyError {
    /// The connection failed or closed.
    #[error("the server connection ended: {0}")]
    Io(#[from] std::io::Error),
    /// Not an FTP reply.
    #[error("the server sent something that is not an FTP reply")]
    Malformed,
}

/// Reads one reply from `reader`.
///
/// # Errors
///
/// [`ReplyError`]: the connection ended, or a line is not an FTP reply or is too long.
pub async fn read_reply(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Reply, ReplyError> {
    let first = read_line(reader).await?;
    let (code, separator, text) = split(&first).ok_or(ReplyError::Malformed)?;
    let mut lines = vec![text.to_owned()];
    if separator == b' ' {
        return Ok(Reply { code, lines });
    }
    // Multi-line: until a line starting with the same code and a space.
    loop {
        if lines.len() >= MAX_LINES {
            return Err(ReplyError::Malformed);
        }
        let line = read_line(reader).await?;
        match split(&line) {
            Some((last, b' ', text)) if last == code => {
                lines.push(text.to_owned());
                return Ok(Reply { code, lines });
            }
            // Inner lines are free text; a leading space is only indentation.
            _ => lines.push(line.strip_prefix(' ').unwrap_or(&line).to_owned()),
        }
    }
}

/// `123 text` or `123-text` as its parts.
fn split(line: &str) -> Option<(u16, u8, &str)> {
    let bytes = line.as_bytes();
    if bytes.len() < 4 || !bytes[..3].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let separator = bytes[3];
    if separator != b' ' && separator != b'-' {
        return None;
    }
    let code: u16 = line[..3].parse().ok()?;
    (100..600)
        .contains(&code)
        .then(|| (code, separator, &line[4..]))
}

/// One line, its CR LF (or lone LF) removed; text not in UTF-8 is replaced, not refused.
async fn read_line(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<String, ReplyError> {
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Err(ReplyError::Io(std::io::ErrorKind::UnexpectedEof.into()));
        }
        let (taken, done) = match available.iter().position(|byte| *byte == b'\n') {
            Some(end) => (end + 1, true),
            None => (available.len(), false),
        };
        bytes.extend_from_slice(&available[..taken]);
        reader.consume(taken);
        if bytes.len() > MAX_LINE {
            return Err(ReplyError::Malformed);
        }
        if done {
            break;
        }
    }
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::{MAX_LINE, Reply, ReplyError, read_reply};

    async fn parse(text: &str) -> Result<Reply, ReplyError> {
        read_reply(&mut text.as_bytes()).await
    }

    #[tokio::test]
    async fn a_single_line_reply() {
        let reply = parse("220 Service ready\r\n").await.expect("reply");
        assert_eq!((reply.code, reply.class()), (220, 2));
        assert_eq!(reply.lines, ["Service ready"]);
    }

    #[tokio::test]
    async fn a_multi_line_reply_runs_to_its_code_and_a_space() {
        let reply =
            parse("211-Features:\r\n MLSD\r\n EPSV\r\n211-not the end\r\n211 End\r\n220 next\r\n")
                .await
                .expect("reply");
        assert_eq!(reply.code, 211);
        assert_eq!(
            reply.lines,
            ["Features:", "MLSD", "EPSV", "211-not the end", "End"]
        );
    }

    #[tokio::test]
    async fn a_lone_line_feed_ends_a_line_too() {
        assert_eq!(parse("230 in\n").await.expect("reply").lines, ["in"]);
    }

    #[tokio::test]
    async fn anything_else_is_malformed() {
        for text in [
            "hello\r\n",
            "22 short\r\n",
            "220x\r\n",
            "999 no\r\n",
            "099 no\r\n",
        ] {
            assert!(
                matches!(parse(text).await, Err(ReplyError::Malformed)),
                "{text:?}"
            );
        }
        let long = format!("220 {}\r\n", "x".repeat(MAX_LINE));
        assert!(matches!(parse(&long).await, Err(ReplyError::Malformed)));
        assert!(
            matches!(parse("220-open\r\n").await, Err(ReplyError::Io(_))),
            "cut short"
        );
    }
}
