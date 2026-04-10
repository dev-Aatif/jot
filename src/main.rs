mod db;
mod tui;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::*;
use db::Database;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "jot")]
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
        /// Optional source tag for the note.
        #[arg(short, long, default_value = "manual")]
        source: String,
    },
    /// List all notes
    Ls,
    /// View a specific note
    Show {
        /// The ID of the note to show.
        id: i64,
    },
    /// Find notes by searching their content
    Find {
        /// The search query.
        query: String,
    },
    /// Edit a specific note in your default editor
    Edit {
        /// The ID of the note to edit.
        id: i64,
    },
    /// Delete a specific note
    Rm {
        /// The ID of the note to delete.
        id: i64,
        /// Proceed without confirmation.
        #[arg(short, long)]
        force: bool,
    },
    /// Copy a note to the system clipboard
    Cp {
        /// The ID of the note to copy.
        id: i64,
    },
    /// Create a new note from the current clipboard content
    Paste,
    /// Launch the interactive TUI dashboard
    Dash,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = get_db_path()?;
    let db = Database::new(db_path)?;

    match cli.command {
        Commands::New { text, source } => {
            let content = match text {
                Some(t) => t,
                None => {
                    let mut buffer = String::new();
                    io::stdin().read_to_string(&mut buffer)?;
                    buffer.trim().to_string()
                }
            };
            if content.is_empty() {
                return Err(anyhow!("Note content cannot be empty."));
            }
            let id = db.create_note(&content, &source)?;
            println!("{} Jotun: Saved note #{}", "✓".green(), id);
        }
        Commands::Ls => {
            let notes = db.list_notes()?;
            if notes.is_empty() {
                println!("No notes found. Create one with `jotun new \"text\"`.");
                return Ok(());
            }
            println!("{}", "Note ID | Created | Preview".dimmed());
            for note in notes {
                let preview = note.body.lines().next().unwrap_or("").chars().take(50).collect::<String>();
                let created_fmt = note.created.format("%Y-%m-%d %H:%M").to_string();
                println!(
                    "{} | {} | {}",
                    note.id.to_string().bold().cyan(),
                    created_fmt.dimmed(),
                    preview
                );
            }
        }
        Commands::Show { id } => {
            let note = db.get_note(id)?;
            println!(
                "{} #{} ({})",
                "Jotun Note".bold().cyan(),
                note.id,
                note.created.format("%Y-%m-%d %H:%M").to_string().dimmed()
            );
            println!("{}", "-".repeat(40).dimmed());
            println!("{}", note.body);
        }
        Commands::Find { query } => {
            let notes = db.search_notes(&query)?;
            if notes.is_empty() {
                println!("No results found for '{}'.", query);
                return Ok(());
            }
            println!("{} Jotun: Found {} results:", "✓".green(), notes.len());
            for note in notes {
                let preview = note.body.lines().next().unwrap_or("").chars().take(50).collect::<String>();
                println!(
                    "{} | {}",
                    note.id.to_string().bold().cyan(),
                    preview
                );
            }
        }
        Commands::Edit { id } => {
            let note = db.get_note(id)?;
            let editor = std::env::var("VISUAL")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| "nano".to_string());
            
            let temp_file = tempfile::NamedTempFile::new()?;
            std::fs::write(temp_file.path(), &note.body)?;

            let status = Command::new(editor)
                .arg(temp_file.path())
                .status()?;

            if status.success() {
                let updated_body = std::fs::read_to_string(temp_file.path())?;
                let updated_body = updated_body.trim();
                if updated_body != note.body {
                    db.update_note(id, updated_body)?;
                    println!("{} Jotun: Updated note #{}", "✓".green(), id);
                } else {
                    println!("No changes made.");
                }
            } else {
                return Err(anyhow!("Editor exited with a non-zero status code."));
            }
        }
        Commands::Rm { id, force } => {
            if !force {
                println!("Are you sure you want to delete note #{}? [y/N]", id);
                let mut response = String::new();
                io::stdin().read_line(&mut response)?;
                if response.trim().to_lowercase() != "y" {
                    println!("Aborted.");
                    return Ok(());
                }
            }
            db.delete_note(id)?;
            println!("{} Jotun: Deleted note #{}", "✓".green(), id);
        }
        Commands::Cp { id } => {
            let note = db.get_note(id)?;
            copy_to_clipboard(&note.body)?;
            println!("{} Jotun: Copied note #{} to clipboard.", "✓".green(), id);
        }
        Commands::Paste => {
            let content = get_from_clipboard()?;
            if content.is_empty() {
                return Err(anyhow!("Clipboard is empty."));
            }
            let id = db.create_note(&content, "clipboard")?;
            println!("{} Jotun: Saved note #{} from clipboard.", "✓".green(), id);
        }
        Commands::Dash => {
            tui::run_tui(&db)?;
        }
    }

    Ok(())
}

fn get_db_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("JOTUN_DB_PATH") {
        return Ok(PathBuf::from(p));
    }
    
    let mut path = dirs::data_dir()
        .ok_or_else(|| anyhow!("Could not find user data directory"))?;
    path.push("jotun");
    path.push("jotun.db");
    Ok(path)
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    let (cmd, args) = if std::env::var("WAYLAND_DISPLAY").is_ok() {
        ("wl-copy", vec![])
    } else {
        ("xclip", vec!["-selection", "clipboard"])
    };

    let mut child = Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn clipboard command '{}': {}. Make sure it is installed.", cmd, e))?;

    let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("Failed to open stdin for clipboard command"))?;
    io::Write::write_all(&mut stdin, text.as_bytes())?;
    drop(stdin); // Close stdin to signal EOF

    let status = child.wait()?;
    if !status.success() {
        return Err(anyhow!("Clipboard command '{}' failed.", cmd));
    }
    Ok(())
}

fn get_from_clipboard() -> Result<String> {
    let (cmd, args) = if std::env::var("WAYLAND_DISPLAY").is_ok() {
        ("wl-paste", vec!["-n"])
    } else {
        ("xclip", vec!["-selection", "clipboard", "-o"])
    };

    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| anyhow!("Failed to exec clipboard command '{}': {}. Make sure it is installed.", cmd, e))?;

    if !output.status.success() {
        return Err(anyhow!("Clipboard command '{}' failed.", cmd));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
