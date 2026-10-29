<div align="center">

# ✦ Conkio

### Your Desktop, Your Style.

**A modern, fast and beautiful Conky theme manager for Linux — written in Rust with a native Slint interface.**

<br>

[![Release](https://img.shields.io/github/v/release/almezali/conkio?style=for-the-badge&logo=github&color=4FC3F7&labelColor=0D1117)](https://github.com/almezali/conkio/releases/latest)
[![License](https://img.shields.io/badge/License-GPL--3.0--or--later-blue?style=for-the-badge&logo=gnu&labelColor=0D1117)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021-DEA584?style=for-the-badge&logo=rust&logoColor=white&labelColor=0D1117)](https://www.rust-lang.org/)
[![Slint](https://img.shields.io/badge/UI-Slint-2379F4?style=for-the-badge&labelColor=0D1117)](https://slint.dev/)
[![Platform](https://img.shields.io/badge/Platform-Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black&labelColor=0D1117)](#-installation)

[![Downloads](https://img.shields.io/github/downloads/almezali/conkio/total?style=flat-square&logo=github&label=Downloads&color=success)](https://github.com/almezali/conkio/releases)
[![Stars](https://img.shields.io/github/stars/almezali/conkio?style=flat-square&logo=github&color=yellow)](https://github.com/almezali/conkio/stargazers)
[![Issues](https://img.shields.io/github/issues/almezali/conkio?style=flat-square&color=orange)](https://github.com/almezali/conkio/issues)
[![Last commit](https://img.shields.io/github/last-commit/almezali/conkio?style=flat-square&color=blueviolet)](https://github.com/almezali/conkio/commits)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen?style=flat-square)](https://github.com/almezali/conkio/pulls)
![Arch](https://img.shields.io/badge/Arch-supported-1793D1?style=flat-square&logo=archlinux&logoColor=white)
![Debian](https://img.shields.io/badge/Debian%20%2F%20Ubuntu-supported-A81D33?style=flat-square&logo=debian&logoColor=white)
![Fedora](https://img.shields.io/badge/Fedora-supported-51A2DA?style=flat-square&logo=fedora&logoColor=white)
![AppImage](https://img.shields.io/badge/AppImage-available-orange?style=flat-square&logo=appimage&logoColor=white)

<br>

[**Download**](https://github.com/almezali/conkio/releases/latest) ·
[**Features**](#-features) ·
[**Installation**](#-installation) ·
[**Build from source**](#-building-from-source) ·
[**Contributing**](#-contributing)

</div>

---

## 🌟 Overview

**Conkio** takes the pain out of managing [Conky](https://github.com/brndnmtthws/conky) themes. Instead of juggling folders, editing config files by hand and hunting for broken scripts, you get a single polished application to **discover, run, customize, repair and share** your desktop widgets.

It is built with **Rust** for speed and safety and **Slint** for a smooth, GPU-friendly native UI — no web runtime, no Electron, no heavy toolkit. It understands modern Linux desktops, including **Wayland compositors** like Hyprland, Sway, Niri and COSMIC, as well as the classic X11 desktops.

> Think of it as a control center for everything Conky: your themes, your colors, your placement, your profiles.

<!--
  📸 Add a screenshot here once you have one:

  <div align="center">
    <img src="docs/screenshots/themes.png" alt="Conkio — Themes page" width="860">
  </div>
-->

---

## ✨ Features

### 🎨 Theme library
- **Automatic discovery** of themes in `~/.conky` and any extra folders you add
- **Instant search** plus smart filters: *All · Favorites · Running · Recent · Needs Fix · Broken*
- **Favorites** and a **recently used** history
- **Clone, rename, export and delete** themes (deleted themes go to the trash, not into the void)
- Handles both the **modern `conky.config` (Lua) syntax** and the **legacy syntax**

### ▶️ One-click control
- **Run, stop and restart** any theme — or run several checked themes at once
- Tracks every Conky process it starts, so **"Stop all managed"** never touches processes it doesn't own
- **Startup health-check** and configuration validation before a theme is shown
- Configurable **instance limit** and **process priority (nice level)**
- Choose how themes sit on your desktop: **above or below** other windows

### 🛒 Built-in Theme Marketplace
- Browse and search real catalogs from **GNOME-Look**, **KDE-Look**, **Pling** and **OpenDesktop**
- Sort, filter by category, read metadata and previews, then **Download & Install** straight into `~/.conky`
- Downloads are limited to trusted store hosts and can be cancelled at any time

### 📦 Safe, flexible importing
- **Drag & drop** a folder or archive anywhere onto the window
- Supports `zip`, `tar.gz`, `tar.bz2`, `tar.xz` and plain `tar` archives
- **Hardened extraction**: path-traversal protection and size limits guard against malicious archives
- Detects the real theme inside messy archives and installs each theme under a clean, unique name
- **Asset path rewriting** so images, fonts and scripts keep working after import

### 🩺 Health scan
- Every theme gets a **Stable / Needs Fix / Broken** indicator
- **Deep scan** detects missing assets and missing external commands (`curl`, `jq`, `playerctl`, `sensors`, `nvidia-smi`, `nmcli` and many more)
- Gives you a **ready-to-run install command** for your package manager (`pacman`, `apt`, `dnf` or `zypper`)
- Warns about Conky-version mismatches and Wayland/XWayland quirks

### 🖌️ Customize without touching a config file
- **Desktop placement**: pick a monitor, set the alignment and fine-tune the X/Y gap
- **Color palettes** with one click: *Tokyo Night, Dracula, Nord, Catppuccin Mocha, Rosé Pine, Gruvbox Dark, Solarized Dark, Everforest*
- **Native color picker** and a simple `name=value` color editor
- Smart color remapping that also updates colors embedded in a theme's scripts
- Every change is backed up first — **restore the latest backup** whenever you like

### 🗂️ Desktop profiles
- Save a group of themes as a **profile** and switch between setups in one click (work, gaming, minimal…)
- Optionally **launch a profile automatically at login**

### 🖥️ Made for modern Linux desktops
| | |
|---|---|
| **Desktop environments** | KDE Plasma · GNOME · XFCE · Cinnamon · MATE · LXQt · LXDE · COSMIC |
| **Wayland compositors** | Hyprland · Sway · Niri |
| **Display servers** | X11 · Wayland · XWayland |
| **Multi-monitor** | Detected via `xrandr`, `hyprctl` and `swaymsg` |

### 🧩 Polish & integration
- **System tray** support (StatusNotifier) with a full menu: open, marketplace, run/stop, switch profile, refresh, quit
- **Dark and light** interface
- Responsive layout with a collapsible sidebar
- Tiny, fast, native — written in Rust

---

## 📥 Installation

Grab the package for your distribution from the [**latest release**](https://github.com/almezali/conkio/releases/latest).

### Downloads (v1.0.1)

| Distribution | Architecture | Package |
|---|---|---|
| **Arch Linux** · Manjaro · EndeavourOS | x86_64 | [`conkio-1.0.1-1-x86_64.pkg.tar.zst`](https://github.com/almezali/conkio/releases/download/1.0.1/conkio-1.0.1-1-x86_64.pkg.tar.zst) |
| | aarch64 | [`conkio-1.0.1-1-aarch64.pkg.tar.zst`](https://github.com/almezali/conkio/releases/download/1.0.1/conkio-1.0.1-1-aarch64.pkg.tar.zst) |
| **Fedora** · RHEL family · openSUSE | x86_64 | [`conkio-1.0.1-1.x86_64.rpm`](https://github.com/almezali/conkio/releases/download/1.0.1/conkio-1.0.1-1.x86_64.rpm) |
| | aarch64 | [`conkio-1.0.1-1.aarch64.rpm`](https://github.com/almezali/conkio/releases/download/1.0.1/conkio-1.0.1-1.aarch64.rpm) |
| **Debian** · Ubuntu · Linux Mint · Pop!_OS | x86_64 | [`conkio_1.0.1_x86_64.deb`](https://github.com/almezali/conkio/releases/download/1.0.1/conkio_1.0.1_x86_64.deb) |
| | aarch64 | [`conkio_1.0.1_aarch64.deb`](https://github.com/almezali/conkio/releases/download/1.0.1/conkio_1.0.1_aarch64.deb) |
| **Any distribution** (portable) | x86_64 | [`Conkio-x86_64.AppImage`](https://github.com/almezali/conkio/releases/download/1.0.1/Conkio-x86_64.AppImage) |

> 💡 Not sure which architecture you have? Run `uname -m` — `x86_64` is a regular 64-bit PC, `aarch64` is ARM64.

### 🏹 Arch Linux / Manjaro / EndeavourOS

```bash
# Download (x86_64)
wget https://github.com/almezali/conkio/releases/download/1.0.1/conkio-1.0.1-1-x86_64.pkg.tar.zst

# Install (pacman pulls in dependencies such as conky automatically if declared)
sudo pacman -U conkio-1.0.1-1-x86_64.pkg.tar.zst

# Conky itself is required
sudo pacman -S --needed conky
```

### 🎩 Fedora / RHEL / Nobara

```bash
wget https://github.com/almezali/conkio/releases/download/1.0.1/conkio-1.0.1-1.x86_64.rpm
sudo dnf install ./conkio-1.0.1-1.x86_64.rpm

# Conky itself is required
sudo dnf install conky
```

### 🦎 openSUSE

```bash
wget https://github.com/almezali/conkio/releases/download/1.0.1/conkio-1.0.1-1.x86_64.rpm
sudo zypper install --allow-unsigned-rpm ./conkio-1.0.1-1.x86_64.rpm

sudo zypper install conky
```

### 🌀 Debian / Ubuntu / Linux Mint / Pop!_OS

**Install from the APT repository** (after the repository has been published):

```bash
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://almezali.github.io/conkio/public.key | sudo gpg --dearmor --yes -o /etc/apt/keyrings/conkio.gpg
sudo chmod 0644 /etc/apt/keyrings/conkio.gpg
echo "deb [arch=amd64,arm64 signed-by=/etc/apt/keyrings/conkio.gpg] https://almezali.github.io/conkio stable main" | sudo tee /etc/apt/sources.list.d/conkio.list >/dev/null
sudo apt update
sudo apt install conkio conky-all
```

For repository setup and maintainer instructions, see [docs/apt-repository.md](docs/apt-repository.md). The direct `.deb` packages remain available from the [latest release](https://github.com/almezali/conkio/releases/latest).

### 📦 AppImage (any distribution)

```bash
wget https://github.com/almezali/conkio/releases/download/1.0.1/Conkio-x86_64.AppImage
chmod +x Conkio-x86_64.AppImage
./Conkio-x86_64.AppImage
```

> The AppImage bundles Conkio only — you still need **Conky** installed on your system.

### ARM64 (aarch64)

Replace `x86_64` with `aarch64` in any of the commands above (for example on a Raspberry Pi 4/5 or an ARM laptop). An AppImage is currently published for x86_64 only.

---

## 🚀 Quick start

1. **Launch** Conkio from your application menu, or run `conkio` in a terminal.
2. **Add themes** — drag a folder or archive onto the window, press **Folder** / **Archive**, or open the **Theme Marketplace** and install one in a click.
3. **Select** a theme and press **▶ Run**.
4. Open **Customize** to move it to the right monitor, change its alignment, or apply a color palette.
5. Use the **Health** page if something doesn't look right — Conkio tells you what's missing and how to fix it.
6. Save your favorite combination as a **Profile**, and optionally start it with your desktop session in **Settings**.

---

## 🧭 Interface at a glance

| Page | What it does |
|---|---|
| **Themes** | Your full library with search, filters, favorites and bulk actions |
| **Running** | Every theme currently managed by Conkio, with stop/restart controls |
| **Health scan** | Stable / Needs Fix / Broken status, plus deep scans per theme |
| **Customize** | Monitor, alignment, gaps, color palettes and the color editor |
| **Profiles** | Save, restore and auto-start groups of themes |
| **Details** | Files, health report and placement information for the selected theme |
| **Marketplace** | Search and install themes from the online stores |
| **Settings** | Light/dark mode, window behavior, performance, login start, tray |

---

## ⚙️ Requirements

| | |
|---|---|
| **Required** | [Conky](https://github.com/brndnmtthws/conky) (1.10+ for the modern Lua syntax; legacy themes are supported too) |
| **Optional** | Whatever your themes call — Conkio's health scan will tell you exactly what's missing |
| **Display** | X11 or Wayland (via XWayland or compositor rules) |

---

## 🔧 Building from source

You need a recent **stable Rust toolchain** (edition 2021) and the usual development libraries for a GUI application (fontconfig, xkbcommon, GTK 3 for native file dialogs). Exact package names vary by distribution.

```bash
# Debian / Ubuntu
sudo apt install build-essential pkg-config libfontconfig1-dev libxkbcommon-dev libgtk-3-dev

# Fedora
sudo dnf install gcc pkgconf-pkg-config fontconfig-devel libxkbcommon-devel gtk3-devel

# Arch
sudo pacman -S --needed base-devel fontconfig libxkbcommon gtk3
```

```bash
git clone https://github.com/almezali/conkio.git
cd conkio
cargo build --release

# Run it
./target/release/conkio
```

### Project layout

```
conkio/
├── Cargo.toml        # dependencies and package metadata
├── build.rs          # compiles the Slint UI at build time
├── src/
│   ├── main.rs       # application logic, callbacks, tray, marketplace client
│   └── engine.rs     # theme engine: discovery, launch, health, colors, backups, import
└── ui/
    └── main.slint    # the complete user interface
```

### Built with

[Rust](https://www.rust-lang.org/) · [Slint](https://slint.dev/) · [ksni](https://crates.io/crates/ksni) · [serde](https://serde.rs/) · [ureq](https://crates.io/crates/ureq) · [rfd](https://crates.io/crates/rfd) · [walkdir](https://crates.io/crates/walkdir) · [zip](https://crates.io/crates/zip) · [tar](https://crates.io/crates/tar) · [regex](https://crates.io/crates/regex) · [image](https://crates.io/crates/image)

---

## ❓ FAQ

<details>
<summary><b>Where are my themes stored?</b></summary>

Themes live in `~/.conky`. You can add more folders from **Settings → Add folder**, and Conkio will scan them too.
</details>

<details>
<summary><b>Will Conkio change my original theme files?</b></summary>

Only when you ask it to (placement, colors). Before every modification a backup is created, and you can bring it back with **Restore latest backup**. Launch-time tweaks are applied to a temporary copy, so your originals stay clean.
</details>

<details>
<summary><b>Does it work on Wayland?</b></summary>

Yes. Conkio detects Hyprland, Sway, Niri, COSMIC and others, and the health scan will tell you if a theme needs XWayland or a compositor window rule to appear correctly.
</details>

<details>
<summary><b>A theme is invisible after launching. What now?</b></summary>

Open the **Health** page and run a deep scan on it. On tiling Wayland compositors, try switching the window layer to **above** in **Settings**.
</details>

<details>
<summary><b>Does Conkio stop Conky processes I started myself?</b></summary>

No. "Stop all managed" only affects processes that Conkio launched and tracks.
</details>

---

## 🤝 Contributing

Contributions are very welcome! Whether it's a bug report, a feature idea, a translation or a pull request:

1. Fork the repository
2. Create your branch: `git checkout -b feature/amazing-idea`
3. Commit your changes: `git commit -m "Add amazing idea"`
4. Push to the branch: `git push origin feature/amazing-idea`
5. Open a Pull Request

Found a bug? [Open an issue](https://github.com/almezali/conkio/issues) and include your distribution, desktop environment/compositor, session type (X11/Wayland) and Conky version — the **Details** page shows most of this.

---

## 📜 License

Conkio is free software released under the **GNU General Public License v3.0 or later**. 

---

<div align="center">

**Made with ❤️ for the Linux desktop community by [almezali](https://github.com/almezali)**

If Conkio makes your desktop prettier, consider giving it a ⭐ — it helps others find it!

[![Star History](https://img.shields.io/github/stars/almezali/conkio?style=social)](https://github.com/almezali/conkio/stargazers)

</div>
