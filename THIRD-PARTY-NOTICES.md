[Version française](THIRD-PARTY-NOTICES.fr.md)

# Third-party notices

Heimdall-rs embeds the following works in its binaries. Their licences travel with every
release package.

## Source Code Pro

The terminal font: the Regular, Bold, Italic and Bold Italic faces, embedded in the
`heimdall` executable.

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

Rust crates linked into the binaries are listed with their licences by
`cargo deny list`; `deny.toml` holds the licences the project accepts.
