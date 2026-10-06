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

// This file is derived from `src/text_editor.rs` of iced_widget 0.14.2, a part of iced
// (https://github.com/iced-rs/iced), distributed under the MIT license, whose notice
// follows:
//
// Copyright 2019 Héctor Ramón, Iced contributors
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software is furnished to do so,
// subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
// FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
// COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
// IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//
// What changed from it: the lines are numbered in a gutter, never wrapped, and scrolled
// across, with a horizontal scrollbar, the scroll kept in the `Content` beside the text; a
// press in the gutter selects lines. Removed: the placeholder, the wrapping, sizing,
// padding and styling options, the widget id, and the key bindings, `Binding`, `KeyPress`
// and `Status`, which are iced's own, used as they are.

//! The integrated editor's text widget: iced's text editor, its lines numbered in a gutter
//! and never wrapped, as the C# editor's (`ShowLineNumbers`, no `WordWrap`): a long line
//! stays whole and is scrolled across, by the caret, the wheel with Shift, a trackpad or
//! the scrollbar under the text.

mod geometry;

use std::cell::RefCell;
use std::ops;
use std::ops::DerefMut;
use std::sync::Arc;

use iced::advanced::clipboard::{self, Clipboard};
use iced::advanced::graphics::text::Editor as Laid;
use iced::advanced::graphics::text::cosmic_text;
use iced::advanced::input_method;
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::text::editor::Editor as _;
use iced::advanced::text::highlighter::{self, Highlighter};
use iced::advanced::text::{self, LineHeight, Paragraph as _, Text, Wrapping};
use iced::advanced::widget::{self, Widget, operation};
use iced::advanced::{InputMethod, Shell};
use iced::keyboard;
use iced::time::{Duration, Instant};
use iced::widget::text_editor::{Action, Binding, Cursor, Edit, KeyPress, Line, LineEnding};
use iced::widget::text_editor::{Motion, Position, Selection, Status};
use iced::{
    Background, Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme,
    Vector, alignment, theme, window,
};

use geometry::Regions;

/// Highest an editor can be and still scroll: iced's limit, `i32::MAX`, as a float.
const MAX_SCROLL_HEIGHT: f32 = 2_147_483_648.0;

/// Lines a notch of the wheel scrolls, at least one, as iced's.
const WHEEL_LINES: f32 = 4.0;

/// Pixels a notch of the wheel scrolls across, as iced's scrollables.
const WHEEL_PIXELS: f32 = 60.0;

/// Pixels of a trackpad's movement per line scrolled, as iced's.
const PIXELS_PER_LINE: f32 = 4.0;

/// Radius of the scrollbar's corners, as iced's scrollables.
const SCROLLBAR_RADIUS: f32 = 2.0;

/// Radius of the editor's corners, as iced's text editor.
const BORDER_RADIUS: f32 = 2.0;

/// Width of the editor's border, as iced's text editor.
const BORDER_WIDTH: f32 = 1.0;

/// What turns a key press into a [`Binding`].
type KeyBindingFn<'a, Message> = Box<dyn Fn(KeyPress) -> Option<Binding<Message>> + 'a>;

/// A multi-line text input, its lines numbered and never wrapped.
pub struct TextEditor<'a, Highlighter, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Highlighter: text::Highlighter,
    Theme: Catalog,
    Renderer: text::Renderer,
{
    /// The text shown and edited.
    content: &'a Content<Renderer>,
    /// The font of the text and of its line numbers; the renderer's when `None`.
    font: Option<Renderer::Font>,
    /// The style class.
    class: Theme::Class<'a>,
    /// What turns a key press into a binding; iced's default bindings when `None`.
    key_binding: Option<KeyBindingFn<'a, Message>>,
    /// The message of an action; the editor is disabled when `None`.
    on_edit: Option<Box<dyn Fn(Action) -> Message + 'a>>,
    /// What the highlighter colours, and how.
    highlighter_settings: Highlighter::Settings,
    /// How a highlight is drawn.
    highlighter_format: fn(&Highlighter::Highlight, &Theme) -> highlighter::Format<Renderer::Font>,
    /// The status at the last redraw.
    last_status: Option<Status>,
}

/// Creates a new [`TextEditor`] with the given [`Content`].
#[must_use]
pub fn code_editor<Message>(content: &Content) -> TextEditor<'_, highlighter::PlainText, Message> {
    TextEditor::new(content)
}

impl<'a, Message, Theme, Renderer> TextEditor<'a, highlighter::PlainText, Message, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    /// Creates new [`TextEditor`] with the given [`Content`].
    #[must_use]
    pub fn new(content: &'a Content<Renderer>) -> Self {
        Self {
            content,
            font: None,
            class: <Theme as Catalog>::default(),
            key_binding: None,
            on_edit: None,
            highlighter_settings: (),
            highlighter_format: |_highlight, _theme| highlighter::Format::default(),
            last_status: None,
        }
    }
}

impl<'a, Highlighter, Message, Theme, Renderer>
    TextEditor<'a, Highlighter, Message, Theme, Renderer>
