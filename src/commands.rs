use crate::config::{Config, ThemeConfig, VALID_PRESETS};
use crate::db::{Database, DbHealth, ListFilter, Note};
use crate::frontmatter::{parse_frontmatter, serialize_with_frontmatter, slugify};
use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use clap::{Command, ValueEnum};
use colored::*;
use serde::Serialize;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitStatus};

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum ExportFormat {
    Markdown,
    Json,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardMethod {
    Native(&'static str),
    Osc52,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub database_path: String,
    pub database: DbHealth,
    pub config_path: String,
    pub config_exists: bool,
    pub config_valid: bool,
    pub config_error: Option<String>,
    pub theme_preset: String,
    pub theme_preset_known: bool,
    pub editor_configured: String,
    pub editor_resolved: Option<String>,
    pub clipboard_tool: Option<String>,
    pub osc52_available: bool,
}

pub struct NewOptions<'a> {
    pub text: Option<String>,
    pub title: Option<String>,
    pub tags: &'a [String],
    pub source: &'a str,
    pub stdin_is_tty: bool,
}

pub fn handle_new(
    db: &Database,
    opts: NewOptions<'_>,
    stdin: &mut dyn Read,
    out: &mut dyn Write,
) -> Result<i64> {
    let content = match opts.text {
        Some(t) => t,
        None => {
            if opts.stdin_is_tty {
                return Err(anyhow!(
                    "No note text provided. Pass text as an argument or pipe via stdin."
                ));
            }
            let mut buffer = String::new();
            stdin.read_to_string(&mut buffer)?;
            buffer.trim_end_matches(['\r', '\n']).to_string()
        }
    };
    if content.trim().is_empty() {
        return Err(anyhow!("Note content cannot be empty."));
    }
    let id = db.create_note(&content, opts.title.as_deref(), opts.source, opts.tags)?;
    writeln!(out, "{} Jotun: Saved note #{}", "✓".green(), id)?;
    Ok(id)
}

pub fn handle_ls(
    db: &Database,
    tag: Option<&str>,
    source: Option<&str>,
    include_archived: bool,
    json: bool,
    out: &mut dyn Write,
) -> Result<()> {
    let notes = db.query_note_summaries(&ListFilter {
        tag,
        source,
        include_archived,
        query: None,
    })?;

    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&notes)?)?;
        return Ok(());
    }

    if notes.is_empty() {
        writeln!(out, "No notes found.")?;
        return Ok(());
    }

    writeln!(
        out,
        "{}",
        "Note ID | Created | Title / Tags | Preview".dimmed()
    )?;
    for note in notes {
        let display_title = match &note.title {
            Some(t) => t.clone(),
            None => note
                .preview
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(20)
                .collect::<String>(),
        };

        let pin_marker = if note.pinned { "★ " } else { "" };
        let arch_marker = if note.archived { " [archived]" } else { "" };
        let tags_fmt = if note.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", note.tags.join(", ")).yellow().to_string()
        };

        let preview = note
            .preview
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(40)
            .collect::<String>();
        let created_fmt = note.created.format("%m-%d %H:%M").to_string();

        writeln!(
            out,
            "{}{} | {} | {}{}{} | {}",
            pin_marker.yellow(),
            note.id.to_string().bold().cyan(),
            created_fmt.dimmed(),
            display_title.bold(),
            tags_fmt,
            arch_marker.dimmed(),
            preview.dimmed()
        )?;
    }
    Ok(())
}

