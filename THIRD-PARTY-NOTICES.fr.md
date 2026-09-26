[English version](THIRD-PARTY-NOTICES.md)

# Mentions des composants tiers

Heimdall-rs embarque les oeuvres suivantes dans ses exécutables. Leurs licences accompagnent
chaque paquet publié.

## Source Code Pro

La police du terminal : les styles normal, gras, italique et gras italique, embarqués dans
l'exécutable `heimdall-rs`.

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

## ironrdp-connector 0.10.0, modifié

Une copie modifiée de la crate est compilée dans les exécutables à la place de celle
publiée : [`vendor/ironrdp-connector`](vendor/ironrdp-connector).

- Copyright Devolutions Inc. et les contributeurs d'IronRDP.
- Sous licence MIT ou Apache, version 2.0, au choix ; les deux textes sont dans ce
  répertoire.
- Les modifications, trois lignes, et leur raison sont décrites dans
  [`vendor/PATCHES.md`](vendor/PATCHES.md) (en anglais).

## ironrdp-session 0.11.0, modifié

Une copie modifiée de la crate est compilée dans les exécutables à la place de celle
publiée : [`vendor/ironrdp-session`](vendor/ironrdp-session).

- Copyright Devolutions Inc. et les contributeurs d'IronRDP.
- Sous licence MIT ou Apache, version 2.0, au choix ; les deux textes sont dans ce
  répertoire.
- La modification et sa raison sont décrites dans [`vendor/PATCHES.md`](vendor/PATCHES.md)
  (en anglais).

Les crates Rust liées aux exécutables sont listées avec leurs licences par
`cargo deny list` ; `deny.toml` contient les licences acceptées par le projet.
