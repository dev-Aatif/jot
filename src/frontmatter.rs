use std::fmt::Write as _;

/// Serializes optional title and tags as a YAML frontmatter block above `body`.
pub fn serialize_with_frontmatter(title: Option<&str>, tags: &[String], body: &str) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    match title {
        Some(t) if !t.trim().is_empty() => {
            let escaped = t.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = writeln!(out, "title: \"{}\"", escaped);
        }
        _ => {
            out.push_str("title:\n");
        }
    }
    if tags.is_empty() {
        out.push_str("tags: []\n");
    } else {
        let joined = tags.join(", ");
        let _ = writeln!(out, "tags: [{}]", joined);
    }
    out.push_str("---\n");
    out.push_str(body);
    out
}

/// Parses optional YAML frontmatter (`title`, `tags`) from edited Markdown content.
/// If no frontmatter header is present, returns the fallback metadata unchanged.
pub fn parse_frontmatter(
    raw: &str,
    fallback_title: Option<&str>,
    fallback_tags: &[String],
) -> (Option<String>, Vec<String>, String) {
    let normalized = raw.replace("\r\n", "\n");
    let trimmed_end = normalized.trim_end_matches('\n');

    let Some(rest) = trimmed_end.strip_prefix("---\n") else {
        return (
            fallback_title.map(str::to_owned),
            fallback_tags.to_vec(),
            trimmed_end.to_string(),
        );
    };

    let (fm_block, body) = if let Some(idx) = rest.find("\n---\n") {
        (&rest[..idx], &rest[idx + 5..])
    } else if let Some(stripped) = rest.strip_suffix("\n---") {
        (stripped, "")
    } else {
        return (
            fallback_title.map(str::to_owned),
            fallback_tags.to_vec(),
            trimmed_end.to_string(),
        );
    };

    let mut title = fallback_title.map(str::to_owned);
    let mut tags = fallback_tags.to_vec();
    let mut reading_tag_list = false;
    let mut collected_list_tags: Vec<String> = Vec::new();

    for line in fm_block.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if reading_tag_list {
            if let Some(item) = trimmed.strip_prefix("- ") {
                let cleaned = clean_scalar(item).to_lowercase();
                if !cleaned.is_empty() && !collected_list_tags.contains(&cleaned) {
                    collected_list_tags.push(cleaned);
                }
                continue;
            } else {
                reading_tag_list = false;
                tags = std::mem::take(&mut collected_list_tags);
            }
        }

        if let Some(val) = trimmed.strip_prefix("title:") {
            let cleaned = clean_scalar(val);
            title = if cleaned.is_empty() {
                None
            } else {
                Some(cleaned)
            };
        } else if let Some(val) = trimmed.strip_prefix("tags:") {
            let val = val.trim();
            if val.is_empty() {
                reading_tag_list = true;
                collected_list_tags.clear();
            } else if val == "[]" {
                tags.clear();
            } else if val.starts_with('[') && val.ends_with(']') {
                let inner = &val[1..val.len() - 1];
                tags = parse_tag_csv(inner);
            } else {
                tags = parse_tag_csv(val);
            }
        }
    }

    if reading_tag_list {
        tags = collected_list_tags;
    }

    (title, tags, body.trim_end_matches('\n').to_string())
}

pub fn parse_tag_csv(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in input.split(',') {
        let cleaned = clean_scalar(part).to_lowercase();
        if !cleaned.is_empty() && !out.contains(&cleaned) {
            out.push(cleaned);
        }
    }
    out
}

fn clean_scalar(raw: &str) -> String {
    let s = raw.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .trim()
            .to_string()
    } else {
        s.to_string()
    }
}

pub fn slugify(title: Option<&str>, fallback_id: i64) -> String {
    let Some(t) = title.map(str::trim).filter(|s| !s.is_empty()) else {
        return format!("note-{}", fallback_id);
    };
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in t.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
        if slug.len() >= 48 {
            break;
        }
    }
    let trimmed = slug.trim_end_matches('-');
    if trimmed.is_empty() {
        format!("note-{}", fallback_id)
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frontmatter_round_trip() {
        let tags = vec!["work/jotun".to_string(), "critical".to_string()];
        let serialized = serialize_with_frontmatter(Some("Auth \"Fix\""), &tags, "## Notes\nBody");
        let (title, parsed_tags, body) = parse_frontmatter(&serialized, None, &[]);
        assert_eq!(title.as_deref(), Some("Auth \"Fix\""));
        assert_eq!(parsed_tags, tags);
        assert_eq!(body, "## Notes\nBody");
    }

    #[test]
    fn test_frontmatter_yaml_list_and_plain_fallback() {
        let raw = "---\ntitle: Standup\ntags:\n  - work/meetings\n  - weekly\n---\nAction items";
        let (title, tags, body) = parse_frontmatter(raw, None, &[]);
        assert_eq!(title.as_deref(), Some("Standup"));
        assert_eq!(tags, vec!["work/meetings", "weekly"]);
        assert_eq!(body, "Action items");

        let plain = "Just a plain body";
        let (t2, tags2, b2) = parse_frontmatter(plain, Some("Keep"), &["existing".to_string()]);
        assert_eq!(t2.as_deref(), Some("Keep"));
        assert_eq!(tags2, vec!["existing"]);
        assert_eq!(b2, "Just a plain body");
    }

    #[test]
    fn test_slugify_sanitizes_paths() {
        assert_eq!(slugify(Some("../../Auth Fix!"), 12), "auth-fix");
        assert_eq!(slugify(None, 7), "note-7");
        assert_eq!(slugify(Some("!!!"), 3), "note-3");
    }
}