pub fn handle_show(db: &Database, id: i64, json: bool, out: &mut dyn Write) -> Result<()> {
    let note = db.get_note(id)?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&note)?)?;
        return Ok(());
    }

    writeln!(
        out,
        "{} #{} - {} ({})",
        "Jotun Note".bold().cyan(),
        note.id,
        note.title.as_deref().unwrap_or("Untitled").bold(),
        note.created.format("%Y-%m-%d %H:%M").to_string().dimmed()
    )?;
    writeln!(
        out,
        "{} {} | {} {}{}{}",
        "Source:".dimmed(),
        note.source.yellow(),
        "Updated:".dimmed(),
        note.updated.format("%Y-%m-%d %H:%M").to_string().dimmed(),
        if note.pinned { " | ★ Pinned" } else { "" },
        if note.archived { " | Archived" } else { "" }
    )?;
    if !note.tags.is_empty() {
        writeln!(
            out,
            "{} {}",
            "Tags:".dimmed(),
            note.tags.join(", ").yellow()
        )?;
    }
    writeln!(out, "{}", "─".repeat(40).dimmed())?;
    writeln!(out, "{}", note.body)?;
    Ok(())
}

pub fn handle_find(
    db: &Database,
    query: &str,
    tag: Option<&str>,
    source: Option<&str>,
    include_archived: bool,
    json: bool,
    out: &mut dyn Write,
) -> Result<()> {
    let notes = db.query_note_summaries(&ListFilter {
        tag,
        source,
        include_archived,
        query: Some(query),
    })?;

    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&notes)?)?;
        return Ok(());
    }

    if notes.is_empty() {
        writeln!(out, "No results found for '{}'.", query)?;
        return Ok(());
    }

    writeln!(out, "{} Jotun: Found {} results:", "✓".green(), notes.len())?;
    for note in notes {
        let title = note.title.as_deref().unwrap_or("Untitled");
        let pin = if note.pinned { "★ " } else { "" };
        let tags_str = if note.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", note.tags.join(", ")).yellow().to_string()
        };
        writeln!(
            out,
            "{}{} | {}{}",
            pin.yellow(),
            note.id.to_string().bold().cyan(),
            title.bold(),
            tags_str
        )?;
        if let Some(snippet) = note.snippet.as_deref() {
            let clean = snippet.replace('\n', " ");
            writeln!(out, "   {}", clean.dimmed())?;
        }
    }
    Ok(())
}

pub struct EditOptions {
    pub id: i64,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub pin: Option<bool>,
    pub archive: Option<bool>,
    pub frontmatter: bool,
    pub open_editor: bool,
}

pub fn handle_edit(
    db: &Database,
    config: &Config,
    opts: EditOptions,
    out: &mut dyn Write,
) -> Result<()> {
    let id = opts.id;
    let note = db.get_note(id)?;
    let has_meta_flags = opts.title.is_some()
        || !opts.tags.is_empty()
        || opts.pin.is_some()
        || opts.archive.is_some();

    if let Some(p) = opts.pin {
        db.set_pinned(id, p)?;
    }
    if let Some(a) = opts.archive {
        db.set_archived(id, a)?;
    }

    if has_meta_flags && (opts.title.is_some() || !opts.tags.is_empty()) {
        let next_title = opts.title.as_deref().or(note.title.as_deref());
        let next_tags = if opts.tags.is_empty() {
            &note.tags
        } else {
            &opts.tags
        };
        db.update_note_metadata(id, next_title, next_tags)?;
    }

    if has_meta_flags && !opts.open_editor && !opts.frontmatter {
        writeln!(out, "{} Jotun: Updated note #{}", "✓".green(), id)?;
        return Ok(());
    }

    let current = db.get_note(id)?;
    let initial = if opts.frontmatter {
        serialize_with_frontmatter(current.title.as_deref(), &current.tags, &current.body)
    } else {
        current.body.clone()
    };

    let temp_file = create_markdown_tempfile(&initial)?;
    let status = spawn_editor(config, temp_file.path(), true)?;

    if status.success() {
        let raw_edited = read_edited_body(temp_file.path())?;
        let (new_title, new_tags, new_body) = if opts.frontmatter {
            parse_frontmatter(&raw_edited, current.title.as_deref(), &current.tags)
        } else {
            (current.title.clone(), current.tags.clone(), raw_edited)
        };

        if new_body != current.body || new_title != current.title || new_tags != current.tags {
            db.update_note(id, &new_body, new_title.as_deref(), &new_tags)?;
            writeln!(out, "{} Jotun: Updated note #{}", "✓".green(), id)?;
        } else if !has_meta_flags {
            writeln!(out, "No changes made.")?;
        } else {
            writeln!(out, "{} Jotun: Updated note #{}", "✓".green(), id)?;
        }
        Ok(())
    } else {
        Err(anyhow!("Editor exited with a non-zero status code."))
    }
}