where
    Highlighter: text::Highlighter,
    Theme: Catalog,
    Renderer: text::Renderer,
{
    /// Sets the message that should be produced when some action is performed in
    /// the [`TextEditor`].
    ///
    /// If this method is not called, the [`TextEditor`] will be disabled.
    #[must_use]
    pub fn on_action(mut self, on_edit: impl Fn(Action) -> Message + 'a) -> Self {
        self.on_edit = Some(Box::new(on_edit));
        self
    }

    /// Sets the [`Font`] of the [`TextEditor`], its line numbers' too.
    ///
    /// [`Font`]: text::Renderer::Font
    #[must_use]
    pub fn font(mut self, font: impl Into<Renderer::Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    /// Highlights the [`TextEditor`] using the given syntax and theme.
    #[must_use]
    pub fn highlight(
        self,
        syntax: &str,
        theme: iced::highlighter::Theme,
    ) -> TextEditor<'a, iced::highlighter::Highlighter, Message, Theme, Renderer>
    where
        Renderer: text::Renderer<Font = Font>,
    {
        TextEditor {
            content: self.content,
            font: self.font,
            class: self.class,
            key_binding: self.key_binding,
            on_edit: self.on_edit,
            highlighter_settings: iced::highlighter::Settings {
                theme,
                token: syntax.to_owned(),
            },
            highlighter_format: |highlight, _theme| highlight.to_format(),
            last_status: self.last_status,
        }
    }

    /// Sets the closure to produce key bindings on key presses.
    ///
    /// See [`Binding`] for the list of available bindings.
    #[must_use]
    pub fn key_binding(
        mut self,
        key_binding: impl Fn(KeyPress) -> Option<Binding<Message>> + 'a,
    ) -> Self {
        self.key_binding = Some(Box::new(key_binding));
        self
    }

    /// Where the input method's window goes: at the caret, while the editor has the
    /// keyboard.
    fn input_method<'b>(
        &self,
        state: &'b State<Highlighter>,
        renderer: &Renderer,
        layout: Layout<'_>,
    ) -> InputMethod<&'b str> {
        let Some(Focus {
            is_window_focused: true,
            ..
        }) = &state.focus
        else {
            return InputMethod::Disabled;
        };

        let internal = self.content.0.borrow();

        let regions = geometry::regions(layout.bounds(), state.gutter_width, internal.bar);
        let translation =
            regions.text.position() - Point::ORIGIN - Vector::new(internal.scroll_x, 0.0);

        let cursor = match internal.editor.selection() {
            Selection::Caret(position) => position,
            Selection::Range(ranges) => ranges.first().copied().unwrap_or_default().position(),
        };

        let line_height = LineHeight::default().to_absolute(renderer.default_size());

        let position = cursor + translation;

        InputMethod::Enabled {
            cursor: Rectangle::new(position, Size::new(1.0, f32::from(line_height))),
            purpose: input_method::Purpose::Normal,
            preedit: state.preedit.as_ref().map(input_method::Preedit::as_ref),
        }
    }
}

/// The content of a [`TextEditor`]: its text, and how far it is scrolled across.
pub struct Content<R = iced::Renderer>(RefCell<Internal<R>>)
where
    R: text::Renderer;

struct Internal<R>
where
    R: text::Renderer,
{
    /// The text, laid out.
    editor: R::Editor,
    /// How far the text is scrolled across, in pixels.
    scroll_x: f32,
    /// The cursor the scroll across last followed: it follows the cursor when it moves.
    followed: Option<Cursor>,
    /// Where the caret is across its line, at the last layout.
    caret_x: f32,
    /// Width of the text's view, at the last layout.
    view_width: f32,
    /// Width of the widest line shown, or of the caret's reach, at the last layout.
    content_width: f32,
    /// Whether the horizontal scrollbar is shown: the text is wider than its view.
    bar: bool,
}

impl<R> Content<R>
where
    R: text::Renderer,
{
    /// Creates a [`Content`] with the given text.
    #[must_use]
    pub fn with_text(text: &str) -> Self {
        Self(RefCell::new(Internal {
            editor: R::Editor::with_text(text),
            scroll_x: 0.0,
            followed: None,
            caret_x: 0.0,
            view_width: 0.0,
            content_width: 0.0,
            bar: false,
        }))
    }

    /// Performs an [`Action`] on the [`Content`].
    pub fn perform(&mut self, action: Action) {
        let internal = self.0.get_mut();

        internal.editor.perform(action);
    }

    /// Moves the current cursor to reflect the given one.
    pub fn move_to(&mut self, cursor: Cursor) {
        let internal = self.0.get_mut();

        internal.editor.move_to(cursor);
    }

    /// Returns the current cursor position of the [`Content`].
    #[must_use]
    pub fn cursor(&self) -> Cursor {
        self.0.borrow().editor.cursor()
    }

    /// Returns the amount of lines of the [`Content`].
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.0.borrow().editor.line_count()
    }

    /// Returns the text of the line at the given index, if it exists.
    #[must_use]
    pub fn line(&self, index: usize) -> Option<Line<'_>> {
        let internal = self.0.borrow();
        let line = internal.editor.line(index)?;

        Some(Line {
            text: std::borrow::Cow::Owned(line.text.into_owned()),
            ending: line.ending,
        })
    }

    /// Returns an iterator of the text of the lines in the [`Content`].
    fn lines(&self) -> impl Iterator<Item = Line<'_>> {
        (0..)
            .map(|i| self.line(i))
            .take_while(Option::is_some)
            .flatten()
    }

    /// Returns the text of the [`Content`].
    #[must_use]
    pub fn text(&self) -> String {
        let mut contents = String::new();
        let mut lines = self.lines().peekable();

        while let Some(line) = lines.next() {
            contents.push_str(&line.text);

            if lines.peek().is_some() {
                contents.push_str(if line.ending == LineEnding::None {
                    LineEnding::default().as_str()
                } else {
                    line.ending.as_str()
                });
            }
        }

        contents
    }

    /// Returns the selected text of the [`Content`].
    #[must_use]
    pub fn selection(&self) -> Option<String> {
        self.0.borrow().editor.copy()
    }

    /// Returns the kind of [`LineEnding`] used for separating lines in the [`Content`].
    #[must_use]
    pub fn line_ending(&self) -> Option<LineEnding> {
        Some(self.line(0)?.ending)
    }

    /// How far the text is scrolled across, in pixels: 0 at the start of the lines.
    #[must_use]
    pub fn horizontal_scroll(&self) -> f32 {
        self.0.borrow().scroll_x
    }

    /// Whether the caret was in view across, at the last layout.
    #[must_use]
    pub fn caret_in_view(&self) -> bool {
        let internal = self.0.borrow();
        internal.caret_x >= internal.scroll_x
            && internal.caret_x + geometry::CARET_WIDTH <= internal.scroll_x + internal.view_width
    }
}

