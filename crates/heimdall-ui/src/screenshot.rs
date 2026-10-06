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

//! Ctrl+Shift+S, as the C# Heimdall: the session shown, copied to the clipboard as an
//! image. The window is captured, then cut to the area the session is drawn in.

use iced::advanced::widget::Id;
use iced::advanced::widget::operation::{Operation, Outcome};
use iced::window::Screenshot;
use iced::{Rectangle, Size, Task, window};

/// The bytes of a bitmap file's header, then of its information header.
const FILE_HEADER_LEN: usize = 14;
const INFO_HEADER_LEN: usize = 40;

/// Bytes a pixel takes: blue, green, red, and alpha.
const PIXEL_LEN: usize = 4;

/// How many times the clipboard is asked for, another program holding it.
#[cfg(windows)]
const CLIPBOARD_ATTEMPTS: usize = 10;

/// The area the session shown is drawn in.
pub(crate) fn area_id() -> Id {
    Id::new("session-area")
}

/// Where the session's area is drawn, in logical pixels; `None` while it is not.
pub(crate) fn area_bounds() -> Task<Option<Rectangle>> {
    iced::advanced::widget::operate(AreaBounds::default())
}

/// Captures the session shown in the main window, `main`, and copies it to the clipboard;
/// whether it was copied.
pub(crate) fn copy_session(main: Option<window::Id>) -> Task<bool> {
    area_bounds().then(move |bounds| {
        let Some(bounds) = bounds else {
            return Task::done(false);
        };
        crate::shell::main_window_task(main).then(move |window| match window {
            Some(window) => window::screenshot(window).then(move |shot| {
                Task::perform(async move { copy(&shot, bounds) }, |copied| copied)
            }),
            None => Task::done(false),
        })
    })
}

/// Finds where the session's area is drawn, in logical pixels.
#[derive(Default)]
struct AreaBounds {
    found: Option<Rectangle>,
}

impl Operation<Option<Rectangle>> for AreaBounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<Rectangle>>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&area_id()) {
            self.found = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<Option<Rectangle>> {
        Outcome::Some(self.found)
    }
}

/// Copies the part of `shot` that `bounds` covers; whether it was copied.
fn copy(shot: &Screenshot, bounds: Rectangle) -> bool {
    let Some(region) = region(bounds, shot.scale_factor, shot.size) else {
        return false;
    };
    let Ok(cropped) = shot.crop(region) else {
        return false;
    };
    bitmap_file(&cropped.rgba, cropped.size).is_some_and(|bitmap| to_clipboard(&bitmap))
}