pub fn handle_rm(
    db: &Database,
    id: i64,
    force: bool,
    stdin: &mut dyn Read,
    out: &mut dyn Write,
) -> Result<()> {
    if !force {
        writeln!(out, "Are you sure you want to delete note #{}? [y/N]", id)?;
        out.flush()?;
        let mut reader = io::BufReader::new(stdin);
        let mut response = String::new();
        io::BufRead::read_line(&mut reader, &mut response)?;
        if response.trim().to_lowercase() != "y" {
            writeln!(out, "Aborted.")?;
            return Ok(());
        }
    }
    db.delete_note(id)?;
    writeln!(out, "{} Jotun: Deleted note #{}", "✓".green(), id)?;
    Ok(())
}

pub fn handle_cp(db: &Database, id: i64, out: &mut dyn Write) -> Result<()> {
    let note = db.get_note(id)?;
    let method = copy_to_clipboard(&note.body)?;
    match method {
        ClipboardMethod::Native(_) => {
            writeln!(
                out,
                "{} Jotun: Copied note #{} to clipboard.",
                "✓".green(),
                id
            )?;
        }
        ClipboardMethod::Osc52 => {
            writeln!(
                out,
                "{} Jotun: Copied note #{} via OSC 52 terminal clipboard.",
                "✓".green(),
                id
            )?;
        }
    }
    Ok(())
}

pub fn handle_paste(db: &Database, out: &mut dyn Write) -> Result<i64> {
    let content = get_from_clipboard()?;
    if content.is_empty() {
        return Err(anyhow!("Clipboard is empty."));
    }
    let id = db.create_note(&content, None, "clipboard", &[])?;
    writeln!(
        out,
        "{} Jotun: Saved note #{} from clipboard.",
        "✓".green(),
        id
    )?;
    Ok(id)
}

pub fn handle_tags(
    db: &Database,
    include_archived: bool,
    json: bool,
    out: &mut dyn Write,
) -> Result<()> {
    let tags = db.list_tags_with_counts(include_archived)?;
    if json {
        #[derive(Serialize)]
        struct TagCount {
            name: String,
            count: i64,
        }
        let payload: Vec<TagCount> = tags
            .into_iter()
            .map(|(name, count)| TagCount { name, count })
            .collect();
        writeln!(out, "{}", serde_json::to_string_pretty(&payload)?)?;
        return Ok(());
    }

    if tags.is_empty() {
        writeln!(out, "No tags found.")?;
        return Ok(());
    }
    writeln!(out, "{}", "Jotun Tags:".bold().cyan())?;
    for (tag, count) in tags {
        writeln!(out, "  • {} ({})", tag.yellow(), count.to_string().dimmed())?;
    }
    Ok(())
}

