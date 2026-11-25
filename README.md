<div align="center">

# Jotun (`v0.5.0`)

**A fast, local-first personal knowledge base and note-taking CLI/TUI built in Rust.**
*Capture at the speed of thought. Organize with hierarchical tags. Stay under 10 MB RAM.*

<p align="center">
  <a href="https://crates.io/crates/jotun"><img src="https://img.shields.io/crates/v/jotun?style=for-the-badge&logo=rust&color=bd93f9" alt="Crates.io"></a>
  <a href="https://github.com/dev-Aatif/jot/releases"><img src="https://img.shields.io/github/v/release/dev-Aatif/jot?style=for-the-badge&logo=git&color=333333" alt="GitHub Release"></a>
  <a href="https://github.com/dev-Aatif/jot/actions"><img src="https://img.shields.io/github/actions/workflow/status/dev-Aatif/jot/ci.yml?style=for-the-badge&logo=githubactions&color=333333" alt="CI Status"></a>
  <a href="https://github.com/dev-Aatif/jot/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-333333?style=for-the-badge&logo=opensourceinitiative" alt="License"></a>
  <a href="https://github.com/dev-Aatif/jot/stargazers"><img src="https://img.shields.io/github/stars/dev-Aatif/jot?style=for-the-badge&logo=github&color=FFD700" alt="Stars"></a>
</p>

<p align="center">
  <a href="#installation"><strong>Installation</strong></a> ·
  <a href="#usage"><strong>CLI &amp; TUI Docs</strong></a> ·
  <a href="#configuration"><strong>Themes &amp; Config</strong></a> ·
  <a href="https://github.com/dev-Aatif/jot/blob/main/ROADMAP.md"><strong>Roadmap</strong></a> ·
  <a href="https://github.com/dev-Aatif/jot/issues"><strong>Issues</strong></a>
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/dev-Aatif/jot/main/jotun.gif" alt="Jotun Terminal Demo" width="800" />
</p>

</div>

---

## Highlights

- **Local-First SQLite + FTS5**: Embedded `rusqlite` with WAL mode, foreign-key cascades, `bm25` relevance ranking, and highlighted search snippets.
- **Editorial 3-Pane TUI**: Collapsible hierarchical tag tree with note counts, metadata-rich note list, syntax-highlighted Markdown preview (`syntect`), scrollbars, and `< 80`-column responsive layout.
- **Curated Theme Presets & `NO_COLOR`**: Built-in `dracula`, `catppuccin-mocha`, `nord`, `tokyo-night`, and `monochrome` themes with semantic color overrides.
- **Safe Workflows**: Delete confirmation modal (`d`), inline rename (`r`) and tag autocomplete (`t`), note pinning (`p`) and archiving (`a`), plus YAML frontmatter round-tripping in `$EDITOR`.
- **Composable Automation**: `--json` output across `ls`, `show`, `find`, `tags`, `stats`, and `doctor`; Markdown/JSON `export`; `bash`/`zsh`/`fish` completions; and `OSC 52` clipboard fallback for SSH/tmux.
- **Sub-10 MB Footprint**: Lazy `syntect` initialization, `NoteSummary` list projections, and `prepare_cached` queries keep idle TUI memory around `~8.5 MB` RSS.

---

## Performance Weight Class

| Tool | Interface | Idle RAM |
| :--- | :--- | :--- |
| **Jotun (`v0.5.0`)** | **CLI / TUI** | **~8.5 MB (`< 10 MB`)** |
| Neovim / Vim | CLI | 20 MB – 70 MB |
| Obsidian | GUI (Electron) | 400 MB – 900 MB |
| Logseq | GUI (Electron) | 500 MB – 1 GB |
| VS Code | GUI (Electron) | 600 MB – 1.5 GB |

---

## Installation

### From Crates.io (Recommended for Rust users)
Requires Rust `1.85+` (`edition = "2024"`):

```bash
cargo install jotun --locked
```

### Quick Binary Install (Linux & macOS)
Downloads the latest pre-built release binary (`x86_64` / `aarch64`) into `~/.local/bin/jotun` and verifies its SHA-256 checksum:

```bash
curl -fsSL https://raw.githubusercontent.com/dev-Aatif/jot/main/install.sh | bash
```

### Build From Source

```bash
git clone https://github.com/dev-Aatif/jot.git
cd jot
cargo build --release --locked
install -Dm755 target/release/jotun ~/.local/bin/jotun
```

---

## Usage

### Power-User CLI Examples

```bash
# Capture a note with title and hierarchical tags
jotun new "Fix PKCE verifier" --title "Auth Fix" -t work/jotun -t critical

# Pipe from stdin
echo "Action items from standup" | jotun new --title "Standup" -t work/meetings

# Ranked full-text search scoped by tag and source
jotun find "verifier" -t work --source manual

# Update metadata non-interactively
jotun edit 12 --title "OAuth PKCE Fix" -t security -t work/jotun --pin true

# Pipe structured JSON into jq
jotun ls --tag work --json | jq '.[].title'

# Export notes as YAML-frontmatter Markdown files
jotun export --format markdown --output ./notes-backup

# Verify database integrity, FTS5 sync, config, editor, and clipboard
jotun doctor
```

