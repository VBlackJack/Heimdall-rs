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

//! The keyboard of a context menu, as a WPF `ContextMenu` answers it.
//!
//! The C# menus are driven by the keyboard once open: Up and Down move over the entries,
//! skipping separators and disabled ones and wrapping at either end; Home and End jump to
//! the first and the last; Right opens a submenu, Left goes back to its parent; Enter and
//! Space choose; Escape closes (the window's own Escape does). The C# menus declare no
//! access keys, so none are offered here.
//!
//! The entries are buttons in a [`MenuColumn`](crate::menu_column), with no model of their
//! own. [`MenuItem`] wraps one and reports itself to [`MenuNav`], which wraps the whole menu,
//! through a widget operation; [`MenuNav`] keeps the entry that has the keyboard and asks it
//! to draw the focus and, on Enter, to press itself. [`step`] is the pure part: which entry
//! a key leads to.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay, renderer};
use iced::keyboard::{self, key::Named};
use iced::{Border, Element, Event, Length, Point, Rectangle, Size, Theme, Vector, window};
use std::hash::{Hash, Hasher};

/// How far from an entry's left edge the second probe for its clickable part lies: a box
/// with a label (a filter entry) is clickable only where it is, not across its row.
const PROBE_INSET: f32 = 20.0;

/// Width of the ring drawn around the entry that has the keyboard.
const FOCUS_BORDER: f32 = 1.5;

/// Corner radius of the focus ring, as the entries' own highlight.
const FOCUS_RADIUS: f32 = 4.0;

/// What an entry does when it is chosen, for the keys that tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// It acts, or toggles.
    Plain,
    /// It opens another menu.
    Submenu,
}

/// An entry of a menu as the keyboard sees it; separators are not entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// It can be chosen: a disabled entry is passed over.
    pub enabled: bool,
    /// What choosing it does.
    pub kind: Kind,
}

/// The keys a menu answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavKey {
    /// Down.
    Next,
    /// Up.
    Previous,
    /// Home.
    First,
    /// End.
    Last,
    /// Enter or Space.
    Choose,
    /// Right.
    Open,
    /// Left.
    Back,
}

/// What a key leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The keyboard moves to this entry.
    Focus(usize),
    /// This entry is chosen.
    Activate(usize),
    /// The submenu goes back to its parent.
    Back,
    /// Nothing changes.
    Stay,
}

/// What `key` leads to in a menu of `rows` whose entry `focus` has the keyboard.
#[must_use]
pub fn step(rows: &[Row], focus: Option<usize>, key: NavKey) -> Step {
    let enabled = |index: &usize| rows.get(*index).is_some_and(|row| row.enabled);
    let focus = focus.filter(|index| *index < rows.len());
    match key {
        NavKey::First => (0..rows.len())
            .find(enabled)
            .map_or(Step::Stay, Step::Focus),
        NavKey::Last => (0..rows.len())
            .rev()
            .find(enabled)
            .map_or(Step::Stay, Step::Focus),
        // Wrapping, as a WPF menu: past the last is the first, before the first the last.
        NavKey::Next => {
            let from = focus.map_or(0, |index| index + 1);
            (0..rows.len())
                .map(|offset| (from + offset) % rows.len())
                .find(enabled)
                .map_or(Step::Stay, Step::Focus)
        }
        NavKey::Previous => {
            let from = focus.unwrap_or(0);
            (1..=rows.len())
                .map(|offset| (from + rows.len() - offset % rows.len()) % rows.len())
                .find(enabled)
                .map_or(Step::Stay, Step::Focus)
        }
        NavKey::Choose => focus.filter(enabled).map_or(Step::Stay, Step::Activate),
        NavKey::Open => focus
            .filter(enabled)
            .filter(|index| rows[*index].kind == Kind::Submenu)
            .map_or(Step::Stay, Step::Activate),
        NavKey::Back => Step::Back,
    }
}

