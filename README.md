<div align="center">
  
# 🚀 JOTUN (v0.5.0)

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
  <a href="./ROADMAP.md"><strong>View Roadmap »</strong></a> ·
  <a href="https://github.com/dev-Aatif/jot/issues">Report Bug</a> ·
  <a href="https://github.com/dev-Aatif/jot/issues">Request Feature</a>
</p>

<p align="center">
  <img src="./jotun.gif" alt="Jotun Terminal Demo" width="800" />
</p>
</div>

---

## 🆕 New in v0.5.0: Indie TUI Polish, Ranked Search & Composable CLI
v0.5.0 elevates Jotun's TUI, search relevance, diagnostics, and automation ergonomics while staying under **10 MB idle RSS** (`~8.5 MB` measured):

- **Editorial TUI & Semantic Themes**: Five curated presets (`dracula`, `catppuccin-mocha`, `nord`, `tokyo-night`, `monochrome`), rounded active borders, clean Unicode chrome (`NO_COLOR`-ready), collapsible hierarchical tag tree with counts, 2-line rich note list + scrollbars, document header + scroll percentage + line-wrap (`w`) & zoom (`z`) toggles, and `< 80`-column responsive collapse.
- **Safe Modals & Inline Workflows**: Confirmation modal on delete (`d`), `?` keybinding cheatsheet, inline rename (`r`), inline tag editor with `Tab` autocomplete (`t`), pinning (`p`), archiving (`a` / `A`), and optional YAML frontmatter (`title`, `tags`) round-tripping in `$EDITOR`.
- **Ranked Search & Composable CLI**: `bm25` relevance-ranked FTS5 search with highlighted snippets (`jotun find`), `--json` output across `ls`/`show`/`find`/`tags`/`stats`/`doctor`, `jotun export` (Markdown + JSON), `jotun doctor`, `jotun completions <bash|zsh|fish>`, and `OSC 52` clipboard fallback for SSH/tmux.
- **Performance & Security Prep**: Lazy `syntect` initialization, `50 KB / 500 line` large-note highlight cap, `NoteSummary` projections, `prepare_cached` SQLite queries, and FTS5 encryption-exclusion triggers (`is_encrypted`).

---

## 📊 v0.4.0: Insights & Visual Analytics
v0.4.0 introduced the **Jotun Intelligence Suite**, giving you deep visibility into your knowledge base.

- **Insights Dashboard**: Full-screen dashboard in the TUI (press `i`) with Htop-style gauges for source distribution and a recent-notes table.
- **Penguin Stats CLI**: Neofetch-style `jotun stats` command with Penguin ASCII art and system metadata.
- **Dracula Theme Engine**: Default Dracula palette (`#bd93f9`, `#44475a`, `#f8f8f2`).

---

## 📅 v0.3.0: Hierarchical Knowledge & Performance
v0.3.0 transformed Jotun from a simple note-taker into a structured personal knowledge base.

- **Hierarchical Tagging**: Organize notes with nested paths (e.g., `work/project-a`, `personal/finance`).
- **Three-Pane Dashboard**: High-efficiency TUI with a dedicated Tags Sidebar and Note Navigator.

---

## 🚀 Performance: The Weight Class
Jotun is engineered for systems where every megabyte counts (~8 MB idle RSS in TUI mode).

| Editor | Platform | RAM Usage (Idle) |
| :--- | :--- | :--- |
| **Jotun** | **CLI/TUI** | **< 10 MB** |
| Neovim / Vim | CLI | 20 MB - 70 MB |
| Obsidian | GUI (Electron) | 400 MB - 900 MB |
| VS Code | GUI (Electron) | 600 MB - 1.5 GB |
| Logseq | GUI (Electron) | 500 MB - 1 GB |

---

## ✨ Features

- **Lightning Fast**: Built in Pure Rust with a bundled SQLite FTS5 backend (`rusqlite`).
- **Metadata Aware**: Automatically tracks `source`, `created`, and `updated` timestamps.
- **Clipboard Native**: Supports Wayland (`wl-copy`/`wl-paste`), X11 (`xclip`/`xsel`), and macOS (`pbcopy`/`pbpaste`).
- **Local First**: Your data stays on your machine. Always.
- **Minimalist**: 100% terminal focused. No bloat, no unnecessary UI.

---

## 📦 Installation

### Quick Install (Linux & macOS)
Downloads the latest release binary for your OS/architecture (`x86_64` / `aarch64`) into `~/.local/bin/jotun` and verifies its SHA-256 checksum:

```bash
curl -fsSL https://raw.githubusercontent.com/dev-Aatif/jot/main/install.sh | bash
```

### Manual From Source
Requires Rust `1.85+` (`edition = "2024"`):

