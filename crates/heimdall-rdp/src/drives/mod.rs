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

//! This computer's drives shared with an RDP server, as mstsc shares them: the server's
//! Explorer shows each as "C on <this computer>" and reads and writes its files.
//!
//! The server names files by paths on a drive; each is checked (see [`path`]) before it
//! reaches the file system. Requests are answered at once, on the session's task: a large
//! read holds the session for its time.
//!
//! What it does not do: file locks are granted without being held; change notifications are
//! never sent, so a folder the server shows is refreshed by hand; on Windows the free space
//! of a drive is not told, the standard library having no safe way to read it.

mod path;

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ironrdp::pdu::PduResult;
use ironrdp::rdpdr::RdpdrBackend;
use ironrdp::rdpdr::pdu::RdpdrPdu;
use ironrdp::rdpdr::pdu::efs::{
    Boolean, ClientDriveQueryDirectoryResponse, ClientDriveQueryInformationResponse,
    ClientDriveQueryVolumeInformationResponse, ClientDriveSetInformationResponse,
    CreateDisposition, CreateOptions, DeviceCloseRequest, DeviceCloseResponse,
    DeviceControlRequest, DeviceControlResponse, DeviceCreateRequest, DeviceCreateResponse,
    DeviceIoRequest, DeviceIoResponse, DeviceReadRequest, DeviceReadResponse, DeviceWriteRequest,
    DeviceWriteResponse, FileAttributeTagInformation, FileAttributes, FileBasicInformation,
    FileBothDirectoryInformation, FileDirectoryInformation, FileFsAttributeInformation,
    FileFsDeviceInformation, FileFsVolumeInformation, FileFullDirectoryInformation,
    FileInformationClass, FileInformationClassLevel, FileNamesInformation, FileStandardInformation,
    FileSystemAttributes, FileSystemInformationClass, FileSystemInformationClassLevel, Information,
    NtStatus, ServerDeviceAnnounceResponse, ServerDriveIoRequest, ServerDriveQueryDirectoryRequest,
    ServerDriveQueryInformationRequest, ServerDriveQueryVolumeInformationRequest,
    ServerDriveSetInformationRequest,
};
use ironrdp::rdpdr::pdu::esc::{ScardCall, ScardIoCtlCode};
use ironrdp::svc::SvcMessage;
use ironrdp_core::impl_as_any;

/// A drive shared with the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedDrive {
    /// Its name on the server: "C" shows as "C on <this computer>".
    pub name: String,
    /// Its folder on this side.
    pub root: PathBuf,
}

/// This computer's drives, as mstsc shares them: on Windows every drive letter from C that
/// is there (A and B, floppy drives, are never probed); elsewhere the file system's root.
#[must_use]
pub fn local_drives() -> Vec<SharedDrive> {
    if cfg!(windows) {
        ('C'..='Z')
            .map(|letter| SharedDrive {
                name: letter.to_string(),
                root: PathBuf::from(format!("{letter}:\\")),
            })
            .filter(|drive| drive.root.is_dir())
            .collect()
    } else {
        vec![SharedDrive {
            name: "root".to_owned(),
            root: PathBuf::from("/"),
        }]
    }
}

/// Most bytes read for one request: a server asks for 64 KiB at most in practice, this
/// bounds what a bad one could make this side allocate.
const MAX_READ: u32 = 8 * 1024 * 1024;

/// Access rights that write: data, appended data, attributes, deletion, or the generic
/// rights that include them.
const WRITE_ACCESS: u32 =
    0x0000_0002 | 0x0000_0004 | 0x0000_0100 | 0x0001_0000 | 0x1000_0000 | 0x4000_0000;

/// Status codes this crate's `NtStatus` does not name.
const OBJECT_NAME_NOT_FOUND: u32 = 0xC000_0034;
const OBJECT_NAME_INVALID: u32 = 0xC000_0033;
const OBJECT_PATH_NOT_FOUND: u32 = 0xC000_003A;
const FILE_IS_A_DIRECTORY: u32 = 0xC000_00BA;
const SHARING_VIOLATION: u32 = 0xC000_0043;
const DISK_FULL: u32 = 0xC000_007F;
const INVALID_HANDLE: u32 = 0xC000_0008;

/// `Information` of a create that made the file: the crate names the other three.
const FILE_CREATED: u8 = 2;

/// Device type of a disk, as `FileFsDeviceInformation` tells it.
const FILE_DEVICE_DISK: u32 = 0x0000_0007;

/// Hundreds of nanoseconds from 1601, the FILETIME epoch, to 1970.
const FILETIME_UNIX_EPOCH: i64 = 116_444_736_000_000_000;

/// A file or folder the server opened.
#[derive(Debug)]
struct Opened {
    path: PathBuf,
    /// The open file; `None` for a folder.
    file: Option<File>,
    /// Deleted when closed: asked at the opening, or by a disposition set since.
    delete_on_close: bool,
    /// What a directory query listed, still to hand out, one per request.
    listing: VecDeque<(String, Metadata)>,
}

/// The shared drives, and the files the server has open on them.
#[derive(Debug)]
pub(crate) struct DriveBackend {
    /// Folder of each drive, by the device id announced for it.
    drives: HashMap<u32, SharedDrive>,
    opened: HashMap<u32, Opened>,
    next_file: u32,
}

impl_as_any!(DriveBackend);

impl DriveBackend {
    /// The backend of `drives`, the device id of each being its index.
    pub(crate) fn new(drives: &[SharedDrive]) -> Self {
        Self {
            drives: (0_u32..).zip(drives.iter().cloned()).collect(),
            opened: HashMap::new(),
            next_file: 1,
        }
    }

    /// The devices to announce: id and name of each drive.
    pub(crate) fn devices(&self) -> Vec<(u32, String)> {
        let mut devices: Vec<(u32, String)> = self
            .drives
            .iter()
            .map(|(id, drive)| (*id, drive.name.clone()))
            .collect();
        devices.sort_unstable();
        devices
    }

    fn root(&self, device: u32) -> Option<PathBuf> {
        self.drives.get(&device).map(|drive| drive.root.clone())
    }

    /// Answers one request of the server; the answer to send back.
    pub(crate) fn answer(&mut self, request: ServerDriveIoRequest) -> RdpdrPdu {
        match request {
            ServerDriveIoRequest::ServerCreateDriveRequest(request) => {
                RdpdrPdu::from(self.create(&request))
            }
            ServerDriveIoRequest::DeviceCloseRequest(request) => self.close(request),
            ServerDriveIoRequest::DeviceReadRequest(request) => RdpdrPdu::from(self.read(&request)),
            ServerDriveIoRequest::DeviceWriteRequest(request) => {
                RdpdrPdu::from(self.write(&request))
            }
            ServerDriveIoRequest::ServerDriveQueryInformationRequest(request) => {
                RdpdrPdu::from(self.query_information(&request))
            }
            ServerDriveIoRequest::ServerDriveQueryDirectoryRequest(request) => {
                RdpdrPdu::from(self.query_directory(&request))
            }
            ServerDriveIoRequest::ServerDriveQueryVolumeInformationRequest(request) => {
                RdpdrPdu::from(self.query_volume(&request))
            }
            ServerDriveIoRequest::ServerDriveSetInformationRequest(request) => {
                self.set_information(&request)
            }
            ServerDriveIoRequest::DeviceControlRequest(request) => {
                device_control(request, NtStatus::SUCCESS)
            }
            // Granted, not held: another program on this side is not kept out.
            ServerDriveIoRequest::ServerDriveLockControlRequest(request) => {
                completed(request.device_io_request, NtStatus::SUCCESS)
            }
            ServerDriveIoRequest::ServerDriveNotifyChangeDirectoryRequest(_) => {
                unreachable!("answered by handle_drive_io_request: never")
            }
            ServerDriveIoRequest::Unsupported(request) => {
                completed(request, NtStatus::NOT_SUPPORTED)
            }
        }
    }

    fn create(&mut self, request: &DeviceCreateRequest) -> DeviceCreateResponse {
        let io = request.device_io_request.clone();
        match self.open(request) {
            Ok((opened, information)) => {
                let file_id = self.next_file;
                self.next_file = self.next_file.wrapping_add(1).max(1);
                self.opened.insert(file_id, opened);
                DeviceCreateResponse {
                    device_io_reply: DeviceIoResponse::new(io, NtStatus::SUCCESS),
                    file_id,
                    information,
                }
            }
            Err(status) => DeviceCreateResponse {
                device_io_reply: DeviceIoResponse::new(io, status),
                file_id: 0,
                information: Information::empty(),
            },
        }
    }

