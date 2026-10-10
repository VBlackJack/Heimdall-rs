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

//! The remote desktop of a tab, whatever protocol draws it: its pixels, and what the user
//! does on it, translated for the protocol behind it.

use std::fmt;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use heimdall_core::profile::DesktopSizing;
use heimdall_rdp::{
    LocalClipboard, MouseButton, MousePosition, Operation, Scancode, WheelRotations,
};
use heimdall_remote::vnc::{Quality, VncInput};
use tokio::sync::{mpsc, watch};
use zeroize::Zeroizing;

/// The pixels of a desktop.
#[derive(Clone)]
pub enum DesktopFramebuffer {
    /// Decoded by an RDP session.
    Rdp(heimdall_rdp::Framebuffer),
    /// Decoded by a VNC session.
    Vnc(heimdall_remote::vnc::Framebuffer),
}

impl fmt::Debug for DesktopFramebuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (width, height) = self.read(|width, height, _| (width, height));
        write!(f, "DesktopFramebuffer({width}x{height})")
    }
}

impl DesktopFramebuffer {
    /// Calls `read` with the width, height and RGBA pixels, rows top to bottom.
    pub fn read<T>(&self, read: impl FnOnce(u16, u16, &[u8]) -> T) -> T {
        match self {
            Self::Rdp(framebuffer) => framebuffer.read(read),
            Self::Vnc(framebuffer) => framebuffer.read(read),
        }
    }
}

/// A pointer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    /// Left.
    Left,
    /// Middle, or the wheel pressed.
    Middle,
    /// Right.
    Right,
    /// Back, the first side button.
    Back,
    /// Forward, the second side button.
    Forward,
}

/// What the user did on a desktop, before any protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopInput {
    /// The pointer moved to this pixel of the desktop.
    Move {
        /// Column.
        x: u16,
        /// Row.
        y: u16,
    },
    /// A button went down or up, the pointer at this pixel.
    Button {
        /// Which.
        button: PointerButton,
        /// Down, or up.
        pressed: bool,
        /// Column.
        x: u16,
        /// Row.
        y: u16,
    },
    /// The wheel turned, in units of 120 a notch: positive away from the user, or right.
    Wheel {
        /// Up and down, or left and right.
        vertical: bool,
        /// How far.
        units: i16,
    },
    /// A key went down or up. The view gives what it knows of it: where it is (a PC/AT set 1
    /// scancode) and what it types (an X11 keysym).
    Key {
        /// By its position, whatever its label.
        scancode: Option<Scancode>,
        /// By what it types.
        keysym: Option<u32>,
        /// Down, or up.
        pressed: bool,
    },
}

/// A key combination sent to a remote desktop from the session's menu, as the C# Heimdall's
/// "Send keys to remote": those this computer keeps for itself when typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialKeys {
    /// Ctrl+Alt+Del: the secure attention sequence, for the logon screen.
    CtrlAltDel,
    /// The Windows key alone.
    Windows,
    /// Alt+Tab.
    AltTab,
    /// Ctrl+Esc: the Start menu.
    CtrlEsc,
    /// Print Screen.
    PrintScreen,
    /// Escape.
    Escape,
    /// F11, which this computer keeps for its own full screen.
    F11,
    /// Win+L: lock the workstation.
    WinL,
    /// Win+D: show the desktop.
    WinD,
    /// Win+E: the file explorer.
    WinE,
}

/// A key as both protocols name it: its set 1 scancode (extended, value) and its X11 keysym.
type KeyNames = ((bool, u8), u32);

const CONTROL: KeyNames = ((false, 0x1D), 0xFFE3);
const ALT: KeyNames = ((false, 0x38), 0xFFE9);
const SUPER: KeyNames = ((true, 0x5B), 0xFFEB);
const DELETE: KeyNames = ((true, 0x53), 0xFFFF);
const TAB: KeyNames = ((false, 0x0F), 0xFF09);
const ESCAPE: KeyNames = ((false, 0x01), 0xFF1B);
const PRINT: KeyNames = ((true, 0x37), 0xFF61);
const LETTER_L: KeyNames = ((false, 0x26), 0x006C);
const LETTER_D: KeyNames = ((false, 0x20), 0x0064);
const LETTER_E: KeyNames = ((false, 0x12), 0x0065);
const F11: KeyNames = ((false, 0x57), 0xFFC8);
const SHIFT: KeyNames = ((false, 0x2A), 0xFFE1);

impl SpecialKeys {
    /// Every combination, in the C# menu's order.
    pub const ALL: [Self; 10] = [
        Self::CtrlAltDel,
        Self::Windows,
        Self::AltTab,
        Self::CtrlEsc,
        Self::PrintScreen,
        Self::Escape,
        Self::F11,
        Self::WinL,
        Self::WinD,
        Self::WinE,
    ];

