mod commands;
mod config;
mod db;
mod frontmatter;
mod tui;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use colored::*;
use commands::{CompletionShell, EditOptions, ExportFormat, NewOptions};
use config::Config;
use db::Database;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "jotun", version)]
#[command(about = "A terminal-native quick-notes tool.", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new note from text or stdin
    New {
        /// The content of the note. If omitted, reads from stdin.
        text: Option<String>,
        /// Optional title for the note.
        #[arg(long)]
        title: Option<String>,
        /// Hierarchical tags (e.g. "work/proj1"). Can be used multiple times.
        #[arg(short, long)]
        tag: Vec<String>,
        /// Optional source tag for the note.
        #[arg(short, long, default_value = "manual")]
        source: String,
    },
    /// List notes
    Ls {
        /// Filter by tag (supports hierarchical search, e.g. "work" matches "work/p1")
        #[arg(short, long)]
        tag: Option<String>,
        /// Filter by source (e.g. "tui", "manual", "clipboard")
        #[arg(short, long)]
        source: Option<String>,
        /// Include archived notes
        #[arg(long)]
        archived: bool,
        /// Emit structured JSON output
        #[arg(long)]
        json: bool,
    },
    /// View a specific note
    Show {
        /// The ID of the note to show.
        id: i64,
        /// Emit structured JSON output
        #[arg(long)]
        json: bool,
    },
    /// Find notes by full-text search with relevance ranking and snippets
    Find {
        /// The search query.
        query: String,
        /// Filter results by hierarchical tag
        #[arg(short, long)]
        tag: Option<String>,
        /// Filter results by source
        #[arg(short, long)]
        source: Option<String>,
        /// Include archived notes
        #[arg(long)]
        archived: bool,
        /// Emit structured JSON output
        #[arg(long)]
        json: bool,
    },
    /// Edit a specific note in your default editor or update metadata
    Edit {
        /// The ID of the note to edit.
        id: i64,
        /// Update note title without opening an editor (unless --editor is set)
        #[arg(long)]
        title: Option<String>,
        /// Replace note tags (can be passed multiple times)
        #[arg(short, long)]
        tag: Vec<String>,
        /// Pin or unpin the note (true/false)
        #[arg(long)]
        pin: Option<bool>,
        /// Archive or unarchive the note (true/false)
        #[arg(long)]
        archive: Option<bool>,
        /// Include YAML frontmatter (title, tags) in the editor temp file
        #[arg(long)]
        frontmatter: bool,
        /// Also open the external editor after applying metadata flags
        #[arg(long)]
        editor: bool,
    },
    /// Delete a specific note
    Rm {
        /// The ID of the note to delete.
        id: i64,
        /// Proceed without confirmation.
        #[arg(short, long)]
        force: bool,
    },
    /// Copy a note to the system clipboard (with OSC 52 fallback)
    Cp {
        /// The ID of the note to copy.
        id: i64,
    },
    /// Create a new note from the current clipboard content
    Paste,
    /// List all unique tags with note counts
    Tags {
        /// Include tags on archived notes
        #[arg(long)]
        archived: bool,
        /// Emit structured JSON output
        #[arg(long)]
        json: bool,
    },
    /// View note statistics
    Stats {
        /// Emit structured JSON output
        #[arg(long)]
        json: bool,
    },
    /// Export notes to Markdown files or JSON
    Export {
        /// Output format (markdown or json)
        #[arg(short, long, value_enum)]
        format: ExportFormat,
        /// Destination directory (for markdown) or file path / '-' (for json)
        #[arg(short, long)]
        output: PathBuf,
        /// Filter exported notes by tag
        #[arg(short, long)]
        tag: Option<String>,
        /// Include archived notes
        #[arg(long)]
        archived: bool,
    },
    /// Run health and environment diagnostics
    Doctor {
        /// Emit structured JSON output
        #[arg(long)]
        json: bool,
    },
    /// Generate shell completions
    Completions {
        /// Target shell (bash, zsh, fish)
        #[arg(value_enum)]
        shell: CompletionShell,
    },
    /// Launch the interactive TUI dashboard
    Dash,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut stdout = io::stdout().lock();

    if let Commands::Completions { shell } = cli.command {
        let cmd = Cli::command();
        return commands::handle_completions(&cmd, shell, &mut stdout);
    }

    let config = match Config::load() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!(
                "{} Failed to load config ({}); using defaults.",
                "!".yellow(),
                err
            );
            Config::default()
        }
    };
    let db_path = get_db_path(&config)?;
    let db = Database::new(db_path.clone())?;

    match cli.command {
        Commands::New {
            text,
            title,
            tag,
            source,
        } => {
            let mut stdin = io::stdin().lock();
            commands::handle_new(
                &db,
                NewOptions {
                    text,
                    title,
                    tags: &tag,
                    source: &source,
                    stdin_is_tty: io::stdin().is_terminal(),
                },
                &mut stdin,
                &mut stdout,
            )?;
        }
        Commands::Ls {
            tag,
            source,
            archived,
            json,
        } => {
            commands::handle_ls(
                &db,
                tag.as_deref(),
                source.as_deref(),
                archived,
                json,
                &mut stdout,
            )?;
        }
        Commands::Show { id, json } => {
            commands::handle_show(&db, id, json, &mut stdout)?;
        }
        Commands::Find {
            query,
            tag,
            source,
            archived,
            json,
        } => {
            commands::handle_find(
                &db,
                &query,
                tag.as_deref(),
                source.as_deref(),
                archived,
                json,
                &mut stdout,
            )?;
        }
        Commands::Edit {
            id,
            title,
            tag,
            pin,
            archive,
            frontmatter,
            editor,
        } => {
            commands::handle_edit(
                &db,
                &config,
                EditOptions {
                    id,
                    title,
                    tags: tag,
                    pin,
                    archive,
                    frontmatter,
                    open_editor: editor,
                },
                &mut stdout,
            )?;
        }
        Commands::Rm { id, force } => {
            let mut stdin = io::stdin().lock();
            commands::handle_rm(&db, id, force, &mut stdin, &mut stdout)?;
        }
        Commands::Cp { id } => {
            commands::handle_cp(&db, id, &mut stdout)?;
        }
        Commands::Paste => {
            commands::handle_paste(&db, &mut stdout)?;
        }
        Commands::Tags { archived, json } => {
            commands::handle_tags(&db, archived, json, &mut stdout)?;
        }
        Commands::Stats { json } => {
            commands::handle_stats(&db, &db_path, json, &mut stdout)?;
        }
        Commands::Export {
            format,
            output,
            tag,
            archived,
        } => {
            commands::handle_export(&db, format, &output, tag.as_deref(), archived, &mut stdout)?;
        }
        Commands::Doctor { json } => {
            commands::handle_doctor(&db, &db_path, json, &mut stdout)?;
        }
        Commands::Completions { .. } => unreachable!(),
        Commands::Dash => {
            drop(stdout);
            tui::run_tui(&db, &config, db_path)?;
        }
    }

    Ok(())
}

