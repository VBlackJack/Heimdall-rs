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

//! The drag through the Windows shell: each path parsed to its item identifier list, the
//! lists made into a shell item array, the array's own data object asked for, and the
//! shell's drag loop run with it. The shell supplies the drag source and the drag image;
//! every call here is a plain function or method call, with no callback into this crate.
//!
//! Threading: the caller is on the window's thread, which its window library put in a
//! single-threaded apartment with OLE initialised; that is checked, never done here. The
//! drag loop dispatches the window's messages while it runs; the window library queues the
//! events they raise until its handler returns, and nothing here is touched by them: the
//! paths, the identifier lists and the interfaces are this call's own locals.
//!
//! While the loop runs, the window is not redrawn: winit, inside its handler, answers a
//! paint message by asking for another (winit 0.30 `event_loop.rs`, `WM_PAINT`), so the
//! thread may stay busy until the drag ends.
//!
//! The drop is asynchronous, as Explorer makes its own drags: a drag and drop is otherwise
//! synchronous, the target doing the whole copy inside its `IDropTarget::Drop`, any
//! question it asks included, while the source's thread waits in the drag loop ("Handling
//! Shell Data Transfer Scenarios", "Dragging and Dropping Shell Objects Asynchronously").
//! Here that thread is the event loop's: a file dropped on Explorer where one of its name
//! already was froze every window of the application, input ignored and nothing redrawn,
//! until Explorer's "Replace or Skip Files" was answered (manual test of 2026-10-16). The
//! shell's data object only goes asynchronous when made on a thread with a thread
//! reference, which it holds until the target's copy ends (`SHSetThreadRef`); the event
//! loop's thread is given one before its first data object, as the shell's own threads
//! have one.

use std::cell::OnceCell;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicI32;

use windows::Win32::Foundation::{E_UNEXPECTED, HWND};
use windows::Win32::System::Com::{
    APTTYPE, APTTYPE_MAINSTA, APTTYPE_STA, APTTYPEQUALIFIER, CoGetApartmentType, IBindCtx,
    IDataObject,
};
use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_NONE, IDropSource};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    BHID_DataObject, IDataObjectAsyncCapability, ILFree, SHCreateShellItemArrayFromIDLists,
    SHCreateThreadRef, SHDoDragDrop, SHGetThreadRef, SHParseDisplayName, SHSetThreadRef,
};
use windows::core::{IUnknown, Interface, PCWSTR};

use crate::{DragError, DragOutcome};

/// No attribute of the item asked of [`SHParseDisplayName`].
const NO_ATTRIBUTES: u32 = 0;

/// The UTF-16 string terminator.
const NUL: u16 = 0;

/// An item identifier list the shell allocated, freed with it.
struct IdList(*mut ITEMIDLIST);

impl Drop for IdList {
    fn drop(&mut self) {
        // SAFETY: the pointer is a list `SHParseDisplayName` allocated and handed over,
        // never null (an `IdList` is made only of a non-null one), owned by this value
        // alone and freed once, here; nothing reads it afterwards.
        unsafe { ILFree(Some(self.0.cast_const())) };
    }
}

/// The shell's error, kept with its code and the system's words for it.
fn shell_error(error: &windows::core::Error) -> DragError {
    DragError::Shell {
        code: error.code().0,
        message: error.message(),
    }
}

/// Whether this thread is in a single-threaded apartment, the main one included: a drag
/// and its drop targets need one, and the window's thread has it from its window library.
pub(crate) fn single_threaded() -> Result<(), DragError> {
    let mut kind = APTTYPE::default();
    let mut qualifier = APTTYPEQUALIFIER::default();
    // SAFETY: both pointers are to live locals of the types the function writes; it keeps
    // neither. On a thread COM was never initialised on, it fails, which is answered.
    let asked = unsafe { CoGetApartmentType(&raw mut kind, &raw mut qualifier) };
    match asked {
        Ok(()) if kind == APTTYPE_STA || kind == APTTYPE_MAINSTA => Ok(()),
        _ => Err(DragError::NotSingleThreaded),
    }
}