    /// The keys pressed, the last one being the key the modifiers before it hold.
    fn keys(self) -> &'static [KeyNames] {
        match self {
            Self::CtrlAltDel => &[CONTROL, ALT, DELETE],
            Self::Windows => &[SUPER],
            Self::AltTab => &[ALT, TAB],
            Self::CtrlEsc => &[CONTROL, ESCAPE],
            Self::Escape => &[ESCAPE],
            Self::PrintScreen => &[PRINT],
            Self::F11 => &[F11],
            Self::WinL => &[SUPER, LETTER_L],
            Self::WinD => &[SUPER, LETTER_D],
            Self::WinE => &[SUPER, LETTER_E],
        }
    }

    /// What typing the combination sends: every key down in order, then up in reverse, so
    /// no modifier is left held on the remote side.
    #[must_use]
    pub fn inputs(self) -> Vec<DesktopInput> {
        typed(self.keys())
    }
}

/// What an anti-idle tick sends, as the C# one: Shift pressed and released, which the server
/// counts as input and the desktop shows nothing of.
#[must_use]
pub fn anti_idle_inputs() -> Vec<DesktopInput> {
    typed(&[SHIFT])
}

/// `keys` down in order, then up in reverse.
fn typed(keys: &[KeyNames]) -> Vec<DesktopInput> {
    let key = |((extended, value), keysym): KeyNames, pressed| DesktopInput::Key {
        scancode: Some(Scancode::from_u8(extended, value)),
        keysym: Some(keysym),
        pressed,
    };
    keys.iter()
        .map(|names| key(*names, true))
        .chain(keys.iter().rev().map(|names| key(*names, false)))
        .collect()
}

/// Where the input of a desktop goes.
enum DesktopSink {
    Rdp {
        input: mpsc::UnboundedSender<Vec<Operation>>,
        size: watch::Sender<Option<(u16, u16)>>,
        /// Which of the tab's sizes the server is asked for.
        sizing: DesktopSizing,
        /// The tab's last size, kept to be asked again when the user goes back to it.
        tab: std::sync::Mutex<Option<(u16, u16)>>,
        /// Until when the session settles after connecting, as the C# post-connect
        /// stabilization: sizes are kept, not asked, the first one excepted.
        stabilizing_until: Option<Instant>,
    },
    Vnc(VncSink),
}

/// A VNC session's input, and the pointer state VNC reports whole with every event.
struct VncSink {
    input: VncInput,
    /// Buttons held, as the RFB mask.
    buttons: AtomicU8,
    /// Last pointer position, column in the high half.
    position: AtomicU32,
    /// Watch only: nothing is sent.
    view_only: bool,
    /// The server is asked for the tab's size, as noVNC's remote resizing; off, the desktop
    /// keeps its own size, shown scaled.
    remote_resize: bool,
    /// The tab's last size, asked when remote resizing is turned on.
    tab: std::sync::Mutex<Option<(u16, u16)>>,
    /// The quality asked of the server, as the C# toolbar's "Quality" menu: this tab's
    /// only, never kept in its profile.
    quality: Quality,
}

/// The desktop of a tab, once its session is open.
pub struct DesktopPane {
    /// The proportions an RDP desktop following its tab keeps, as the C# "Match window"
    /// sub-menu: the tab's own, or a ratio fitted in it, letterboxed.
    pub aspect: Aspect,
    /// Its pixels.
    pub framebuffer: DesktopFramebuffer,
    /// Grows each time the desktop changes: tells the view to draw it again.
    pub generation: u64,
    sink: DesktopSink,
    /// Where this side's clipboard goes, when the clipboard is shared.
    clipboard: Option<mpsc::UnboundedSender<LocalClipboard>>,
    /// The session gets anti-idle keys: its profile asks for them and the user has not
    /// stopped them for this session.
    pub anti_idle: bool,
    /// A VNC desktop's name, as its server gives it, made safe; shown on the session bar.
    pub desktop_name: Option<String>,
    /// The TLS version a VNC session is encrypted with, as "TLS 1.3"; `None` in clear. Said
    /// on the session bar.
    pub tls: Option<&'static str>,
    /// The server's clipboard holds files to save here.
    remote_files: bool,
    /// Saving the server's files, from the folder asked for until it ends.
    save: Option<SaveState>,
}

/// The proportions of a desktop that follows its tab, kept with its profile.
pub use heimdall_core::profile::Aspect;

/// Where saving an RDP server's copied files is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveState {
    /// The folder is asked for.
    Picking,
    /// The files are fetched: this many entries of all saved so far, the total once known.
    Running {
        /// Entries saved.
        saved: usize,
        /// Entries in the copy; 0 until known.
        total: usize,
    },
}