/// The state of a [`TextEditor`].
struct State<Highlighter: text::Highlighter> {
    /// The keyboard's focus, while the editor has it.
    focus: Option<Focus>,
    /// What the input method composes.
    preedit: Option<input_method::Preedit>,
    /// The last click in the text, to tell a double or triple click.
    last_click: Option<mouse::Click>,
    /// The click a drag in the text started with.
    drag_click: Option<mouse::click::Kind>,
    /// The line a press in the gutter selected first, and the line the pointer is on.
    gutter_drag: Option<(usize, usize)>,
    /// Where the scrollbar's handle is held, from its left edge.
    scroller_grab: Option<f32>,
    /// The keyboard's modifiers: Shift turns the wheel across.
    modifiers: keyboard::Modifiers,
    /// What is left of a scroll down, less than a line.
    partial_scroll: f32,
    /// Width of the gutter, at the last layout.
    gutter_width: f32,
    /// Width of a digit of the line numbers, and the font and size it was measured in.
    digit: Option<(Font, Pixels, f32)>,
    /// The name of the theme last drawn in: another one colours the text again.
    last_theme: RefCell<Option<String>>,
    /// The highlighter.
    highlighter: RefCell<Highlighter>,
    /// What the highlighter was made with.
    highlighter_settings: Highlighter::Settings,
    /// The highlight format the text was coloured with.
    highlighter_format_address: usize,
}

/// The keyboard's focus, and the caret's blink.
#[derive(Debug, Clone)]
struct Focus {
    /// When the caret last moved or the window came back: it blinks from then.
    updated_at: Instant,
    /// The time of the last redraw.
    now: Instant,
    /// Whether the window has the keyboard.
    is_window_focused: bool,
}

impl Focus {
    /// How long the caret shows, then hides.
    const CURSOR_BLINK_INTERVAL_MILLIS: u128 = 500;

    /// The focus taken now.
    fn now() -> Self {
        let now = Instant::now();

        Self {
            updated_at: now,
            now,
            is_window_focused: true,
        }
    }

    /// Whether the caret shows at the last redraw.
    fn is_cursor_visible(&self) -> bool {
        self.is_window_focused
            && ((self.now - self.updated_at).as_millis() / Self::CURSOR_BLINK_INTERVAL_MILLIS)
                .is_multiple_of(2)
    }
}

impl<Highlighter: text::Highlighter> State<Highlighter> {
    /// Width of a digit of `font` at `size`, measured once.
    fn digit_width<R>(&mut self, font: Font, size: Pixels) -> f32
    where
        R: text::Renderer<Font = Font>,
    {
        match self.digit {
            Some((measured, at, width)) if measured == font && at == size => width,
            _ => {
                let width = R::Paragraph::with_text(Text {
                    content: "0",
                    bounds: Size::INFINITE,
                    size,
                    line_height: LineHeight::default(),
                    font,
                    align_x: text::Alignment::Default,
                    align_y: alignment::Vertical::Top,
                    shaping: text::Shaping::Basic,
                    wrapping: Wrapping::None,
                })
                .min_bounds()
                .width;
                self.digit = Some((font, size, width));
                width
            }
        }
    }
}

impl<Highlighter: text::Highlighter> operation::Focusable for State<Highlighter> {
    fn is_focused(&self) -> bool {
        self.focus.is_some()
    }

    fn focus(&mut self) {
        self.focus = Some(Focus::now());
    }

    fn unfocus(&mut self) {
        self.focus = None;
    }
}

