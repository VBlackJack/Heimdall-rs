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

//! Text on one line, cut where its room ends and closed with "...", as a C# `TextBlock`
//! with `TextTrimming="CharacterEllipsis"`.
//!
//! iced 0.14 wraps text or lets it run past its room; it never ends it with an ellipsis. This
//! widget measures the text in the room its parent gives it, and when the whole of it does
//! not fit, draws the longest start of it that does, followed by [`ELLIPSIS`]. Tests and
//! assistive tools read the whole text, as they read a C# `TextBlock`'s `Text`.

use std::borrow::Cow;

use iced::advanced::layout::{self, Layout};
use iced::advanced::text::{
    self as core_text, LineHeight, Paragraph as _, Renderer as _, Shaping, Wrapping,
};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Renderer as _, Widget, renderer};
use iced::widget::text::Style;
use iced::{Element, Font, Length, Pixels, Rectangle, Size, Theme, alignment, mouse};

/// What ends a text cut short: three ASCII dots, as everywhere in the project.
pub const ELLIPSIS: &str = "...";

/// The paragraph type of the window's renderer.
type Paragraph = <iced::Renderer as core_text::Renderer>::Paragraph;

/// The longest start of `content` that fits `room` with [`ELLIPSIS`] after it, as `width_of`
/// measures a line; `content` itself when it fits whole. The cut never splits a character,
/// and the spaces before the ellipsis are dropped, as WPF's `CharacterEllipsis` drops them.
/// With room for the ellipsis alone, the ellipsis; with less, nothing: a stub of dots
/// says nothing.
#[must_use]
pub fn fit(content: &str, room: f32, width_of: impl Fn(&str) -> f32) -> Cow<'_, str> {
    if width_of(content) <= room {
        return Cow::Borrowed(content);
    }
    let cut = |chars: usize| -> String {
        let end = content
            .char_indices()
            .nth(chars)
            .map_or(content.len(), |(at, _)| at);
        format!("{}{ELLIPSIS}", content[..end].trim_end())
    };
    // The most characters kept: the widths grow with them, so a halving search finds it.
    let (mut fits, mut fails) = (0, content.chars().count());
    while fails - fits > 1 {
        let middle = fits + (fails - fits) / 2;
        if width_of(&cut(middle)) <= room {
            fits = middle;
        } else {
            fails = middle;
        }
    }
    let kept = cut(fits);
    if width_of(&kept) <= room {
        Cow::Owned(kept)
    } else {
        Cow::Borrowed("")
    }
}

/// Text cut to its room with an ellipsis; see the module.
pub struct Ellipsis<'a> {
    content: String,
    width: Length,
    max_width: f32,
    size: Option<Pixels>,
    font: Option<Font>,
    line_height: LineHeight,
    style: Box<dyn Fn(&Theme) -> Style + 'a>,
}

/// `content` on one line, ended with an ellipsis where its room ends.
#[must_use]
pub fn text<'a>(content: impl Into<String>) -> Ellipsis<'a> {
    Ellipsis {
        content: content.into(),
        width: Length::Shrink,
        max_width: f32::INFINITY,
        size: None,
        font: None,
        line_height: LineHeight::default(),
        style: Box::new(|_| Style::default()),
    }
}

impl<'a> Ellipsis<'a> {
    /// The text's size.
    #[must_use]
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// The text's font.
    #[must_use]
    pub fn font(mut self, font: impl Into<Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    /// The text's width: as wide as the text, at most its room ([`Length::Shrink`], the
    /// default), or the whole of its room, as a column's cell.
    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// The widest the text grows, as a C# `MaxWidth`, whatever room it is given.
    #[must_use]
    pub fn max_width(mut self, max_width: impl Into<Pixels>) -> Self {
        self.max_width = max_width.into().0;
        self
    }

    /// The text's style, as an iced text's.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> Style + 'a) -> Self {
        self.style = Box::new(style);
        self
    }
}

/// What was last laid out: the text, its room and its look, and what of it was shown.
#[derive(Default)]
struct State {
    paragraph: core_text::paragraph::Plain<Paragraph>,
    fitted: Option<Fitted>,
}

/// A text fitted to a room, kept so that a layout in the same room measures nothing again.
struct Fitted {
    content: String,
    room: f32,
    size: Pixels,
    font: Font,
    shown: String,
}