impl fmt::Debug for DesktopPane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DesktopPane")
            .field("framebuffer", &self.framebuffer)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl DesktopPane {
    /// The desktop of an RDP session.
    pub(crate) fn rdp(
        framebuffer: heimdall_rdp::Framebuffer,
        input: mpsc::UnboundedSender<Vec<Operation>>,
        (size, sizing): (watch::Sender<Option<(u16, u16)>>, DesktopSizing),
        clipboard: Option<mpsc::UnboundedSender<LocalClipboard>>,
    ) -> Self {
        Self {
            aspect: Aspect::Stretch,
            framebuffer: DesktopFramebuffer::Rdp(framebuffer),
            generation: 0,
            sink: DesktopSink::Rdp {
                input,
                size,
                sizing,
                tab: std::sync::Mutex::new(None),
                stabilizing_until: None,
            },
            clipboard,
            anti_idle: false,
            desktop_name: None,
            tls: None,
            remote_files: false,
            save: None,
        }
    }

    /// The size the tab shows the desktop at, in pixels: the RDP session asks the server for
    /// it once it settles, as its profile's sizing allows. VNC keeps the server's size.
    pub(crate) fn resize(&self, width: u16, height: u16) {
        if let DesktopSink::Vnc(sink) = &self.sink {
            *sink
                .tab
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((width, height));
            if sink.remote_resize {
                let _ = sink.input.resize(width, height);
            }
        }
        if let DesktopSink::Rdp { tab, .. } = &self.sink {
            *tab.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((width, height));
        }
        if self.asks_tab_size()
            && let DesktopSink::Rdp {
                size,
                stabilizing_until,
                ..
            } = &self.sink
        {
            // The first size the session gets is the tab's, whatever the wait: it replaces
            // the one it connected with, as the C# control starts at its own size. Only the
            // later ones wait for the session to settle.
            if stabilizing_until.is_some() && size.borrow().is_some() {
                return;
            }
            size.send_replace(Some(self.aspect.fit((width, height))));
        }
    }

    /// Starts the wait after connecting, as the C# post-connect stabilization: for `delay`
    /// from `now`, the tab's sizes and the Resolution menu's choices are kept, then the
    /// latest is asked once. Only for a desktop following its tab, as the C# only with
    /// dynamic resolution; a `delay` of zero waits for nothing.
    ///
    /// Not ported from the C#: the DPI change it drops during the wait and forces after it
    /// (a session here keeps the scale it connected with), and its full screen re-trigger,
    /// an `ActiveX` control's.
    pub(crate) fn stabilize(&mut self, delay: Duration, now: Instant) {
        if let DesktopSink::Rdp {
            sizing: DesktopSizing::FollowsTab,
            stabilizing_until,
            ..
        } = &mut self.sink
        {
            *stabilizing_until = (!delay.is_zero()).then(|| now + delay);
        }
    }

    /// Until when the session settles after connecting, while it does.
    #[must_use]
    pub fn stabilizing_until(&self) -> Option<Instant> {
        match &self.sink {
            DesktopSink::Rdp {
                stabilizing_until, ..
            } => *stabilizing_until,
            DesktopSink::Vnc(_) => None,
        }
    }

    /// Whole seconds left of the wait after connecting at `now`, rounded up as the C#
    /// countdown; `None` once over.
    #[must_use]
    pub fn stabilization_seconds_left(&self, now: Instant) -> Option<u64> {
        let left = self.stabilizing_until()?.checked_duration_since(now)?;
        let seconds = left.as_secs() + u64::from(left.subsec_nanos() > 0);
        (seconds > 0).then_some(seconds)
    }

    /// Ends the wait after connecting when it is over at `now`; whether it ended.
    pub(crate) fn settle(&mut self, now: Instant) -> bool {
        if self.stabilizing_until().is_some_and(|until| until <= now) {
            self.end_stabilization()
        } else {
            false
        }
    }

    /// Ends the wait after connecting now, its time over or skipped from the Resolution menu:
    /// the size the desktop is to have is asked when it is not the one asked last, as the C#
    /// `RdpStabilizationResumePolicy`. Whether there was a wait to end.
    pub(crate) fn end_stabilization(&mut self) -> bool {
        let aspect = self.aspect;
        let DesktopSink::Rdp {
            size,
            sizing,
            tab,
            stabilizing_until,
            ..
        } = &mut self.sink
        else {
            return false;
        };
        if stabilizing_until.take().is_none() {
            return false;
        }
        let wanted = match *sizing {
            DesktopSizing::Fixed { width, height } => Some((width, height)),
            DesktopSizing::FollowsTab => tab
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .map(|shown| aspect.fit(shown)),
            DesktopSizing::TabSizeOnce => None,
        };
        if let Some(wanted) = wanted
            && *size.borrow() != Some(wanted)
        {
            log::info!(
                "RDP desktop settled: {}x{} asked after the wait",
                wanted.0,
                wanted.1
            );
            size.send_replace(Some(wanted));
        }
        true
    }

    /// The size the tab shows the desktop in, kept without asking the server for it: the
    /// tab's size is known whatever the desktop's own, to scale a larger one and to go back
    /// to the tab's.
    pub(crate) fn shown_at(&self, width: u16, height: u16) {
        let tab = match &self.sink {
            DesktopSink::Rdp { tab, .. } => tab,
            DesktopSink::Vnc(sink) => &sink.tab,
        };
        *tab.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((width, height));
    }

    /// Whether a VNC desktop is resized to its tab, as the C# "Remote resizing"; `None` for
    /// a desktop that cannot be: an RDP one, a VNC one watched only, or one whose server
    /// does not take a size asked of it.
    #[must_use]
    pub fn vnc_remote_resize(&self) -> Option<bool> {
        match &self.sink {
            DesktopSink::Vnc(sink) if !sink.view_only && sink.input.can_resize() => {
                Some(sink.remote_resize)
            }
            _ => None,
        }
    }

    /// Turns a VNC desktop's remote resizing on or off; on, the tab's size is asked now.
    pub(crate) fn set_vnc_remote_resize(&mut self, on: bool) {
        let DesktopSink::Vnc(sink) = &mut self.sink else {
            return;
        };
        sink.remote_resize = on;
        let last = *sink
            .tab
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if on && let Some((width, height)) = last {
            let _ = sink.input.resize(width, height);
        }
    }

    /// The quality a VNC desktop is asked at, as the C# toolbar's "Quality" menu; `None` for
    /// an RDP desktop.
    #[must_use]
    pub fn vnc_quality(&self) -> Option<Quality> {
        match &self.sink {
            DesktopSink::Vnc(sink) => Some(sink.quality),
            DesktopSink::Rdp { .. } => None,
        }
    }

    /// Asks a VNC desktop's server for pictures at `quality`: its levels, then the whole
    /// desktop anew. A desktop watched only is asked too: it still receives pictures.
    pub(crate) fn set_vnc_quality(&mut self, quality: Quality) {
        let DesktopSink::Vnc(sink) = &mut self.sink else {
            return;
        };
        sink.quality = quality;
        let _ = sink.input.set_quality(quality);
    }

    /// The size the user chose from the tab's menu, as the C# "Resolution" one: a size of
    /// its own, asked of the server now and kept; or `None`, the tab's size again, followed
    /// from then on.
    /// While the session settles after connecting, the choice is kept and asked once the
    /// wait ends, as the C# defers the menu's choices then.
    pub(crate) fn choose_size(&mut self, chosen: Option<(u16, u16)>) {
        let DesktopSink::Rdp {
            size,
            sizing,
            tab,
            stabilizing_until,
            ..
        } = &mut self.sink
        else {
            return;
        };
        let held = stabilizing_until.is_some();
        if let Some((width, height)) = chosen {
            *sizing = DesktopSizing::Fixed { width, height };
            if !held {
                size.send_replace(Some((width, height)));
            }
        } else {
            *sizing = DesktopSizing::FollowsTab;
            let last = *tab
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(last) = last.filter(|_| !held) {
                size.send_replace(Some(self.aspect.fit(last)));
            }
        }
    }

    /// The size the RDP desktop keeps, whatever the tab's: `None` when it follows the tab.
    #[must_use]
    pub fn fixed_size(&self) -> Option<(u16, u16)> {
        match &self.sink {
            DesktopSink::Rdp {
                sizing: DesktopSizing::Fixed { width, height },
                ..
            } => Some((*width, *height)),
            _ => None,
        }
    }

    /// The tab's last size, in pixels.
    #[must_use]
    pub fn tab_size(&self) -> Option<(u16, u16)> {
        match &self.sink {
            DesktopSink::Rdp { tab, .. } => *tab
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            DesktopSink::Vnc(sink) => *sink
                .tab
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        }
    }

    /// Whether the tab's size is still to be asked of the server even while the desktop is
    /// shown scaled: a profile without dynamic resolution gets it once.
    #[must_use]
    pub fn wants_first_size(&self) -> bool {
        matches!(
            &self.sink,
            DesktopSink::Rdp { size, sizing: DesktopSizing::TabSizeOnce, .. }
                if size.borrow().is_none()
        )
    }

    /// Whether a size the tab reports is asked of the server now: never for a fixed RDP
    /// desktop nor a VNC one, which keep their own.
    #[must_use]
    pub fn asks_tab_size(&self) -> bool {
        match &self.sink {
            DesktopSink::Rdp { sizing, .. } => match sizing {
                DesktopSizing::FollowsTab => true,
                DesktopSizing::TabSizeOnce => self.wants_first_size(),
                DesktopSizing::Fixed { .. } => false,
            },
            DesktopSink::Vnc(sink) => sink.remote_resize,
        }
    }

    /// Whether the desktop keeps a size of its own the tab does not change: drawn centred
    /// when shown pixel for pixel.
    #[must_use]
    pub fn has_fixed_size(&self) -> bool {
        matches!(
            &self.sink,
            DesktopSink::Rdp {
                sizing: DesktopSizing::Fixed { .. },
                ..
            }
        )
    }

    /// Whether this desktop shares the clipboard with its server by itself: this side's
    /// clipboard offered each time the tab is shown. RDP, when its profile shares it.
    #[must_use]
    pub fn shares_clipboard(&self) -> bool {
        self.clipboard.is_some()
    }

    /// Whether this side's clipboard can be sent to the server when the user asks: a
    /// shared RDP clipboard, or a VNC desktop not only watched. VNC carries it in clear,
    /// so it goes on a click only, as the C# Heimdall's noVNC sync does, never by itself.
    #[must_use]
    pub fn accepts_clipboard(&self) -> bool {
        match &self.sink {
            DesktopSink::Rdp { .. } => self.shares_clipboard(),
            DesktopSink::Vnc(sink) => !sink.view_only,
        }
    }

    /// Offers `text`, this side's clipboard, to the server; whether it could be. Asked only
    /// of a desktop that [accepts it](Self::accepts_clipboard).
    pub(crate) fn offer_clipboard(&self, text: String) -> bool {
        // Read before a save began: offering would take the server's clipboard back.
        if self.save.is_some() {
            return false;
        }
        match &self.sink {
            DesktopSink::Rdp { .. } => self.clipboard.as_ref().is_some_and(|clipboard| {
                clipboard
                    .send(LocalClipboard::Text(Zeroizing::new(text)))
                    .is_ok()
            }),
            DesktopSink::Vnc(sink) => sink.input.cut_text(text).is_ok(),
        }
    }

    /// Offers `image`, this side's clipboard as a device-independent bitmap, to an RDP
    /// server sharing the clipboard; whether it could be. VNC carries text only.
    pub(crate) fn offer_image(&self, image: Vec<u8>) -> bool {
        // Read before a save began: offering would take the server's clipboard back.
        if self.save.is_some() {
            return false;
        }
        match &self.sink {
            DesktopSink::Rdp { .. } => self
                .clipboard
                .as_ref()
                .is_some_and(|clipboard| clipboard.send(LocalClipboard::Image(image)).is_ok()),
            DesktopSink::Vnc(_) => false,
        }
    }

    /// Whether the server's copied files can be saved now: an RDP desktop sharing the
    /// clipboard, files copied there, no save under way.
    #[must_use]
    pub fn can_save_files(&self) -> bool {
        self.clipboard.is_some() && self.remote_files && self.save.is_none()
    }

    /// Where saving the server's files is, while under way.
    #[must_use]
    pub fn save_state(&self) -> Option<SaveState> {
        self.save
    }

    /// The server's clipboard holds files to save, or no longer.
    pub(crate) fn set_remote_files(&mut self, available: bool) {
        self.remote_files = available;
    }

    /// The user asks to save the server's files: whether a folder is to be asked for.
    pub(crate) fn ask_save(&mut self) -> bool {
        if !self.can_save_files() {
            return false;
        }
        // The session drops what it was about to offer: the server's clipboard is kept.
        if let Some(clipboard) = &self.clipboard {
            let _ = clipboard.send(LocalClipboard::HoldOffers);
        }
        self.save = Some(SaveState::Picking);
        true
    }

    /// The folder picked for the server's files, `None` when the dialog was closed: the
    /// session saves them there.
    pub(crate) fn save_into(&mut self, folder: Option<std::path::PathBuf>) {
        if self.save != Some(SaveState::Picking) {
            return;
        }
        let Some(clipboard) = &self.clipboard else {
            self.save = None;
            return;
        };
        let sent = if let Some(folder) = folder {
            clipboard
                .send(LocalClipboard::SaveRemoteFiles(folder))
                .is_ok()
        } else {
            // The offers held are released.
            let _ = clipboard.send(LocalClipboard::CancelSave);
            false
        };
        self.save = sent.then_some(SaveState::Running { saved: 0, total: 0 });
    }

    /// The user stops saving the server's files: what was saved stays.
    pub(crate) fn cancel_save(&mut self) {
        match self.save {
            Some(SaveState::Picking | SaveState::Running { .. }) => {
                if let Some(clipboard) = &self.clipboard {
                    let _ = clipboard.send(LocalClipboard::CancelSave);
                }
                if self.save == Some(SaveState::Picking) {
                    self.save = None;
                }
            }
            None => {}
        }
    }

    /// How far saving the server's files is.
    pub(crate) fn save_progress(&mut self, saved: usize, total: usize) {
        if let Some(SaveState::Running { .. }) = self.save {
            self.save = Some(SaveState::Running { saved, total });
        }
    }

    /// Saving the server's files ended.
    pub(crate) fn save_ended(&mut self) {
        self.save = None;
    }

    /// Offers `paths`, the files copied in Explorer, to the server; whether they could be.
    /// Only an RDP desktop that [shares the clipboard](Self::shares_clipboard) takes them.
    pub(crate) fn offer_files(&self, paths: Vec<std::path::PathBuf>) -> bool {
        if self.save.is_some() {
            return false;
        }
        self.clipboard
            .as_ref()
            .is_some_and(|clipboard| clipboard.send(LocalClipboard::Files(paths)).is_ok())
    }

    /// The desktop of a VNC session; `view_only` sends it nothing.
    pub(crate) fn vnc(
        framebuffer: heimdall_remote::vnc::Framebuffer,
        input: VncInput,
        view_only: bool,
    ) -> Self {
        Self {
            aspect: Aspect::Stretch,
            framebuffer: DesktopFramebuffer::Vnc(framebuffer),
            generation: 0,
            // Not by itself: VNC carries text in clear, sent on a click only.
            clipboard: None,
            remote_files: false,
            save: None,
            sink: DesktopSink::Vnc(VncSink {
                input,
                buttons: AtomicU8::new(0),
                position: AtomicU32::new(0),
                view_only,
                remote_resize: false,
                tab: std::sync::Mutex::new(None),
                quality: Quality::default(),
            }),
            anti_idle: false,
            desktop_name: None,
            tls: None,
        }
    }

    /// Sends `inputs` to the session, in the protocol's terms: whether they went. A closed
    /// session drops its receiver: the input then goes nowhere, as it should.
    pub(crate) fn send(&self, inputs: &[DesktopInput]) -> bool {
        match &self.sink {
            DesktopSink::Rdp { input, .. } => {
                let operations = rdp_operations(inputs);
                operations.is_empty() || input.send(operations).is_ok()
            }
            DesktopSink::Vnc(sink) => sink.send(inputs),
        }
    }
}