/// The item identifier list of `path`, a full path of this computer.
fn id_list(path: &Path) -> Result<IdList, DragError> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    // A NUL inside would cut the path short: another item than the one named.
    if wide.contains(&NUL) {
        return Err(DragError::InvalidPath(path.to_path_buf()));
    }
    wide.push(NUL);
    let mut list: *mut ITEMIDLIST = std::ptr::null_mut();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call, which only
    // reads it; `list` is a live local the call writes the allocated list to, owned by
    // the caller from then on; no bind context, no attributes asked.
    let parsed = unsafe {
        SHParseDisplayName(
            PCWSTR(wide.as_ptr()),
            None::<&IBindCtx>,
            &raw mut list,
            NO_ATTRIBUTES,
            None,
        )
    };
    // Owned before the answer is read: a list written despite a failure is freed too.
    let owned = (!list.is_null()).then_some(IdList(list));
    parsed.map_err(|error| shell_error(&error))?;
    owned.ok_or_else(unexpected)
}

/// The shell answered success without the object it was asked for.
fn unexpected() -> DragError {
    DragError::Shell {
        code: E_UNEXPECTED.0,
        message: String::new(),
    }
}

/// The shell's data object of `paths`, all in one folder: the same Explorer drags, file
/// drop format included.
pub(crate) fn data_object(paths: &[PathBuf]) -> Result<IDataObject, DragError> {
    let lists = paths
        .iter()
        .map(|path| id_list(path))
        .collect::<Result<Vec<_>, _>>()?;
    let raw: Vec<*const ITEMIDLIST> = lists.iter().map(|list| list.0.cast_const()).collect();
    // SAFETY: every pointer of `raw` is a full item identifier list from
    // `SHParseDisplayName`, alive until `lists` drops at the end of this function; the
    // call only reads them while it runs (the array keeps its own copies), and the slice
    // gives it their count.
    let items =
        unsafe { SHCreateShellItemArrayFromIDLists(&raw) }.map_err(|error| shell_error(&error))?;
    // SAFETY: `items` is a live interface this function holds; the identifier is a static
    // GUID the call only reads; no bind context. The result is a new reference, released
    // when dropped.
    unsafe { items.BindToHandler::<_, IDataObject>(None::<&IBindCtx>, &BHID_DataObject) }
        .map_err(|error| shell_error(&error))
}

/// `data` dropped asynchronously: the target returns from its drop at once and takes the
/// files on a thread of its own, its progress and its questions included, as from
/// Explorer's own drags; the source's thread is free again as soon as the drag loop ends.
/// The shell's data object takes care of the end of that operation itself.
///
/// # Errors
///
/// [`DragError::Synchronous`] when the data object stays synchronous, as one made on a
/// thread without a thread reference ([`thread_ref`]) does: its drop would hold this
/// thread for as long as the target takes.
pub(crate) fn asynchronous(data: &IDataObject) -> Result<(), DragError> {
    let capability: IDataObjectAsyncCapability =
        data.cast().map_err(|error| shell_error(&error))?;
    // SAFETY: `capability` is a live interface of the data object, held until the call
    // returns; the flag is passed by value.
    unsafe { capability.SetAsyncMode(true) }.map_err(|error| shell_error(&error))?;
    // SAFETY: as above; the call only writes its own result.
    let mode = unsafe { capability.GetAsyncMode() }.map_err(|error| shell_error(&error))?;
    if mode.as_bool() {
        Ok(())
    } else {
        Err(DragError::Synchronous)
    }
}

/// The thread reference this crate gave its thread, while the thread lasts: the shell's
/// data objects hold it, through `SHGetThreadRef`, while a target copies their files on
/// its own thread.
struct ThreadRef(Option<IUnknown>);