    /// Opens, or creates, what `request` names, as its disposition and options say.
    fn open(&self, request: &DeviceCreateRequest) -> Result<(Opened, Information), NtStatus> {
        let root = self
            .root(request.device_io_request.device_id)
            .ok_or(NtStatus::from(INVALID_HANDLE))?;
        let path =
            path::resolve(&root, &request.path).map_err(|_| NtStatus::from(OBJECT_NAME_INVALID))?;
        let disposition = request.create_disposition;
        let options = &request.create_options;
        let wants_folder = options.contains(CreateOptions::FILE_DIRECTORY_FILE);
        let wants_file = options.contains(CreateOptions::FILE_NON_DIRECTORY_FILE);
        let writes = request.desired_access.bits() & WRITE_ACCESS != 0;
        let delete_on_close = options.contains(CreateOptions::FILE_DELETE_ON_CLOSE);
        let opened = |file| Opened {
            path: path.clone(),
            file,
            delete_on_close,
            listing: VecDeque::new(),
        };
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_dir() => {
                if disposition == CreateDisposition::FILE_CREATE {
                    return Err(NtStatus::OBJECT_NAME_COLLISION);
                }
                if wants_file {
                    return Err(NtStatus::from(FILE_IS_A_DIRECTORY));
                }
                Ok((opened(None), Information::FILE_OPENED))
            }
            Ok(_) => {
                if wants_folder {
                    return Err(NtStatus::NOT_A_DIRECTORY);
                }
                let (options, information) = match disposition {
                    CreateDisposition::FILE_CREATE => {
                        return Err(NtStatus::OBJECT_NAME_COLLISION);
                    }
                    CreateDisposition::FILE_OPEN | CreateDisposition::FILE_OPEN_IF => {
                        (reading(writes), Information::FILE_OPENED)
                    }
                    CreateDisposition::FILE_SUPERSEDE => {
                        (truncating(), Information::FILE_SUPERSEDED)
                    }
                    _ => (truncating(), Information::FILE_OVERWRITTEN),
                };
                let file = options.open(&path).map_err(|error| status(&error))?;
                Ok((opened(Some(file)), information))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if !path.parent().is_some_and(std::path::Path::is_dir) {
                    return Err(NtStatus::from(OBJECT_PATH_NOT_FOUND));
                }
                if matches!(
                    disposition,
                    CreateDisposition::FILE_OPEN | CreateDisposition::FILE_OVERWRITE
                ) {
                    return Err(NtStatus::from(OBJECT_NAME_NOT_FOUND));
                }
                let created = Information::from_bits_retain(FILE_CREATED);
                if wants_folder {
                    fs::create_dir(&path).map_err(|error| status(&error))?;
                    return Ok((opened(None), created));
                }
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|error| status(&error))?;
                Ok((opened(Some(file)), created))
            }
            Err(error) => Err(status(&error)),
        }
    }

    fn close(&mut self, request: DeviceCloseRequest) -> RdpdrPdu {
        let io = request.device_io_request;
        let Some(opened) = self.opened.remove(&io.file_id) else {
            return completed(io, NtStatus::from(INVALID_HANDLE));
        };
        let Opened {
            path,
            file,
            delete_on_close,
            ..
        } = opened;
        // Closed first: Windows deletes nothing still open.
        let folder = file.is_none();
        drop(file);
        let mut io_status = NtStatus::SUCCESS;
        if delete_on_close {
            let removed = if folder {
                fs::remove_dir(&path)
            } else {
                fs::remove_file(&path)
            };
            if let Err(error) = removed {
                io_status = status(&error);
            }
        }
        completed(io, io_status)
    }

    fn read(&mut self, request: &DeviceReadRequest) -> DeviceReadResponse {
        let io = request.device_io_request.clone();
        let result = self.file(io.file_id).and_then(|file| {
            file.seek(SeekFrom::Start(request.offset))
                .map_err(|error| status(&error))?;
            let mut data = Vec::new();
            file.take(u64::from(request.length.min(MAX_READ)))
                .read_to_end(&mut data)
                .map_err(|error| status(&error))?;
            Ok(data)
        });
        match result {
            Ok(read_data) => DeviceReadResponse {
                device_io_reply: DeviceIoResponse::new(io, NtStatus::SUCCESS),
                read_data,
            },
            Err(io_status) => DeviceReadResponse {
                device_io_reply: DeviceIoResponse::new(io, io_status),
                read_data: Vec::new(),
            },
        }
    }

    fn write(&mut self, request: &DeviceWriteRequest) -> DeviceWriteResponse {
        let io = request.device_io_request.clone();
        let result = self.file(io.file_id).and_then(|file| {
            file.seek(SeekFrom::Start(request.offset))
                .map_err(|error| status(&error))?;
            file.write_all(&request.write_data)
                .map_err(|error| status(&error))?;
            u32::try_from(request.write_data.len()).map_err(|_| NtStatus::UNSUCCESSFUL)
        });
        match result {
            Ok(length) => DeviceWriteResponse {
                device_io_reply: DeviceIoResponse::new(io, NtStatus::SUCCESS),
                length,
            },
            Err(io_status) => DeviceWriteResponse {
                device_io_reply: DeviceIoResponse::new(io, io_status),
                length: 0,
            },
        }
    }

    /// The open file `file_id`; a folder or an unknown id is refused.
    fn file(&mut self, file_id: u32) -> Result<&mut File, NtStatus> {
        match self.opened.get_mut(&file_id) {
            Some(Opened {
                file: Some(file), ..
            }) => Ok(file),
            Some(_) => Err(NtStatus::from(FILE_IS_A_DIRECTORY)),
            None => Err(NtStatus::from(INVALID_HANDLE)),
        }
    }

    fn query_information(
        &self,
        request: &ServerDriveQueryInformationRequest,
    ) -> ClientDriveQueryInformationResponse {
        let io = request.device_io_request.clone();
        let answer = self
            .opened
            .get(&io.file_id)
            .ok_or(NtStatus::from(INVALID_HANDLE))
            .and_then(|opened| {
                let metadata = fs::metadata(&opened.path).map_err(|error| status(&error))?;
                let name = file_name(&opened.path);
                information(
                    &request.file_info_class_lvl,
                    &metadata,
                    &name,
                    opened.delete_on_close,
                )
                .ok_or(NtStatus::NOT_SUPPORTED)
            });
        match answer {
            Ok(buffer) => ClientDriveQueryInformationResponse {
                device_io_response: DeviceIoResponse::new(io, NtStatus::SUCCESS),
                buffer: Some(buffer),
            },
            Err(io_status) => ClientDriveQueryInformationResponse {
                device_io_response: DeviceIoResponse::new(io, io_status),
                buffer: None,
            },
        }
    }

    /// One entry of a folder per request: the first query lists what matches, the next ones
    /// hand out the rest, and the end says there are no more files.
    fn query_directory(
        &mut self,
        request: &ServerDriveQueryDirectoryRequest,
    ) -> ClientDriveQueryDirectoryResponse {
        let io = request.device_io_request.clone();
        let root = self.root(io.device_id);
        let answer = match (self.opened.get_mut(&io.file_id), root) {
            (Some(opened), Some(root)) => if request.initial_query != 0 {
                match list(&root, &request.path) {
                    Ok(listing) if !listing.is_empty() => {
                        opened.listing = listing;
                        Ok(())
                    }
                    Ok(_) => Err(NtStatus::NO_SUCH_FILE),
                    Err(io_status) => Err(io_status),
                }
            } else {
                Ok(())
            }
            .and_then(|()| opened.listing.pop_front().ok_or(NtStatus::NO_MORE_FILES))
            .and_then(|(name, metadata)| {
                entry(&request.file_info_class_lvl, &metadata, name).ok_or(NtStatus::NOT_SUPPORTED)
            }),
            _ => Err(NtStatus::from(INVALID_HANDLE)),
        };
        match answer {
            Ok(buffer) => ClientDriveQueryDirectoryResponse {
                device_io_reply: DeviceIoResponse::new(io, NtStatus::SUCCESS),
                buffer: Some(buffer),
            },
            Err(io_status) => ClientDriveQueryDirectoryResponse {
                device_io_reply: DeviceIoResponse::new(io, io_status),
                buffer: None,
            },
        }
    }

    fn query_volume(
        &self,
        request: &ServerDriveQueryVolumeInformationRequest,
    ) -> ClientDriveQueryVolumeInformationResponse {
        let io = request.device_io_request.clone();
        let Some(drive) = self.drives.get(&io.device_id) else {
            return ClientDriveQueryVolumeInformationResponse::new(
                io,
                NtStatus::from(INVALID_HANDLE),
                None,
            );
        };
        let buffer = match request.fs_info_class_lvl {
            FileSystemInformationClassLevel::FILE_FS_VOLUME_INFORMATION => Some(
                FileSystemInformationClass::FileFsVolumeInformation(FileFsVolumeInformation {
                    volume_creation_time: 0,
                    volume_serial_number: 0,
                    supports_objects: Boolean::False,
                    volume_label: drive.name.clone(),
                }),
            ),
            FileSystemInformationClassLevel::FILE_FS_ATTRIBUTE_INFORMATION => {
                Some(FileSystemInformationClass::FileFsAttributeInformation(
                    FileFsAttributeInformation {
                        file_system_attributes: file_system_attributes(),
                        max_component_name_len: 255,
                        file_system_name: "NTFS".to_owned(),
                    },
                ))
            }
            FileSystemInformationClassLevel::FILE_FS_DEVICE_INFORMATION => Some(
                FileSystemInformationClass::FileFsDeviceInformation(FileFsDeviceInformation {
                    device_type: FILE_DEVICE_DISK,
                    characteristics: ironrdp::rdpdr::pdu::efs::Characteristics::empty(),
                }),
            ),
            _ => space::information(&request.fs_info_class_lvl, &drive.root),
        };
        match buffer {
            Some(buffer) => {
                ClientDriveQueryVolumeInformationResponse::new(io, NtStatus::SUCCESS, Some(buffer))
            }
            None => {
                ClientDriveQueryVolumeInformationResponse::new(io, NtStatus::NOT_SUPPORTED, None)
            }
        }
    }

    fn set_information(&mut self, request: &ServerDriveSetInformationRequest) -> RdpdrPdu {
        let io_status = self.change(request);
        match ClientDriveSetInformationResponse::new(request, io_status) {
            Ok(response) => RdpdrPdu::from(response),
            Err(_) => completed(request.device_io_request.clone(), NtStatus::UNSUCCESSFUL),
        }
    }

    /// Applies what a set request changes; its status.
    fn change(&mut self, request: &ServerDriveSetInformationRequest) -> NtStatus {
        let io = &request.device_io_request;
        let root = self.root(io.device_id);
        match (self.opened.get_mut(&io.file_id), root) {
            (Some(opened), Some(root)) => set(opened, &root, &request.set_buffer)
                .err()
                .unwrap_or(NtStatus::SUCCESS),
            _ => NtStatus::from(INVALID_HANDLE),
        }
    }
}