/// Wheel units of one notch.
const WHEEL_NOTCH: i32 = 120;

/// RFB button bits: buttons 1 to 8 as bits 0 to 7.
const VNC_LEFT: u8 = 1;
const VNC_MIDDLE: u8 = 1 << 1;
const VNC_RIGHT: u8 = 1 << 2;
const VNC_WHEEL_UP: u8 = 1 << 3;
const VNC_WHEEL_DOWN: u8 = 1 << 4;
const VNC_WHEEL_LEFT: u8 = 1 << 5;
const VNC_WHEEL_RIGHT: u8 = 1 << 6;
const VNC_BACK: u8 = 1 << 7;

/// A button's RFB bit. Forward is button 9, past the 8 the mask holds: it has none.
fn vnc_button(button: PointerButton) -> Option<u8> {
    match button {
        PointerButton::Left => Some(VNC_LEFT),
        PointerButton::Middle => Some(VNC_MIDDLE),
        PointerButton::Right => Some(VNC_RIGHT),
        PointerButton::Back => Some(VNC_BACK),
        PointerButton::Forward => None,
    }
}

impl VncSink {
    /// Sends `inputs`. VNC reports the pointer whole each time: position and every button
    /// held. A wheel notch is a press and a release of buttons 4 to 7; a key goes by keysym,
    /// and one the view could not name is dropped. Nothing goes from a view-only session:
    /// whether they went.
    fn send(&self, inputs: &[DesktopInput]) -> bool {
        if self.view_only {
            return false;
        }
        for input in inputs {
            match *input {
                DesktopInput::Move { x, y } => self.pointer(x, y),
                DesktopInput::Button {
                    button,
                    pressed,
                    x,
                    y,
                } => {
                    if let Some(bit) = vnc_button(button) {
                        if pressed {
                            self.buttons.fetch_or(bit, Ordering::Relaxed);
                        } else {
                            self.buttons.fetch_and(!bit, Ordering::Relaxed);
                        }
                    }
                    self.pointer(x, y);
                }
                DesktopInput::Wheel { vertical, units } => self.wheel(vertical, units),
                DesktopInput::Key {
                    keysym: Some(keysym),
                    pressed,
                    ..
                } => {
                    let _ = self.input.key(keysym, pressed);
                }
                DesktopInput::Key { keysym: None, .. } => {}
            }
        }
        true
    }