```bash
git clone https://github.com/dev-Aatif/jot.git
cd jot
cargo build --release --locked
install -Dm755 target/release/jotun ~/.local/bin/jotun
```

---

## 🧠 Usage

### CLI Power-User Commands
```bash
# Create a note with metadata
jotun new "Fix login bug" --title "Auth Fix" -t work/jotun -t critical

# Pipe from stdin
echo "Meeting notes" | jotun new --title "Standup" -t work/meetings

# List notes in a specific tag hierarchy
jotun ls --tag work

# View all unique tags
jotun tags
```

### Subcommands Reference
| Command | Action |
| :--- | :--- |
| `jotun dash` | Launch the **Three-Pane Interactive Dashboard**. |
| `jotun new [text]` | Save a note with optional `--title`, `-t/--tag`, and `-s/--source` (reads piped stdin if text is omitted). |
| `jotun ls [-t tag] [-s src] [--archived] [--json]` | List notes with optional hierarchical tag/source filtering or JSON output. |
| `jotun tags [--archived] [--json]` | Display all unique tags with note counts. |
| `jotun show <id> [--json]` | Full note display with complete metadata. |
| `jotun find <query> [-t tag] [-s src] [--archived] [--json]` | Ranked FTS5 full-text search (`bm25`) with highlighted snippets. |
| `jotun edit <id> [--title ...] [-t ...] [--pin bool] [--archive bool] [--frontmatter]` | Edit a note in `$EDITOR` or update metadata non-interactively. |
| `jotun rm <id> [-f]` | Delete a note (prompts for confirmation unless `--force`). |
| `jotun cp <id>` | Copy a note's body to the system clipboard (with `OSC 52` fallback). |
| `jotun paste` | Create a new note from the current system clipboard content. |
| `jotun stats [--json]` | View **Neofetch-style analytics** and top tag distribution. |
| `jotun export -f <markdown\|json> -o <path>` | Export notes to YAML-frontmatter Markdown files or JSON. |
| `jotun doctor [--json]` | Verify SQLite integrity, FTS5 sync, config, editor, and clipboard tools. |
| `jotun completions <bash\|zsh\|fish>` | Generate shell completion scripts. |

### TUI Keybindings (`jotun dash`)
| Key | Action |
| :--- | :--- |
| `Tab` / `Shift+Tab` | Cycle active pane (`Tags` → `Notes` → `Preview`) |
| `j` / `k` or `↓` / `↑` | Navigate tags/notes or scroll `Preview` |
| `Space` / `h` / `l` | Fold or expand nodes in the hierarchical tag tree |
| `PageUp` / `PageDown` | Page scroll inside `Preview` |
| `/` or `s` (`c` clears) | Search notes (`Enter` keeps query, `Esc` clears & exits) |
| `n` / `e` | Create or edit note in `$EDITOR` (with YAML frontmatter round-trip) |
| `r` / `t` | Inline rename title or edit tags (with `Tab` autocomplete) |
| `p` / `a` / `A` | Pin/unpin, archive/unarchive, or toggle viewing archived notes |
| `w` / `z` | Toggle line wrapping or zoom active pane |
| `y` | Copy the selected note to clipboard (or `OSC 52`) |
| `d` | Open delete confirmation modal |
| `i` / `?` | Toggle **Insights Dashboard** or **Keybindings Help** modal |
| `q` or `Esc` | Close modal/filter or quit |

---

## ⚙️ Configuration

Jotun creates a default configuration file at `~/.config/jotun/config.toml` on its first run. All fields are optional—partial configs merge cleanly with defaults.

```toml
editor = "nvim"                # Overrides $VISUAL / $EDITOR (supports args, e.g. "code --wait")
db_path = "/path/to/jotun.db"  # Optional custom SQLite path (or set JOTUN_DB_PATH)
syntax_highlighting = true     # Toggle Markdown highlighting in TUI (also respects NO_COLOR)
frontmatter = true             # Include YAML frontmatter (title, tags) in TUI $EDITOR sessions
wrap_lines = true              # Initial line-wrap state in TUI Preview (toggle with `w`)

[theme]
preset = "dracula"             # dracula | catppuccin-mocha | nord | tokyo-night | monochrome
# Optional semantic overrides (legacy active_border/highlight_bg/highlight_fg also supported):
# border_active = "#bd93f9"
# bg_elevated = "#44475a"
# text_primary = "#f8f8f2"
```

---

## 🛣 Roadmap

Jotun has an ambitious path ahead. From visual insights to full encryption and sync.

👉 **[View the full Roadmap here](./ROADMAP.md)**

---

## 📄 License

Distributed under the **MIT License**. See `LICENSE` for more information.
