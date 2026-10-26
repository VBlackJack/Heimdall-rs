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

//! The one foreign function declared here, `DwmSetWindowAttribute` of `dwmapi.dll`, and the
//! only unsafe code of the crate. Declared by hand, with the types it needs, so that no
//! Windows binding crate becomes a dependency for a single call.

use std::ffi::c_void;

use crate::{
    ATTR_CAPTION_COLOR, ATTR_DARK_MODE, ATTR_DARK_MODE_LEGACY, ATTR_TEXT_COLOR, Hwnd, Style,
};

#[link(name = "dwmapi")]
unsafe extern "system" {
    /// `HRESULT DwmSetWindowAttribute(HWND, DWORD, LPCVOID, DWORD)`.
    fn DwmSetWindowAttribute(hwnd: Hwnd, attribute: u32, value: *const c_void, size: u32) -> i32;
}

/// Size of the 32-bit value every attribute used here takes.
const VALUE_SIZE: u32 = u32::BITS / 8;

/// Sets one 32-bit attribute of the window; the result is ignored, as the C# does, since a
/// Windows too old for an attribute simply keeps its default title bar.
fn set(hwnd: Hwnd, attribute: u32, value: u32) {
    // SAFETY: `value` is a live `u32` for the whole call and `VALUE_SIZE` is its size, which
    // is what the function reads; the window manager validates `hwnd` itself and fails with an
    // error code, ignored, for a handle that is not a window.
    let _ =
        unsafe { DwmSetWindowAttribute(hwnd, attribute, (&raw const value).cast(), VALUE_SIZE) };
}

/// Sets the four attributes the C# `WindowThemeHelper.Apply` sets, in its order.
pub fn apply(hwnd: Hwnd, style: Style) {
    let dark = u32::from(style.dark);
    set(hwnd, ATTR_DARK_MODE, dark);
    set(hwnd, ATTR_DARK_MODE_LEGACY, dark);
    set(hwnd, ATTR_CAPTION_COLOR, style.caption);
    set(hwnd, ATTR_TEXT_COLOR, style.text);
}