    fn pointer(&self, x: u16, y: u16) {
        self.position
            .store((u32::from(x) << 16) | u32::from(y), Ordering::Relaxed);
        let _ = self
            .input
            .pointer(self.buttons.load(Ordering::Relaxed), x, y);
    }

    fn wheel(&self, vertical: bool, units: i16) {
        let bit = match (vertical, units > 0) {
            (true, true) => VNC_WHEEL_UP,
            (true, false) => VNC_WHEEL_DOWN,
            (false, true) => VNC_WHEEL_RIGHT,
            (false, false) => VNC_WHEEL_LEFT,
        };
        // Whole notches, and at least one for any turn.
        let notches = ((i32::from(units).abs() + WHEEL_NOTCH / 2) / WHEEL_NOTCH).max(1);
        let position = self.position.load(Ordering::Relaxed);
        let (x, y) = (
            u16::try_from(position >> 16).unwrap_or(0),
            u16::try_from(position & 0xffff).unwrap_or(0),
        );
        let held = self.buttons.load(Ordering::Relaxed);
        for _ in 0..notches {
            let _ = self.input.pointer(held | bit, x, y);
            let _ = self.input.pointer(held, x, y);
        }
    }
}

fn rdp_button(button: PointerButton) -> MouseButton {
    match button {
        PointerButton::Left => MouseButton::Left,
        PointerButton::Middle => MouseButton::Middle,
        PointerButton::Right => MouseButton::Right,
        PointerButton::Back => MouseButton::X1,
        PointerButton::Forward => MouseButton::X2,
    }
}