impl RdpdrBackend for DriveBackend {
    fn handle_server_device_announce_response(
        &mut self,
        _pdu: ServerDeviceAnnounceResponse,
    ) -> PduResult<()> {
        Ok(())
    }

    fn handle_scard_call(
        &mut self,
        _req: DeviceControlRequest<ScardIoCtlCode>,
        _call: ScardCall,
    ) -> PduResult<()> {
        // No smart card is announced.
        Ok(())
    }

    fn handle_drive_io_request(&mut self, req: ServerDriveIoRequest) -> PduResult<Vec<SvcMessage>> {
        // Never sent, so left waiting: the server then shows a folder as it was listed.
        if matches!(
            req,
            ServerDriveIoRequest::ServerDriveNotifyChangeDirectoryRequest(_)
        ) {
            return Ok(Vec::new());
        }
        Ok(vec![SvcMessage::from(self.answer(req))])
    }
}

/// Open options that read, and write when the server asked to.
fn reading(writes: bool) -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(writes);
    options
}

/// Open options that empty the file first.
fn truncating() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true).truncate(true);
    options
}

/// A completion with no more than its status: the answer to a close, a lock, and to
/// anything answered as not supported.
fn completed(request: DeviceIoRequest, io_status: NtStatus) -> RdpdrPdu {
    RdpdrPdu::from(DeviceCloseResponse {
        device_io_response: DeviceIoResponse::new(request, io_status),
    })
}

fn device_control(
    request: DeviceControlRequest<ironrdp::rdpdr::pdu::efs::AnyIoCtlCode>,
    io_status: NtStatus,
) -> RdpdrPdu {
    RdpdrPdu::from(DeviceControlResponse::new(request, io_status, None))
}

/// The status Windows would give for `error`.
fn status(error: &io::Error) -> NtStatus {
    // Windows' own reasons first: a file another program holds, a full disk.
    match error.raw_os_error() {
        Some(32 | 33) if cfg!(windows) => return NtStatus::from(SHARING_VIOLATION),
        Some(112) if cfg!(windows) => return NtStatus::from(DISK_FULL),
        _ => {}
    }
    match error.kind() {
        io::ErrorKind::NotFound => NtStatus::from(OBJECT_NAME_NOT_FOUND),
        io::ErrorKind::PermissionDenied => NtStatus::ACCESS_DENIED,
        io::ErrorKind::AlreadyExists => NtStatus::OBJECT_NAME_COLLISION,
        io::ErrorKind::DirectoryNotEmpty => NtStatus::DIRECTORY_NOT_EMPTY,
        io::ErrorKind::NotADirectory => NtStatus::NOT_A_DIRECTORY,
        io::ErrorKind::IsADirectory => NtStatus::from(FILE_IS_A_DIRECTORY),
        io::ErrorKind::StorageFull => NtStatus::from(DISK_FULL),
        _ => NtStatus::UNSUCCESSFUL,
    }
}

/// The last component of `path`, as the server names it.
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What a directory query at `query` finds under `root`: each name matching its pattern
/// with what is known of it, in name order; `.` and `..` are never listed.
fn list(root: &std::path::Path, query: &str) -> Result<VecDeque<(String, Metadata)>, NtStatus> {
    let (folder, pattern) =
        path::query(root, query).map_err(|_| NtStatus::from(OBJECT_NAME_INVALID))?;
    let mut found: Vec<(String, Metadata)> = fs::read_dir(&folder)
        .map_err(|error| status(&error))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !path::matches(&pattern, &name) {
                return None;
            }
            // Followed, as Explorer shows a link by what it points to.
            let metadata = fs::metadata(entry.path()).ok()?;
            Some((name, metadata))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found.into())
}

/// FILETIME of `time`: hundreds of nanoseconds since 1601; 0 when unknown.
pub(crate) fn filetime(time: Option<SystemTime>) -> i64 {
    let Some(time) = time else {
        return 0;
    };
    let since_1970 = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_nanos() / 100).unwrap_or(i64::MAX),
        Err(before) => -i64::try_from(before.duration().as_nanos() / 100).unwrap_or(i64::MAX),
    };
    FILETIME_UNIX_EPOCH.saturating_add(since_1970)
}

/// The time a FILETIME stands for, when it names one: 0 and -1 mean "leave it".
fn system_time(filetime: i64) -> Option<SystemTime> {
    if filetime <= 0 {
        return None;
    }
    let since_1970 = filetime - FILETIME_UNIX_EPOCH;
    let hundreds = since_1970.unsigned_abs();
    let offset = std::time::Duration::from_nanos(hundreds.checked_mul(100)?);
    if since_1970 >= 0 {
        UNIX_EPOCH.checked_add(offset)
    } else {
        UNIX_EPOCH.checked_sub(offset)
    }
}

/// The attributes Windows would show for a file with `metadata`, named `name`.
fn attributes(metadata: &Metadata, name: &str) -> FileAttributes {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        let _ = name;
        FileAttributes::from_bits_retain(metadata.file_attributes())
    }
    #[cfg(not(windows))]
    {
        let mut attributes = if metadata.is_dir() {
            FileAttributes::FILE_ATTRIBUTE_DIRECTORY
        } else {
            FileAttributes::FILE_ATTRIBUTE_ARCHIVE
        };
        if metadata.permissions().readonly() {
            attributes |= FileAttributes::FILE_ATTRIBUTE_READONLY;
        }
        if name.starts_with('.') {
            attributes |= FileAttributes::FILE_ATTRIBUTE_HIDDEN;
        }
        attributes
    }
}

/// The four times of `metadata`: creation, last access, last write, change.
fn times(metadata: &Metadata) -> (i64, i64, i64, i64) {
    let written = filetime(metadata.modified().ok());
    (
        filetime(metadata.created().ok()),
        filetime(metadata.accessed().ok()),
        written,
        written,
    )
}

fn size(metadata: &Metadata) -> i64 {
    if metadata.is_dir() {
        0
    } else {
        i64::try_from(metadata.len()).unwrap_or(i64::MAX)
    }
}

/// `level` of what is known of an open file, when this backend answers it.
fn information(
    level: &FileInformationClassLevel,
    metadata: &Metadata,
    name: &str,
    delete_pending: bool,
) -> Option<FileInformationClass> {
    let file_attributes = attributes(metadata, name);
    let (creation_time, last_access_time, last_write_time, change_time) = times(metadata);
    let boolean = |value: bool| if value { Boolean::True } else { Boolean::False };
    Some(match *level {
        FileInformationClassLevel::FILE_BASIC_INFORMATION => {
            FileInformationClass::Basic(FileBasicInformation {
                creation_time,
                last_access_time,
                last_write_time,
                change_time,
                file_attributes,
            })
        }
        FileInformationClassLevel::FILE_STANDARD_INFORMATION => {
            FileInformationClass::Standard(FileStandardInformation {
                allocation_size: size(metadata),
                end_of_file: size(metadata),
                number_of_links: 1,
                delete_pending: boolean(delete_pending),
                directory: boolean(metadata.is_dir()),
            })
        }
        FileInformationClassLevel::FILE_ATTRIBUTE_TAG_INFORMATION => {
            FileInformationClass::AttributeTag(FileAttributeTagInformation {
                file_attributes,
                reparse_tag: 0,
            })
        }
        _ => return None,
    })
}