### Subcommands Reference

| Command | Description |
| :--- | :--- |
| `jotun dash` | Launch the interactive 3-pane TUI dashboard. |
| `jotun new [text]` | Create a note (or read from piped stdin) with `--title`, `-t/--tag`, `-s/--source`. |
| `jotun ls [-t tag] [-s src] [--archived] [--json]` | List notes filtered by tag hierarchy, source, or archive state. |
| `jotun show <id> [--json]` | Print full note content and metadata. |
| `jotun find <query> [-t tag] [-s src] [--archived] [--json]` | Ranked FTS5 search (`bm25`) with highlighted match snippets. |
| `jotun edit <id> [flags]` | Edit in `$EDITOR` (`--frontmatter`) or update `--title`, `-t`, `--pin`, `--archive`. |
| `jotun rm <id> [-f]` | Delete a note and prune orphan tags (`--force` skips confirmation). |
| `jotun cp <id>` | Copy note body to clipboard (`wl-copy`, `xclip`, `xsel`, `pbcopy`, or `OSC 52`). |
| `jotun paste` | Save a new note from clipboard contents. |
| `jotun tags [--archived] [--json]` | List unique tags with note counts. |
| `jotun stats [--json]` | Show Neofetch-style summary and top tag distribution. |
| `jotun export -f <markdown\|json> -o <path>` | Export notes to Markdown files or JSON (`-` for stdout). |
| `jotun doctor [--json]` | Check SQLite `integrity_check`, FTS5 sync, config, `$EDITOR`, and clipboard. |
| `jotun completions <bash\|zsh\|fish>` | Generate shell completion scripts. |

### TUI Keybindings (`jotun dash`)

| Category | Key | Action |
| :--- | :--- | :--- |
| **Navigation** | `Tab` / `Shift+Tab` | Cycle panes (`Tags` → `Notes` → `Preview`) |
| | `j` / `k` or `↓` / `↑` | Move selection or scroll `Preview` |
| | `Space` / `h` / `l` | Fold or expand nodes in the hierarchical tag tree |
| | `PageUp` / `PageDown` | Page scroll inside `Preview` |
| **Editing & Org** | `n` / `e` | Create or edit note in `$EDITOR` (YAML frontmatter round-trip) |
| | `r` / `t` | Inline rename title or edit tags (`Tab` autocomplete) |
| | `p` / `a` / `A` | Pin/unpin, archive/unarchive, or toggle archived visibility |
| | `d` / `y` | Open delete confirmation modal / copy to clipboard |
| **View & Search** | `/` or `s` (`c` clears) | Filter notes by FTS5 query (`Enter` keeps, `Esc` clears) |
| | `w` / `z` | Toggle line wrapping / zoom active pane |
| | `i` / `?` | Toggle **Insights Dashboard** / **Keybindings Help** modal |
| | `q` / `Esc` | Close modal/filter or quit |

---

## Configuration

Jotun creates `~/.config/jotun/config.toml` on first launch. All fields are optional and merge cleanly with defaults:

```toml
editor = "nvim"                # Overrides $VISUAL / $EDITOR (supports args, e.g. "code --wait")
db_path = "/path/to/jotun.db"  # Custom SQLite path (or set JOTUN_DB_PATH)
syntax_highlighting = true     # Toggle Markdown highlighting (also respects NO_COLOR)
frontmatter = true             # Include YAML frontmatter in TUI $EDITOR sessions
wrap_lines = true              # Initial Preview line-wrap state (toggle with `w`)

[theme]
preset = "dracula"             # dracula | catppuccin-mocha | nord | tokyo-night | monochrome
# Optional semantic overrides (legacy active_border/highlight_bg/highlight_fg also supported):
# border_active = "#bd93f9"
# bg_elevated = "#44475a"
# text_primary = "#f8f8f2"
```

**Environment Variables:**
- `JOTUN_DB_PATH` — Override SQLite database path.
- `JOTUN_CONFIG_PATH` — Override `config.toml` path.
- `NO_COLOR` — Disable terminal colors while preserving typographic hierarchy.

---

## Release History

- **v0.5.0**: Semantic theme presets, collapsible tag tree, rich 2-line note list + scrollbars, safe modals (`d`, `?`, `r`, `t`), pinning/archiving, YAML frontmatter, `< 80`-col layout, `bm25` ranked snippets, `--json`, `export`, `doctor`, `completions`, `OSC 52`, and lazy `syntect` loading.

<details>
<summary>Earlier releases (v0.3.0 – v0.4.1)</summary>

- **v0.4.1**: Foreign-key cascades, WAL mode, orphan tag pruning, `TerminalGuard`, event-driven rendering, and cross-platform SHA-256 release artifacts.
- **v0.4.0**: Insights Dashboard (`i`), `jotun stats` Penguin ASCII view, and Dracula theme.
- **v0.3.0**: Hierarchical tagging (`work/project`) and three-pane TUI dashboard.

</details>

See [ROADMAP.md](https://github.com/dev-Aatif/jot/blob/main/ROADMAP.md) for upcoming milestones.

---

## License

Distributed under the **MIT License**. See [LICENSE](https://github.com/dev-Aatif/jot/blob/main/LICENSE) for details.
