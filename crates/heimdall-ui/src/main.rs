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

//! The Heimdall desktop application.

mod i18n;

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::i18n::fl;

/// Text size of the welcome line, in logical pixels.
const WELCOME_TEXT_SIZE: f32 = 28.0;

/// Vertical gap between the lines of the home view, in logical pixels.
const HOME_SPACING: f32 = 8.0;

/// Top-level application state.
#[derive(Debug, Default)]
struct Shell;

/// Events the shell reacts to. None exist yet.
#[derive(Debug, Clone, Copy)]
enum Message {}

impl Shell {
    fn title(_state: &Self) -> String {
        fl!("shell-window-title")
    }

    fn update(_state: &mut Self, message: Message) {
        match message {}
    }

    fn view(_state: &Self) -> Element<'_, Message> {
        container(
            column![
                text(fl!("shell-home-welcome")).size(WELCOME_TEXT_SIZE),
                text(fl!("shell-home-status")),
            ]
            .spacing(HOME_SPACING),
        )
        .center(Length::Fill)
        .into()
    }
}

fn main() -> iced::Result {
    i18n::init();
    iced::application(Shell::default, Shell::update, Shell::view)
        .title(Shell::title)
        .run()
}