impl<Highlighter, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for TextEditor<'_, Highlighter, Message, Theme, Renderer>
where
    Highlighter: text::Highlighter,
    Theme: Catalog,
    Renderer: text::Renderer<Font = Font, Editor = Laid>,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State<Highlighter>>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State {
            focus: None,
            preedit: None,
            last_click: None,
            drag_click: None,
            gutter_drag: None,
            scroller_grab: None,
            modifiers: keyboard::Modifiers::default(),
            partial_scroll: 0.0,
            gutter_width: 0.0,
            digit: None,
            last_theme: RefCell::default(),
            highlighter: RefCell::new(Highlighter::new(&self.highlighter_settings)),
            highlighter_settings: self.highlighter_settings.clone(),
            highlighter_format_address: self.highlighter_format as usize,
        })
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let mut internal = self.content.0.borrow_mut();
        let state = tree.state.downcast_mut::<State<Highlighter>>();

        if state.highlighter_format_address != self.highlighter_format as usize {
            state.highlighter.borrow_mut().change_line(0);

            state.highlighter_format_address = self.highlighter_format as usize;
        }

        if state.highlighter_settings != self.highlighter_settings {
            state
                .highlighter
                .borrow_mut()
                .update(&self.highlighter_settings);

            state.highlighter_settings = self.highlighter_settings.clone();
        }

        let limits = limits.width(Length::Fill).height(Length::Fill);

        let font = self.font.unwrap_or_else(|| renderer.default_font());
        let size = renderer.default_size();
        let digit_width = state.digit_width::<Renderer>(font, size);
        state.gutter_width = geometry::gutter_width(internal.editor.line_count(), digit_width);

        lay_out(
            &mut internal,
            limits.max(),
            state.gutter_width,
            (font, size),
            state.highlighter.borrow_mut().deref_mut(),
        );

        layout::Node::new(limits.max())
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let Some(on_edit) = self.on_edit.as_ref() else {
            return;
        };

        let state = tree.state.downcast_mut::<State<Highlighter>>();
        let is_redraw = matches!(event, Event::Window(window::Event::RedrawRequested(_now)),);

        window_event(state, event, shell);

        let regions = geometry::regions(
            layout.bounds(),
            state.gutter_width,
            self.content.0.borrow().bar,
        );

        if let Some(update) = Update::from_event(
            event,
            state,
            &regions,
            layout.bounds(),
            cursor,
            self.key_binding.as_deref(),
        ) {
            let editing = Editing {
                content: self.content,
                on_edit: on_edit.as_ref(),
                regions,
            };
            editing.apply(update, state, clipboard, shell);
        }

        let status = {
            let is_disabled = self.on_edit.is_none();
            let is_hovered = cursor.is_over(layout.bounds());

            if is_disabled {
                Status::Disabled
            } else if state.focus.is_some() {
                Status::Focused { is_hovered }
            } else if is_hovered {
                Status::Hovered
            } else {
                Status::Active
            }
        };

        if is_redraw {
            self.last_status = Some(status);

            shell.request_input_method(&self.input_method(state, renderer, layout));
        } else if self
            .last_status
            .is_some_and(|last_status| status != last_status)
        {
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        let mut internal = self.content.0.borrow_mut();
        let state = tree.state.downcast_ref::<State<Highlighter>>();

        let font = self.font.unwrap_or_else(|| renderer.default_font());

        let theme_name = theme.name();

        if state
            .last_theme
            .borrow()
            .as_ref()
            .is_none_or(|last_theme| last_theme != theme_name)
        {
            state.highlighter.borrow_mut().change_line(0);
            let _ = state.last_theme.borrow_mut().replace(theme_name.to_owned());
        }

        internal.editor.highlight(
            font,
            state.highlighter.borrow_mut().deref_mut(),
            |highlight| (self.highlighter_format)(highlight, theme),
        );

        let style = theme.style(&self.class, self.last_status.unwrap_or(Status::Active));

        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: style.border,
                ..renderer::Quad::default()
            },
            style.background,
        );

        let regions = geometry::regions(bounds, state.gutter_width, internal.bar);
        let digit_width = state.digit.map_or(0.0, |(_, _, width)| width);

        draw_gutter(renderer, &internal, &regions, &style, (font, digit_width));

        let translation =
            regions.text.position() - Point::ORIGIN - Vector::new(internal.scroll_x, 0.0);

        renderer.fill_editor(
            &internal.editor,
            Point::ORIGIN + translation,
            style.value,
            regions.text,
        );

        if let Some(focus) = state.focus.as_ref() {
            draw_selection(
                renderer,
                &internal,
                focus,
                regions.text,
                translation,
                &style,
            );
        }

        if let Some(rail) = regions.bar {
            draw_scrollbar(renderer, &internal, rail, &style);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let is_disabled = self.on_edit.is_none();
        let state = tree.state.downcast_ref::<State<Highlighter>>();
        let regions = geometry::regions(
            layout.bounds(),
            state.gutter_width,
            self.content.0.borrow().bar,
        );

        if cursor.is_over(layout.bounds()) && is_disabled {
            mouse::Interaction::NotAllowed
        } else if cursor.is_over(regions.text) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        let state = tree.state.downcast_mut::<State<Highlighter>>();
        let gutter_width = state.gutter_width;

        operation.focusable(None, layout.bounds(), state);

        // The line numbers shown, each over its line's part of the gutter.
        let internal = self.content.0.borrow();
        let regions = geometry::regions(layout.bounds(), gutter_width, internal.bar);

        for run in internal.editor.buffer().layout_runs() {
            let bounds = Rectangle::new(
                Point::new(regions.gutter.x, regions.text.y + run.line_top),
                Size::new(regions.gutter.width, run.line_height),
            );

            operation.text(None, bounds, &(run.line_i + 1).to_string());
        }
    }
}

impl<'a, Highlighter, Message, Theme, Renderer>
    From<TextEditor<'a, Highlighter, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Highlighter: text::Highlighter,
    Message: 'a,
    Theme: Catalog + 'a,
    Renderer: text::Renderer<Font = Font, Editor = Laid>,
{
    fn from(text_editor: TextEditor<'a, Highlighter, Message, Theme, Renderer>) -> Self {
        Self::new(text_editor)
    }
}

/// Lays the text out in an editor `max` large, its gutter `gutter_width` wide, with its
/// font and size: the scrollbar shown when the lines shown are wider than the text's view,
/// and the scroll across following the cursor when it moved.
fn lay_out<R, H>(
    internal: &mut Internal<R>,
    max: Size,
    gutter_width: f32,
    (font, size): (Font, Pixels),
    highlighter: &mut H,
) where
    R: text::Renderer<Font = Font, Editor = Laid>,
    H: text::Highlighter,
{
    let mut update = |internal: &mut Internal<R>, bar: bool| {
        let view = geometry::text_size(max, gutter_width, bar);
        internal.editor.update(
            view,
            font,
            size,
            LineHeight::default(),
            Wrapping::None,
            &mut *highlighter,
        );
        view.width
    };

    let bar = internal.bar;
    let mut view = update(internal, bar);
    let cursor = internal.editor.cursor();
    let caret = caret_x(internal.editor.buffer(), cursor.position);
    let reach = caret.map_or(0.0, |caret| caret + geometry::CARET_WIDTH);
    let mut content = widest(internal.editor.buffer()).max(reach);

    if (content > view) != bar {
        internal.bar = !bar;
        view = update(internal, !bar);
        content = widest(internal.editor.buffer()).max(reach);
    }

    if let Some(caret) = caret {
        internal.caret_x = caret;

        if internal.followed != Some(cursor) {
            internal.scroll_x = geometry::follow(internal.scroll_x, caret, view);
            internal.followed = Some(cursor);
        }
    }

    internal.view_width = view;
    internal.content_width = content;
    internal.scroll_x = geometry::clamp(internal.scroll_x, content, view);
}