/// The menu key `key` is, unmodified.
fn nav_key(key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Option<NavKey> {
    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return None;
    }
    match key {
        keyboard::Key::Named(Named::ArrowDown) => Some(NavKey::Next),
        keyboard::Key::Named(Named::ArrowUp) => Some(NavKey::Previous),
        keyboard::Key::Named(Named::Home) => Some(NavKey::First),
        keyboard::Key::Named(Named::End) => Some(NavKey::Last),
        keyboard::Key::Named(Named::ArrowRight) => Some(NavKey::Open),
        keyboard::Key::Named(Named::ArrowLeft) => Some(NavKey::Back),
        keyboard::Key::Named(Named::Enter | Named::Space) => Some(NavKey::Choose),
        _ => None,
    }
}

/// A token telling two menus apart, so a new menu starts without a focus.
#[must_use]
pub fn menu_token(menu: &impl std::fmt::Debug) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{menu:?}").hash(&mut hasher);
    hasher.finish()
}

/// What an entry keeps between two frames.
#[derive(Debug)]
struct ItemState {
    /// What choosing it does.
    kind: Kind,
    /// The keyboard is on it.
    focused: bool,
    /// It was asked to press itself at the next event.
    activate: bool,
    /// Where it can be clicked, `None` while it cannot be chosen.
    hit: Option<Point>,
}

impl ItemState {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            focused: false,
            activate: false,
            hit: None,
        }
    }
}

/// One entry of a menu, the button it wraps reporting to the [`MenuNav`] around it.
pub struct MenuItem<'a, Message> {
    content: Element<'a, Message>,
    kind: Kind,
}

/// A menu entry `content`, a button, that acts.
#[must_use]
pub fn item<'a, Message>(content: impl Into<Element<'a, Message>>) -> MenuItem<'a, Message> {
    MenuItem {
        content: content.into(),
        kind: Kind::Plain,
    }
}

/// A menu entry `content`, a button, that opens another menu.
#[must_use]
pub fn submenu_item<'a, Message>(
    content: impl Into<Element<'a, Message>>,
) -> MenuItem<'a, Message> {
    MenuItem {
        content: content.into(),
        kind: Kind::Submenu,
    }
}

impl<Message> MenuItem<'_, Message> {
    /// Where `layout` can be clicked, if anywhere: its centre, else near its left edge.
    fn hit(&self, tree: &Tree, layout: Layout<'_>, renderer: &iced::Renderer) -> Option<Point> {
        let bounds = layout.bounds();
        let left = Point::new(
            (bounds.x + PROBE_INSET).min(bounds.center_x()),
            bounds.center_y(),
        );
        [bounds.center(), left].into_iter().find(|point| {
            self.content.as_widget().mouse_interaction(
                &tree.children[0],
                layout,
                mouse::Cursor::Available(*point),
                &bounds,
                renderer,
            ) == mouse::Interaction::Pointer
        })
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for MenuItem<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ItemState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ItemState::new(self.kind))
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        let hit = self.hit(tree, layout, renderer);
        let state = tree.state.downcast_mut::<ItemState>();
        state.hit = hit;
        state.kind = self.kind;
        operation.custom(None, layout.bounds(), state);
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let hit = {
            let state = tree.state.downcast_mut::<ItemState>();
            if std::mem::take(&mut state.activate) {
                state.hit
            } else {
                None
            }
        };
        if let Some(point) = hit {
            // The entry's own press, as a click on it: whatever the button does for the
            // mouse, it does for Enter.
            let at = mouse::Cursor::Available(point);
            for press in [
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                self.content.as_widget_mut().update(
                    &mut tree.children[0],
                    &Event::Mouse(press),
                    layout,
                    at,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if tree.state.downcast_ref::<ItemState>().focused {
            // The hover colours of the theme, ringed, so the entry the keyboard is on shows
            // in every theme as the one under the pointer does, and more.
            let palette = theme.extended_palette();
            iced::advanced::Renderer::fill_quad(
                renderer,
                renderer::Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        color: palette.primary.strong.color,
                        width: FOCUS_BORDER,
                        radius: FOCUS_RADIUS.into(),
                    },
                    ..renderer::Quad::default()
                },
                palette.primary.weak.color,
            );
        }
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: 'a> From<MenuItem<'a, Message>> for Element<'a, Message> {
    fn from(item: MenuItem<'a, Message>) -> Self {
        Element::new(item)
    }
}

/// Lists the entries in order.
struct Collect {
    rows: Vec<Row>,
}

