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

//! What the key, certificate and password tools draw and do alike, as their C# views: a
//! card of a section, a label before its field, the system's save dialog held by the main
//! window, the work that takes seconds run off the window's thread.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use iced::widget::{Column, column, container, row, text};
use iced::{Alignment, Element, Length, Task, window};

use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Width of a field's label before it, as the C# `MinWidth` of the tools' labels.
pub const LABEL_WIDTH: f32 = 140.0;

/// Padding of a section's card, as the C# `PaddingSectionCard`.
const CARD_PADDING: f32 = 12.0;

/// The place picked in a save dialog, once it closes; `None` when cancelled.
type Pick = Pin<Box<dyn Future<Output = Option<rfd::FileHandle>> + Send>>;

/// A section's card holding `content`, as the C# `Border` of `CardBrush`.
pub fn card(content: Column<'_, Message>) -> Element<'_, Message> {
    container(content.spacing(spacing::SM))
        .padding(CARD_PADDING)
        .width(Length::Fill)
        .style(styles::card)
        .into()
}

/// A section's title, as the C# semibold `FontSizeBodyLarge` headers of the cards.
pub fn section_title<'a>(title: String) -> Element<'a, Message> {
    text(title)
        .size(font_size::BODY_LARGE)
        .font(styles::SEMIBOLD)
        .into()
}

/// `field` after its `label`, as the C# `DockPanel` rows.
pub fn labeled<'a>(label: String, field: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    row![
        text(label)
            .size(font_size::BODY)
            .style(text::secondary)
            .width(LABEL_WIDTH),
        container(field).width(Length::Fill),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center)
    .into()
}

/// A small note in the secondary text, as the C# caption hints.
pub fn hint<'a>(said: String) -> Element<'a, Message> {
    text(said)
        .size(font_size::CAPTION)
        .style(text::secondary)
        .into()
}

/// Asks where to save, over the main window `main`: a dialog titled `title`, offering
/// `file_name` and the filter `filter` of `extensions`; the place picked, or `None`, handed
/// to `done`.
pub fn ask_save(
    main: Option<window::Id>,
    title: String,
    file_name: String,
    filter: String,
    extensions: &'static [&'static str],
    done: impl Fn(Option<PathBuf>) -> Message + Clone + Send + 'static,
) -> Task<Message> {
    crate::shell::main_window_task(main).then(move |id| {
        let dialog = {
            let (title, file_name, filter) = (title.clone(), file_name.clone(), filter.clone());
            move || {
                rfd::AsyncFileDialog::new()
                    .set_title(title)
                    .set_file_name(file_name)
                    .add_filter(filter, extensions)
            }
        };
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                Box::pin(dialog().set_parent(&window).save_file()) as Pick
            }),
            None => Task::done(Box::pin(dialog().save_file()) as Pick),
        };
        let done = done.clone();
        pick.then(move |pick| {
            let done = done.clone();
            Task::perform(pick, move |file| {
                done(file.map(|file| file.path().to_owned()))
            })
        })
    })
}

/// `work` run off the window's thread, its result handed to `done`.
pub fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    done: impl Fn(Option<T>) -> Message + Send + 'static,
) -> Task<Message> {
    Task::perform(
        async move { tokio::task::spawn_blocking(work).await.ok() },
        done,
    )
}

/// A column of `items` spaced as the tools' stacks.
pub fn stack<'a>(items: impl IntoIterator<Item = Element<'a, Message>>) -> Column<'a, Message> {
    column(items).spacing(spacing::SM)
}