pub fn handle_stats(db: &Database, db_path: &Path, json: bool, out: &mut dyn Write) -> Result<()> {
    let stats = db.get_statistics()?;
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&stats)?)?;
        return Ok(());
    }

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
    info.push(format!(
        "{} {}",
        "jotun".bright_green().bold(),
        concat!("v", env!("CARGO_PKG_VERSION")).dimmed()
    ));
    info.push("─".repeat(20).dimmed().to_string());
    info.push(format!(
        "{} {} ({} pinned, {} archived)",
        "Notes:".bright_purple(),
        stats.total_notes,
        stats.pinned_count,
        stats.archived_count
    ));
    info.push(format!("{} {}", "Tags: ".bright_purple(), stats.tag_count));
    info.push(format!(
        "{} {:.1} KB",
        "Size: ".bright_purple(),
        stats.total_chars as f64 / 1024.0
    ));
    info.push(format!(
        "{} {}",
        "Loc:  ".bright_purple(),
        db_path.to_string_lossy().dimmed()
    ));

    writeln!(out)?;
    for i in 0..lines.len().max(info.len()) {
        let art_part = if i < lines.len() {
            lines[i].bright_cyan().bold().to_string()
        } else {
            " ".repeat(12)
        };
        let info_part = if i < info.len() { &info[i] } else { "" };
        writeln!(out, "  {:<15} {}", art_part, info_part)?;
    }

    if !stats.top_tags.is_empty() && stats.total_notes > 0 {
        writeln!(out, "\n  {}", "── Top Tags ──".bright_cyan())?;
        for (tag, count) in &stats.top_tags {
            let percent = (*count as f64 / stats.total_notes as f64 * 100.0).min(100.0);
            let bar = "█".repeat((percent / 4.0) as usize);
            writeln!(
                out,
                "  {:<10} {} {}%",
                tag.yellow(),
                bar.bright_yellow(),
                percent as u32
            )?;
        }
    }
    writeln!(out)?;
    Ok(())
}

pub fn handle_export(
    db: &Database,
    format: ExportFormat,
    output: &Path,
    tag: Option<&str>,
    include_archived: bool,
    out: &mut dyn Write,
) -> Result<()> {
    let notes = db.list_all_notes_for_export(&ListFilter {
        tag,
        source: None,
        include_archived,
        query: None,
    })?;

    match format {
        ExportFormat::Json => {
            let json = serde_json::to_string_pretty(&notes)?;
            if output == Path::new("-") {
                writeln!(out, "{}", json)?;
            } else {
                if let Some(parent) = output.parent()
                    && !parent.as_os_str().is_empty()
                {
                    fs::create_dir_all(parent)?;
                }
                fs::write(output, json)?;
                writeln!(
                    out,
                    "{} Exported {} notes as JSON to {}",
                    "✓".green(),
                    notes.len(),
                    output.display()
                )?;
            }
        }
        ExportFormat::Markdown => {
            fs::create_dir_all(output)?;
            for note in &notes {
                let md = render_export_markdown(note);
                let filename =
                    format!("{}-{}.md", note.id, slugify(note.title.as_deref(), note.id));
                fs::write(output.join(filename), md)?;
            }
            writeln!(
                out,
                "{} Exported {} Markdown notes to {}",
                "✓".green(),
                notes.len(),
                output.display()
            )?;
        }
    }

    Ok(())
}

fn render_export_markdown(note: &Note) -> String {
    let mut header = String::new();
    header.push_str("---\n");
    header.push_str(&format!("id: {}\n", note.id));
    match &note.title {
        Some(t) if !t.trim().is_empty() => {
            let escaped = t.replace('\\', "\\\\").replace('"', "\\\"");
            header.push_str(&format!("title: \"{}\"\n", escaped));
        }
        _ => header.push_str("title:\n"),
    }
    if note.tags.is_empty() {
        header.push_str("tags: []\n");
    } else {
        header.push_str(&format!("tags: [{}]\n", note.tags.join(", ")));
    }
    header.push_str(&format!("source: {}\n", note.source));
    header.push_str(&format!("created: {}\n", note.created.to_rfc3339()));
    header.push_str(&format!("updated: {}\n", note.updated.to_rfc3339()));
    header.push_str(&format!("pinned: {}\n", note.pinned));
    header.push_str(&format!("archived: {}\n", note.archived));
    header.push_str("---\n");
    if note.is_encrypted {
        header.push_str("[Encrypted note body omitted]\n");
    } else {
        header.push_str(&note.body);
        header.push('\n');
    }
    header
}

