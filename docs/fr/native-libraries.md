[English version](../native-libraries.md)

# Bibliothèques natives chargées sous Linux

Cette page recense chaque bibliothèque C dont le binaire `heimdall-rs` a besoin sous Linux,
la façon dont il l'atteint, et ce qui se passe quand elle manque. Elle s'adresse au
déploiement sur des postes durcis (recommandations de l'ANSSI, systèmes de type Securix),
où chaque objet partagé qu'un programme projette en mémoire doit être connu à l'avance.

Le binaire est entièrement en Rust, hormis les bibliothèques ci-dessous. Il n'apporte ni
GTK, ni liaison à libdbus, ni OpenSSL, ni bibliothèque fontconfig ou FreeType : D-Bus est
parlé par `zbus`, TLS par `rustls` sur `ring`, les polices sont lues par `fontdb` et
`ttf-parser`.

Inventaire établi depuis le `Cargo.lock` du commit `c6c8463` (`master`, 2026-10-10), cible
`x86_64-unknown-linux-gnu`, profil de publication. Une mise à jour de dépendance peut le
changer : voir [Garder cette page exacte](#garder-cette-page-exacte).

## Résumé

| Catégorie | Bibliothèques |
|---|---|
| Liées : dans `DT_NEEDED`, le binaire ne démarre pas sans elles | `libc.so.6`, `libm.so.6`, `libgcc_s.so.1`, `ld-linux-x86-64.so.2` (glibc et le support d'exécution de GCC), `libasound.so.2` (ALSA) |
| `dlopen`, requises dans une session Wayland | `libwayland-client.so.0`, `libxkbcommon.so.0` |
| `dlopen`, requises dans une session X11 | `libX11.so.6`, `libX11-xcb.so.1`, `libXcursor.so.1`, `libXi.so.6`, `libxcb.so.1`, `libxkbcommon.so.0`, `libxkbcommon-x11.so.0` |
| `dlopen`, facultatives avec repli | `libvulkan.so.1`, `libEGL.so.1`, `libwayland-egl.so.1`, `libdbus-1.so.3` |
| `dlopen` qui ne charge jamais depuis le disque | `librenderdoc.so` (rattachée seulement si elle est déjà chargée) |

Les trois façons d'atteindre une bibliothèque :

- **Liée** : inscrite dans la section dynamique de l'ELF (`DT_NEEDED`). Le chargeur
  dynamique la projette avant `main` ; si elle manque, le programme ne démarre pas du tout.
- **dlopen, requise** : ouverte par son nom à l'exécution. Le programme démarre, mais la
  fonction qui en a besoin échoue, dans certains cas en arrêtant le programme.
- **dlopen, facultative** : ouverte à l'exécution ; quand elle manque, un autre chemin est
  pris.

## 1. Liées à la compilation

| Bibliothèque | soname | Apportée par | Pourquoi |
|---|---|---|---|
| Bibliothèque C GNU | `libc.so.6` | Bibliothèque standard de Rust | Appels système, threads, `dlopen` lui-même |
| Bibliothèque mathématique | `libm.so.6` | Bibliothèque standard de Rust | Fonctions en virgule flottante |
| Support d'exécution de GCC | `libgcc_s.so.1` | Bibliothèque standard de Rust | Déroulement de pile |
| Chargeur dynamique | `ld-linux-x86-64.so.2` | Bibliothèque standard de Rust | Interpréteur du programme (`PT_INTERP`) |
| ALSA | `libasound.so.2` | `alsa-sys` 0.4.0, via `alsa` 0.11.0 et `cpal` 0.17.3, depuis `heimdall-rdp` | Joue sur cet ordinateur le son d'un serveur RDP |

Mesuré sur une compilation de publication de ce commit (`cargo build --release --locked
-p heimdall-ui`, AlmaLinux 9, glibc 2.34) :

```text
$ readelf -d heimdall-rs | grep NEEDED
 0x0000000000000001 (NEEDED)             Shared library: [libasound.so.2]
 0x0000000000000001 (NEEDED)             Shared library: [libgcc_s.so.1]
 0x0000000000000001 (NEEDED)             Shared library: [libm.so.6]
 0x0000000000000001 (NEEDED)             Shared library: [libc.so.6]
$ readelf -l heimdall-rs | grep interpreter
      [Requesting program interpreter: /lib64/ld-linux-x86-64.so.2]
```

Aucun `RPATH` ni `RUNPATH` n'est défini. `x11-dl` demande `libdl` à l'édition de liens,
mais depuis glibc 2.34 `dlopen` est dans `libc.so.6` et aucune entrée `libdl.so.2` n'est
inscrite.

`libasound.so.2` est la seule bibliothèque liée en dehors de glibc et du support
d'exécution de GCC. Elle est liée même quand aucune session RDP ne joue de son :
`alsa-sys` demande `alsa` à `pkg-config` en liaison dynamique (`build.rs` ligne 7). La
compilation demande donc `alsa-lib-devel` (Fedora, RHEL, AlmaLinux) ou `libasound2-dev`
(Debian, Ubuntu), et l'exécution demande `alsa-lib` ou `libasound2`. Voir
[Évolution prévue du son RDP](#évolution-prévue-du-son-rdp).

## 2. Fenêtrage dans une session Wayland

`winit` 0.30.13 choisit Wayland quand `WAYLAND_DISPLAY` (ou `WAYLAND_SOCKET`) est défini,
X11 sinon. iced 0.14 active la fonctionnalité `wayland-dlopen` de winit : rien de Wayland
n'est lié.

| Bibliothèque | soname | Chargée par | Comment | Si elle manque |
|---|---|---|---|---|
| Client Wayland | `libwayland-client.so.0` | `wayland-sys` 0.31.11 (via `wayland-backend` 0.3.17, utilisé par `winit`, `smithay-clipboard` 0.7.3 et `softbuffer` 0.4.8) | dlopen, requise | La boucle d'événements ne peut pas se connecter (`NoWaylandLib`) : la fenêtre ne s'ouvre pas. Retirer `WAYLAND_DISPLAY` fait passer winit par X11, via XWayland |
| xkbcommon | `libxkbcommon.so.0` | `xkbcommon-dl` 0.4.2, depuis `winit` | dlopen, requise | Le programme s'arrête (panique) quand le compositeur annonce un clavier |
| EGL Wayland | `libwayland-egl.so.1` | `wgpu-hal` 27.0.4, moteur OpenGL ES seulement | dlopen, facultative | Le moteur OpenGL ES n'est pas proposé sous Wayland ; Vulkan ou le rendu logiciel prend le relais |

Non chargées : `libwayland-cursor` (la crate `wayland-cursor` lit elle-même les thèmes de
curseurs), `libwayland-server`, `libdecor`. Les décorations de fenêtre dessinées par le
client viennent de `sctk-adwaita` 0.10.1, en Rust.

## 3. Fenêtrage dans une session X11

| Bibliothèque | soname | Chargée par | Comment | Si elle manque |
|---|---|---|---|---|
| Xlib | `libX11.so.6` | `x11-dl` 2.21.0 (winit), `tiny-xlib` 0.2.5 (softbuffer), `wgpu-hal` 27.0.4 (EGL sous X11) | dlopen, requise | winit déclare X11 non pris en charge : la fenêtre ne s'ouvre pas |
| Pont Xlib/XCB | `libX11-xcb.so.1` | `x11-dl` 2.21.0, `tiny-xlib` 0.2.5 | dlopen, requise | Idem |
| Xcursor | `libXcursor.so.1` | `x11-dl` 2.21.0, depuis winit | dlopen, requise | Idem |
| XInput 2 | `libXi.so.6` | `x11-dl` 2.21.0, depuis winit | dlopen, requise | Idem |
| XCB | `libxcb.so.1` | `x11rb` 0.13.2 (fonctionnalité `dl-libxcb`), depuis winit et softbuffer | dlopen, requise | Idem |
| xkbcommon | `libxkbcommon.so.0` | `xkbcommon-dl` 0.4.2, depuis winit | dlopen, requise | Le programme s'arrête (panique) au démarrage de la boucle d'événements |
| xkbcommon X11 | `libxkbcommon-x11.so.0` | `xkbcommon-dl` 0.4.2, depuis winit | dlopen, requise | Idem |

Non chargées bien que `x11-dl` sache les nommer : `libXrandr`, `libXrender`, `libXext`,
`libXinerama`, `libXxf86vm`, `libXss`, `libXft`, `libXmu`, `libXt`, `libXtst`, `libGL`
(GLX). winit 0.30 n'ouvre que Xlib, Xcursor, Xlib-XCB et XInput 2 (`xdisplay.rs` lignes
71 à 74) et parle RandR, XFixes et le reste en protocole X11 via `x11rb`. Le presse-papiers
(`clipboard_x11` 0.4.3) utilise `x11rb` avec sa propre connexion en Rust, sans `libxcb`.

## 4. Affichage : rendu GPU et repli logiciel

iced dessine avec `wgpu` 27 quand un adaptateur GPU est trouvé, et se replie sinon sur
`tiny-skia`, dessiné par le processeur et affiché via `softbuffer`. Les deux sont compilés.

| Bibliothèque | soname | Chargée par | Comment | Quand | Si elle manque |
|---|---|---|---|---|---|
| Chargeur Vulkan | `libvulkan.so.1` | `ash` 0.38.0 (fonctionnalité `loaded`), depuis `wgpu-hal` 27.0.4 | dlopen, facultative | Toujours essayée en premier | Le moteur Vulkan est écarté |
| EGL | `libEGL.so.1` | `khronos-egl` 6.0.0 (fonctionnalité `dynamic`), depuis `wgpu-hal` 27.0.4 | dlopen, facultative | Moteur OpenGL ES | Le moteur OpenGL ES est écarté |
| EGL Wayland | `libwayland-egl.so.1` | `wgpu-hal` 27.0.4 | dlopen, facultative | OpenGL ES dans une session Wayland | Voir section 2 |
| RenderDoc | `librenderdoc.so` | `wgpu-hal` 27.0.4 | dlopen avec `RTLD_NOLOAD`, jamais depuis le disque | Seulement si une capture RenderDoc l'a déjà injectée | Rien : c'est le cas normal |

Quand ni Vulkan ni OpenGL ES ne fournit d'adaptateur, le rendu logiciel n'a besoin de rien
de plus que les bibliothèques de fenêtrage des sections 2 et 3 : sous X11 il utilise
`libX11`, `libX11-xcb` et `libxcb` (avec l'extension MIT-SHM quand le serveur l'offre),
sous Wayland `libwayland-client` et la mémoire partagée.

`libvulkan.so.1` et `libEGL.so.1` chargent ensuite elles-mêmes le pilote GPU du système
(Mesa, NVIDIA), d'après leurs fichiers ICD. Ces pilotes ne sont pas choisis par
Heimdall-rs et ne sont pas recensés ici.

Deux variables d'environnement choisissent le chemin sur un poste :

- `ICED_BACKEND=tiny-skia` n'utilise que le rendu logiciel : ni `libvulkan` ni `libEGL`
  n'est ouverte.
- `WGPU_BACKEND=vulkan` (ou `gl`) restreint wgpu à un seul moteur GPU.

## 5. Intégration au bureau

| Fonction | Crate | Bibliothèque | Comment | Si elle manque |
|---|---|---|---|---|
| Dialogues d'enregistrement et d'ouverture | `rfd` 0.17.2, fonctionnalité `xdg-portal` (sans GTK) | `libdbus-1.so.3` | dlopen, facultative | `rfd` lance à la place le programme `zenity` ; sans lui, le dialogue n'apparaît pas |
| Mots de passe enregistrés (Secret Service) | `zbus-secret-service-keyring-store` 1.0.1, `secret-service` 5.2.0 (fonctionnalité `crypto-rust`), `zbus` 5.19.0 | aucune | D-Bus et cryptographie en Rust | - |
| Thème clair ou sombre | `mundy` 0.2.3, via `linux-theme-detection` d'iced | aucune | D-Bus en Rust (`zbus`) | - |
| Polices du système | `fontdb` 0.23.0 avec `fontconfig-parser` | aucune | Lit les fichiers de fontconfig, pas la bibliothèque | - |
| Langue | `sys-locale` 0.3.2 | aucune | Lit l'environnement | - |
| Magasin de certificats | `rustls-native-certs` 0.8.4, `openssl-probe` 0.2.1 | aucune | Lit les fichiers de certificats, sans OpenSSL | - |

`rfd` est la seule crate ici qui atteint `libdbus` : son moteur de portail parle à
`xdg-desktop-portal` via `libdbus-1.so.3`, ouverte par `dlopen` (`ffi.rs` ligne 199), et
non via `zbus`. Un poste sans `libdbus-1.so.3` (inhabituel : systemd en dépend) obtient le
repli `zenity`. Le dialogue lui-même est dessiné par le portail du bureau
(`xdg-desktop-portal-gtk`, `-gnome` ou `-kde`), dans un autre processus.

## 6. Son

| Bibliothèque | soname | Chargée par | Comment | Quand |
|---|---|---|---|---|
| ALSA | `libasound.so.2` | `alsa-sys` 0.4.0 | Liée | Toujours, voir section 1 |

`cpal` 0.17.3 n'utilise qu'ALSA sous Linux : sa fonctionnalité `jack` est désactivée et
aucune crate PulseAudio ou PipeWire n'est dans l'arbre. ALSA peut ensuite charger ses
propres greffons (`libasound_module_pcm_pulse.so`, `libasound_module_pcm_pipewire.so`, et
ainsi de suite) selon la configuration `/etc/alsa` du poste, quand du son est joué.

## 7. Code C compilé dans le binaire

`ring` 0.17.14 compile ses sources C et assembleur avec le compilateur C du système et les
lie statiquement. Il n'ajoute aucune bibliothèque d'exécution ni rien à `DT_NEEDED` ; il
figure ici parce qu'une revue de durcissement peut s'interroger sur le code C, pas
seulement sur les objets partagés.

## 8. Programmes lancés par des dépendances

Pas des bibliothèques, mais des processus qu'une dépendance peut lancer sous Linux :

| Programme | Lancé par | Quand |
|---|---|---|
| `zenity` | `rfd` 0.17.2 | Dialogue de fichier, seulement quand `libdbus-1.so.3` ne s'ouvre pas ou que le portail échoue |
| `gsettings`, `dbus-send`, `fc-match` | `sctk-adwaita` 0.10.1 | Session Wayland avec décorations côté client : recherche de la police du titre et du thème ; un programme absent laisse les valeurs par défaut |

## Chemins de recherche figés à la compilation

`x11-dl` et `tiny-xlib` demandent à `pkg-config`, à la compilation, le répertoire des
bibliothèques X11 et le conservent dans le binaire :

- `x11-dl` essaie d'abord le soname seul, par la recherche normale du chargeur, puis le
  répertoire de la machine de compilation.
- `tiny-xlib` essaie d'abord le répertoire de la machine de compilation (par exemple
  `/usr/lib64/libX11.so.6`), puis le soname seul.

Un binaire compilé sur une distribution et lancé sur une autre trouve donc toujours les
bibliothèques par leur soname ; sur un poste de même organisation, `tiny-xlib` les ouvre
par chemin absolu.

## Vérifier sur un poste cible

Ce que le binaire lie (sans l'exécuter) :

```bash
readelf -d heimdall-rs | grep NEEDED
readelf -l heimdall-rs | grep interpreter
ldd heimdall-rs
```

Ce qu'il ouvre à l'exécution, dans une vraie session (la liste dépend du type de session
et du chemin GPU pris) :

```bash
LD_DEBUG=libs ./heimdall-rs 2>&1 | grep -E 'calling init|find library'
strace -f -e trace=openat ./heimdall-rs 2>&1 | grep -E '\.so'
```

Pour vérifier le chemin logiciel sans bibliothèque GPU :

```bash
ICED_BACKEND=tiny-skia strace -f -e trace=openat ./heimdall-rs 2>&1 | grep -E '\.so'
```

`ldd` fait exécuter le fichier par le chargeur dynamique ; sur un binaire non fiable,
préférer `readelf -d`.

## Évolution prévue du son RDP

Le son RDP deviendra une fonctionnalité Cargo facultative (`alsa`), comme le propriétaire
l'a prévu. Une compilation sans elle n'aura pas `libasound.so.2` dans `DT_NEEDED`, ne
laissant comme bibliothèques liées que glibc et le support d'exécution de GCC, et ne
demandera aucun paquet ALSA pour compiler ni pour s'exécuter. Les sessions RDP
fonctionneront toujours ; le son du serveur ne sera pas joué sur cet ordinateur.

## Windows

Sous Windows, le binaire n'importe que des DLL du système (kernel32, user32, advapi32 et
semblables, via `windows`, `winsafe` et la famille `windows-sys`) et le support
d'exécution C de MSVC. L'espace de travail ne règle pas `crt-static` : le support
d'exécution est donc celui par défaut de la chaîne de compilation, l'Universal CRT
(`api-ms-win-crt-*.dll`, intégré à Windows 10 et suivants) et `vcruntime140.dll`, fourni
par le Visual C++ Redistributable. Le chemin GPU charge `d3d12.dll`, `dxgi.dll` ou
`opengl32.dll` et les DLL du pilote à l'exécution. Il n'y a aucune bibliothèque C tierce.
Cette section est lue dans la configuration de compilation, pas mesurée sur un binaire
Windows : `dumpbin /imports heimdall-rs.exe` affiche la table d'importation.

macOS est hors du champ.

## Garder cette page exacte

Les tableaux viennent des sources des versions verrouillées des crates, pas de leur
documentation. Après une mise à jour de dépendance touchant `winit`, `wgpu`, `softbuffer`,
`cpal`, `rfd`, `x11rb`, `wayland-*` ou `xkbcommon-dl` :

1. Chercher dans les nouvelles sources `dlopen`, `libloading`, `dlib`, `#[link(` et
   `cargo:rustc-link-lib`.
2. Recompiler en publication et comparer `readelf -d` avec la section 1.
3. Lancer le binaire sous `strace -e trace=openat` dans une session Wayland et une
   session X11.

## Sources

Numéros de ligne dans les sources des crates du registre cargo, aux versions verrouillées :

| Crate | Emplacement | Montre |
|---|---|---|
| `wayland-sys` 0.31.11 | `src/client.rs:95` | `libwayland-client.so.0`, dlopen |
| `wayland-sys` 0.31.11 | `src/egl.rs:25` | `libwayland-egl.so.1`, dlopen |
| `iced_winit` 0.14.1 | `Cargo.toml`, fonctionnalité `wayland` | active `winit/wayland-dlopen` |
| `x11-dl` 2.21.0 | `src/xlib.rs:25`, `src/xlib_xcb.rs:4`, `src/xcursor.rs:14`, `src/xinput2.rs:27` | sonames de Xlib, Xlib-XCB, Xcursor, XInput 2 |
| `winit` 0.30.13 | `src/platform_impl/linux/x11/xdisplay.rs:71-74` | les quatre bibliothèques `x11-dl` qu'ouvre winit |
| `x11rb` 0.13.2 | `src/xcb_ffi/raw_ffi/ffi.rs:39` | `libxcb.so.1`, dlopen |
| `tiny-xlib` 0.2.5 | `src/ffi.rs:149-151` | `libX11.so.6`, `libX11-xcb.so.1`, dlopen |
| `xkbcommon-dl` 0.4.2 | `src/lib.rs:313`, `src/x11.rs:50` | `libxkbcommon.so.0`, `libxkbcommon-x11.so.0` |
| `winit` 0.30.13 | `src/platform_impl/linux/x11/mod.rs:282`, `wayland/seat/keyboard/mod.rs:300` | échec de xkbcommon déballé par `unwrap` |
| `ash` 0.38.0 | `src/entry.rs:74` | `libvulkan.so.1`, dlopen (fonctionnalité `loaded`, par défaut) |
| `khronos-egl` 6.0.0 | `src/lib.rs:2493` | `libEGL.so.1`, dlopen |
| `wgpu-hal` 27.0.4 | `src/gles/egl.rs:156,185,196` | bibliothèques X11 et Wayland ouvertes pour EGL |
| `wgpu-hal` 27.0.4 | `src/auxil/renderdoc.rs:45-55` | `librenderdoc.so` avec `RTLD_NOLOAD` |
| `rfd` 0.17.2 | `src/backend/xdg_desktop_portal/portal/ffi.rs:199` | `libdbus-1.so.3`, dlopen |
| `rfd` 0.17.2 | `src/backend/linux/zenity.rs:41` | repli `zenity` |
| `alsa-sys` 0.4.0 | `build.rs:7` | sonde `pkg-config` de `alsa`, liaison dynamique |
| `iced_renderer` 0.14.0 | `src/fallback.rs:278` | `ICED_BACKEND` |
| `iced_tiny_skia` 0.14.1 | `Cargo.toml`, fonctionnalités `x11`, `wayland` | `softbuffer/x11-dlopen`, `softbuffer/wayland-dlopen` |
| `sctk-adwaita` 0.10.1 | `src/config.rs:7`, `src/title/config.rs:8`, `src/title/ab_glyph_renderer.rs:186` | `dbus-send`, `gsettings`, `fc-match` |