/// Width of the widest line shown.
fn widest(buffer: &cosmic_text::Buffer) -> f32 {
    buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0, f32::max)
}

/// Where the caret at `position` is across its line, as iced draws it: the width of the
/// glyphs before it. `None` while its line is not laid out.
fn caret_x(buffer: &cosmic_text::Buffer, position: Position) -> Option<f32> {
    let layout = buffer.lines.get(position.line)?.layout_opt()?;
    let line = layout.first()?;

    Some(
        line.glyphs
            .iter()
            .take_while(|glyph| position.column > glyph.start)
            .map(|glyph| glyph.w)
            .sum(),
    )
}

/// The line shown `y` below the top of the text, and the middle of its row there.
fn line_at(buffer: &cosmic_text::Buffer, y: f32) -> Option<(usize, f32)> {
    buffer
        .layout_runs()
        .find(|run| y >= run.line_top && y < run.line_top + run.line_height)
        .map(|run| (run.line_i, run.line_top + run.line_height / 2.0))
}

/// The middle of the row of line `index`, when it is shown.
fn row_of(buffer: &cosmic_text::Buffer, index: usize) -> Option<f32> {
    buffer
        .layout_runs()
        .find(|run| run.line_i == index)
        .map(|run| run.line_top + run.line_height / 2.0)
}

/// What selects the lines from `anchor` to `target`, each a line and the middle of its
/// row, their endings included, as the C# editor's gutter does: the caret ends at the start
/// of the line after the last one selected, or at the end of the text.
fn select_lines(anchor: (usize, f32), target: (usize, f32)) -> [Action; 4] {
    let start = |y: f32| Point::new(0.0, y);

    if target.0 >= anchor.0 {
        [
            Action::Click(start(anchor.1)),
            Action::Drag(start(target.1)),
            Action::Select(Motion::End),
            Action::Select(Motion::Right),
        ]
    } else {
        [
            Action::Click(start(anchor.1)),
            Action::Move(Motion::End),
            Action::Move(Motion::Right),
            Action::Drag(start(target.1)),
        ]
    }
}

/// The scroll down asked by `lines` and what is left of the last ones: iced's editor
/// scrolls by whole lines.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a wheel's few lines, as iced's own text editor counts them"
)]
fn whole_lines(lines: f32) -> (i32, f32) {
    (lines as i32, lines.fract())
}

/// Keeps the caret blinking and the window's focus and the keyboard's modifiers known.
fn window_event<H: text::Highlighter, Message>(
    state: &mut State<H>,
    event: &Event,
    shell: &mut Shell<'_, Message>,
) {
    match event {
        Event::Window(window::Event::Unfocused) => {
            if let Some(focus) = &mut state.focus {
                focus.is_window_focused = false;
            }
        }
        Event::Window(window::Event::Focused) => {
            if let Some(focus) = &mut state.focus {
                focus.is_window_focused = true;
                focus.updated_at = Instant::now();

                shell.request_redraw();
            }
        }
        Event::Window(window::Event::RedrawRequested(now)) => {
            if let Some(focus) = &mut state.focus
                && focus.is_window_focused
            {
                focus.now = *now;

                let millis_until_redraw = Focus::CURSOR_BLINK_INTERVAL_MILLIS
                    - (focus.now - focus.updated_at).as_millis()
                        % Focus::CURSOR_BLINK_INTERVAL_MILLIS;

                shell.request_redraw_at(
                    focus.now
                        + Duration::from_millis(
                            u64::try_from(millis_until_redraw).unwrap_or(u64::MAX),
                        ),
                );
            }
        }
        Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            state.modifiers = *modifiers;
        }
        _ => {}
    }
}

/// What an update acts on: the content, the message of an action, and the editor's parts.
struct Editing<'a, 'b, R, Message>
where
    R: text::Renderer,
{
    content: &'a Content<R>,
    on_edit: &'b dyn Fn(Action) -> Message,
    regions: Regions,
}