fn get_db_path(config: &Config) -> Result<PathBuf> {
    if let Some(path) = &config.db_path {
        return Ok(PathBuf::from(path));
    }

    if let Ok(p) = std::env::var("JOTUN_DB_PATH") {
        return Ok(PathBuf::from(p));
    }

    let mut path =
        dirs::data_dir().ok_or_else(|| anyhow::anyhow!("Could not find user data directory"))?;
    path.push("jotun");
    path.push("jotun.db");
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_cli_parses_subcommands() {
        let cli = Cli::try_parse_from(["jotun", "new", "hello", "-t", "work/p1", "--title", "T"])
            .expect("new command should parse");
        match cli.command {
            Commands::New {
                text,
                title,
                tag,
                source,
            } => {
                assert_eq!(text.as_deref(), Some("hello"));
                assert_eq!(title.as_deref(), Some("T"));
                assert_eq!(tag, vec!["work/p1"]);
                assert_eq!(source, "manual");
            }
            _ => panic!("expected New command"),
        }

        assert!(Cli::try_parse_from(["jotun", "dash"]).is_ok());
        assert!(Cli::try_parse_from(["jotun", "stats", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["jotun", "ls", "-t", "work", "--json", "--archived"]).is_ok());
        assert!(Cli::try_parse_from(["jotun", "show", "1", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["jotun", "find", "rust", "-t", "work", "--json"]).is_ok());
        assert!(
            Cli::try_parse_from(["jotun", "edit", "1", "--title", "New", "--pin", "true"]).is_ok()
        );
        assert!(Cli::try_parse_from(["jotun", "rm", "1", "--force"]).is_ok());
        assert!(Cli::try_parse_from(["jotun", "cp", "1"]).is_ok());
        assert!(Cli::try_parse_from(["jotun", "paste"]).is_ok());
        assert!(Cli::try_parse_from(["jotun", "tags", "--json"]).is_ok());
        assert!(
            Cli::try_parse_from(["jotun", "export", "-f", "markdown", "-o", "/tmp/out"]).is_ok()
        );
        assert!(Cli::try_parse_from(["jotun", "doctor", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["jotun", "completions", "fish"]).is_ok());
    }

    #[test]
    fn test_read_edited_body_preserves_leading_indent() -> Result<()> {
        let tf = commands::create_markdown_tempfile("  indented line\nsecond line\n")?;
        let read_back = commands::read_edited_body(tf.path())?;
        assert_eq!(read_back, "  indented line\nsecond line");
        Ok(())
    }

    #[test]
    fn test_cli_json_outputs_and_metadata_edit() -> Result<()> {
        let db = Database::in_memory()?;
        let config = Config::default();
        let mut out = Vec::new();
        let mut stdin = Cursor::new(b"");
        let id = commands::handle_new(
            &db,
            NewOptions {
                text: Some("PKCE verifier auth fix".to_string()),
                title: Some("Auth Fix".to_string()),
                tags: &["work/jotun".to_string()],
                source: "cli",
                stdin_is_tty: true,
            },
            &mut stdin,
            &mut out,
        )?;

        out.clear();
        commands::handle_ls(&db, None, None, false, true, &mut out)?;
        let ls_json: serde_json::Value = serde_json::from_slice(&out)?;
        assert_eq!(ls_json.as_array().unwrap().len(), 1);
        assert_eq!(ls_json[0]["title"], "Auth Fix");

        out.clear();
        commands::handle_find(
            &db,
            "PKCE",
            Some("work"),
            Some("cli"),
            false,
            true,
            &mut out,
        )?;
        let find_json: serde_json::Value = serde_json::from_slice(&out)?;
        assert_eq!(find_json.as_array().unwrap().len(), 1);
        assert!(find_json[0]["snippet"].as_str().unwrap().contains('«'));

        out.clear();
        commands::handle_edit(
            &db,
            &config,
            EditOptions {
                id,
                title: Some("Renamed Auth".to_string()),
                tags: vec!["security".to_string()],
                pin: Some(true),
                archive: None,
                frontmatter: false,
                open_editor: false,
            },
            &mut out,
        )?;
        let updated = db.get_note(id)?;
        assert_eq!(updated.title.as_deref(), Some("Renamed Auth"));
        assert_eq!(updated.tags, vec!["security".to_string()]);
        assert!(updated.pinned);

        out.clear();
        commands::handle_tags(&db, false, true, &mut out)?;
        let tags_json: serde_json::Value = serde_json::from_slice(&out)?;
        assert_eq!(tags_json[0]["name"], "security");
        assert_eq!(tags_json[0]["count"], 1);
        Ok(())
    }

    #[test]
    fn test_export_doctor_completions_and_osc52() -> Result<()> {
        let db = Database::in_memory()?;
        db.create_note(
            "Markdown export body",
            Some("Export Title"),
            "cli",
            &["docs".to_string()],
        )?;

        let dir = tempfile::tempdir()?;
        let md_dir = dir.path().join("md");
        let mut out = Vec::new();
        commands::handle_export(&db, ExportFormat::Markdown, &md_dir, None, false, &mut out)?;
        let exported_file = md_dir.join("1-export-title.md");
        assert!(exported_file.exists());
        let content = std::fs::read_to_string(exported_file)?;
        assert!(content.contains("title: \"Export Title\""));
        assert!(content.contains("Markdown export body"));

        let json_path = dir.path().join("notes.json");
        out.clear();
        commands::handle_export(&db, ExportFormat::Json, &json_path, None, false, &mut out)?;
        let parsed: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(json_path)?)?;
        assert_eq!(parsed.as_array().unwrap().len(), 1);

        out.clear();
        commands::handle_doctor(&db, dir.path(), true, &mut out)?;
        let report: serde_json::Value = serde_json::from_slice(&out)?;
        assert_eq!(report["database"]["integrity_ok"], true);
        assert_eq!(report["database"]["fts_in_sync"], true);

        let cmd = Cli::command();
        for shell in [
            CompletionShell::Bash,
            CompletionShell::Zsh,
            CompletionShell::Fish,
        ] {
            out.clear();
            commands::handle_completions(&cmd, shell, &mut out)?;
            let script = String::from_utf8(out.clone())?;
            assert!(script.contains("jotun"));
        }

        let seq = commands::build_osc52_sequence("hello", false);
        assert_eq!(seq, "\x1b]52;c;aGVsbG8=\x07");
        let tmux_seq = commands::build_osc52_sequence("hello", true);
        assert!(tmux_seq.starts_with("\x1bPtmux;\x1b\x1b]52;c;aGVsbG8="));
        Ok(())
    }
}