impl Operation for Collect {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn custom(
        &mut self,
        _id: Option<&iced::advanced::widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn std::any::Any,
    ) {
        if let Some(seen) = state.downcast_mut::<ItemState>() {
            self.rows.push(Row {
                enabled: seen.hit.is_some(),
                kind: seen.kind,
            });
        }
    }
}

/// Puts the keyboard on one entry, and presses it when asked.
struct Apply {
    focus: Option<usize>,
    press: bool,
    index: usize,
}

impl Operation for Apply {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn custom(
        &mut self,
        _id: Option<&iced::advanced::widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn std::any::Any,
    ) {
        if let Some(seen) = state.downcast_mut::<ItemState>() {
            let here = self.focus == Some(self.index);
            seen.focused = here;
            seen.activate = here && self.press;
            self.index += 1;
        }
    }
}

/// What a menu keeps between two frames.
#[derive(Debug, Default)]
struct NavState {
    /// The entry the keyboard is on.
    focus: Option<usize>,
    /// Which menu this is, to start a new one without a focus.
    token: u64,
    /// A key moved the focus, so the next menu to open (a submenu) starts on its first entry.
    keyboard: bool,
    /// The first entry gets the keyboard at the next redraw, once it is laid out.
    first_pending: bool,
}

/// A menu whose entries ([`MenuItem`]) the keyboard drives; see the module. `back` is what
/// Left sends in a submenu.
pub struct MenuNav<'a, Message> {
    content: Element<'a, Message>,
    token: u64,
    back: Option<Message>,
}

/// `content` driven by the keyboard; `token` tells this menu from another (see
/// [`menu_token`]), `back` is what Left sends, `None` in a menu with no parent.
#[must_use]
pub fn menu_nav<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    token: u64,
    back: Option<Message>,
) -> MenuNav<'a, Message> {
    MenuNav {
        content: content.into(),
        token,
        back,
    }
}