impl<R, Message> Editing<'_, '_, R, Message>
where
    R: text::Renderer<Font = Font, Editor = Laid>,
{
    /// Applies `update`.
    fn apply<H: text::Highlighter>(
        &self,
        update: Update<Message>,
        state: &mut State<H>,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        match update {
            Update::Click(click) => {
                let action = match click.kind() {
                    mouse::click::Kind::Single => {
                        let scroll = self.content.0.borrow().scroll_x;
                        Action::Click(click.position() + Vector::new(scroll, 0.0))
                    }
                    mouse::click::Kind::Double => Action::SelectWord,
                    mouse::click::Kind::Triple => Action::SelectLine,
                };

                state.focus = Some(Focus::now());
                state.last_click = Some(click);
                state.drag_click = Some(click.kind());

                shell.publish((self.on_edit)(action));
                shell.capture_event();
            }
            Update::Drag(position) => {
                let scroll = self.content.0.borrow().scroll_x;
                shell.publish((self.on_edit)(Action::Drag(
                    position + Vector::new(scroll, 0.0),
                )));
            }
            Update::Release => {
                state.drag_click = None;
                state.gutter_drag = None;
                state.scroller_grab = None;
            }
            Update::Scroll { lines, across } => self.scroll(state, lines, across, shell),
            Update::Gutter { y, pressed } => self.gutter(state, y, pressed, shell),
            Update::Scrollbar { x, pressed } => self.scrollbar(state, x, pressed, shell),
            Update::InputMethod(update) => self.input_method(state, update, shell),
            Update::Binding(binding) => {
                if !matches!(binding, Binding::Unfocus) {
                    shell.capture_event();
                }

                apply_binding(binding, self.content, state, self.on_edit, clipboard, shell);

                if let Some(focus) = &mut state.focus {
                    focus.updated_at = Instant::now();
                }
            }
        }
    }

    /// Scrolls `lines` down, as iced's editor, and `across` pixels to the right.
    fn scroll<H: text::Highlighter>(
        &self,
        state: &mut State<H>,
        lines: f32,
        across: f32,
        shell: &mut Shell<'_, Message>,
    ) {
        if across != 0.0 {
            let mut internal = self.content.0.borrow_mut();
            let scroll = geometry::clamp(
                internal.scroll_x + across,
                internal.content_width,
                internal.view_width,
            );

            if (scroll - internal.scroll_x).abs() > f32::EPSILON {
                internal.scroll_x = scroll;
                shell.request_redraw();
            }

            shell.capture_event();
        }

        if lines == 0.0 {
            return;
        }

        let bounds = self.content.0.borrow().editor.bounds();

        if bounds.height >= MAX_SCROLL_HEIGHT {
            return;
        }

        let (lines, rest) = whole_lines(lines + state.partial_scroll);
        state.partial_scroll = rest;

        shell.publish((self.on_edit)(Action::Scroll { lines }));
        shell.capture_event();
    }

    /// A press in the gutter (`pressed`), or the pointer dragged there, `y` below the top
    /// of the text: the lines from the one pressed to the one pointed at selected.
    fn gutter<H: text::Highlighter>(
        &self,
        state: &mut State<H>,
        y: f32,
        pressed: bool,
        shell: &mut Shell<'_, Message>,
    ) {
        let internal = self.content.0.borrow();
        let buffer = internal.editor.buffer();

        if pressed {
            state.focus = Some(Focus::now());
            state.last_click = None;
            shell.capture_event();
        }

        let Some(target) = line_at(buffer, y) else {
            return;
        };

        let anchor = match state.gutter_drag {
            Some((_, last)) if !pressed && last == target.0 => return,
            Some((anchor, _)) if !pressed => match row_of(buffer, anchor) {
                Some(row) => (anchor, row),
                None => return,
            },
            _ => target,
        };

        state.gutter_drag = Some((anchor.0, target.0));

        for action in select_lines(anchor, target) {
            shell.publish((self.on_edit)(action));
        }
    }

    /// A press on the horizontal scrollbar (`pressed`), or its handle dragged, the pointer
    /// `x` across the window.
    fn scrollbar<H: text::Highlighter>(
        &self,
        state: &mut State<H>,
        x: f32,
        pressed: bool,
        shell: &mut Shell<'_, Message>,
    ) {
        let Some(rail) = self.regions.bar else {
            return;
        };

        let mut internal = self.content.0.borrow_mut();
        let (content, view) = (internal.content_width, internal.view_width);
        let handle = geometry::scroller(rail, internal.scroll_x, content, view);

        let grab = match state.scroller_grab {
            Some(grab) if !pressed => grab,
            // On the handle, it is held where it was pressed; beside it, by its middle.
            _ if x >= handle.x && x <= handle.x + handle.width => x - handle.x,
            _ => handle.width / 2.0,
        };

        state.scroller_grab = Some(grab);
        internal.scroll_x = geometry::scroll_at(rail, handle.width, x - grab, content, view);

        shell.request_redraw();
        shell.capture_event();
    }

    /// What the input method says.
    fn input_method<H: text::Highlighter>(
        &self,
        state: &mut State<H>,
        update: Ime,
        shell: &mut Shell<'_, Message>,
    ) {
        match update {
            Ime::Toggle(is_open) => {
                state.preedit = is_open.then(input_method::Preedit::new);

                shell.request_redraw();
            }
            Ime::Preedit { content, selection } => {
                state.preedit = Some(input_method::Preedit {
                    content,
                    selection,
                    text_size: None,
                });

                shell.request_redraw();
            }
            Ime::Commit(text) => {
                shell.publish((self.on_edit)(Action::Edit(Edit::Paste(Arc::new(text)))));
            }
        }
    }
}

/// Does what a key binding asks.
fn apply_binding<H: text::Highlighter, R: text::Renderer, Message>(
    binding: Binding<Message>,
    content: &Content<R>,
    state: &mut State<H>,
    on_edit: &dyn Fn(Action) -> Message,
    clipboard: &mut dyn Clipboard,
    shell: &mut Shell<'_, Message>,
) {
    let mut publish = |action| shell.publish(on_edit(action));

    match binding {
        Binding::Unfocus => {
            state.focus = None;
            state.drag_click = None;
        }
        Binding::Copy => {
            if let Some(selection) = content.selection() {
                clipboard.write(clipboard::Kind::Standard, selection);
            }
        }
        Binding::Cut => {
            if let Some(selection) = content.selection() {
                clipboard.write(clipboard::Kind::Standard, selection);

                publish(Action::Edit(Edit::Delete));
            }
        }
        Binding::Paste => {
            if let Some(contents) = clipboard.read(clipboard::Kind::Standard) {
                publish(Action::Edit(Edit::Paste(Arc::new(contents))));
            }
        }
        Binding::Move(motion) => {
            publish(Action::Move(motion));
        }
        Binding::Select(motion) => {
            publish(Action::Select(motion));
        }
        Binding::SelectWord => {
            publish(Action::SelectWord);
        }
        Binding::SelectLine => {
            publish(Action::SelectLine);
        }
        Binding::SelectAll => {
            publish(Action::SelectAll);
        }
        Binding::Insert(c) => {
            publish(Action::Edit(Edit::Insert(c)));
        }
        Binding::Enter => {
            publish(Action::Edit(Edit::Enter));
        }
        Binding::Backspace => {
            publish(Action::Edit(Edit::Backspace));
        }
        Binding::Delete => {
            publish(Action::Edit(Edit::Delete));
        }
        Binding::Sequence(sequence) => {
            for binding in sequence {
                apply_binding(binding, content, state, on_edit, clipboard, shell);
            }
        }
        Binding::Custom(message) => {
            shell.publish(message);
        }
    }
}