impl ThreadRef {
    /// The thread's reference: the one it already has, left as it is, or a new one set.
    fn set() -> Result<Self, DragError> {
        // SAFETY: no argument; the reference returned, if any, is owned by the result and
        // released when dropped.
        if unsafe { SHGetThreadRef() }.is_ok() {
            return Ok(Self(None));
        }
        // The object counts its references in this number and frees itself at zero, which
        // can be after this thread is gone, a target still copying: the number is left for
        // the rest of the process, four bytes once per thread.
        let count: &'static AtomicI32 = Box::leak(Box::new(AtomicI32::new(0)));
        let mut object = None;
        // SAFETY: `count` lives until the process ends and the object only changes it by
        // interlocked operations, as an `AtomicI32` of the same layout; `object` is a live
        // local the call writes the new object to, owned by this function from then on.
        unsafe { SHCreateThreadRef(count.as_ptr(), &raw mut object) }
            .map_err(|error| shell_error(&error))?;
        let object = object.ok_or_else(unexpected)?;
        // SAFETY: the thread's slot keeps `object` without a reference of its own: `drop`
        // empties it before `object` is released; the call only stores the pointer.
        unsafe { SHSetThreadRef(&object) }.map_err(|error| shell_error(&error))?;
        Ok(Self(Some(object)))
    }
}

impl Drop for ThreadRef {
    fn drop(&mut self) {
        if self.0.is_some() {
            // SAFETY: empties this thread's slot, which holds the object of `self.0`, before
            // that object is released with `self`; no pointer is given.
            let _ = unsafe { SHSetThreadRef(None::<&IUnknown>) };
        }
    }
}

thread_local! {
    /// The thread reference of this thread, once a drag needed one.
    static THREAD_REF: OnceCell<ThreadRef> = const { OnceCell::new() };
}

/// This thread with a thread reference, for good: set by its first drag, before the data
/// objects it makes, which take it when made.
pub(crate) fn thread_ref() -> Result<(), DragError> {
    THREAD_REF.with(|cell| {
        if cell.get().is_none() {
            let _ = cell.set(ThreadRef::set()?);
        }
        Ok(())
    })
}

/// The drag of `paths` out of the window `window`, a copy, until dropped or given up.
/// A data object that cannot be dropped asynchronously is not dragged: its drop would hold
/// the window's thread for as long as the target takes.
pub(crate) fn drag(window: isize, paths: &[PathBuf]) -> Result<DragOutcome, DragError> {
    single_threaded()?;
    thread_ref()?;
    let data = data_object(paths)?;
    asynchronous(&data)?;
    // A window handle is a number the system checks, never memory read here.
    let owner = HWND(std::ptr::without_provenance_mut(window.cast_unsigned()));
    // SAFETY: on a single-threaded apartment, as checked above; `data` is a live data
    // object held until the call returns; no drag source, so the shell supplies its own;
    // the handle is only used by the system, which refuses an invalid one. The drag loop
    // dispatches this thread's messages: the window library queues what they raise, and
    // none of them reaches the locals borrowed here.
    let effect = unsafe { SHDoDragDrop(Some(owner), &data, None::<&IDropSource>, DROPEFFECT_COPY) }
        .map_err(|error| shell_error(&error))?;
    Ok(if effect == DROPEFFECT_NONE {
        DragOutcome::Cancelled
    } else {
        DragOutcome::Copied
    })
}