/// `bounds`, in logical pixels, as the physical pixels of a capture of `size` taken at
/// `scale`, kept inside it; none when nothing of it is left.
fn region(bounds: Rectangle, scale: f32, size: Size<u32>) -> Option<Rectangle<u32>> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a pixel position, clamped to the capture first"
    )]
    let pixel = |logical: f32, limit: u32| {
        #[expect(clippy::cast_precision_loss, reason = "a window's size in pixels")]
        let limit = limit as f32;
        (logical * scale).round().clamp(0.0, limit) as u32
    };
    let (left, top) = (pixel(bounds.x, size.width), pixel(bounds.y, size.height));
    let right = pixel(bounds.x + bounds.width, size.width);
    let bottom = pixel(bounds.y + bounds.height, size.height);
    (right > left && bottom > top).then(|| Rectangle {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// `rgba`, an image of `size`, as a bitmap file: 32 bits a pixel, rows bottom up, as
/// the clipboard takes it. None when the sizes do not fit the format.
fn bitmap_file(rgba: &[u8], size: Size<u32>) -> Option<Vec<u8>> {
    let width = usize::try_from(size.width).ok()?;
    let height = usize::try_from(size.height).ok()?;
    let row_len = width.checked_mul(PIXEL_LEN)?;
    let pixels_len = row_len.checked_mul(height)?;
    if rgba.len() != pixels_len || pixels_len == 0 {
        return None;
    }
    let offset = FILE_HEADER_LEN + INFO_HEADER_LEN;
    let file_len = u32::try_from(offset.checked_add(pixels_len)?).ok()?;
    let mut file = Vec::with_capacity(offset + pixels_len);
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&file_len.to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&u32::try_from(offset).ok()?.to_le_bytes());
    file.extend_from_slice(&u32::try_from(INFO_HEADER_LEN).ok()?.to_le_bytes());
    file.extend_from_slice(&i32::try_from(size.width).ok()?.to_le_bytes());
    // A positive height: rows from the bottom up.
    file.extend_from_slice(&i32::try_from(size.height).ok()?.to_le_bytes());
    file.extend_from_slice(&1_u16.to_le_bytes());
    file.extend_from_slice(&u16::try_from(PIXEL_LEN * 8).ok()?.to_le_bytes());
    // Uncompressed; the image's size; no resolution or palette given.
    file.extend_from_slice(&0_u32.to_le_bytes());
    file.extend_from_slice(&u32::try_from(pixels_len).ok()?.to_le_bytes());
    file.extend_from_slice(&[0; 16]);
    for row in rgba.chunks_exact(row_len).rev() {
        for [red, green, blue, alpha] in row.as_chunks::<PIXEL_LEN>().0 {
            file.extend_from_slice(&[*blue, *green, *red, *alpha]);
        }
    }
    Some(file)
}

/// Puts the bitmap file `bitmap` on the clipboard in place of what it held; whether it
/// is there. Windows only, as the C#.
fn to_clipboard(bitmap: &[u8]) -> bool {
    #[cfg(windows)]
    {
        let Ok(_open) = clipboard_win::Clipboard::new_attempts(CLIPBOARD_ATTEMPTS) else {
            return false;
        };
        clipboard_win::raw::set_bitmap_with(bitmap, clipboard_win::options::DoClear).is_ok()
    }
    #[cfg(not(windows))]
    {
        let _ = bitmap;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{FILE_HEADER_LEN, INFO_HEADER_LEN, bitmap_file, region};
    use iced::{Rectangle, Size};

    #[test]
    fn the_session_area_is_cut_in_physical_pixels_and_kept_inside_the_capture() {
        let bounds = Rectangle {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
        };
        assert_eq!(
            region(bounds, 1.5, Size::new(1_000, 1_000)),
            Some(Rectangle {
                x: 15,
                y: 30,
                width: 150,
                height: 75
            })
        );
        assert_eq!(
            region(bounds, 1.0, Size::new(60, 40)),
            Some(Rectangle {
                x: 10,
                y: 20,
                width: 50,
                height: 20
            }),
            "what goes past the window is left out"
        );
        assert_eq!(region(bounds, 1.0, Size::new(5, 5)), None, "nothing left");
    }

    #[test]
    fn an_image_becomes_a_bitmap_file_rows_bottom_up_in_blue_green_red() {
        // Two pixels wide, two high: red, green on top; blue, white below.
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 255, //
            0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let file = bitmap_file(&rgba, Size::new(2, 2)).expect("a bitmap");
        let offset = FILE_HEADER_LEN + INFO_HEADER_LEN;
        assert_eq!(&file[..2], b"BM");
        assert_eq!(file.len(), offset + 16);
        let read =
            |at: usize| u32::from_le_bytes([file[at], file[at + 1], file[at + 2], file[at + 3]]);
        assert_eq!(read(2) as usize, file.len(), "the file's size");
        assert_eq!(read(10) as usize, offset, "where the pixels start");
        assert_eq!(
            (read(18), read(22)),
            (2, 2),
            "width, then height: bottom up"
        );
        assert_eq!(u16::from_le_bytes([file[28], file[29]]), 32, "bits a pixel");
        assert_eq!(
            &file[offset..],
            [
                255, 0, 0, 255, 255, 255, 255, 255, // blue, white: the bottom row first
                0, 0, 255, 255, 0, 255, 0, 255, // red, green
            ]
        );
        assert_eq!(bitmap_file(&rgba[..12], Size::new(2, 2)), None, "short");
        assert_eq!(bitmap_file(&[], Size::new(0, 0)), None, "empty");
    }
}
