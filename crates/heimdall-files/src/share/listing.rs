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

//! A shared folder's listing, as the C# server's HTML table: names escaped, a link to the
//! parent only below the root, and the access token carried by every link (a browser does
//! not send the bearer header when a link is followed).

use std::fmt::Write as _;
use std::path::Path;

use super::paths::{is_link, segment_allowed};

/// Most servable entries a listing shows (links are capped apart, see [`MAX_LISTING_LINKS`]):
/// a folder with more is cut, so a huge one cannot make the server build an unbounded page.
pub const MAX_LISTING_ENTRIES: usize = 10_000;

/// Most links a listing shows, counted apart from the entries: a folder full of links cannot
/// push the files out of the page, nor make the page unbounded.
pub const MAX_LISTING_LINKS: usize = 1_000;

/// The words of a listing, in the user's language: the page is read in a browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingLabels {
    /// The title before the folder's path.
    pub index_of: String,
    /// The heading of the names.
    pub name: String,
    /// The heading of the sizes, in bytes.
    pub size: String,
    /// Why a link is not served ("not served: Windows link or cloud placeholder"): shown
    /// beside its name in a listing, and as the page of the 403 answer for it.
    pub not_served: String,
}

/// An entry of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its name.
    pub name: String,
    /// Whether it is a folder.
    pub is_dir: bool,
    /// Its size in bytes; 0 for a folder.
    pub len: u64,
    /// Whether it is a link of any kind (symbolic link, junction, cloud placeholder,
    /// deduplicated file): shown with the reason, never linked, since a request for it is
    /// refused. The C# server followed such links.
    pub not_served: bool,
}

/// The entries of `dir`: those that can be served, and the links, flagged
/// [`Entry::not_served`] so that the listing says why they cannot be fetched instead of
/// dropping them. Names a request would refuse for their form are left out. Folders come
/// first, then files, then the links, each by name.
///
/// # Errors
///
/// The folder cannot be read.
pub fn read_entries(dir: &Path) -> std::io::Result<Vec<Entry>> {
    read_entries_capped(dir, MAX_LISTING_ENTRIES, MAX_LISTING_LINKS)
}