impl Ellipsis<'_> {
    /// The text `content` as this widget lays it: on one line, at `size` in `font`.
    fn line<'b>(
        &self,
        content: &'b str,
        size: Pixels,
        font: Font,
        bounds: Size,
    ) -> core_text::Text<&'b str> {
        core_text::Text {
            content,
            bounds,
            size,
            line_height: self.line_height,
            font,
            align_x: core_text::Alignment::Default,
            align_y: alignment::Vertical::Top,
            shaping: Shaping::default(),
            wrapping: Wrapping::None,
        }
    }

    /// What of the text is shown in `room`, measured once for each text, room and look.
    fn shown(&self, state: &mut State, room: f32, size: Pixels, font: Font) -> String {
        if let Some(fitted) = &state.fitted
            && fitted.content == self.content
            && fitted.room.to_bits() == room.to_bits()
            && fitted.size == size
            && fitted.font == font
        {
            return fitted.shown.clone();
        }
        let shown = fit(&self.content, room, |candidate| {
            let line = self.line(candidate, size, font, Size::INFINITE);
            Paragraph::with_text(line).min_width()
        })
        .into_owned();
        state.fitted = Some(Fitted {
            content: self.content.clone(),
            room,
            size,
            font,
            shown: shown.clone(),
        });
        shown
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Ellipsis<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        let limits = limits.max_width(self.max_width);
        layout::sized(&limits, self.width, Length::Shrink, |limits| {
            let room = limits.max().width;
            let size = self.size.unwrap_or_else(|| renderer.default_size());
            let font = self.font.unwrap_or_else(|| renderer.default_font());
            let shown = self.shown(state, room, size, font);
            let line = self.line(&shown, size, font, limits.max());
            let _ = state.paragraph.update(line);
            let bounds = state.paragraph.min_bounds();
            Size::new(bounds.width.min(room), bounds.height)
        })
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let paragraph = state.paragraph.raw();
        let appearance = (self.style)(theme);
        let draw = |renderer: &mut iced::Renderer| {
            iced::advanced::widget::text::draw(
                renderer, style, bounds, paragraph, appearance, viewport,
            );
        };
        // Past its room by a rounding of the measure: cut at its edge.
        if paragraph.min_width() > bounds.width {
            renderer.with_layer(bounds, draw);
        } else {
            draw(renderer);
        }
    }

    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        // The whole text, as a C# `TextBlock` hands its `Text` to UI Automation.
        operation.text(None, layout.bounds(), &self.content);
    }
}

impl<'a, Message: 'a> From<Ellipsis<'a>> for Element<'a, Message> {
    fn from(text: Ellipsis<'a>) -> Self {
        Element::new(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Width of a character in the measures below.
    const ADVANCE: f32 = 10.0;

    /// A line's width when every character is [`ADVANCE`] wide.
    #[allow(
        clippy::cast_precision_loss,
        reason = "test lines are a few characters"
    )]
    fn width_of(line: &str) -> f32 {
        line.chars().count() as f32 * ADVANCE
    }

    #[test]
    fn a_text_that_fits_is_shown_whole() {
        assert_eq!(fit("server a", 80.0, width_of), "server a");
        assert_eq!(fit("", 0.0, width_of), "");
    }

    #[test]
    fn a_text_too_long_keeps_the_most_characters_the_ellipsis_leaves_room_for() {
        // Nine characters in 80: five, then the three dots.
        assert_eq!(fit("abcdefghi", 80.0, width_of), "abcde...");
        assert_eq!(fit("abcdefghi", 89.0, width_of), "abcde...");
        assert_eq!(fit("abcdefghi", 70.0, width_of), "abcd...");
    }

    #[test]
    fn the_spaces_before_the_ellipsis_are_dropped() {
        assert_eq!(fit("abc defghij", 70.0, width_of), "abc...");
    }

    #[test]
    fn a_cut_never_splits_a_character() {
        assert_eq!(fit("éèàùçâêîôû", 60.0, width_of), "éèà...");
    }

    #[test]
    fn with_room_for_the_ellipsis_alone_the_ellipsis_is_left_and_with_less_nothing() {
        assert_eq!(fit("abcdef", 30.0, width_of), ELLIPSIS);
        assert_eq!(fit("abcdef", 20.0, width_of), "");
    }
}