impl<Message: Clone> Widget<Message, Theme, iced::Renderer> for MenuNav<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<NavState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(NavState {
            token: self.token,
            ..NavState::default()
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<NavState>();
        if state.token != self.token {
            // Another menu: a submenu opened from the keyboard starts on its first entry,
            // any other on none.
            state.token = self.token;
            state.focus = None;
            state.first_pending = state.keyboard;
        }
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let key = match event {
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                nav_key(key, *modifiers)
            }
            _ => None,
        };
        let redraw = matches!(event, Event::Window(window::Event::RedrawRequested(_)));
        let first_pending = tree.state.downcast_ref::<NavState>().first_pending;
        if key.is_some() || (redraw && first_pending) {
            let mut rows = Collect { rows: Vec::new() };
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                &mut rows,
            );
            let state = tree.state.downcast_mut::<NavState>();
            state.first_pending = false;
            let outcome = match key {
                Some(key) => step(&rows.rows, state.focus, key),
                None => step(&rows.rows, None, NavKey::First),
            };
            let (focus, press) = match outcome {
                Step::Focus(index) => (Some(index), false),
                Step::Activate(index) => (Some(index), true),
                Step::Back => {
                    if let Some(back) = &self.back {
                        shell.publish(back.clone());
                    }
                    (state.focus, false)
                }
                Step::Stay => (state.focus, false),
            };
            state.focus = focus;
            state.keyboard = true;
            let mut apply = Apply {
                focus,
                press,
                index: 0,
            };
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                &mut apply,
            );
            if key.is_some() {
                // The menu has the keys while it is open: the tree under it does not move.
                shell.capture_event();
            }
            shell.request_redraw();
            if key.is_some() {
                return;
            }
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: Clone + 'a> From<MenuNav<'a, Message>> for Element<'a, Message> {
    fn from(nav: MenuNav<'a, Message>) -> Self {
        Element::new(nav)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(enabled: bool) -> Row {
        Row {
            enabled,
            kind: Kind::Plain,
        }
    }

    fn sub() -> Row {
        Row {
            enabled: true,
            kind: Kind::Submenu,
        }
    }

    #[test]
    fn down_from_nothing_lands_on_the_first_enabled_entry() {
        let rows = [plain(false), plain(true), plain(true)];
        assert_eq!(step(&rows, None, NavKey::Next), Step::Focus(1));
    }

    #[test]
    fn up_from_nothing_lands_on_the_last_enabled_entry() {
        let rows = [plain(true), plain(true), plain(false)];
        assert_eq!(step(&rows, None, NavKey::Previous), Step::Focus(1));
    }

    #[test]
    fn down_and_up_skip_disabled_entries() {
        // Separators are not entries: they are skipped by never being listed.
        let rows = [plain(true), plain(false), plain(false), plain(true)];
        assert_eq!(step(&rows, Some(0), NavKey::Next), Step::Focus(3));
        assert_eq!(step(&rows, Some(3), NavKey::Previous), Step::Focus(0));
    }

    #[test]
    fn navigation_wraps_as_a_wpf_menu() {
        let rows = [plain(true), plain(true), plain(true)];
        assert_eq!(step(&rows, Some(2), NavKey::Next), Step::Focus(0));
        assert_eq!(step(&rows, Some(0), NavKey::Previous), Step::Focus(2));
    }

    #[test]
    fn a_single_entry_wraps_onto_itself() {
        let rows = [plain(true)];
        assert_eq!(step(&rows, Some(0), NavKey::Next), Step::Focus(0));
        assert_eq!(step(&rows, Some(0), NavKey::Previous), Step::Focus(0));
    }

    #[test]
    fn home_and_end_jump_over_disabled_ends() {
        let rows = [plain(false), plain(true), plain(true), plain(false)];
        assert_eq!(step(&rows, Some(2), NavKey::First), Step::Focus(1));
        assert_eq!(step(&rows, Some(1), NavKey::Last), Step::Focus(2));
    }

    #[test]
    fn a_menu_with_nothing_enabled_stays_put() {
        let rows = [plain(false), plain(false)];
        for key in [NavKey::Next, NavKey::Previous, NavKey::First, NavKey::Last] {
            assert_eq!(step(&rows, None, key), Step::Stay);
        }
        assert_eq!(step(&[], None, NavKey::Next), Step::Stay);
        assert_eq!(step(&[], Some(0), NavKey::Previous), Step::Stay);
    }

    #[test]
    fn enter_chooses_the_focused_entry_only_when_it_can_be_chosen() {
        let rows = [plain(true), plain(false)];
        assert_eq!(step(&rows, Some(0), NavKey::Choose), Step::Activate(0));
        assert_eq!(step(&rows, Some(1), NavKey::Choose), Step::Stay);
        assert_eq!(step(&rows, None, NavKey::Choose), Step::Stay);
    }

    #[test]
    fn right_opens_a_submenu_and_only_a_submenu() {
        let rows = [plain(true), sub()];
        assert_eq!(step(&rows, Some(1), NavKey::Open), Step::Activate(1));
        assert_eq!(step(&rows, Some(0), NavKey::Open), Step::Stay);
        assert_eq!(step(&rows, None, NavKey::Open), Step::Stay);
    }

    #[test]
    fn left_goes_back_to_the_parent() {
        assert_eq!(step(&[sub()], Some(0), NavKey::Back), Step::Back);
        assert_eq!(step(&[], None, NavKey::Back), Step::Back);
    }

    #[test]
    fn a_focus_past_the_end_is_forgotten() {
        let rows = [plain(true), plain(true)];
        assert_eq!(step(&rows, Some(9), NavKey::Next), Step::Focus(0));
        assert_eq!(step(&rows, Some(9), NavKey::Choose), Step::Stay);
    }

    #[test]
    fn only_unmodified_menu_keys_are_the_menus() {
        let down = keyboard::Key::Named(Named::ArrowDown);
        assert_eq!(
            nav_key(&down, keyboard::Modifiers::empty()),
            Some(NavKey::Next)
        );
        assert_eq!(nav_key(&down, keyboard::Modifiers::CTRL), None);
        assert_eq!(
            nav_key(
                &keyboard::Key::Named(Named::Space),
                keyboard::Modifiers::empty()
            ),
            Some(NavKey::Choose)
        );
        assert_eq!(
            nav_key(
                &keyboard::Key::Named(Named::Escape),
                keyboard::Modifiers::empty()
            ),
            None
        );
    }

    #[test]
    fn two_menus_have_two_tokens() {
        assert_ne!(menu_token(&"a"), menu_token(&"b"));
        assert_eq!(menu_token(&"a"), menu_token(&"a"));
    }
}