pub fn handle_doctor(db: &Database, db_path: &Path, json: bool, out: &mut dyn Write) -> Result<()> {
    let db_health = db.check_health()?;
    let config_path = Config::get_path()?;
    let config_exists = config_path.exists();
    let (config_valid, config_error, loaded_cfg) = if config_exists {
        match fs::read_to_string(&config_path)
            .map_err(anyhow::Error::from)
            .and_then(|s| toml::from_str::<Config>(&s).map_err(anyhow::Error::from))
        {
            Ok(c) => (true, None, c),
            Err(e) => (false, Some(e.to_string()), Config::default()),
        }
    } else {
        (true, None, Config::default())
    };

    let preset_name = loaded_cfg
        .theme
        .preset
        .as_deref()
        .unwrap_or("dracula")
        .to_string();
    let theme_preset_known = ThemeConfig::is_preset_known(&preset_name);

    let editor_configured = resolve_editor_command(&loaded_cfg);
    let editor_bin = editor_configured
        .split_whitespace()
        .next()
        .unwrap_or("nano");
    let editor_resolved = find_in_path(editor_bin).map(|p| p.to_string_lossy().to_string());

    let clipboard_tool = clipboard_candidates(true)
        .into_iter()
        .find_map(|(bin, _)| find_in_path(bin).map(|_| bin.to_string()));
    let osc52_available = io::stdout().is_terminal()
        || std::env::var_os("TMUX").is_some()
        || std::env::var_os("SSH_TTY").is_some();

    let report = DoctorReport {
        database_path: db_path.to_string_lossy().to_string(),
        database: db_health,
        config_path: config_path.to_string_lossy().to_string(),
        config_exists,
        config_valid,
        config_error,
        theme_preset: preset_name,
        theme_preset_known,
        editor_configured,
        editor_resolved,
        clipboard_tool,
        osc52_available,
    };

    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(&report)?)?;
        return Ok(());
    }

    writeln!(out, "{}", "Jotun Doctor".bold().cyan())?;
    writeln!(out, "{}", "─".repeat(40).dimmed())?;
    let ok = "✓".green();
    let warn = "!".yellow();
    let fail = "✗".red();

    writeln!(
        out,
        "{} SQLite integrity: {} (journal={}, fk={})",
        if report.database.integrity_ok {
            &ok
        } else {
            &fail
        },
        report.database.integrity_message,
        report.database.journal_mode,
        report.database.foreign_keys
    )?;
    writeln!(
        out,
        "{} FTS5 sync: {} plaintext notes / {} indexed",
        if report.database.fts_in_sync {
            &ok
        } else {
            &fail
        },
        report.database.plaintext_notes_count,
        report.database.fts_rows_count
    )?;
    writeln!(
        out,
        "{} Config ({}): {}",
        if report.config_valid { &ok } else { &fail },
        report.config_path,
        report
            .config_error
            .as_deref()
            .unwrap_or(if report.config_exists {
                "valid"
            } else {
                "using defaults (not created yet)"
            })
    )?;
    let preset_hint = if report.theme_preset_known {
        "built-in".to_string()
    } else {
        format!(
            "unknown; falls back to dracula (valid: {})",
            VALID_PRESETS.join(", ")
        )
    };
    writeln!(
        out,
        "{} Theme preset: {} ({})",
        if report.theme_preset_known {
            &ok
        } else {
            &warn
        },
        report.theme_preset,
        preset_hint
    )?;
    writeln!(
        out,
        "{} Editor: {} -> {}",
        if report.editor_resolved.is_some() {
            &ok
        } else {
            &warn
        },
        report.editor_configured,
        report
            .editor_resolved
            .as_deref()
            .unwrap_or("not found in PATH (will fall back to nano)")
    )?;
    writeln!(
        out,
        "{} Clipboard: native={} | OSC 52={}",
        if report.clipboard_tool.is_some() || report.osc52_available {
            &ok
        } else {
            &warn
        },
        report.clipboard_tool.as_deref().unwrap_or("none"),
        if report.osc52_available {
            "ready"
        } else {
            "non-TTY"
        }
    )?;

    Ok(())
}