/// One directory entry at `level`, when this backend answers it.
fn entry(
    level: &FileInformationClassLevel,
    metadata: &Metadata,
    name: String,
) -> Option<FileInformationClass> {
    let attributes = attributes(metadata, &name);
    let (created, accessed, written, changed) = times(metadata);
    let size = size(metadata);
    Some(match *level {
        FileInformationClassLevel::FILE_BOTH_DIRECTORY_INFORMATION => {
            FileInformationClass::BothDirectory(FileBothDirectoryInformation::new(
                created, accessed, written, changed, size, attributes, name,
            ))
        }
        FileInformationClassLevel::FILE_FULL_DIRECTORY_INFORMATION => {
            FileInformationClass::FullDirectory(FileFullDirectoryInformation::new(
                created, accessed, written, changed, size, attributes, name,
            ))
        }
        FileInformationClassLevel::FILE_DIRECTORY_INFORMATION => {
            FileInformationClass::Directory(FileDirectoryInformation::new(
                created, accessed, written, changed, size, attributes, name,
            ))
        }
        FileInformationClassLevel::FILE_NAMES_INFORMATION => {
            FileInformationClass::Names(FileNamesInformation::new(name))
        }
        _ => return None,
    })
}

/// What a set request changes of an open file.
fn set(
    opened: &mut Opened,
    root: &std::path::Path,
    change: &FileInformationClass,
) -> Result<(), NtStatus> {
    match change {
        FileInformationClass::EndOfFile(end) => {
            let file = opened
                .file
                .as_ref()
                .ok_or(NtStatus::from(FILE_IS_A_DIRECTORY))?;
            let length = u64::try_from(end.end_of_file).map_err(|_| NtStatus::UNSUCCESSFUL)?;
            file.set_len(length).map_err(|error| status(&error))
        }
        // Space is taken as written, not reserved ahead.
        FileInformationClass::Allocation(_) => Ok(()),
        FileInformationClass::Disposition(disposition) => {
            let delete = disposition.delete_pending != 0;
            if delete
                && opened.file.is_none()
                && fs::read_dir(&opened.path)
                    .map_err(|error| status(&error))?
                    .next()
                    .is_some()
            {
                return Err(NtStatus::DIRECTORY_NOT_EMPTY);
            }
            opened.delete_on_close = delete;
            Ok(())
        }
        FileInformationClass::Rename(rename) => {
            let target = path::resolve(root, &rename.file_name)
                .map_err(|_| NtStatus::from(OBJECT_NAME_INVALID))?;
            let replace = matches!(rename.replace_if_exists, Boolean::True);
            if !replace && fs::symlink_metadata(&target).is_ok() {
                return Err(NtStatus::OBJECT_NAME_COLLISION);
            }
            fs::rename(&opened.path, &target).map_err(|error| status(&error))?;
            opened.path = target;
            Ok(())
        }
        FileInformationClass::Basic(basic) => set_basic(opened, basic),
        _ => Err(NtStatus::NOT_SUPPORTED),
    }
}

/// Times and the read-only attribute of an open file, as a basic set request gives them.
fn set_basic(opened: &Opened, basic: &FileBasicInformation) -> Result<(), NtStatus> {
    if let Some(file) = &opened.file {
        let mut times = fs::FileTimes::new();
        if let Some(accessed) = system_time(basic.last_access_time) {
            times = times.set_accessed(accessed);
        }
        if let Some(written) = system_time(basic.last_write_time) {
            times = times.set_modified(written);
        }
        file.set_times(times).map_err(|error| status(&error))?;
    }
    // 0 leaves the attributes as they are.
    if !basic.file_attributes.is_empty() {
        let metadata = fs::metadata(&opened.path).map_err(|error| status(&error))?;
        let mut permissions = metadata.permissions();
        let readonly = basic
            .file_attributes
            .contains(FileAttributes::FILE_ATTRIBUTE_READONLY);
        if permissions.readonly() != readonly {
            #[allow(
                clippy::permissions_set_readonly_false,
                reason = "the server clears the attribute: the file becomes writable for all \
                          as Windows' read-only attribute works, on this side's rights"
            )]
            permissions.set_readonly(readonly);
            fs::set_permissions(&opened.path, permissions).map_err(|error| status(&error))?;
        }
    }
    Ok(())
}

/// The attributes of the file systems shared, as the server is told them.
fn file_system_attributes() -> FileSystemAttributes {
    let attributes = FileSystemAttributes::FILE_CASE_PRESERVED_NAMES
        | FileSystemAttributes::FILE_UNICODE_ON_DISK;
    if cfg!(windows) {
        attributes
    } else {
        attributes | FileSystemAttributes::FILE_CASE_SENSITIVE_SEARCH
    }
}

/// The size and free space of a drive.
mod space {
    use std::path::Path;

    use ironrdp::rdpdr::pdu::efs::{FileSystemInformationClass, FileSystemInformationClassLevel};

    /// `level` of the size of the drive at `root`, when it can be read.
    #[cfg(unix)]
    pub(super) fn information(
        level: &FileSystemInformationClassLevel,
        root: &Path,
    ) -> Option<FileSystemInformationClass> {
        use ironrdp::rdpdr::pdu::efs::{FileFsFullSizeInformation, FileFsSizeInformation};

        let stat = rustix::fs::statvfs(root).ok()?;
        let units = |blocks: u64| i64::try_from(blocks).unwrap_or(i64::MAX);
        let unit = u32::try_from(stat.f_frsize).ok()?;
        match *level {
            FileSystemInformationClassLevel::FILE_FS_SIZE_INFORMATION => Some(
                FileSystemInformationClass::FileFsSizeInformation(FileFsSizeInformation {
                    total_alloc_units: units(stat.f_blocks),
                    available_alloc_units: units(stat.f_bavail),
                    sectors_per_alloc_unit: 1,
                    bytes_per_sector: unit,
                }),
            ),
            FileSystemInformationClassLevel::FILE_FS_FULL_SIZE_INFORMATION => Some(
                FileSystemInformationClass::FileFsFullSizeInformation(FileFsFullSizeInformation {
                    total_alloc_units: units(stat.f_blocks),
                    caller_available_alloc_units: units(stat.f_bavail),
                    actual_available_alloc_units: units(stat.f_bfree),
                    sectors_per_alloc_unit: 1,
                    bytes_per_sector: unit,
                }),
            ),
            _ => None,
        }
    }

    /// Not told: the standard library has no safe way to read a drive's free space here.
    #[cfg(not(unix))]
    pub(super) fn information(
        _level: &FileSystemInformationClassLevel,
        _root: &Path,
    ) -> Option<FileSystemInformationClass> {
        None
    }
}

#[cfg(test)]
mod tests {
    use ironrdp::rdpdr::pdu::efs::{
        DesiredAccess, FileDispositionInformation, FileEndOfFileInformation, FileRenameInformation,
        MajorFunction, MinorFunction, ServerDriveNotifyChangeDirectoryRequest, SharedAccess,
    };

    use super::*;

    const READ: u32 = 0x0000_0001;
    const WRITE: u32 = 0x0000_0002;

    /// A backend sharing a fresh folder as drive "T", device 0.
    fn backend() -> (tempfile::TempDir, DriveBackend) {
        let root = tempfile::tempdir().expect("temp dir");
        let backend = DriveBackend::new(&[SharedDrive {
            name: "T".to_owned(),
            root: root.path().to_path_buf(),
        }]);
        (root, backend)
    }

    fn io(file_id: u32, major_function: MajorFunction) -> DeviceIoRequest {
        DeviceIoRequest {
            device_id: 0,
            file_id,
            completion_id: 7,
            major_function,
            minor_function: MinorFunction::from(0),
        }
    }

    fn create_on(
        backend: &mut DriveBackend,
        device_id: u32,
        path: &str,
        disposition: CreateDisposition,
        options: CreateOptions,
        access: u32,
    ) -> DeviceCreateResponse {
        backend.create(&DeviceCreateRequest {
            device_io_request: DeviceIoRequest {
                device_id,
                ..io(0, MajorFunction::Create)
            },
            desired_access: DesiredAccess::from_bits_retain(access),
            allocation_size: 0,
            file_attributes: FileAttributes::empty(),
            shared_access: SharedAccess::empty(),
            create_disposition: disposition,
            create_options: options,
            path: path.to_owned(),
        })
    }

