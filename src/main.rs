mod db;
mod tui;
mod config;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::*;
use db::Database;
use config::Config;
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
    /// List all notes
    Ls {
        /// Filter by tag (supports hierarchical search, e.g. "work" matches "work/p1")
        #[arg(short, long)]
        tag: Option<String>,
    },
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
    /// List all unique tags
    Tags,
    /// View note statistics
    Stats,
    /// Launch the interactive TUI dashboard
    Dash,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load().unwrap_or_default();
    let db_path = get_db_path(&config)?;
    let db = Database::new(db_path.clone())?;

    match cli.command {
        Commands::New { text, title, tag, source } => {
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
            let id = db.create_note(&content, title.as_deref(), &source, &tag)?;
            println!("{} Jotun: Saved note #{}", "✓".green(), id);
        }
        Commands::Ls { tag } => {
            let notes = if let Some(t) = tag {
                db.find_by_tag(&t)?
            } else {
                db.list_notes()?
            };

            if notes.is_empty() {
                println!("No notes found.");
                return Ok(());
            }

            println!("{}", "Note ID | Created | Title / Tags | Preview".dimmed());
            for note in notes {
                let display_title = match &note.title {
                    Some(t) => t.clone(),
                    None => note.body.lines().next().unwrap_or("").chars().take(20).collect::<String>(),
                };
                
                let tags_fmt = if note.tags.is_empty() {
                    "".to_string()
                } else {
                    format!(" [{}]", note.tags.join(", ")).yellow().to_string()
                };

                let preview = note.body.lines().next().unwrap_or("").chars().take(40).collect::<String>();
                let created_fmt = note.created.format("%m-%d %H:%M").to_string();
                
                println!(
                    "{} | {} | {}{} | {}",
                    note.id.to_string().bold().cyan(),
                    created_fmt.dimmed(),
                    display_title.bold(),
                    tags_fmt,
                    preview.dimmed()
                );
            }
        }
        Commands::Show { id } => {
            let note = db.get_note(id)?;
            println!(
                "{} #{} - {} ({})",
                "Jotun Note".bold().cyan(),
                note.id,
                note.title.as_deref().unwrap_or("Untitled").bold(),
                note.created.format("%Y-%m-%d %H:%M").to_string().dimmed()
            );
            println!("{} {} | {} {}", 
                "Source:".dimmed(), note.source.yellow(),
                "Updated:".dimmed(), note.updated.format("%Y-%m-%d %H:%M").to_string().dimmed()
            );
            if !note.tags.is_empty() {
                println!("{} {}", "Tags:".dimmed(), note.tags.join(", ").yellow());
            }
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
                let title = note.title.as_deref().unwrap_or("Untitled");
                println!(
                    "{} | {}",
                    note.id.to_string().bold().cyan(),
                    title.bold()
                );
            }
        }
        Commands::Edit { id } => {
            let note = db.get_note(id)?;
            let preferred_editor = config.editor.clone()
                .or_else(|| std::env::var("VISUAL").ok())
                .or_else(|| std::env::var("EDITOR").ok())
                .unwrap_or_else(|| "nano".to_string());
            
            let temp_file = tempfile::NamedTempFile::new()?;
            std::fs::write(temp_file.path(), &note.body)?;

            let status = Command::new(&preferred_editor)
                .arg(temp_file.path())
                .status();

            let status = match status {
                Ok(s) => s,
                Err(_) if preferred_editor != "nano" => {
                    println!("{} Preferred editor '{}' not found. Falling back to nano...", "!".yellow(), preferred_editor);
                    Command::new("nano")
                        .arg(temp_file.path())
                        .status()?
                }
                Err(e) => return Err(anyhow!("Failed to launch editor: {}", e)),
            };

            if status.success() {
                let updated_body = std::fs::read_to_string(temp_file.path())?;
                let updated_body = updated_body.trim();
                if updated_body != note.body {
                    db.update_note(id, updated_body, note.title.as_deref(), &note.tags)?;
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
            let id = db.create_note(&content, None, "clipboard", &[])?;
            println!("{} Jotun: Saved note #{} from clipboard.", "✓".green(), id);
        }
        Commands::Tags => {
            let tags = db.list_all_tags()?;
            if tags.is_empty() {
                println!("No tags found.");
                return Ok(());
            }
            println!("{}", "Jotun Tags:".bold().cyan());
            for tag in tags {
                println!("  • {}", tag.yellow());
            }
        }
        Commands::Stats => {
            let stats = db.get_statistics()?;
            let art = r#"
    .--.
   |o_o |
   |:_/ |
  //   \ \
 (|     | )
/'\_   _/`\
\___)=(___/
            "#;
            
            let lines: Vec<&str> = art.lines().filter(|s| !s.is_empty()).collect();
            let mut info = Vec::new();
            info.push(format!("{} {}", "jotun".bright_green().bold(), "v0.4.0".dimmed()));
            info.push("-".repeat(20).dimmed().to_string());
            info.push(format!("{} {}", "Notes:".bright_purple(), stats.total_notes));
            info.push(format!("{} {}", "Tags: ".bright_purple(), stats.tag_count));
            info.push(format!("{} {:.1} KB", "Size: ".bright_purple(), stats.total_chars as f64 / 1024.0));
            info.push(format!("{} {}", "Loc:  ".bright_purple(), db_path.to_string_lossy().dimmed()));
            
            println!("");
            for i in 0..lines.len().max(info.len()) {
                let art_part = if i < lines.len() { lines[i].bright_cyan().bold().to_string() } else { " ".repeat(12) };
                let info_part = if i < info.len() { &info[i] } else { "" };
                println!("  {:<15} {}", art_part, info_part);
            }

            if !stats.top_tags.is_empty() {
                println!("\n  {}", "── Top Tags ──".bright_cyan());
                for (tag, count) in &stats.top_tags {
                    let percent = (*count as f64 / stats.total_notes as f64 * 100.0).min(100.0);
                    let bar = "█".repeat((percent / 4.0) as usize);
                    println!("  {:<10} {} {}%", tag.yellow(), bar.bright_yellow(), percent as u32);
                }
            }
            println!("");
        }
        Commands::Dash => {
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

