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

//! Note storage, search, and Markdown/HTML conversion engine.
//!
//! Manages notes with title, tags, markdown body, and creation/update timestamps.

use std::collections::BTreeSet;

/// A single note entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteEntry {
    /// Unique identifier for the note.
    pub id: String,
    /// Human-readable title of the note.
    pub title: String,
    /// Collection of tags assigned to the note.
    pub tags: Vec<String>,
    /// Markdown content body.
    pub content: String,
    /// Last update timestamp display string.
    pub updated_at: String,
}

impl NoteEntry {
    /// Creates a new note entry.
    #[must_use]
    pub fn new(id: String, title: String, tags: Vec<String>, content: String) -> Self {
        Self {
            id,
            title,
            tags,
            content,
            updated_at: "Just now".to_owned(),
        }
    }

    /// Converts note markdown content into a clean HTML representation.
    #[must_use]
    pub fn to_html(&self) -> String {
        let mut html = format!("<h1>{}</h1>\n", html_escape(&self.title));
        if !self.tags.is_empty() {
            html.push_str("<p><em>Tags: ");
            html.push_str(
                &self
                    .tags
                    .iter()
                    .map(|t| format!("#{t}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            html.push_str("</em></p>\n");
        }

        html.push_str("<hr />\n");

        for line in self.content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some(h1) = trimmed.strip_prefix("# ") {
                html.push_str(&format!("<h2>{}</h2>\n", html_escape(h1)));
            } else if let Some(h2) = trimmed.strip_prefix("## ") {
                html.push_str(&format!("<h3>{}</h3>\n", html_escape(h2)));
            } else if let Some(h3) = trimmed.strip_prefix("### ") {
                html.push_str(&format!("<h4>{}</h4>\n", html_escape(h3)));
            } else if let Some(item) = trimmed.strip_prefix("- ") {
                html.push_str(&format!("<li>{}</li>\n", html_escape(item)));
            } else if let Some(code) = trimmed.strip_prefix("```") {
                if code.is_empty() {
                    html.push_str("<pre><code>\n");
                } else {
                    html.push_str(&format!(
                        "<pre><code class=\"language-{}\">\n",
                        html_escape(code)
                    ));
                }
            } else {
                html.push_str(&format!("<p>{}</p>\n", html_escape(trimmed)));
            }
        }

        html
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Collection manager for notes storage.
#[derive(Debug, Clone, Default)]
pub struct NotesStorage {
    notes: Vec<NoteEntry>,
}

impl NotesStorage {
    /// Creates a new empty notes storage.
    #[must_use]
    pub fn new() -> Self {
        Self { notes: Vec::new() }
    }

    /// Creates and prepopulates default initial sample notes.
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut storage = Self::new();
        storage.notes.push(NoteEntry::new(
            "1".to_owned(),
            "Incident Checklist".to_owned(),
            vec!["incident".to_owned(), "operations".to_owned()],
            "# Incident Checklist\n\n- [ ] Isolate compromised node\n- [ ] Snapshot volatile memory\n- [ ] Export auth logs\n- [ ] Notify SOC team".to_owned(),
        ));
        storage.notes.push(NoteEntry::new(
            "2".to_owned(),
            "Production Gateway IPs".to_owned(),
            vec!["network".to_owned(), "production".to_owned()],
            "# Production Gateway IPs\n\n- Primary: 10.0.0.1\n- Secondary: 10.0.0.2\n- DMZ Bastion: 192.168.10.254".to_owned(),
        ));
        storage
    }

    /// Returns a slice of all stored notes.
    #[must_use]
    pub fn all_notes(&self) -> &[NoteEntry] {
        &self.notes
    }

    /// Alias for `all_notes`.
    #[must_use]
    pub fn notes(&self) -> &[NoteEntry] {
        &self.notes
    }

    /// Creates and persists a new note entry.
    pub fn create_note(&mut self, title: &str, content: &str, tags: &[&str]) -> NoteEntry {
        let id = format!("{}", self.notes.len() + 1);
        let note = NoteEntry::new(
            id,
            title.to_owned(),
            tags.iter().map(|s| (*s).to_owned()).collect(),
            content.to_owned(),
        );
        self.notes.push(note.clone());
        note
    }

    /// Updates title, content, and tags of an existing note by ID.
    pub fn update_note(&mut self, id: &str, title: &str, content: &str, tags: &[&str]) {
        if let Some(note) = self.notes.iter_mut().find(|n| n.id == id) {
            note.title = title.to_owned();
            note.content = content.to_owned();
            note.tags = tags.iter().map(|s| (*s).to_owned()).collect();
            note.updated_at = "Just now".to_owned();
        }
    }

    /// Finds a note by its identifier.
    #[must_use]
    pub fn get_note(&self, id: &str) -> Option<&NoteEntry> {
        self.notes.iter().find(|n| n.id == id)
    }

    /// Searches notes matching query across title, content, or tags.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<&NoteEntry> {
        self.filter_notes(query, None)
    }

    /// Inserts or replaces a note.
    pub fn save_note(&mut self, note: NoteEntry) {
        if let Some(idx) = self.notes.iter().position(|n| n.id == note.id) {
            self.notes[idx] = note;
        } else {
            self.notes.push(note);
        }
    }

    /// Deletes a note by identifier. Returns true if a note was removed.
    pub fn delete_note(&mut self, id: &str) -> bool {
        let initial_len = self.notes.len();
        self.notes.retain(|n| n.id != id);
        self.notes.len() < initial_len
    }

    /// Returns a sorted deduplicated set of all tags present across notes.
    #[must_use]
    pub fn all_tags(&self) -> Vec<String> {
        let mut tags = BTreeSet::new();
        for n in &self.notes {
            for tag in &n.tags {
                tags.insert(tag.clone());
            }
        }
        tags.into_iter().collect()
    }

    /// Filters notes by search query text and optional tag filter.
    #[must_use]
    pub fn filter_notes<'a>(
        &'a self,
        query: &str,
        selected_tag: Option<&str>,
    ) -> Vec<&'a NoteEntry> {
        let q = query.trim().to_lowercase();
        self.notes
            .iter()
            .filter(|n| {
                if let Some(tag) = selected_tag {
                    if !n.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                        return false;
                    }
                }

                if q.is_empty() {
                    true
                } else {
                    n.title.to_lowercase().contains(&q)
                        || n.content.to_lowercase().contains(&q)
                        || n.tags.iter().any(|t| t.to_lowercase().contains(&q))
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notes_crud_and_tags() {
        let mut storage = NotesStorage::new();
        let note = NoteEntry::new(
            "test-1".to_owned(),
            "Server Setup".to_owned(),
            vec!["dev".to_owned(), "linux".to_owned()],
            "Install nginx and certbot".to_owned(),
        );

        storage.save_note(note);
        assert_eq!(storage.all_notes().len(), 1);
        assert_eq!(storage.all_tags(), vec!["dev", "linux"]);

        let filtered = storage.filter_notes("nginx", Some("dev"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Server Setup");

        assert!(storage.delete_note("test-1"));
        assert_eq!(storage.all_notes().len(), 0);
    }

    #[test]
    fn test_note_html_conversion() {
        let note = NoteEntry::new(
            "1".to_owned(),
            "My Note".to_owned(),
            vec!["sec".to_owned()],
            "# Header 1\n- Item 1".to_owned(),
        );
        let html = note.to_html();
        assert!(html.contains("<h1>My Note</h1>"));
        assert!(html.contains("<h2>Header 1</h2>"));
        assert!(html.contains("<li>Item 1</li>"));
    }
}