    fn open(
        backend: &mut DriveBackend,
        path: &str,
        disposition: CreateDisposition,
        options: CreateOptions,
        access: u32,
    ) -> DeviceCreateResponse {
        create_on(backend, 0, path, disposition, options, access)
    }

    /// The id of `path`, opened as `disposition` says; the test fails if it is refused.
    fn opened(backend: &mut DriveBackend, path: &str, disposition: CreateDisposition) -> u32 {
        let response = open(
            backend,
            path,
            disposition,
            CreateOptions::empty(),
            READ | WRITE,
        );
        assert_eq!(status_of(&response), NtStatus::SUCCESS, "{path}");
        response.file_id
    }

    fn status_of(response: &DeviceCreateResponse) -> NtStatus {
        response.device_io_reply.io_status
    }

    fn close(backend: &mut DriveBackend, file_id: u32) {
        let _ = backend.close(DeviceCloseRequest {
            device_io_request: io(file_id, MajorFunction::Close),
        });
    }

    fn read(
        backend: &mut DriveBackend,
        file_id: u32,
        offset: u64,
        length: u32,
    ) -> DeviceReadResponse {
        backend.read(&DeviceReadRequest {
            device_io_request: io(file_id, MajorFunction::Read),
            length,
            offset,
        })
    }

    fn write(backend: &mut DriveBackend, file_id: u32, offset: u64, data: &[u8]) -> NtStatus {
        let response = backend.write(&DeviceWriteRequest {
            device_io_request: io(file_id, MajorFunction::Write),
            offset,
            write_data: data.to_vec(),
        });
        if response.device_io_reply.io_status == NtStatus::SUCCESS {
            assert_eq!(response.length as usize, data.len());
        }
        response.device_io_reply.io_status
    }

    fn set(backend: &mut DriveBackend, file_id: u32, change: FileInformationClass) -> NtStatus {
        backend.change(&ServerDriveSetInformationRequest {
            device_io_request: io(file_id, MajorFunction::SetInformation),
            set_buffer: change,
        })
    }

    fn delete(pending: bool) -> FileInformationClass {
        FileInformationClass::Disposition(FileDispositionInformation {
            delete_pending: u8::from(pending),
        })
    }

