<div align="center">
  
# 🚀 JOTUN (v0.2.0)

**The lightning-fast, terminal-native note manager. Capture at the speed of thought. Retrieve at the speed of Rust.**

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-333333?style=for-the-badge&logo=opensourceinitiative" alt="License"></a>
  <a href="https://github.com/dev-Aatif/jot/releases"><img src="https://img.shields.io/github/v/release/dev-Aatif/jot?style=for-the-badge&logo=git&color=333333" alt="Version"></a>
  <a href="https://github.com/dev-Aatif/jot/actions"><img src="https://img.shields.io/github/actions/workflow/status/dev-Aatif/jot/ci.yml?style=for-the-badge&logo=githubactions&color=333333" alt="Tests"></a>
  <a href="https://github.com/dev-Aatif/jot/stargazers"><img src="https://img.shields.io/github/stars/dev-Aatif/jot?style=for-the-badge&logo=github&color=FFD700" alt="Stars"></a>
</p>

<p align="center">
  <a href="#🧠-usage"><strong>Explore the Docs »</strong></a>
  <br>
  <br>
  <a href="https://github.com/dev-Aatif/jot">View Demo</a> ·
  <a href="https://github.com/dev-Aatif/jot/issues">Report Bug</a> ·
  <a href="https://github.com/dev-Aatif/jot/issues">Request Feature</a>
</p>

<p align="center">
  <img src="./jotun.gif" alt="Jotun Terminal Demo" width="800" />
</p>
</div>

---

## 🆕 New in v0.2.0: The Interactive Dashboard
Jotun isn't just a CLI tool anymore. v0.2.0 introduces a **vibrant, btop-inspired TUI dashboard** that turns your note-taking into a high-speed command center.

- **Immersive Sidebar**: Browse your notes with Vim-style `j/k` navigation.
- **Instant Preview**: See full note content instantly as you scroll.
- **Dynamic Search**: Type `/` to filter your entire database in real-time.
- **One-Key Actions**: 
  - `n` — Create a new note without leaving the dashboard.
  - `y` — Copy current note to clipboard.
  - `e` — Edit note in your preferred `$EDITOR`.
  - `d` — Delete with confirmation.

---

## ✨ Features

- **Lightning Fast**: Built in Pure Rust with a SQLite FTS5 backend.
- **Clipboard Native**: First-class support for Wayland (`wl-copy`) and X11 (`xclip`).
- **Zero Friction**: Pipe from stdin, capture from args, or use the interactive UI.
- **Local First**: Your data stays on your machine. Always.
- **Minimalist**: 100% terminal focused. No bloat, no unnecessary UI.

---

## ⚡ Quick Start

```bash
# Enter the interactive dashboard
jotun dash

# Save a quick command from anywhere
jotun new "kubectl get pods --all-namespaces"

# Copy Note #1 back to your clipboard
jotun cp 1
```

---

## 📦 Installation

### 1. The Developer Way (Recommended)
```bash
cargo install jotun
```

### 2. The One-Liner (Pre-built Binary)
```bash
curl -sSL https://raw.githubusercontent.com/dev-Aatif/jot/main/install.sh | bash
```

### 3. Manual Build
```bash
git clone https://github.com/dev-Aatif/jot && cd jot
cargo build --release
cp target/release/jotun ~/.local/bin/
```

---

## 🧠 Usage

### Global Interface
| Command | Action |
| :--- | :--- |
| `jotun dash` | **Launch the Interactive Dashboard (v0.2.0)** |
| `jotun -h / --help` | Display quick/full help metadata. |

---

### CLI Subcommands
- `jotun new [text]` – Save a new note (reads from stdin if no text provided).
- `jotun ls` – List notes with previews.
- `jotun show [id]` – Full note display.
- `jotun find [query]` – Global search.
- `jotun edit [id]` – Open in system editor.
- `jotun cp [id]` – Copy to clipboard.
- `jotun paste` – Create note from clipboard.
- `jotun rm [id]` – Delete note.

---

## ⚙️ Configuration

Override the default database location with `JOTUN_DB_PATH`:
- **Default:** `~/.local/share/jotun/jotun.db`

---

## 🧪 Testing

```bash
# Run the local test suite
cargo test
```

---

## 🛣 Roadmap

- [x] V0.1.0: Core CLI (Stable)
- [x] V0.2.0: Interactive TUI Dashboard (Current)
- [ ] V0.3.0: Tagging & Categorization
- [ ] V0.4.0: Insights & Statistics Dashboard

---

## 🤝 Contributing

We welcome your PRs and bug reports in the [issue tracker](https://github.com/dev-Aatif/jot/issues)!

---

## 📄 License

Distributed under the **MIT License**. See `LICENSE` for more information.
