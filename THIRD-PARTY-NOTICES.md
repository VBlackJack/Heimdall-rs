[Version française](THIRD-PARTY-NOTICES.fr.md)

# Third-party notices

Heimdall-rs embeds the following works in its binaries. Their licences travel with every
release package.

## Source Code Pro

The terminal font: the Regular, Bold, Italic and Bold Italic faces, embedded in the
`heimdall-rs` executable.

- Copyright 2010, 2012 Adobe Systems Incorporated (http://www.adobe.com/), with Reserved
  Font Name "Source".
- Licensed under the SIL Open Font License, Version 1.1. The full text is in
  [`crates/heimdall-ui/assets/fonts/SourceCodePro-LICENSE.txt`](crates/heimdall-ui/assets/fonts/SourceCodePro-LICENSE.txt).

## Fira Sans

The font of the window's text, Regular face, version 4.203, embedded through iced's
`fira-sans` feature.

- Digitized data copyright 2012-2016, The Mozilla Foundation and Telefonica S.A.
- Licensed under the SIL Open Font License, Version 1.1. The full text is in
  [`crates/heimdall-ui/assets/fonts/FiraSans-LICENSE.txt`](crates/heimdall-ui/assets/fonts/FiraSans-LICENSE.txt).

## ironrdp-connector 0.10.0, modified

A copy of the crate, patched, is built into the binaries in place of the published one:
[`vendor/ironrdp-connector`](vendor/ironrdp-connector).

- Copyright Devolutions Inc. and the IronRDP contributors.
- Licensed under the MIT license or the Apache License, Version 2.0, at your option; both
  texts are in that directory.
- The changes, three lines, and the reason for them are stated in
  [`vendor/PATCHES.md`](vendor/PATCHES.md).

## ironrdp-session 0.11.0, modified

A copy of the crate, patched, is built into the binaries in place of the published one:
[`vendor/ironrdp-session`](vendor/ironrdp-session).

- Copyright Devolutions Inc. and the IronRDP contributors.
- Licensed under the MIT license or the Apache License, Version 2.0, at your option; both
  texts are in that directory.
- The change and its reason are stated in [`vendor/PATCHES.md`](vendor/PATCHES.md).

## iced_widget 0.14.2, text editor, modified

The integrated editor's text widget,
[`crates/heimdall-ui/src/code_editor.rs`](crates/heimdall-ui/src/code_editor.rs), is
derived from iced's text editor (`src/text_editor.rs` of iced_widget 0.14.2), with a
line-number gutter and horizontal scrolling added.

- Copyright 2019 Héctor Ramón, Iced contributors.
- Licensed under the MIT license; its full text heads that file, and states what changed.

## Dracula Theme palette

The colours of the window's Dracula theme and of the terminals' Dracula colour scheme,
reproduced in [`crates/heimdall-ui/src/themes.rs`](crates/heimdall-ui/src/themes.rs) and
[`crates/heimdall-term/src/palette.rs`](crates/heimdall-term/src/palette.rs). The Drakul
theme is derived from it, its comment colour lifted to be readable.

- Copyright Zeno Rocha, [Dracula Theme](https://github.com/dracula/dracula-theme).
- Licensed under the MIT license.

## ThemeForge palettes

The seventeen window themes are the palettes of ThemeForge 2.1.0, by Julien Bombled, under
the Apache License, Version 2.0, as Heimdall-rs; Dracula aside, as said above. The Magellan
theme adapts the background hue and the accent of the Magellan corporate identity:
"Magellan" and its colours are trademarks of their owner, and no claim is made over them.

Rust crates linked into the binaries are listed with their licences by
`cargo deny list`; `deny.toml` holds the licences the project accepts.