pub fn handle_completions(
    cmd: &Command,
    shell: CompletionShell,
    out: &mut dyn Write,
) -> Result<()> {
    let bin = cmd.get_name();
    let subcommands: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();

    match shell {
        CompletionShell::Bash => {
            let subs = subcommands.join(" ");
            writeln!(
                out,
                "_{bin}_completions() {{\n    local cur prev\n    cur=\"${{COMP_WORDS[COMP_CWORD]}}\"\n    prev=\"${{COMP_WORDS[COMP_CWORD-1]}}\"\n    if [[ $COMP_CWORD -eq 1 ]]; then\n        COMPREPLY=( $(compgen -W \"{subs} --help --version\" -- \"$cur\") )\n        return 0\n    fi\n}}\ncomplete -F _{bin}_completions {bin}"
            )?;
        }
        CompletionShell::Zsh => {
            writeln!(
                out,
                "#compdef {bin}\n_{bin}() {{\n    local -a commands\n    commands=("
            )?;
            for sub in cmd.get_subcommands() {
                let about = sub
                    .get_about()
                    .map(|a| a.to_string().replace('\'', "'\\''"))
                    .unwrap_or_default();
                writeln!(out, "        '{}:{}'", sub.get_name(), about)?;
            }
            writeln!(
                out,
                "    )\n    _describe 'command' commands\n}}\n_{bin} \"$@\""
            )?;
        }
        CompletionShell::Fish => {
            for sub in cmd.get_subcommands() {
                let about = sub
                    .get_about()
                    .map(|a| a.to_string().replace('\'', "\\'"))
                    .unwrap_or_default();
                writeln!(
                    out,
                    "complete -c {bin} -f -n '__fish_use_subcommand' -a '{}' -d '{}'",
                    sub.get_name(),
                    about
                )?;
            }
        }
    }
    Ok(())
}

pub fn create_markdown_tempfile(initial_content: &str) -> Result<tempfile::NamedTempFile> {
    let temp_file = tempfile::Builder::new().suffix(".md").tempfile()?;
    if !initial_content.is_empty() {
        std::fs::write(temp_file.path(), initial_content)?;
    }
    Ok(temp_file)
}

pub fn read_edited_body(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    Ok(content.trim_end_matches(['\r', '\n']).to_string())
}

pub fn resolve_editor_command(config: &Config) -> String {
    config
        .editor
        .clone()
        .or_else(|| std::env::var("VISUAL").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "nano".to_string())
}

