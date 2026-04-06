<div align="center">

# 🚀 JOTUN (v0.1.0)

**A lightning-fast, terminal-native note-taking tool built in Rust. One command to capture, one command to retrieve.**

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

<br />
<img src="https://via.placeholder.com/800x400?text=Jotun+-+The+Fastest+Notes+in+the+Terminal" alt="Project Preview" width="800" />
</div>

---

## 📚 Table of Contents

- [✨ Features](#-features)
- [🏗 Tech Stack](#-tech-stack)
- [⚡ Quick Start](#-quick-start)
- [📦 Installation](#-installation)
- [🧠 Usage](#-usage)
- [⚙️ Configuration](#-configuration)
- [🗂 Project Structure](#-project-structure)
- [🧪 Testing](#-testing)
- [🛣 Roadmap](#-roadmap)
- [🤝 Contributing](#-contributing)
- [🚀 What's Next?](#-whats-next)
- [📄 License](#-license)

---

## ✨ Features

- **Quick Capture**: Save notes from arguments or pipe from stdin (`echo "cmd" | jotun new`).
- **Clipboard Sync**: One-command copy/paste (`jotun cp` / `jotun paste`) for Wayland and X11.
- **Fast Search**: Instant full-text search powered by SQLite FTS5.
- **Local First**: Everything is stored in a single SQLite database on your machine.
- **Default Editor**: Edit notes in your preferred `$EDITOR` (vim, nano, etc.).

---

## 🏗 Tech Stack

- **Language**: [Rust](https://www.rust-lang.org/)
- **Database**: [SQLite](https://www.sqlite.org/) (via [rusqlite](https://github.com/rusqlite/rusqlite))
- **CLI Framework**: [clap](https://github.com/clap-rs/clap)
- **Styling**: [colored](https://github.com/colored-rs/colored)

---

## ⚡ Quick Start

```bash
# Save a command you always forget
jotun new "deploy to production: kubectl rollout restart..."

# Copy it to your clipboard for use
jotun cp 1
```

---

## 📦 Installation

### 1. Pre-built Binary (Fedora/Linux)
```bash
curl -sSL https://raw.githubusercontent.com/dev-Aatif/jot/main/install.sh | bash
```

### 2. Manual (From Source)
```bash
git clone https://github.com/dev-Aatif/jot
cd jot
cargo build --release
cp target/release/jotun ~/.local/bin/
```

---

## 🧠 Usage

### Global Commands
| Command | Description |
| :--- | :--- |
| `jotun` | Print current help and usage metadata. |
| `jotun -h` | Print a concise summary of all subcommands. |
| `jotun --help` | Print the full documentation of all subcommands. |

---

### Command Summary

<<<<<<< HEAD
- `jot` - list all commands.
- `jot -h or --help` Displays all commands
- `jot new [text]` – Save a new note (reads from stdin if no text provided).
- `jot ls` – List all notes with IDs and previews.
- `jot show [id]` – Display the full content of a specific note.
- `jot find [query]` – Search notes using SQLite FTS5.
- `jot edit [id]` – Open a note in your system's `$EDITOR`.
- `jot cp [id]` – Copy a note's body to the clipboard.
- `jot paste` – Create a new note from your clipboard content.
- `jot rm [id]` – Delete a note.
=======
- `jotun new [text]` – Save a new note (reads from stdin if no text provided).
- `jotun ls` – List all notes with IDs and previews.
- `jotun show [id]` – Display the full content of a specific note.
- `jotun find [query]` – Search notes using SQLite FTS5.
- `jotun edit [id]` – Open a note in your system's `$EDITOR`.
- `jotun cp [id]` – Copy a note's body to the clipboard.
- `jotun paste` – Create a new note from your clipboard content.
- `jotun rm [id]` – Delete a note.
>>>>>>> f28fcd9 (feat: Official v0.1.0 Release of Jotun)

---

---

## ⚙️ Configuration

Set the `JOTUN_DB_PATH` environment variable to override the default database location:
- **Default:** `~/.local/share/jotun/jotun.db`

---

---

## 🗂 Project Structure

```text
├── Cargo.toml      # Build & Dependency manifest
├── README.md       # Professional documentation
├── src/
│   ├── main.rs     # CLI Entry point & Router
│   └── db.rs       # SQLite & Search engine logic
└── tests/          # Integration tests
```

---

## 🧪 Testing

```bash
# Run the local test suite
cargo test
```

---

## 🛣 Roadmap

- [x] V0.1.0: Core CLI (Stable)
- [ ] V0.2.0: Tagging & Categorization
- [ ] V0.3.0: macOS Clipboard support

---

## 🚀 What's Next?

Our **V1.0 Goal** is a full interactive TUI dashboard. Imagine a system as premium as `btop` but for managing thousands of code snippets, notes, and task lists, all powered by our lightning-fast search engine.

---

## 🤝 Contributing

We welcome your PRs and bug reports in the [issue tracker](https://github.com/dev-Aatif/jot/issues)!

---

## 📄 License

Distributed under the **MIT License**. See `LICENSE` for more information.