/// Draws the number of each line shown, right-aligned in the gutter, the cursor's line's
/// in the text's colour.
fn draw_gutter<R>(
    renderer: &mut R,
    internal: &Internal<R>,
    regions: &Regions,
    style: &Style,
    (font, digit_width): (Font, f32),
) where
    R: text::Renderer<Font = Font, Editor = Laid>,
{
    let size = renderer.default_size();
    let right = regions.text.x - geometry::GUTTER_GAP;
    let clip = Rectangle {
        height: regions.text.height,
        ..regions.gutter
    };
    let current = internal.editor.cursor().position.line;

    for run in internal.editor.buffer().layout_runs() {
        let number = run.line_i + 1;
        let width = f32::from(geometry::digits(number)) * digit_width;

        renderer.fill_text(
            Text {
                content: number.to_string(),
                bounds: Size::new(width, run.line_height),
                size,
                line_height: LineHeight::default(),
                font,
                align_x: text::Alignment::Default,
                align_y: alignment::Vertical::Top,
                shaping: text::Shaping::Basic,
                wrapping: Wrapping::None,
            },
            Point::new(right - width, regions.text.y + run.line_top),
            if run.line_i == current {
                style.current_line_number
            } else {
                style.line_number
            },
            clip,
        );
    }
}

/// Draws the caret, blinking, or the selection, moved by `translation` and kept in `area`.
fn draw_selection<R>(
    renderer: &mut R,
    internal: &Internal<R>,
    focus: &Focus,
    area: Rectangle,
    translation: Vector,
    style: &Style,
) where
    R: text::Renderer<Font = Font, Editor = Laid>,
{
    match internal.editor.selection() {
        Selection::Caret(position) if focus.is_cursor_visible() => {
            let cursor = Rectangle::new(
                position + translation,
                Size::new(
                    geometry::CARET_WIDTH,
                    LineHeight::default()
                        .to_absolute(renderer.default_size())
                        .into(),
                ),
            );

            if let Some(clipped_cursor) = area.intersection(&cursor) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: clipped_cursor,
                        ..renderer::Quad::default()
                    },
                    style.value,
                );
            }
        }
        Selection::Range(ranges) => {
            for range in ranges
                .into_iter()
                .filter_map(|range| area.intersection(&(range + translation)))
            {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: range,
                        ..renderer::Quad::default()
                    },
                    style.selection,
                );
            }
        }
        Selection::Caret(_) => {}
    }
}

/// Draws the horizontal scrollbar in `rail`, its handle where the text is scrolled.
fn draw_scrollbar<R>(renderer: &mut R, internal: &Internal<R>, rail: Rectangle, style: &Style)
where
    R: text::Renderer,
{
    let handle = geometry::scroller(
        rail,
        internal.scroll_x,
        internal.content_width,
        internal.view_width,
    );

    for (bounds, background) in [(rail, style.rail), (handle, style.scroller)] {
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border::default().rounded(SCROLLBAR_RADIUS),
                ..renderer::Quad::default()
            },
            background,
        );
    }
}

/// What an event asks of the editor.
enum Update<Message> {
    /// A click in the text, from the text's top left corner as shown.
    Click(mouse::Click),
    /// The pointer dragged in the text, from the text's top left corner as shown.
    Drag(Point),
    /// The button let go.
    Release,
    /// Lines to scroll down, and pixels to scroll across.
    Scroll { lines: f32, across: f32 },
    /// A press in the gutter, or the pointer moved there while pressed, `y` below the
    /// top of the text.
    Gutter { y: f32, pressed: bool },
    /// A press on the horizontal scrollbar, or its handle dragged, the pointer `x` across
    /// the window.
    Scrollbar { x: f32, pressed: bool },
    /// What the input method says.
    InputMethod(Ime),
    /// A key's binding.
    Binding(Binding<Message>),
}

/// What the input method says.
enum Ime {
    /// It opened, or closed.
    Toggle(bool),
    /// What it composes, and what of it is selected.
    Preedit {
        content: String,
        selection: Option<ops::Range<usize>>,
    },
    /// What it composed, to insert.
    Commit(String),
}

