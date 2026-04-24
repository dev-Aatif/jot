<div align="center">
  
# 🚀 JOTUN (v0.3.0)

**The lightning-fast, terminal-native personal knowledge base. Capture at the speed of thought. Organize with hierarchical power.**

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

## 🆕 New in v0.3.0: Hierarchical Knowledge & Performance
v0.3.0 transforms Jotun from a simple note-taker into a structured personal knowledge base.

- **Hierarchical Tagging**: Organize notes with nested paths (e.g., `work/project-a`, `personal/finance`).
- **Three-Pane Dashboard**: A new high-efficiency TUI with a dedicated Tags Sidebar and Note Navigator.
- **Syntax Highlighting**: Beautiful, high-performance Markdown rendering in the preview pane.
- **Note Titles**: Explicit titles for your notes with smart fallbacks to body content.
- **Safe Editor Fallback**: Never crash again—Jotun intelligently falls back to `nano` if your preferred editor (like `nvim`) isn't available.

---

## 🚀 Performance: The Weight Class
Jotun is engineered for systems where every megabyte counts. It lives in your RAM like it's not even there.

| Editor | Platform | RAM Usage (Idle) |
| :--- | :--- | :--- |
| **Jotun** | **CLI/TUI** | **< 10 MB** |
| Neovim / Vim | CLI | 20 MB - 70 MB |
| Obsidian | GUI (Electron) | 400 MB - 900 MB |
| VS Code | GUI (Electron) | 600 MB - 1.5 GB |
| Logseq | GUI (Electron) | 500 MB - 1 GB |

---

## ✨ Features

- **Lightning Fast**: Built in Pure Rust with a SQLite FTS5 backend.
- **Metadata Aware**: Automatically tracks `source`, `created`, and `updated` timestamps.
- **Clipboard Native**: First-class support for Wayland (`wl-copy`) and X11 (`xclip`).
- **Local First**: Your data stays on your machine. Always.
- **Minimalist**: 100% terminal focused. No bloat, no unnecessary UI.

---

## 🧠 Usage

### CLI Power-User Commands
```bash
# Create a note with metadata
jotun new "Fix login bug" --title "Auth Fix" -t work/jotun -t critical

# List notes in a specific tag hierarchy
jotun ls --tag work

# View all unique tags
jotun tags
```

### Subcommands Reference
| Command | Action |
| :--- | :--- |
| `jotun dash` | Launch the **Three-Pane Interactive Dashboard**. |
| `jotun new [text]` | Save a note (reads from stdin if text is missing). |
| `jotun ls [-t tag]` | List notes with hierarchical tag filtering. |
| `jotun tags` | Display all unique tags in a tree-like list. |
| `jotun show [id]` | Full note display with complete metadata. |
| `jotun find [query]` | Global FTS5 full-text search across titles and bodies. |

---

## ⚙️ Configuration

Jotun creates a default configuration file at `~/.config/jotun/config.toml` on its first run.

```toml
editor = "nvim"                # Your preferred system editor
syntax_highlighting = true     # Toggle Markdown highlighting in TUI

[theme]
active_border = "yellow"       # Color for the active pane
highlight_bg = "cyan"          # Selection background color
highlight_fg = "black"         # Selection text color
```

---

## 🛣 Roadmap

- [x] V0.1.0: Core CLI (Stable)
- [x] V0.2.0: Interactive TUI Dashboard
- [x] V0.3.0: Hierarchical Tagging & Titles (Completed)
- [ ] V0.4.0: Insights & Statistics Dashboard
- [ ] V0.5.0: Encryption & Secure Notes

---

## 📄 License

Distributed under the **MIT License**. See `LICENSE` for more information.
