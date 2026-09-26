[English version](THIRD-PARTY-NOTICES.md)

# Mentions des composants tiers

Heimdall-rs embarque les oeuvres suivantes dans ses exécutables. Leurs licences accompagnent
chaque paquet publié.

## Source Code Pro

La police du terminal : les styles normal, gras, italique et gras italique, embarqués dans
l'exécutable `heimdall`.

- Copyright 2010, 2012 Adobe Systems Incorporated (http://www.adobe.com/), avec le nom de
  police réservé "Source".
- Sous licence SIL Open Font License, version 1.1. Le texte complet est dans
  [`crates/heimdall-ui/assets/fonts/SourceCodePro-LICENSE.txt`](crates/heimdall-ui/assets/fonts/SourceCodePro-LICENSE.txt).

## Fira Sans

La police du texte de la fenêtre, style normal, version 4.203, embarquée par la
fonctionnalité `fira-sans` d'iced.

- Digitized data copyright 2012-2016, The Mozilla Foundation and Telefonica S.A.
- Sous licence SIL Open Font License, version 1.1. Le texte complet est dans
  [`crates/heimdall-ui/assets/fonts/FiraSans-LICENSE.txt`](crates/heimdall-ui/assets/fonts/FiraSans-LICENSE.txt).

Les crates Rust liées aux exécutables sont listées avec leurs licences par
`cargo deny list` ; `deny.toml` contient les licences acceptées par le projet.