impl<Message> Update<Message> {
    /// What `event` asks of the editor drawn in `bounds`, its parts `regions`.
    fn from_event<H: Highlighter>(
        event: &Event,
        state: &State<H>,
        regions: &Regions,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        key_binding: Option<&dyn Fn(KeyPress) -> Option<Binding<Message>>>,
    ) -> Option<Self> {
        match event {
            Event::Mouse(event) => Self::from_mouse(event, state, regions, bounds, cursor),
            Event::InputMethod(event) => Self::from_input_method(event, state),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                modifiers,
                text,
                ..
            }) => {
                let status = if state.focus.is_some() {
                    Status::Focused {
                        is_hovered: cursor.is_over(bounds),
                    }
                } else {
                    Status::Active
                };

                let key_press = KeyPress {
                    key: key.clone(),
                    modified_key: modified_key.clone(),
                    physical_key: *physical_key,
                    modifiers: *modifiers,
                    text: text.clone(),
                    status,
                };

                if let Some(key_binding) = key_binding {
                    key_binding(key_press)
                } else {
                    Binding::from_key_press(key_press)
                }
                .map(Self::Binding)
            }
            _ => None,
        }
    }

    /// What a mouse event asks.
    fn from_mouse<H: Highlighter>(
        event: &mouse::Event,
        state: &State<H>,
        regions: &Regions,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Self> {
        let text_origin = regions.text.position() - Point::ORIGIN;

        match event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let Some(position) = cursor.position_over(bounds) else {
                    return state
                        .focus
                        .is_some()
                        .then_some(Update::Binding(Binding::Unfocus));
                };

                if regions.bar.is_some_and(|bar| bar.contains(position)) {
                    Some(Update::Scrollbar {
                        x: position.x,
                        pressed: true,
                    })
                } else if position.x < regions.text.x {
                    Some(Update::Gutter {
                        y: position.y - regions.text.y,
                        pressed: true,
                    })
                } else {
                    let click = mouse::Click::new(
                        position - text_origin,
                        mouse::Button::Left,
                        state.last_click,
                    );

                    Some(Update::Click(click))
                }
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => Some(Update::Release),
            mouse::Event::CursorMoved { .. } => {
                if state.scroller_grab.is_some() {
                    return cursor.position().map(|position| Update::Scrollbar {
                        x: position.x,
                        pressed: false,
                    });
                }

                let position = cursor.position_over(bounds)?;

                if state.gutter_drag.is_some() {
                    return Some(Update::Gutter {
                        y: position.y - regions.text.y,
                        pressed: false,
                    });
                }

                match state.drag_click {
                    Some(mouse::click::Kind::Single) => Some(Update::Drag(position - text_origin)),
                    _ => None,
                }
            }
            mouse::Event::WheelScrolled { delta } if cursor.is_over(bounds) => {
                Some(wheel(*delta, state.modifiers))
            }
            _ => None,
        }
    }

    /// What an input method event asks.
    fn from_input_method<H: Highlighter>(
        event: &input_method::Event,
        state: &State<H>,
    ) -> Option<Self> {
        match event {
            input_method::Event::Opened | input_method::Event::Closed => Some(Update::InputMethod(
                Ime::Toggle(matches!(event, input_method::Event::Opened)),
            )),
            input_method::Event::Preedit(content, selection) if state.focus.is_some() => {
                Some(Update::InputMethod(Ime::Preedit {
                    content: content.clone(),
                    selection: selection.clone(),
                }))
            }
            input_method::Event::Commit(content) if state.focus.is_some() => {
                Some(Update::InputMethod(Ime::Commit(content.clone())))
            }
            _ => None,
        }
    }
}

/// The scroll a turn of the wheel or a trackpad asks: down as iced's editor, and across
/// for a sideways movement or, with Shift, for the wheel's.
fn wheel<Message>(delta: mouse::ScrollDelta, modifiers: keyboard::Modifiers) -> Update<Message> {
    // macOS turns the wheel across with Shift itself, as iced's scrollables note.
    let sideways = modifiers.shift() && !cfg!(target_os = "macos");

    let (x, y, notches) = match delta {
        mouse::ScrollDelta::Lines { x, y } => (x, y, true),
        mouse::ScrollDelta::Pixels { x, y } => (x, y, false),
    };
    let (x, y) = if sideways { (y, x) } else { (x, y) };

    if notches {
        Update::Scroll {
            lines: if y.abs() > 0.0 {
                y.signum() * -(y.abs() * WHEEL_LINES).max(1.0)
            } else {
                0.0
            },
            across: -x * WHEEL_PIXELS,
        }
    } else {
        Update::Scroll {
            lines: -y / PIXELS_PER_LINE,
            across: -x,
        }
    }
}

/// The appearance of a [`TextEditor`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// The [`Background`] of the text editor.
    pub background: Background,
    /// The [`Border`] of the text editor.
    pub border: Border,
    /// The [`Color`] of the value of the text editor.
    pub value: Color,
    /// The [`Color`] of the selection of the text editor.
    pub selection: Color,
    /// The [`Color`] of the line numbers.
    pub line_number: Color,
    /// The [`Color`] of the number of the cursor's line.
    pub current_line_number: Color,
    /// The [`Background`] of the horizontal scrollbar.
    pub rail: Background,
    /// The [`Background`] of the horizontal scrollbar's handle.
    pub scroller: Background,
}

/// The theme catalog of a [`TextEditor`].
pub trait Catalog: theme::Base {
    /// The item class of the [`Catalog`].
    type Class<'a>;

    /// The default class produced by the [`Catalog`].
    fn default<'a>() -> Self::Class<'a>;

    /// The [`Style`] of a class with the given status.
    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style;
}

/// A styling function for a [`TextEditor`].
pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme, Status) -> Style + 'a>;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style {
        class(self, status)
    }
}

/// The default style of a [`TextEditor`]: iced's, the line numbers in the theme's weak
/// text colour, and the scrollbar as iced's scrollables'.
fn default(theme: &Theme, status: Status) -> Style {
    let palette = theme.extended_palette();

    let active = Style {
        background: Background::Color(palette.background.base.color),
        border: Border {
            radius: BORDER_RADIUS.into(),
            width: BORDER_WIDTH,
            color: palette.background.strong.color,
        },
        value: palette.background.base.text,
        selection: palette.primary.weak.color,
        line_number: palette.secondary.base.color,
        current_line_number: palette.background.base.text,
        rail: Background::Color(palette.background.weak.color),
        scroller: Background::Color(palette.background.strongest.color),
    };

    match status {
        Status::Active => active,
        Status::Hovered => Style {
            border: Border {
                color: palette.background.base.text,
                ..active.border
            },
            ..active
        },
        Status::Focused { .. } => Style {
            border: Border {
                color: palette.primary.strong.color,
                ..active.border
            },
            ..active
        },
        Status::Disabled => Style {
            background: Background::Color(palette.background.weak.color),
            value: palette.secondary.base.color,
            ..active
        },
    }
}