/// [`read_entries`] with its two caps given: servable entries, then links.
fn read_entries_capped(
    dir: &Path,
    max_served: usize,
    max_links: usize,
) -> std::io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut links = 0;
    let mut served = 0;
    for item in std::fs::read_dir(dir)? {
        let Ok(item) = item else { continue };
        let Ok(name) = item.file_name().into_string() else {
            continue;
        };
        let Ok(metadata) = std::fs::symlink_metadata(item.path()) else {
            continue;
        };
        if !segment_allowed(&name) {
            continue;
        }
        if is_link(&metadata) {
            if links >= max_links {
                continue;
            }
            links += 1;
            entries.push(Entry {
                name,
                is_dir: false,
                len: 0,
                not_served: true,
            });
        } else if metadata.is_dir() || metadata.is_file() {
            if served >= max_served {
                continue;
            }
            served += 1;
            entries.push(Entry {
                name,
                is_dir: metadata.is_dir(),
                len: if metadata.is_dir() { 0 } else { metadata.len() },
                not_served: false,
            });
        }
        if served >= max_served && links >= max_links {
            break;
        }
    }
    entries.sort_by(|a, b| {
        a.not_served
            .cmp(&b.not_served)
            .then_with(|| b.is_dir.cmp(&a.is_dir))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

/// `text` safe to put in HTML text or in a quoted attribute.
#[must_use]
pub fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// `segment` as a URL path segment: everything but unreserved characters percent-encoded.
#[must_use]
pub fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The URL path of the folder named by `segments`, with its trailing slash.
fn folder_path(segments: &[String]) -> String {
    let mut out = String::from("/");
    for segment in segments {
        out.push_str(&encode_segment(segment));
        out.push('/');
    }
    out
}

/// The page listing `entries`, the content of the folder named by `segments` (none for the
/// root); its links carry `token`.
#[must_use]
pub fn render(
    segments: &[String],
    entries: &[Entry],
    token: &str,
    labels: &ListingLabels,
) -> String {
    let here = folder_path(segments);
    let title = format!("{} {}", labels.index_of, here);
    let mut page = String::new();
    let _ = write!(
        page,
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>{title}</title></head>\n\
         <body><h1>{title}</h1>\n<table><tr><th>{name}</th><th>{size}</th></tr>\n",
        title = html_escape(&title),
        name = html_escape(&labels.name),
        size = html_escape(&labels.size),
    );
    // Never a link above the root.
    if !segments.is_empty() {
        let parent = folder_path(&segments[..segments.len() - 1]);
        let _ = writeln!(
            page,
            "<tr><td><a href=\"{parent}?token={token}\">..</a></td><td></td></tr>",
            parent = html_escape(&parent),
            token = html_escape(token),
        );
    }
    for entry in entries {
        if entry.not_served {
            // No link: the entry cannot be fetched, and says why.
            let _ = writeln!(
                page,
                "<tr><td>{shown}</td><td>{reason}</td></tr>",
                shown = html_escape(&entry.name),
                reason = html_escape(&labels.not_served),
            );
            continue;
        }
        let encoded = encode_segment(&entry.name);
        let (href, shown, size) = if entry.is_dir {
            (
                format!("{here}{encoded}/"),
                format!("{}/", entry.name),
                "-".to_owned(),
            )
        } else {
            (
                format!("{here}{encoded}"),
                entry.name.clone(),
                entry.len.to_string(),
            )
        };
        let _ = writeln!(
            page,
            "<tr><td><a href=\"{href}?token={token}\">{shown}</a></td><td>{size}</td></tr>",
            href = html_escape(&href),
            token = html_escape(token),
            shown = html_escape(&shown),
        );
    }
    page.push_str("</table></body></html>\n");
    page
}

/// The page answering a request for a link: the reason, and nothing about the target.
#[must_use]
pub fn render_not_served(labels: &ListingLabels) -> String {
    let reason = html_escape(&labels.not_served);
    format!(
        "<!DOCTYPE html>
<html><head><meta charset=\"utf-8\"><title>{reason}</title></head>
         <body><h1>403</h1>
<p>{reason}</p></body></html>
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> ListingLabels {
        ListingLabels {
            index_of: "Index of".to_owned(),
            name: "Name".to_owned(),
            size: "Size".to_owned(),
            not_served: "not served: link".to_owned(),
        }
    }

    fn entry(name: &str, is_dir: bool) -> Entry {
        Entry {
            name: name.to_owned(),
            is_dir,
            len: 3,
            not_served: false,
        }
    }

    #[test]
    fn a_link_is_listed_with_its_reason_and_without_a_link() {
        let mut link = entry("<b>cloud</b>", false);
        link.not_served = true;
        let page = render(&[], &[entry("a.txt", false), link], "tok", &labels());
        assert!(
            page.contains("<tr><td>&lt;b&gt;cloud&lt;/b&gt;</td><td>not served: link</td></tr>")
        );
        assert!(!page.contains("cloud%"), "no href for a link");
        assert!(page.contains("href=\"/a.txt?token=tok\""));
        let refusal = render_not_served(&labels());
        assert!(refusal.contains("<p>not served: link</p>"));
    }

    #[test]
    fn names_are_escaped_in_text_and_in_links() {
        let page = render(
            &[],
            &[
                entry("<script>alert(1)</script>.txt", false),
                entry("a\"b'&c", true),
            ],
            "tok",
            &labels(),
        );
        assert!(!page.contains("<script>"));
        assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;.txt"));
        assert!(page.contains("a&quot;b&#39;&amp;c/"));
        assert!(page.contains("%3Cscript%3Ealert%281%29%3C%2Fscript%3E.txt?token=tok"));
        assert!(!page.contains("href=\"/a\"b"));
    }

    #[test]
    fn there_is_no_link_above_the_root() {
        let root = render(&[], &[entry("sub", true)], "t", &labels());
        assert!(!root.contains(">..<"));
        let below = render(&["sub".to_owned()], &[], "t", &labels());
        assert!(below.contains("<a href=\"/?token=t\">..</a>"));
        let deeper = render(&["sub".to_owned(), "x y".to_owned()], &[], "t", &labels());
        assert!(deeper.contains("<a href=\"/sub/?token=t\">..</a>"));
        assert!(deeper.contains("/sub/x%20y/"));
    }

    #[test]
    fn entries_are_read_folders_first() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("zdir")).unwrap();
        std::fs::write(dir.path().join("b.txt"), "bb").unwrap();
        std::fs::write(dir.path().join("A.txt"), "a").unwrap();
        let names: Vec<String> = read_entries(dir.path())
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["zdir", "A.txt", "b.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_read_as_not_served_and_sorted_last() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.txt"), "bb").unwrap();
        std::fs::write(dir.path().join("target"), "t").unwrap();
        std::os::unix::fs::symlink(dir.path().join("target"), dir.path().join("a-link")).unwrap();
        let entries = read_entries(dir.path()).unwrap();
        let summary: Vec<(&str, bool)> = entries
            .iter()
            .map(|e| (e.name.as_str(), e.not_served))
            .collect();
        assert_eq!(
            summary,
            [("b.txt", false), ("target", false), ("a-link", true)]
        );
    }

    #[cfg(unix)]
    #[test]
    fn links_do_not_count_toward_the_cap_of_servable_entries() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..4 {
            std::os::unix::fs::symlink("/nonexistent", dir.path().join(format!("l{index}")))
                .unwrap();
        }
        for index in 0..3 {
            std::fs::write(dir.path().join(format!("f{index}")), "x").unwrap();
        }
        let entries = read_entries_capped(dir.path(), 2, 3).unwrap();
        let served = entries.iter().filter(|e| !e.not_served).count();
        let links = entries.iter().filter(|e| e.not_served).count();
        assert_eq!((served, links), (2, 3));
    }
}