pub fn find_in_path(bin: &str) -> Option<PathBuf> {
    let path = Path::new(bin);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn spawn_editor(config: &Config, path: &Path, warn_on_fallback: bool) -> Result<ExitStatus> {
    let preferred_editor = resolve_editor_command(config);
    let mut parts = preferred_editor.split_whitespace();
    let bin = parts.next().unwrap_or("nano");
    let args: Vec<&str> = parts.collect();

    let status = ProcessCommand::new(bin).args(&args).arg(path).status();

    match status {
        Ok(s) => Ok(s),
        Err(_) if bin != "nano" => {
            if warn_on_fallback {
                println!(
                    "{} Preferred editor '{}' not found. Falling back to nano...",
                    "!".yellow(),
                    preferred_editor
                );
            }
            ProcessCommand::new("nano")
                .arg(path)
                .status()
                .map_err(|e| anyhow!("Failed to launch editor: {}", e))
        }
        Err(e) => Err(anyhow!("Failed to launch editor: {}", e)),
    }
}

pub fn clipboard_candidates(copy: bool) -> Vec<(&'static str, Vec<&'static str>)> {
    if cfg!(target_os = "macos") {
        return if copy {
            vec![("pbcopy", vec![])]
        } else {
            vec![("pbpaste", vec![])]
        };
    }

    let mut cmds = Vec::new();
    if std::env::var("WAYLAND_DISPLAY").is_ok_and(|v| !v.is_empty()) {
        if copy {
            cmds.push(("wl-copy", vec![]));
        } else {
            cmds.push(("wl-paste", vec!["-n"]));
        }
    }
    if copy {
        cmds.push(("xclip", vec!["-selection", "clipboard"]));
        cmds.push(("xsel", vec!["--clipboard", "--input"]));
    } else {
        cmds.push(("xclip", vec!["-selection", "clipboard", "-o"]));
        cmds.push(("xsel", vec!["--clipboard", "--output"]));
    }
    cmds
}

pub fn build_osc52_sequence(text: &str, in_tmux: bool) -> String {
    let encoded = BASE64_STANDARD.encode(text.as_bytes());
    if in_tmux {
        format!("\x1bPtmux;\x1b\x1b]52;c;{}\x07\x1b\\", encoded)
    } else {
        format!("\x1b]52;c;{}\x07", encoded)
    }
}

pub fn copy_to_clipboard(text: &str) -> Result<ClipboardMethod> {
    let is_ssh_headless = !cfg!(target_os = "macos")
        && (std::env::var("SSH_TTY").is_ok_and(|v| !v.is_empty())
            || std::env::var("SSH_CONNECTION").is_ok_and(|v| !v.is_empty()))
        && std::env::var("WAYLAND_DISPLAY")
            .map(|v| v.is_empty())
            .unwrap_or(true)
        && std::env::var("DISPLAY")
            .map(|v| v.is_empty())
            .unwrap_or(true);

    if is_ssh_headless && io::stdout().is_terminal() {
        let in_tmux = std::env::var("TMUX").is_ok_and(|v| !v.is_empty());
        let seq = build_osc52_sequence(text, in_tmux);
        let mut stdout = io::stdout().lock();
        stdout.write_all(seq.as_bytes())?;
        stdout.flush()?;
        return Ok(ClipboardMethod::Osc52);
    }

    let candidates = clipboard_candidates(true);
    let mut last_err = None;

    for (cmd, args) in candidates {
        match ProcessCommand::new(cmd)
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                let mut stdin = child
                    .stdin
                    .take()
                    .ok_or_else(|| anyhow!("Failed to open stdin for clipboard command"))?;
                let write_res = stdin.write_all(text.as_bytes());
                drop(stdin);
                let status = child.wait()?;
                write_res?;
                if !status.success() {
                    return Err(anyhow!("Clipboard command '{}' failed.", cmd));
                }
                return Ok(ClipboardMethod::Native(cmd));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                last_err = Some(e);
            }
            Err(e) => {
                return Err(anyhow!(
                    "Failed to spawn clipboard command '{}': {}.",
                    cmd,
                    e
                ));
            }
        }
    }

    if io::stdout().is_terminal() {
        let in_tmux = std::env::var("TMUX").is_ok_and(|v| !v.is_empty());
        let seq = build_osc52_sequence(text, in_tmux);
        let mut stdout = io::stdout().lock();
        stdout.write_all(seq.as_bytes())?;
        stdout.flush()?;
        return Ok(ClipboardMethod::Osc52);
    }

    Err(anyhow!(
        "No supported clipboard tool found ({}).",
        last_err.map(|e| e.to_string()).unwrap_or_default()
    ))
}

pub fn get_from_clipboard() -> Result<String> {
    let candidates = clipboard_candidates(false);
    let mut last_err = None;

    for (cmd, args) in candidates {
        match ProcessCommand::new(cmd).args(&args).output() {
            Ok(output) => {
                if !output.status.success() {
                    return Err(anyhow!("Clipboard command '{}' failed.", cmd));
                }
                return Ok(String::from_utf8_lossy(&output.stdout).to_string());
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                last_err = Some(e);
            }
            Err(e) => {
                return Err(anyhow!(
                    "Failed to exec clipboard command '{}': {}.",
                    cmd,
                    e
                ));
            }
        }
    }

    Err(anyhow!(
        "No supported clipboard tool found ({}).",
        last_err.map(|e| e.to_string()).unwrap_or_default()
    ))
}