/// `inputs` as RDP operations. A key the view could not place gives none: RDP sends keys by
/// position.
fn rdp_operations(inputs: &[DesktopInput]) -> Vec<Operation> {
    let mut operations = Vec::with_capacity(inputs.len() * 2);
    for input in inputs {
        match *input {
            DesktopInput::Move { x, y } => {
                operations.push(Operation::MouseMove(MousePosition { x, y }));
            }
            DesktopInput::Button {
                button,
                pressed: true,
                x,
                y,
            } => {
                operations.push(Operation::MouseMove(MousePosition { x, y }));
                operations.push(Operation::MouseButtonPressed(rdp_button(button)));
            }
            DesktopInput::Button {
                button,
                pressed: false,
                ..
            } => operations.push(Operation::MouseButtonReleased(rdp_button(button))),
            DesktopInput::Wheel { vertical, units } => {
                operations.push(Operation::WheelRotations(WheelRotations {
                    is_vertical: vertical,
                    rotation_units: units,
                }));
            }
            DesktopInput::Key {
                scancode: Some(code),
                pressed,
                ..
            } => operations.push(if pressed {
                Operation::KeyPressed(code)
            } else {
                Operation::KeyReleased(code)
            }),
            DesktopInput::Key { scancode: None, .. } => {}
        }
    }
    operations
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The C# default wait after connecting.
    const WAIT: Duration = Duration::from_secs(10);

    /// An RDP desktop sized by `sizing`, and what its session is asked.
    fn rdp_pane(sizing: DesktopSizing) -> (DesktopPane, watch::Receiver<Option<(u16, u16)>>) {
        let (size, watched) = watch::channel(None);
        let (input, _received) = mpsc::unbounded_channel();
        let pane = DesktopPane::rdp(
            heimdall_rdp::Framebuffer::new(64, 48),
            input,
            (size, sizing),
            None,
        );
        (pane, watched)
    }

    #[test]
    fn the_first_size_replaces_the_connection_s_and_later_ones_wait_then_go_once() {
        let (mut pane, watched) = rdp_pane(DesktopSizing::FollowsTab);
        let start = Instant::now();
        pane.stabilize(WAIT, start);
        assert_eq!(pane.stabilizing_until(), Some(start + WAIT));

        pane.resize(1600, 900);
        assert_eq!(
            *watched.borrow(),
            Some((1600, 900)),
            "the size connected with is replaced at once"
        );
        pane.resize(1400, 850);
        pane.resize(1200, 800);
        assert_eq!(
            *watched.borrow(),
            Some((1600, 900)),
            "held while it settles"
        );

        assert!(!pane.settle(start + Duration::from_millis(9_999)));
        assert_eq!(*watched.borrow(), Some((1600, 900)), "not yet");
        assert!(pane.settle(start + WAIT), "over");
        assert_eq!(*watched.borrow(), Some((1200, 800)), "the latest, once");
        assert_eq!(pane.stabilizing_until(), None);
        pane.resize(1000, 700);
        assert_eq!(*watched.borrow(), Some((1000, 700)), "followed again");
    }

    #[test]
    fn a_wait_ending_on_the_size_asked_last_asks_nothing() {
        let (mut pane, mut watched) = rdp_pane(DesktopSizing::FollowsTab);
        let start = Instant::now();
        pane.stabilize(WAIT, start);
        pane.resize(1600, 900);
        pane.resize(1200, 800);
        pane.resize(1600, 900);
        watched.mark_unchanged();
        assert!(pane.end_stabilization());
        assert!(
            !watched.has_changed().expect("open"),
            "back at the size asked: nothing sent"
        );
        assert!(!pane.end_stabilization(), "nothing left to end");
    }

    #[test]
    fn skipped_the_latest_size_goes_at_once() {
        let (mut pane, watched) = rdp_pane(DesktopSizing::FollowsTab);
        pane.stabilize(WAIT, Instant::now());
        pane.resize(1600, 900);
        pane.resize(1280, 720);
        assert!(pane.end_stabilization(), "skipped");
        assert_eq!(*watched.borrow(), Some((1280, 720)));
        assert_eq!(pane.stabilization_seconds_left(Instant::now()), None);
    }

    #[test]
    fn a_menu_choice_made_while_it_settles_waits_too() {
        let (mut pane, watched) = rdp_pane(DesktopSizing::FollowsTab);
        let start = Instant::now();
        pane.stabilize(WAIT, start);
        pane.resize(1600, 900);
        pane.choose_size(Some((1920, 1080)));
        assert_eq!(pane.fixed_size(), Some((1920, 1080)), "chosen");
        assert_eq!(*watched.borrow(), Some((1600, 900)), "not asked yet");
        assert!(pane.settle(start + WAIT));
        assert_eq!(*watched.borrow(), Some((1920, 1080)), "asked at the end");

        // Back to the tab while it settles: the tab's last size at the end.
        let (mut pane, watched) = rdp_pane(DesktopSizing::FollowsTab);
        pane.stabilize(WAIT, start);
        pane.resize(1600, 900);
        pane.choose_size(Some((1920, 1080)));
        pane.resize(1500, 850);
        pane.choose_size(None);
        assert_eq!(*watched.borrow(), Some((1600, 900)));
        assert!(pane.end_stabilization());
        assert_eq!(*watched.borrow(), Some((1500, 850)));
    }

    #[test]
    fn a_desktop_not_following_its_tab_never_waits_nor_does_a_zero_wait() {
        let start = Instant::now();
        for sizing in [
            DesktopSizing::TabSizeOnce,
            DesktopSizing::Fixed {
                width: 1366,
                height: 768,
            },
        ] {
            let (mut pane, _watched) = rdp_pane(sizing);
            pane.stabilize(WAIT, start);
            assert_eq!(pane.stabilizing_until(), None, "{sizing:?}");
        }
        let (mut pane, watched) = rdp_pane(DesktopSizing::FollowsTab);
        pane.stabilize(Duration::ZERO, start);
        assert_eq!(pane.stabilizing_until(), None, "0 is off");
        pane.resize(1600, 900);
        pane.resize(1200, 800);
        assert_eq!(*watched.borrow(), Some((1200, 800)));
    }

    #[test]
    fn the_countdown_rounds_up_as_the_csharp_one() {
        let (mut pane, _watched) = rdp_pane(DesktopSizing::FollowsTab);
        let start = Instant::now();
        pane.stabilize(WAIT, start);
        assert_eq!(pane.stabilization_seconds_left(start), Some(10));
        assert_eq!(
            pane.stabilization_seconds_left(start + Duration::from_millis(100)),
            Some(10)
        );
        assert_eq!(
            pane.stabilization_seconds_left(start + Duration::from_millis(9_001)),
            Some(1)
        );
        assert_eq!(pane.stabilization_seconds_left(start + WAIT), None);
    }

    #[test]
    fn a_size_chosen_is_asked_and_kept_then_the_tab_s_asked_again_and_followed() {
        let (size, watched) = watch::channel(None);
        let (input, _received) = mpsc::unbounded_channel();
        let mut pane = DesktopPane::rdp(
            heimdall_rdp::Framebuffer::new(64, 48),
            input,
            (size, DesktopSizing::FollowsTab),
            None,
        );
        pane.resize(1200, 800);
        assert_eq!(*watched.borrow(), Some((1200, 800)));
        assert_eq!(pane.fixed_size(), None);

        pane.choose_size(Some((1920, 1080)));
        assert_eq!(*watched.borrow(), Some((1920, 1080)));
        assert_eq!(pane.fixed_size(), Some((1920, 1080)));
        pane.resize(1000, 700);
        assert_eq!(
            *watched.borrow(),
            Some((1920, 1080)),
            "kept whatever the tab's size"
        );
        assert_eq!(
            pane.tab_size(),
            Some((1000, 700)),
            "the tab's size is remembered"
        );

        pane.choose_size(None);
        assert_eq!(
            *watched.borrow(),
            Some((1000, 700)),
            "the tab's last size asked again"
        );
        pane.resize(900, 600);
        assert_eq!(*watched.borrow(), Some((900, 600)), "and followed");
    }

    #[test]
    fn a_press_moves_there_first_and_a_release_does_not() {
        let operations = rdp_operations(&[
            DesktopInput::Button {
                button: PointerButton::Back,
                pressed: true,
                x: 3,
                y: 4,
            },
            DesktopInput::Button {
                button: PointerButton::Back,
                pressed: false,
                x: 9,
                y: 9,
            },
        ]);
        assert!(
            matches!(
                operations.as_slice(),
                [
                    Operation::MouseMove(MousePosition { x: 3, y: 4 }),
                    Operation::MouseButtonPressed(MouseButton::X1),
                    Operation::MouseButtonReleased(MouseButton::X1),
                ]
            ),
            "{operations:?}"
        );
    }

    #[test]
    fn keys_go_by_scancode_and_one_without_is_dropped() {
        let enter = Scancode::from_u8(false, 0x1c);
        let operations = rdp_operations(&[
            DesktopInput::Key {
                scancode: Some(enter),
                keysym: Some(0xff0d),
                pressed: true,
            },
            DesktopInput::Key {
                scancode: None,
                keysym: Some(0x20ac),
                pressed: true,
            },
            DesktopInput::Key {
                scancode: Some(enter),
                keysym: None,
                pressed: false,
            },
        ]);
        assert!(
            matches!(
                operations.as_slice(),
                [Operation::KeyPressed(pressed), Operation::KeyReleased(released)]
                    if *pressed == enter && *released == enter
            ),
            "{operations:?}"
        );
    }

    #[test]
    fn the_menu_follows_the_cs_order_and_f11_goes_by_its_scancode() {
        assert_eq!(
            SpecialKeys::ALL,
            [
                SpecialKeys::CtrlAltDel,
                SpecialKeys::Windows,
                SpecialKeys::AltTab,
                SpecialKeys::CtrlEsc,
                SpecialKeys::PrintScreen,
                SpecialKeys::Escape,
                SpecialKeys::F11,
                SpecialKeys::WinL,
                SpecialKeys::WinD,
                SpecialKeys::WinE,
            ]
        );
        let f11 = Scancode::from_u8(false, 0x57);
        let operations = rdp_operations(&SpecialKeys::F11.inputs());
        assert!(
            matches!(
                operations.as_slice(),
                [Operation::KeyPressed(pressed), Operation::KeyReleased(released)]
                    if *pressed == f11 && *released == f11
            ),
            "{operations:?}"
        );
    }

    #[test]
    fn a_ratio_is_fitted_inside_the_tab_as_the_csharp_letterboxes_it() {
        assert_eq!(Aspect::Stretch.fit((1600, 1000)), (1600, 1000));
        assert_eq!(
            Aspect::Wide.fit((1600, 1000)),
            (1600, 900),
            "bars above and below"
        );
        assert_eq!(
            Aspect::Standard.fit((1600, 900)),
            (1200, 900),
            "bars at the sides"
        );
        assert_eq!(Aspect::UltraWide.fit((2100, 1200)), (2100, 900));
        // Never below what an RDP server takes.
        assert_eq!(Aspect::UltraWide.fit((210, 90)), (208, 200));
    }

    #[test]
    fn the_wheel_keeps_its_axis_and_units() {
        let operations = rdp_operations(&[DesktopInput::Wheel {
            vertical: false,
            units: -240,
        }]);
        assert!(
            matches!(
                operations.as_slice(),
                [Operation::WheelRotations(WheelRotations {
                    is_vertical: false,
                    rotation_units: -240
                })]
            ),
            "{operations:?}"
        );
    }
}
