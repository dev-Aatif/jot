# Jot — Terminal Quick Notes

A fast, terminal-native note-taking tool built in Rust. It lets you capture, retrieve, and search text snippets without leaving your shell.

## ⚡️ Features

- **Quick Capture:** Save notes from arguments or pipe from stdin (`echo "cmd" | jot new`).
- **Clipboard Sync:** One-command copy/paste (`jot cp` / `jot paste`) for Wayland and X11.
- **Fast Search:** Instant full-text search powered by SQLite FTS5.
- **Local First:** Everything is stored in a single SQLite database on your machine.
- **Default Editor:** Edit notes in your preferred `$EDITOR` (vim, nano, etc.).

---

## 🛠️ Installation

```bash
# Clone and build
git clone <this-repo>
cd jot
cargo build --release

# Optional: Add to your PATH
cp target/release/jot ~/.local/bin/jot
```

### Dependencies (Linux)
- **Wayland users:** `wl-clipboard`
- **X11 users:** `xclip`

---

## 📖 Usage

### Saving Notes
```bash
# From command line
jot new "This is my first note"

# From stdin (pipe)
cat logs.txt | grep "ERROR" | jot new

# From clipboard
jot paste
```

### Retrieving & Managing
```bash
# List all notes
jot ls

# View a specific note
jot show 1

# Search all notes
jot find "error"

# Copy a note back to clipboard
jot cp 1

# Edit a note (opens your $EDITOR)
jot edit 1

# Delete a note
jot rm 1
```

---

## 🗄️ Storage

By default, notes are stored at:
- **Fedora/Linux:** `~/.local/share/jot/jot.db`

You can override this by setting the `JOT_DB_PATH` environment variable.

---

## 🚀 What's Next? (V2)
- **TUI Dashboard:** A proper interactive `btop`-style interface for note management.
- **Sync to Disk:** Automatically mirror notes as `.md` files for use in Obsidian/VS Code.