#[cfg(test)]
mod tests {
    use windows::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize, DVASPECT_CONTENT, FORMATETC,
        TYMED_HGLOBAL,
    };
    use windows::Win32::System::Ole::CF_HDROP;

    use super::*;

    /// No item of a list: the whole object, the format's own meaning.
    const WHOLE: i32 = -1;

    /// This test's thread in a single-threaded apartment of its own, left at the end.
    struct Apartment;

    impl Apartment {
        fn enter() -> Self {
            // SAFETY: called on this test's own thread, no reserved pointer; balanced by
            // `CoUninitialize` in `drop`, on the same thread.
            let entered = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            entered.ok().expect("an apartment");
            Self
        }
    }

    impl Drop for Apartment {
        fn drop(&mut self) {
            // SAFETY: balances the successful `CoInitializeEx` of `enter`, on the same
            // thread, every interface of the test released before.
            unsafe { CoUninitialize() };
        }
    }

    #[test]
    fn a_thread_outside_any_apartment_is_refused() {
        let refused = std::thread::spawn(single_threaded).join().expect("ran");
        assert_eq!(refused, Err(DragError::NotSingleThreaded));
    }

    #[test]
    fn the_files_of_a_folder_make_a_file_drop_without_dragging() {
        let dir = tempfile::tempdir().expect("dir");
        let paths = [dir.path().join("a.txt"), dir.path().join("b.txt")];
        for path in &paths {
            std::fs::write(path, b"x").expect("written");
        }
        std::thread::spawn(move || {
            let apartment = Apartment::enter();
            assert_eq!(single_threaded(), Ok(()));
            let data = data_object(&paths).expect("a data object");
            let format = FORMATETC {
                cfFormat: CF_HDROP.0,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: WHOLE,
                tymed: TYMED_HGLOBAL.0.cast_unsigned(),
            };
            // SAFETY: `data` is live; `format` a local the call only reads.
            let offered = unsafe { data.QueryGetData(&raw const format) };
            assert!(offered.is_ok(), "a file drop: {offered:?}");
            drop(data);
            drop(apartment);
        })
        .join()
        .expect("ran");
    }

    /// Whether `data` is dropped asynchronously.
    fn async_mode(data: &IDataObject) -> bool {
        let capability: IDataObjectAsyncCapability = data.cast().expect("the capability");
        // SAFETY: `capability` is live for the call, which only writes its own result.
        unsafe { capability.GetAsyncMode() }
            .expect("a mode")
            .as_bool()
    }

    /// The freeze of the manual test of 2026-10-16: the shell's data object is dropped
    /// synchronously unless told otherwise, so the target's copy, and its "Replace or Skip
    /// Files" question, held the event loop's thread. Made on a thread without a thread
    /// reference, it stays so even when told otherwise; the one dragged is asynchronous.
    #[test]
    fn the_files_are_dropped_asynchronously() {
        let dir = tempfile::tempdir().expect("dir");
        let paths = [dir.path().join("binaire.bin")];
        std::fs::write(&paths[0], b"x").expect("written");
        std::thread::spawn(move || {
            let apartment = Apartment::enter();
            let data = data_object(&paths).expect("a data object");
            assert!(!async_mode(&data), "synchronous by default");
            let capability: IDataObjectAsyncCapability = data.cast().expect("the capability");
            // SAFETY: `capability` is live for the call; the flag is passed by value.
            unsafe { capability.SetAsyncMode(true) }.expect("accepted");
            assert!(
                !async_mode(&data),
                "still synchronous without a thread reference"
            );
            assert_eq!(asynchronous(&data), Err(DragError::Synchronous));
            thread_ref().expect("a thread reference");
            let dragged = data_object(&paths).expect("a data object");
            asynchronous(&dragged).expect("made asynchronous");
            assert!(async_mode(&dragged));
            // A second drag on the same thread keeps its reference.
            thread_ref().expect("the same reference");
            let again = data_object(&paths).expect("a second data object");
            asynchronous(&again).expect("made asynchronous");
            drop((capability, again, dragged, data));
            drop(apartment);
        })
        .join()
        .expect("ran");
    }

    #[test]
    fn a_missing_file_is_refused_by_the_shell() {
        let dir = tempfile::tempdir().expect("dir");
        let missing = [dir.path().join("absent.txt")];
        std::thread::spawn(move || {
            let apartment = Apartment::enter();
            let refused = data_object(&missing);
            assert!(
                matches!(refused, Err(DragError::Shell { .. })),
                "{refused:?}"
            );
            drop(apartment);
        })
        .join()
        .expect("ran");
    }
}