    /// The names a directory query at `query` hands out, one request after the other, and
    /// the status it ends on.
    fn listed(backend: &mut DriveBackend, folder: u32, query: &str) -> (Vec<String>, NtStatus) {
        let mut names = Vec::new();
        let mut initial_query = 1;
        loop {
            let response = backend.query_directory(&ServerDriveQueryDirectoryRequest {
                device_io_request: io(folder, MajorFunction::DirectoryControl),
                file_info_class_lvl: FileInformationClassLevel::FILE_NAMES_INFORMATION,
                initial_query,
                path: query.to_owned(),
            });
            match response.buffer {
                Some(FileInformationClass::Names(entry)) => names.push(entry.file_name),
                Some(other) => panic!("not a names entry: {other:?}"),
                None => return (names, response.device_io_reply.io_status),
            }
            assert!(names.len() < 100, "the listing never ends");
            initial_query = 0;
        }
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_file_is_created_written_and_read_back() {
        let (root, mut backend) = backend();
        let created = open(
            &mut backend,
            "\\notes.txt",
            CreateDisposition::FILE_CREATE,
            CreateOptions::FILE_NON_DIRECTORY_FILE,
            READ | WRITE,
        );
        assert_eq!(status_of(&created), NtStatus::SUCCESS);
        assert_eq!(
            created.information,
            Information::from_bits_retain(FILE_CREATED)
        );
        let file = created.file_id;
        assert_eq!(
            write(&mut backend, file, 0, b"hello world"),
            NtStatus::SUCCESS
        );
        assert_eq!(write(&mut backend, file, 6, b"there"), NtStatus::SUCCESS);
        let read_back = read(&mut backend, file, 1, 4);
        assert_eq!(read_back.device_io_reply.io_status, NtStatus::SUCCESS);
        assert_eq!(read_back.read_data, b"ello");
        assert_eq!(read(&mut backend, file, 6, 100).read_data, b"there");
        close(&mut backend, file);
        assert_eq!(
            fs::read(root.path().join("notes.txt")).expect("file"),
            b"hello there"
        );
        assert!(backend.opened.is_empty(), "closed");
        assert_eq!(
            read(&mut backend, file, 0, 1).device_io_reply.io_status,
            NtStatus::from(INVALID_HANDLE)
        );
        // Each open file has its own id.
        let first = opened(&mut backend, "\\notes.txt", CreateDisposition::FILE_OPEN);
        let second = opened(&mut backend, "\\notes.txt", CreateDisposition::FILE_OPEN);
        assert_ne!(first, second);
        assert_ne!(first, 0);
    }

    #[test]
    fn opening_what_is_missing_or_taken_is_refused() {
        let (root, mut backend) = backend();
        let kept = root.path().join("kept.txt");
        fs::write(&kept, b"content").expect("file");
        let any = CreateOptions::empty;
        let collision = open(
            &mut backend,
            "\\kept.txt",
            CreateDisposition::FILE_CREATE,
            any(),
            WRITE,
        );
        assert_eq!(status_of(&collision), NtStatus::OBJECT_NAME_COLLISION);
        for disposition in [
            CreateDisposition::FILE_OPEN,
            CreateDisposition::FILE_OVERWRITE,
        ] {
            let missing = open(&mut backend, "\\missing.txt", disposition, any(), WRITE);
            assert_eq!(status_of(&missing), NtStatus::from(OBJECT_NAME_NOT_FOUND));
        }
        assert!(!root.path().join("missing.txt").exists(), "nothing created");
        let no_folder = open(
            &mut backend,
            "\\nowhere\\x.txt",
            CreateDisposition::FILE_OPEN_IF,
            any(),
            WRITE,
        );
        assert_eq!(status_of(&no_folder), NtStatus::from(OBJECT_PATH_NOT_FOUND));
        let outside = open(
            &mut backend,
            "\\..\\x.txt",
            CreateDisposition::FILE_OPEN_IF,
            any(),
            WRITE,
        );
        assert_eq!(status_of(&outside), NtStatus::from(OBJECT_NAME_INVALID));
        let unknown = create_on(
            &mut backend,
            9,
            "\\kept.txt",
            CreateDisposition::FILE_OPEN,
            any(),
            READ,
        );
        assert_eq!(status_of(&unknown), NtStatus::from(INVALID_HANDLE));
    }

    #[test]
    fn opening_an_existing_file_follows_the_disposition() {
        let (root, mut backend) = backend();
        let kept = root.path().join("kept.txt");
        fs::write(&kept, b"content").expect("file");
        let any = CreateOptions::empty;
        let reading = open(
            &mut backend,
            "\\kept.txt",
            CreateDisposition::FILE_OPEN,
            any(),
            READ,
        );
        assert_eq!(reading.information, Information::FILE_OPENED);
        assert_eq!(
            read(&mut backend, reading.file_id, 0, 100).read_data,
            b"content"
        );
        // Opened to read only: the file system refuses a write.
        assert_ne!(
            write(&mut backend, reading.file_id, 0, b"x"),
            NtStatus::SUCCESS
        );
        let writing = open(
            &mut backend,
            "\\kept.txt",
            CreateDisposition::FILE_OPEN_IF,
            any(),
            WRITE,
        );
        assert_eq!(writing.information, Information::FILE_OPENED);
        assert_eq!(
            write(&mut backend, writing.file_id, 0, b"C"),
            NtStatus::SUCCESS
        );
        assert_eq!(fs::read(&kept).expect("file"), b"Content", "not truncated");

        let overwritten = open(
            &mut backend,
            "\\kept.txt",
            CreateDisposition::FILE_OVERWRITE_IF,
            any(),
            WRITE,
        );
        assert_eq!(overwritten.information, Information::FILE_OVERWRITTEN);
        assert_eq!(fs::read(&kept).expect("file"), b"");
        fs::write(&kept, b"again").expect("file");
        let superseded = open(
            &mut backend,
            "\\kept.txt",
            CreateDisposition::FILE_SUPERSEDE,
            any(),
            WRITE,
        );
        assert_eq!(superseded.information, Information::FILE_SUPERSEDED);
        assert_eq!(fs::read(&kept).expect("file"), b"");

        let made = open(
            &mut backend,
            "\\new.txt",
            CreateDisposition::FILE_OPEN_IF,
            any(),
            WRITE,
        );
        assert_eq!(
            made.information,
            Information::from_bits_retain(FILE_CREATED)
        );
        assert!(root.path().join("new.txt").is_file());
    }

    #[test]
    fn a_folder_is_not_a_file() {
        let (root, mut backend) = backend();
        fs::write(root.path().join("file.txt"), b"x").expect("file");
        let folder = CreateOptions::FILE_DIRECTORY_FILE;
        let created = open(
            &mut backend,
            "\\Docs",
            CreateDisposition::FILE_CREATE,
            folder.clone(),
            READ,
        );
        assert_eq!(status_of(&created), NtStatus::SUCCESS);
        assert_eq!(
            created.information,
            Information::from_bits_retain(FILE_CREATED)
        );
        assert!(root.path().join("Docs").is_dir());
        assert_eq!(
            read(&mut backend, created.file_id, 0, 1)
                .device_io_reply
                .io_status,
            NtStatus::from(FILE_IS_A_DIRECTORY)
        );
        let again = open(
            &mut backend,
            "\\Docs",
            CreateDisposition::FILE_CREATE,
            folder.clone(),
            READ,
        );
        assert_eq!(status_of(&again), NtStatus::OBJECT_NAME_COLLISION);
        let as_file = open(
            &mut backend,
            "\\Docs",
            CreateDisposition::FILE_OPEN,
            CreateOptions::FILE_NON_DIRECTORY_FILE,
            READ,
        );
        assert_eq!(status_of(&as_file), NtStatus::from(FILE_IS_A_DIRECTORY));
        let as_folder = open(
            &mut backend,
            "\\file.txt",
            CreateDisposition::FILE_OPEN,
            folder,
            READ,
        );
        assert_eq!(status_of(&as_folder), NtStatus::NOT_A_DIRECTORY);
        let the_root = open(
            &mut backend,
            "\\",
            CreateDisposition::FILE_OPEN,
            CreateOptions::empty(),
            READ,
        );
        assert_eq!(the_root.information, Information::FILE_OPENED);
    }

    #[test]
    fn a_folder_lists_what_matches_one_entry_at_a_time() {
        let (root, mut backend) = backend();
        fs::write(root.path().join("b.log"), b"").expect("file");
        fs::write(root.path().join("a.txt"), b"").expect("file");
        fs::create_dir(root.path().join("sub")).expect("folder");
        fs::write(root.path().join("sub").join("inner.txt"), b"").expect("file");
        let folder = opened(&mut backend, "\\", CreateDisposition::FILE_OPEN);
        assert_eq!(
            listed(&mut backend, folder, "\\*"),
            (names(&["a.txt", "b.log", "sub"]), NtStatus::NO_MORE_FILES)
        );
        assert_eq!(
            listed(&mut backend, folder, "\\*.TXT"),
            (names(&["a.txt"]), NtStatus::NO_MORE_FILES)
        );
        assert_eq!(
            listed(&mut backend, folder, "\\sub\\*"),
            (names(&["inner.txt"]), NtStatus::NO_MORE_FILES)
        );
        assert_eq!(
            listed(&mut backend, folder, "\\*.none"),
            (Vec::new(), NtStatus::NO_SUCH_FILE)
        );
        assert_eq!(
            listed(&mut backend, folder, "\\..\\*"),
            (Vec::new(), NtStatus::from(OBJECT_NAME_INVALID))
        );
        assert_eq!(
            listed(&mut backend, 99, "\\*"),
            (Vec::new(), NtStatus::from(INVALID_HANDLE))
        );
    }

    #[test]
    fn an_entry_tells_its_size_times_and_kind() {
        let (root, mut backend) = backend();
        fs::write(root.path().join("five.bin"), b"12345").expect("file");
        fs::create_dir(root.path().join("dir")).expect("folder");
        let folder = opened(&mut backend, "\\", CreateDisposition::FILE_OPEN);
        let response = backend.query_directory(&ServerDriveQueryDirectoryRequest {
            device_io_request: io(folder, MajorFunction::DirectoryControl),
            file_info_class_lvl: FileInformationClassLevel::FILE_BOTH_DIRECTORY_INFORMATION,
            initial_query: 1,
            path: "\\five.bin".to_owned(),
        });
        let Some(FileInformationClass::BothDirectory(entry)) = response.buffer else {
            panic!("no entry: {:?}", response.device_io_reply.io_status);
        };
        assert_eq!(entry.file_name, "five.bin");
        assert_eq!(entry.end_of_file, 5);
        assert!(
            !entry
                .file_attributes
                .contains(FileAttributes::FILE_ATTRIBUTE_DIRECTORY)
        );
        let modified = fs::metadata(root.path().join("five.bin"))
            .expect("file")
            .modified();
        let written = filetime(modified.ok());
        assert_eq!(entry.last_write_time, written);
        assert!(written > FILETIME_UNIX_EPOCH, "after 1970");

        let file = opened(&mut backend, "\\five.bin", CreateDisposition::FILE_OPEN);
        let dir = opened(&mut backend, "\\dir", CreateDisposition::FILE_OPEN);
        let query = |backend: &DriveBackend, file_id, file_info_class_lvl| {
            backend
                .query_information(&ServerDriveQueryInformationRequest {
                    device_io_request: io(file_id, MajorFunction::QueryInformation),
                    file_info_class_lvl,
                })
                .buffer
        };
        let standard = FileInformationClassLevel::FILE_STANDARD_INFORMATION;
        let Some(FileInformationClass::Standard(of_file)) = query(&backend, file, standard.clone())
        else {
            panic!("not standard");
        };
        assert_eq!(
            (of_file.end_of_file, of_file.directory),
            (5, Boolean::False)
        );
        let Some(FileInformationClass::Standard(of_dir)) = query(&backend, dir, standard) else {
            panic!("not standard");
        };
        assert_eq!((of_dir.end_of_file, of_dir.directory), (0, Boolean::True));
        let basic = FileInformationClassLevel::FILE_BASIC_INFORMATION;
        let Some(FileInformationClass::Basic(of_dir)) = query(&backend, dir, basic.clone()) else {
            panic!("not basic");
        };
        assert!(
            of_dir
                .file_attributes
                .contains(FileAttributes::FILE_ATTRIBUTE_DIRECTORY)
        );
        assert!(query(&backend, 99, basic).is_none(), "unknown file");
    }

    #[test]
    fn a_file_deleted_on_close_is_gone_once_closed() {
        let (root, mut backend) = backend();
        let temporary = open(
            &mut backend,
            "\\scratch.tmp",
            CreateDisposition::FILE_CREATE,
            CreateOptions::FILE_DELETE_ON_CLOSE,
            READ | WRITE,
        );
        assert!(
            root.path().join("scratch.tmp").is_file(),
            "there while open"
        );
        close(&mut backend, temporary.file_id);
        assert!(!root.path().join("scratch.tmp").exists());

        fs::write(root.path().join("doomed.txt"), b"x").expect("file");
        let doomed = opened(&mut backend, "\\doomed.txt", CreateDisposition::FILE_OPEN);
        assert_eq!(set(&mut backend, doomed, delete(true)), NtStatus::SUCCESS);
        assert!(
            root.path().join("doomed.txt").exists(),
            "deleted at the close only"
        );
        close(&mut backend, doomed);
        assert!(!root.path().join("doomed.txt").exists());

        fs::create_dir(root.path().join("full")).expect("folder");
        fs::write(root.path().join("full").join("x"), b"").expect("file");
        let full = opened(&mut backend, "\\full", CreateDisposition::FILE_OPEN);
        assert_eq!(
            set(&mut backend, full, delete(true)),
            NtStatus::DIRECTORY_NOT_EMPTY
        );
        close(&mut backend, full);
        assert!(root.path().join("full").join("x").exists());

        fs::create_dir(root.path().join("empty")).expect("folder");
        let empty = opened(&mut backend, "\\empty", CreateDisposition::FILE_OPEN);
        assert_eq!(set(&mut backend, empty, delete(true)), NtStatus::SUCCESS);
        assert_eq!(set(&mut backend, empty, delete(false)), NtStatus::SUCCESS);
        close(&mut backend, empty);
        assert!(
            root.path().join("empty").is_dir(),
            "the deletion was taken back"
        );
        let gone = opened(&mut backend, "\\empty", CreateDisposition::FILE_OPEN);
        assert_eq!(set(&mut backend, gone, delete(true)), NtStatus::SUCCESS);
        close(&mut backend, gone);
        assert!(!root.path().join("empty").exists());
    }

    #[test]
    fn a_rename_moves_the_file_and_keeps_it_open() {
        let (root, mut backend) = backend();
        fs::write(root.path().join("old.txt"), b"data").expect("file");
        fs::write(root.path().join("taken.txt"), b"other").expect("file");
        let file = opened(&mut backend, "\\old.txt", CreateDisposition::FILE_OPEN);
        let rename = |to: &str, replace_if_exists| {
            FileInformationClass::Rename(FileRenameInformation {
                replace_if_exists,
                file_name: to.to_owned(),
            })
        };
        assert_eq!(
            set(&mut backend, file, rename("\\taken.txt", Boolean::False)),
            NtStatus::OBJECT_NAME_COLLISION
        );
        assert_eq!(
            fs::read(root.path().join("taken.txt")).expect("file"),
            b"other"
        );
        assert_eq!(
            set(&mut backend, file, rename("\\..\\out.txt", Boolean::True)),
            NtStatus::from(OBJECT_NAME_INVALID)
        );
        assert_eq!(
            set(&mut backend, file, rename("\\new.txt", Boolean::False)),
            NtStatus::SUCCESS
        );
        assert!(!root.path().join("old.txt").exists());
        assert_eq!(
            fs::read(root.path().join("new.txt")).expect("file"),
            b"data"
        );
        assert_eq!(
            set(&mut backend, file, rename("\\taken.txt", Boolean::True)),
            NtStatus::SUCCESS
        );
        assert_eq!(
            fs::read(root.path().join("taken.txt")).expect("file"),
            b"data"
        );
        // It follows the file: deleting it now deletes it under its new name.
        assert_eq!(set(&mut backend, file, delete(true)), NtStatus::SUCCESS);
        close(&mut backend, file);
        assert!(!root.path().join("taken.txt").exists());
    }

    #[test]
    fn the_end_of_a_file_is_set() {
        let (root, mut backend) = backend();
        fs::write(root.path().join("long.txt"), b"0123456789").expect("file");
        let file = opened(&mut backend, "\\long.txt", CreateDisposition::FILE_OPEN);
        let end =
            |end_of_file| FileInformationClass::EndOfFile(FileEndOfFileInformation { end_of_file });
        assert_eq!(set(&mut backend, file, end(4)), NtStatus::SUCCESS);
        assert_eq!(
            fs::read(root.path().join("long.txt")).expect("file"),
            b"0123"
        );
        assert_eq!(set(&mut backend, file, end(-1)), NtStatus::UNSUCCESSFUL);
        let folder = opened(&mut backend, "\\", CreateDisposition::FILE_OPEN);
        assert_eq!(
            set(&mut backend, folder, end(0)),
            NtStatus::from(FILE_IS_A_DIRECTORY)
        );
        assert_eq!(
            set(&mut backend, 99, end(0)),
            NtStatus::from(INVALID_HANDLE)
        );
    }

    #[test]
    fn a_read_is_bounded() {
        let (root, mut backend) = backend();
        let size = usize::try_from(MAX_READ).expect("size") + 10;
        fs::write(root.path().join("big.bin"), vec![1_u8; size]).expect("file");
        let file = opened(&mut backend, "\\big.bin", CreateDisposition::FILE_OPEN);
        assert_eq!(
            read(&mut backend, file, 0, u32::MAX).read_data.len(),
            size - 10
        );
        let near_end = u64::try_from(size).expect("size") - 3;
        assert_eq!(read(&mut backend, file, near_end, 100).read_data.len(), 3);
    }

    #[test]
    fn a_volume_tells_its_name_and_file_system() {
        let (_root, backend) = backend();
        let query = |device_id, fs_info_class_lvl| {
            backend.query_volume(&ServerDriveQueryVolumeInformationRequest {
                device_io_request: DeviceIoRequest {
                    device_id,
                    ..io(0, MajorFunction::QueryVolumeInformation)
                },
                fs_info_class_lvl,
            })
        };
        let Some(FileSystemInformationClass::FileFsVolumeInformation(volume)) = query(
            0,
            FileSystemInformationClassLevel::FILE_FS_VOLUME_INFORMATION,
        )
        .buffer
        else {
            panic!("no volume");
        };
        assert_eq!(volume.volume_label, "T");
        let Some(FileSystemInformationClass::FileFsAttributeInformation(attributes)) = query(
            0,
            FileSystemInformationClassLevel::FILE_FS_ATTRIBUTE_INFORMATION,
        )
        .buffer
        else {
            panic!("no attributes");
        };
        assert_eq!(attributes.file_system_name, "NTFS");
        assert_eq!(attributes.max_component_name_len, 255);
        let Some(FileSystemInformationClass::FileFsDeviceInformation(device)) = query(
            0,
            FileSystemInformationClassLevel::FILE_FS_DEVICE_INFORMATION,
        )
        .buffer
        else {
            panic!("no device");
        };
        assert_eq!(device.device_type, FILE_DEVICE_DISK);
        let full = query(
            0,
            FileSystemInformationClassLevel::FILE_FS_FULL_SIZE_INFORMATION,
        );
        let size = query(0, FileSystemInformationClassLevel::FILE_FS_SIZE_INFORMATION);
        if cfg!(unix) {
            let Some(FileSystemInformationClass::FileFsFullSizeInformation(full)) = full.buffer
            else {
                panic!("no full size");
            };
            assert!(full.total_alloc_units > 0);
            assert!(full.actual_available_alloc_units >= full.caller_available_alloc_units);
            let Some(FileSystemInformationClass::FileFsSizeInformation(size)) = size.buffer else {
                panic!("no size");
            };
            assert_eq!(size.total_alloc_units, full.total_alloc_units);
        } else {
            assert_eq!(full.device_io_reply.io_status, NtStatus::NOT_SUPPORTED);
            assert_eq!(size.device_io_reply.io_status, NtStatus::NOT_SUPPORTED);
        }
        assert_eq!(
            query(
                9,
                FileSystemInformationClassLevel::FILE_FS_VOLUME_INFORMATION
            )
            .device_io_reply
            .io_status,
            NtStatus::from(INVALID_HANDLE)
        );
    }

    #[test]
    fn filetimes_count_from_1601() {
        assert_eq!(filetime(Some(UNIX_EPOCH)), FILETIME_UNIX_EPOCH);
        let later = UNIX_EPOCH + std::time::Duration::from_secs(1);
        assert_eq!(filetime(Some(later)), FILETIME_UNIX_EPOCH + 10_000_000);
        let earlier = UNIX_EPOCH - std::time::Duration::from_secs(1);
        assert_eq!(filetime(Some(earlier)), FILETIME_UNIX_EPOCH - 10_000_000);
        assert_eq!(filetime(None), 0);
        assert_eq!(system_time(FILETIME_UNIX_EPOCH + 10_000_000), Some(later));
        assert_eq!(system_time(FILETIME_UNIX_EPOCH - 10_000_000), Some(earlier));
        assert_eq!(system_time(0), None);
        assert_eq!(system_time(-1), None);
    }

    #[test]
    fn times_set_by_the_server_are_kept() {
        let (root, mut backend) = backend();
        fs::write(root.path().join("dated.txt"), b"x").expect("file");
        let file = opened(&mut backend, "\\dated.txt", CreateDisposition::FILE_OPEN);
        let written = FILETIME_UNIX_EPOCH + 1_000_000_000 * 10_000_000;
        let basic = |last_write_time, file_attributes| {
            FileInformationClass::Basic(FileBasicInformation {
                creation_time: 0,
                last_access_time: 0,
                last_write_time,
                change_time: 0,
                file_attributes,
            })
        };
        assert_eq!(
            set(&mut backend, file, basic(written, FileAttributes::empty())),
            NtStatus::SUCCESS
        );
        let metadata = fs::metadata(root.path().join("dated.txt")).expect("file");
        assert_eq!(filetime(metadata.modified().ok()), written);
        assert!(!metadata.permissions().readonly());
        assert_eq!(
            set(
                &mut backend,
                file,
                basic(0, FileAttributes::FILE_ATTRIBUTE_READONLY)
            ),
            NtStatus::SUCCESS
        );
        let metadata = fs::metadata(root.path().join("dated.txt")).expect("file");
        assert!(metadata.permissions().readonly());
        assert_eq!(filetime(metadata.modified().ok()), written, "0 leaves it");
        assert_eq!(
            set(&mut backend, file, basic(written, FileAttributes::empty())),
            NtStatus::SUCCESS
        );
        assert!(
            fs::metadata(root.path().join("dated.txt"))
                .expect("file")
                .permissions()
                .readonly(),
            "no attribute given leaves them as they are"
        );
        assert_eq!(
            set(
                &mut backend,
                file,
                basic(0, FileAttributes::FILE_ATTRIBUTE_ARCHIVE)
            ),
            NtStatus::SUCCESS
        );
        assert!(
            !fs::metadata(root.path().join("dated.txt"))
                .expect("file")
                .permissions()
                .readonly()
        );
    }

    #[test]
    fn every_listing_level_is_answered_in_its_own_form() {
        let (root, mut backend) = backend();
        fs::write(root.path().join("one.txt"), b"1").expect("file");
        let folder = opened(&mut backend, "\\", CreateDisposition::FILE_OPEN);
        let mut entry = |file_info_class_lvl| {
            backend
                .query_directory(&ServerDriveQueryDirectoryRequest {
                    device_io_request: io(folder, MajorFunction::DirectoryControl),
                    file_info_class_lvl,
                    initial_query: 1,
                    path: "\\*".to_owned(),
                })
                .buffer
        };
        assert!(matches!(
            entry(FileInformationClassLevel::FILE_BOTH_DIRECTORY_INFORMATION),
            Some(FileInformationClass::BothDirectory(_))
        ));
        assert!(matches!(
            entry(FileInformationClassLevel::FILE_FULL_DIRECTORY_INFORMATION),
            Some(FileInformationClass::FullDirectory(_))
        ));
        assert!(matches!(
            entry(FileInformationClassLevel::FILE_DIRECTORY_INFORMATION),
            Some(FileInformationClass::Directory(_))
        ));
        assert!(matches!(
            entry(FileInformationClassLevel::FILE_NAMES_INFORMATION),
            Some(FileInformationClass::Names(_))
        ));
        assert!(entry(FileInformationClassLevel::FILE_ATTRIBUTE_TAG_INFORMATION).is_none());
    }

    #[test]
    fn a_deletion_asked_shows_as_pending() {
        let (root, mut backend) = backend();
        fs::write(root.path().join("x.txt"), b"").expect("file");
        let file = opened(&mut backend, "\\x.txt", CreateDisposition::FILE_OPEN);
        let pending = |backend: &DriveBackend| match backend
            .query_information(&ServerDriveQueryInformationRequest {
                device_io_request: io(file, MajorFunction::QueryInformation),
                file_info_class_lvl: FileInformationClassLevel::FILE_STANDARD_INFORMATION,
            })
            .buffer
        {
            Some(FileInformationClass::Standard(standard)) => standard.delete_pending,
            other => panic!("not standard: {other:?}"),
        };
        assert_eq!(pending(&backend), Boolean::False);
        assert_eq!(set(&mut backend, file, delete(true)), NtStatus::SUCCESS);
        assert_eq!(pending(&backend), Boolean::True);
    }

    #[cfg(unix)]
    #[test]
    fn attributes_are_told_as_windows_shows_them() {
        let (root, mut backend) = backend();
        fs::write(root.path().join(".hidden"), b"").expect("file");
        fs::write(root.path().join("locked"), b"").expect("file");
        let mut permissions = fs::metadata(root.path().join("locked"))
            .expect("file")
            .permissions();
        permissions.set_readonly(true);
        fs::set_permissions(root.path().join("locked"), permissions).expect("read-only");
        fs::create_dir(root.path().join("dir")).expect("folder");
        let of = |backend: &mut DriveBackend, path: &str| {
            let file = opened(backend, path, CreateDisposition::FILE_OPEN);
            match backend
                .query_information(&ServerDriveQueryInformationRequest {
                    device_io_request: io(file, MajorFunction::QueryInformation),
                    file_info_class_lvl: FileInformationClassLevel::FILE_ATTRIBUTE_TAG_INFORMATION,
                })
                .buffer
            {
                Some(FileInformationClass::AttributeTag(tag)) => tag.file_attributes,
                other => panic!("no attributes: {other:?}"),
            }
        };
        assert_eq!(
            of(&mut backend, "\\.hidden"),
            FileAttributes::FILE_ATTRIBUTE_ARCHIVE | FileAttributes::FILE_ATTRIBUTE_HIDDEN
        );
        let locked = opened_read_only(&mut backend, "\\locked");
        assert_eq!(
            locked,
            FileAttributes::FILE_ATTRIBUTE_ARCHIVE | FileAttributes::FILE_ATTRIBUTE_READONLY
        );
        assert_eq!(
            of(&mut backend, "\\dir"),
            FileAttributes::FILE_ATTRIBUTE_DIRECTORY
        );
        assert!(
            file_system_attributes().contains(FileSystemAttributes::FILE_CASE_SENSITIVE_SEARCH),
            "names differing by case are two files here"
        );
    }

    /// The attributes of `path`, opened to read only: a read-only file opens no other way.
    #[cfg(unix)]
    fn opened_read_only(backend: &mut DriveBackend, path: &str) -> FileAttributes {
        let response = open(
            backend,
            path,
            CreateDisposition::FILE_OPEN,
            CreateOptions::empty(),
            READ,
        );
        assert_eq!(status_of(&response), NtStatus::SUCCESS);
        let metadata = fs::metadata(&backend.opened[&response.file_id].path).expect("file");
        attributes(
            &metadata,
            &file_name(&backend.opened[&response.file_id].path),
        )
    }

    #[test]
    fn a_failure_is_told_as_windows_tells_it() {
        use io::ErrorKind;
        for (kind, expected) in [
            (ErrorKind::NotFound, NtStatus::from(OBJECT_NAME_NOT_FOUND)),
            (ErrorKind::PermissionDenied, NtStatus::ACCESS_DENIED),
            (ErrorKind::AlreadyExists, NtStatus::OBJECT_NAME_COLLISION),
            (ErrorKind::DirectoryNotEmpty, NtStatus::DIRECTORY_NOT_EMPTY),
            (ErrorKind::NotADirectory, NtStatus::NOT_A_DIRECTORY),
            (ErrorKind::IsADirectory, NtStatus::from(FILE_IS_A_DIRECTORY)),
            (ErrorKind::StorageFull, NtStatus::from(DISK_FULL)),
            (ErrorKind::Other, NtStatus::UNSUCCESSFUL),
        ] {
            assert_eq!(status(&io::Error::from(kind)), expected, "{kind:?}");
        }
        if cfg!(windows) {
            assert_eq!(
                status(&io::Error::from_raw_os_error(32)),
                NtStatus::from(SHARING_VIOLATION)
            );
            assert_eq!(
                status(&io::Error::from_raw_os_error(112)),
                NtStatus::from(DISK_FULL)
            );
        }
    }

    #[test]
    fn the_drives_are_announced_in_order() {
        let drives = ["C", "D", "E"].map(|name| SharedDrive {
            name: name.to_owned(),
            root: PathBuf::from(name),
        });
        let backend = DriveBackend::new(&drives);
        assert_eq!(
            backend.devices(),
            vec![
                (0, "C".to_owned()),
                (1, "D".to_owned()),
                (2, "E".to_owned())
            ]
        );
    }

    #[test]
    fn a_change_notification_is_left_waiting_and_anything_else_answered() {
        let (_root, mut backend) = backend();
        let notify = ServerDriveIoRequest::ServerDriveNotifyChangeDirectoryRequest(
            ServerDriveNotifyChangeDirectoryRequest {
                device_io_request: io(1, MajorFunction::DirectoryControl),
                watch_tree: 0,
                completion_filter: 0,
            },
        );
        assert!(
            backend
                .handle_drive_io_request(notify)
                .expect("answered")
                .is_empty()
        );
        let unsupported = ServerDriveIoRequest::Unsupported(io(1, MajorFunction::QuerySecurity));
        assert_eq!(
            backend
                .handle_drive_io_request(unsupported)
                .expect("answered")
                .len(),
            1
        );
    }

    /// The vendored patch: a request the crate cannot decode is answered, the session goes on.
    #[test]
    fn a_request_not_understood_is_answered_not_fatal() {
        for major_function in [MajorFunction::QuerySecurity, MajorFunction::Read] {
            let request = io(1, major_function);
            let decoded = ServerDriveIoRequest::decode(
                request.clone(),
                &mut ironrdp_core::ReadCursor::new(&[]),
            )
            .expect("decoded");
            assert_eq!(decoded, ServerDriveIoRequest::Unsupported(request));
        }
    }

    /// The vendored patch: the short name of a drive, the only one xrdp reads, is its name.
    #[test]
    fn a_drive_is_announced_by_its_name() {
        use ironrdp::rdpdr::pdu::efs::{ClientDeviceListAnnounce, Devices};

        for (name, short) in [("C", b"C\0\0\0\0\0\0\0"), ("root", b"root\0\0\0\0")] {
            let mut devices = Devices::new();
            devices.add_drive(3, name.to_owned());
            let announce = ClientDeviceListAnnounce {
                device_list: devices.clone_inner(),
            };
            let mut bytes = vec![0; announce.size()];
            announce
                .encode(&mut ironrdp_core::WriteCursor::new(&mut bytes))
                .expect("encoded");
            // Device count, type and id come first, 4 bytes each.
            assert_eq!(&bytes[12..20], short, "{name}");
        }
    }

    #[test]
    fn this_computer_has_a_drive_to_share() {
        let drives = local_drives();
        assert!(!drives.is_empty());
        assert!(drives.iter().all(|drive| drive.root.is_dir()));
        if cfg!(windows) {
            assert!(drives.iter().all(|drive| drive.name.len() == 1));
        } else {
            assert_eq!(drives[0].root, PathBuf::from("/"));
        }
    }
}
