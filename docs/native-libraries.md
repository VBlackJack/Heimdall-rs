[Version française](fr/native-libraries.md)

# Native libraries loaded on Linux

This page lists every C library the `heimdall-rs` binary needs on Linux, how it reaches
it, and what happens when it is missing. It is written for deployment on hardened hosts
(ANSSI recommendations, Securix-style systems), where each shared object a program maps
has to be known in advance.

The binary is pure Rust except for the libraries below. It brings no GTK, no libdbus
(neither linked nor opened), no OpenSSL, no fontconfig or FreeType library: D-Bus is spoken by `zbus`, TLS by
`rustls` on `ring`, fonts are parsed by `fontdb` and `ttf-parser`.

Inventory taken from `Cargo.lock` at commit `c6c8463` (`master`, 2026-10-10), target
`x86_64-unknown-linux-gnu`, release profile. A dependency bump can change it: see
[Keeping this page true](#keeping-this-page-true).

## Summary

| Category | Libraries |
|---|---|
| Linked: in `DT_NEEDED`, the binary does not start without them | `libc.so.6`, `libm.so.6`, `libgcc_s.so.1`, `ld-linux-x86-64.so.2` (glibc and the GCC runtime), `libasound.so.2` (ALSA) |
| `dlopen`, required in a Wayland session | `libwayland-client.so.0`, `libxkbcommon.so.0` |
| `dlopen`, required in an X11 session | `libX11.so.6`, `libX11-xcb.so.1`, `libXcursor.so.1`, `libXi.so.6`, `libxcb.so.1`, `libxkbcommon.so.0`, `libxkbcommon-x11.so.0` |
| `dlopen`, optional with a fallback | `libvulkan.so.1`, `libEGL.so.1`, `libwayland-egl.so.1` |
| `dlopen` that never loads from disk | `librenderdoc.so` (attached only when already loaded) |

The three ways a library is reached:

- **Linked**: recorded in the ELF dynamic section (`DT_NEEDED`). The dynamic loader maps
  it before `main`; if it is missing, the program does not start at all.
- **dlopen, required**: opened at run time by name. The program starts, but the feature
  that needs it fails, in some cases by stopping the program.
- **dlopen, optional**: opened at run time; when it is missing, another path is taken.

## 1. Linked at build time

| Library | soname | Brought by | Why |
|---|---|---|---|
| GNU C library | `libc.so.6` | Rust standard library | System calls, threads, `dlopen` itself |
| Math library | `libm.so.6` | Rust standard library | Floating-point functions |
| GCC runtime | `libgcc_s.so.1` | Rust standard library | Stack unwinding support |
| Dynamic loader | `ld-linux-x86-64.so.2` | Rust standard library | Program interpreter (`PT_INTERP`) |
| ALSA | `libasound.so.2` | `alsa-sys` 0.4.0, through `alsa` 0.11.0 and `cpal` 0.17.3, from `heimdall-rdp` | Plays an RDP server's sound on this computer |

Measured on a release build of this commit (`cargo build --release --locked -p
heimdall-ui`, AlmaLinux 9, glibc 2.34):

```text
$ readelf -d heimdall-rs | grep NEEDED
 0x0000000000000001 (NEEDED)             Shared library: [libasound.so.2]
 0x0000000000000001 (NEEDED)             Shared library: [libgcc_s.so.1]
 0x0000000000000001 (NEEDED)             Shared library: [libm.so.6]
 0x0000000000000001 (NEEDED)             Shared library: [libc.so.6]
$ readelf -l heimdall-rs | grep interpreter
      [Requesting program interpreter: /lib64/ld-linux-x86-64.so.2]
```

No `RPATH` or `RUNPATH` is set. `x11-dl` asks for `libdl` at link time, but with glibc
2.34 and later `dlopen` lives in `libc.so.6` and no `libdl.so.2` entry is recorded.

`libasound.so.2` is the only linked library outside glibc and the GCC runtime. It is
linked even when no RDP session plays sound: `alsa-sys` asks `pkg-config` for `alsa` with
dynamic linking (`build.rs` line 7). Building therefore needs `alsa-lib-devel` (Fedora,
RHEL, AlmaLinux) or `libasound2-dev` (Debian, Ubuntu), and running needs `alsa-lib` or
`libasound2`. See [Planned change: RDP audio](#planned-change-rdp-audio).

## 2. Windowing in a Wayland session

`winit` 0.30.13 picks Wayland when `WAYLAND_DISPLAY` (or `WAYLAND_SOCKET`) is set, X11
otherwise. iced 0.14 enables winit's `wayland-dlopen` feature, so nothing Wayland is linked.

| Library | soname | Loaded by | How | What happens if absent |
|---|---|---|---|---|
| Wayland client | `libwayland-client.so.0` | `wayland-sys` 0.31.11 (through `wayland-backend` 0.3.17, used by `winit`, `smithay-clipboard` 0.7.3 and `softbuffer` 0.4.8) | dlopen, required | The event loop cannot connect (`NoWaylandLib`): the window does not open. Unsetting `WAYLAND_DISPLAY` makes winit use X11 through XWayland instead |
| xkbcommon | `libxkbcommon.so.0` | `xkbcommon-dl` 0.4.2, from `winit` | dlopen, required | The program stops (panic) when the compositor announces a keyboard |
| Wayland EGL | `libwayland-egl.so.1` | `wgpu-hal` 27.0.4, OpenGL ES backend only | dlopen, optional | The OpenGL ES backend is not offered on Wayland; Vulkan or the software renderer is used |

Not loaded: `libwayland-cursor` (the `wayland-cursor` crate reads cursor themes itself),
`libwayland-server`, `libdecor`. Window decorations drawn by the client come from
`sctk-adwaita` 0.10.1, in Rust.

## 3. Windowing in an X11 session

| Library | soname | Loaded by | How | What happens if absent |
|---|---|---|---|---|
| Xlib | `libX11.so.6` | `x11-dl` 2.21.0 (winit), `tiny-xlib` 0.2.5 (softbuffer), `wgpu-hal` 27.0.4 (EGL on X11) | dlopen, required | winit reports X11 as not supported: the window does not open |
| Xlib/XCB bridge | `libX11-xcb.so.1` | `x11-dl` 2.21.0, `tiny-xlib` 0.2.5 | dlopen, required | Same |
| Xcursor | `libXcursor.so.1` | `x11-dl` 2.21.0, from winit | dlopen, required | Same |
| XInput 2 | `libXi.so.6` | `x11-dl` 2.21.0, from winit | dlopen, required | Same |
| XCB | `libxcb.so.1` | `x11rb` 0.13.2 (feature `dl-libxcb`), from winit and softbuffer | dlopen, required | Same |
| xkbcommon | `libxkbcommon.so.0` | `xkbcommon-dl` 0.4.2, from winit | dlopen, required | The program stops (panic) when the event loop starts |
| xkbcommon X11 | `libxkbcommon-x11.so.0` | `xkbcommon-dl` 0.4.2, from winit | dlopen, required | Same |

Not loaded although `x11-dl` can name them: `libXrandr`, `libXrender`, `libXext`,
`libXinerama`, `libXxf86vm`, `libXss`, `libXft`, `libXmu`, `libXt`, `libXtst`, `libGL`
(GLX). winit 0.30 opens only Xlib, Xcursor, Xlib-XCB and XInput 2 (`xdisplay.rs` lines
71 to 74) and speaks RandR, XFixes and the rest as X11 protocol through `x11rb`. The
clipboard (`clipboard_x11` 0.4.3) uses `x11rb` with its own Rust connection, without
`libxcb`.

## 4. Graphics: GPU renderer and software fallback

iced draws with `wgpu` 27 when a GPU adapter is found, and falls back to `tiny-skia`
drawn on the CPU and shown through `softbuffer` otherwise. Both are compiled in.

| Library | soname | Loaded by | How | When | What happens if absent |
|---|---|---|---|---|---|
| Vulkan loader | `libvulkan.so.1` | `ash` 0.38.0 (feature `loaded`), from `wgpu-hal` 27.0.4 | dlopen, optional | Always tried first | The Vulkan backend is skipped |
| EGL | `libEGL.so.1` | `khronos-egl` 6.0.0 (feature `dynamic`), from `wgpu-hal` 27.0.4 | dlopen, optional | OpenGL ES backend | The OpenGL ES backend is skipped |
| Wayland EGL | `libwayland-egl.so.1` | `wgpu-hal` 27.0.4 | dlopen, optional | OpenGL ES in a Wayland session | See section 2 |
| RenderDoc | `librenderdoc.so` | `wgpu-hal` 27.0.4 | dlopen with `RTLD_NOLOAD`, never from disk | Only if a RenderDoc capture already injected it | Nothing: the normal case |

When neither Vulkan nor OpenGL ES gives an adapter, the software renderer needs nothing
beyond the windowing libraries of sections 2 and 3: on X11 it uses `libX11`,
`libX11-xcb` and `libxcb` (with the MIT-SHM extension when the server offers it), on
Wayland `libwayland-client` and shared memory.

The libraries `libvulkan.so.1` and `libEGL.so.1` then load the system's GPU driver
(Mesa, NVIDIA) themselves, from their ICD files. Those drivers are not chosen by
Heimdall-rs and are not listed here.

Two environment variables choose the path on a host:

- `ICED_BACKEND=tiny-skia` uses the software renderer only: no `libvulkan`, no `libEGL`
  is opened.
- `WGPU_BACKEND=vulkan` (or `gl`) limits wgpu to one GPU backend.

## 5. Desktop integration

| Function | Crate | Library | How | What happens if absent |
|---|---|---|---|---|
| Open, save and folder dialogs | Heimdall's own `FileChooser` client (`heimdall-ui`, `file_dialog::portal`) over `zbus` 5.19.0 | none | Pure Rust D-Bus to `xdg-desktop-portal` | Without a session bus or a portal, the dialog does not open and the status bar says why; no program is started in its place |
| Saved passwords (Secret Service) | `zbus-secret-service-keyring-store` 1.0.1, `secret-service` 5.2.0 (feature `crypto-rust`), `zbus` 5.19.0 | none | Pure Rust D-Bus and cryptography | - |
| Light or dark theme | `mundy` 0.2.3, through iced's `linux-theme-detection` | none | Pure Rust D-Bus (`zbus`) | - |
| System fonts | `fontdb` 0.23.0 with `fontconfig-parser` | none | Reads the fontconfig files, not the library | - |
| Language | `sys-locale` 0.3.2 | none | Reads the environment | - |
| Certificate store | `rustls-native-certs` 0.8.4, `openssl-probe` 0.2.1 | none | Reads the certificate files, no OpenSSL | - |

No crate here reaches `libdbus`. The file dialogs call the `FileChooser` interface of
`xdg-desktop-portal` over the session bus through `zbus`, in Rust, as the theme detection
and the keyring do. `rfd`, which opens `libdbus-1.so.3` with `dlopen` for its portal
backend and falls back on starting `zenity`, is built on Windows and macOS only: the Linux
binary does not contain it (`cargo tree -i rfd --target x86_64-unknown-linux-gnu` finds
nothing). The dialog itself is drawn by the desktop's portal (`xdg-desktop-portal-gtk`,
`-gnome` or `-kde`), in another process.

## 6. Audio

| Library | soname | Loaded by | How | When |
|---|---|---|---|---|
| ALSA | `libasound.so.2` | `alsa-sys` 0.4.0 | Linked | Always, see section 1 |

`cpal` 0.17.3 uses ALSA only on Linux: its `jack` feature is off and no PulseAudio or
PipeWire crate is in the tree. ALSA itself may then load its own plugins
(`libasound_module_pcm_pulse.so`, `libasound_module_pcm_pipewire.so`, and so on) according
to the host's `/etc/alsa` configuration, when sound is played.

## 7. C code compiled into the binary

`ring` 0.17.14 compiles its C and assembly sources with the system C compiler and links
them statically. It adds no runtime library and nothing to `DT_NEEDED`; it is listed
because a hardening review may ask about C code, not about shared objects.

## 8. Programs started by dependencies

Not libraries, but processes a dependency may start on Linux:

| Program | Started by | When |
|---|---|---|
| `gsettings`, `dbus-send`, `fc-match` | `sctk-adwaita` 0.10.1 | Wayland session with client-side decorations: title font and theme lookup; absent programs fall back to defaults |

## Search paths baked at build time

`x11-dl` and `tiny-xlib` ask `pkg-config` at build time for the directory of the X11
libraries and keep it in the binary:

- `x11-dl` tries the plain soname first, through the loader's normal search, then the
  directory of the build host.
- `tiny-xlib` tries the directory of the build host first (for example
  `/usr/lib64/libX11.so.6`), then the plain soname.

A binary built on one distribution and run on another therefore still finds the
libraries by soname; on a host with the same layout, `tiny-xlib` opens them by absolute
path.

## How to verify on a target host

What the binary links (no execution):

```bash
readelf -d heimdall-rs | grep NEEDED
readelf -l heimdall-rs | grep interpreter
ldd heimdall-rs
```

What it opens at run time, in a real session (the list depends on the session type and
the GPU path taken):

```bash
LD_DEBUG=libs ./heimdall-rs 2>&1 | grep -E 'calling init|find library'
strace -f -e trace=openat ./heimdall-rs 2>&1 | grep -E '\.so'
```

To check the software path without a GPU library:

```bash
ICED_BACKEND=tiny-skia strace -f -e trace=openat ./heimdall-rs 2>&1 | grep -E '\.so'
```

`ldd` runs the dynamic loader on the file; on an untrusted binary prefer `readelf -d`.

## Planned change: RDP audio

RDP sound will become an optional Cargo feature (`alsa`), as the owner has planned. A
build without it will have no `libasound.so.2` in `DT_NEEDED`, leaving glibc and the GCC
runtime as the only linked libraries, and will need no ALSA package to build or run. RDP
sessions will still work; the server's sound will not be played on this computer.

## Windows

On Windows the binary imports only system DLLs (kernel32, user32, advapi32 and the like,
through `windows`, `winsafe` and the `windows-sys` family) and the MSVC C runtime. The
workspace does not set `crt-static`, so the runtime is the toolchain's default: the
Universal CRT (`api-ms-win-crt-*.dll`, part of Windows 10 and later) and
`vcruntime140.dll`, which comes with the Visual C++ Redistributable. The GPU path loads
`d3d12.dll`, `dxgi.dll` or `opengl32.dll` and the driver's own DLLs at run time. There is
no third-party C library. This section is read from the build configuration, not
measured on a Windows binary: `dumpbin /imports heimdall-rs.exe` shows the import table.

macOS is out of scope.

## Keeping this page true

The tables come from the sources of the locked crate versions, not from their
documentation. After a dependency bump touching `winit`, `wgpu`, `softbuffer`, `cpal`,
`zbus`, `x11rb`, `wayland-*` or `xkbcommon-dl`:

1. Search the new sources for `dlopen`, `libloading`, `dlib`, `#[link(` and
   `cargo:rustc-link-lib`.
2. Rebuild in release and compare `readelf -d` with section 1.
3. Run the binary under `strace -e trace=openat` in a Wayland and an X11 session.

## Sources

Line numbers in the crate sources of the cargo registry, at the locked versions:

| Crate | Location | Shows |
|---|---|---|
| `wayland-sys` 0.31.11 | `src/client.rs:95` | `libwayland-client.so.0`, dlopen |
| `wayland-sys` 0.31.11 | `src/egl.rs:25` | `libwayland-egl.so.1`, dlopen |
| `iced_winit` 0.14.1 | `Cargo.toml`, feature `wayland` | enables `winit/wayland-dlopen` |
| `x11-dl` 2.21.0 | `src/xlib.rs:25`, `src/xlib_xcb.rs:4`, `src/xcursor.rs:14`, `src/xinput2.rs:27` | Xlib, Xlib-XCB, Xcursor, XInput 2 sonames |
| `winit` 0.30.13 | `src/platform_impl/linux/x11/xdisplay.rs:71-74` | the four `x11-dl` libraries winit opens |
| `x11rb` 0.13.2 | `src/xcb_ffi/raw_ffi/ffi.rs:39` | `libxcb.so.1`, dlopen |
| `tiny-xlib` 0.2.5 | `src/ffi.rs:149-151` | `libX11.so.6`, `libX11-xcb.so.1`, dlopen |
| `xkbcommon-dl` 0.4.2 | `src/lib.rs:313`, `src/x11.rs:50` | `libxkbcommon.so.0`, `libxkbcommon-x11.so.0` |
| `winit` 0.30.13 | `src/platform_impl/linux/x11/mod.rs:282`, `wayland/seat/keyboard/mod.rs:300` | xkbcommon failure unwrapped |
| `ash` 0.38.0 | `src/entry.rs:74` | `libvulkan.so.1`, dlopen (feature `loaded`, default) |
| `khronos-egl` 6.0.0 | `src/lib.rs:2493` | `libEGL.so.1`, dlopen |
| `wgpu-hal` 27.0.4 | `src/gles/egl.rs:156,185,196` | X11 and Wayland libraries opened for EGL |
| `wgpu-hal` 27.0.4 | `src/auxil/renderdoc.rs:45-55` | `librenderdoc.so` with `RTLD_NOLOAD` |
| `alsa-sys` 0.4.0 | `build.rs:7` | `pkg-config` probe of `alsa`, dynamic link |
| `iced_renderer` 0.14.0 | `src/fallback.rs:278` | `ICED_BACKEND` |
| `iced_tiny_skia` 0.14.1 | `Cargo.toml`, features `x11`, `wayland` | `softbuffer/x11-dlopen`, `softbuffer/wayland-dlopen` |
| `sctk-adwaita` 0.10.1 | `src/config.rs:7`, `src/title/config.rs:8`, `src/title/ab_glyph_renderer.rs:186` | `dbus-send`, `gsettings`, `fc-match` |
