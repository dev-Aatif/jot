use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagRow {
    pub full_path: Option<String>,
    pub label: String,
    pub depth: usize,
    pub count: i64,
    pub has_children: bool,
    pub collapsed: bool,
    pub is_last_sibling: bool,
}

#[derive(Default)]
struct TrieNode {
    direct_count: i64,
    subtree_count: i64,
    children: BTreeMap<String, TrieNode>,
}

impl TrieNode {
    fn insert(&mut self, segments: &[&str], count: i64) {
        if let Some((first, rest)) = segments.split_first() {
            let child = self.children.entry((*first).to_string()).or_default();
            if rest.is_empty() {
                child.direct_count += count;
            } else {
                child.insert(rest, count);
            }
            self.recompute_subtree();
        }
    }

    fn recompute_subtree(&mut self) {
        let mut total = self.direct_count;
        for child in self.children.values_mut() {
            child.recompute_subtree();
            total += child.subtree_count;
        }
        self.subtree_count = total;
    }
}

pub fn build_tag_rows(
    tags_with_counts: &[(String, i64)],
    total_notes: i64,
    collapsed_paths: &HashSet<String>,
) -> Vec<TagRow> {
    let mut rows = vec![TagRow {
        full_path: None,
        label: "All".to_string(),
        depth: 0,
        count: total_notes,
        has_children: false,
        collapsed: false,
        is_last_sibling: false,
    }];

    let mut root = TrieNode::default();
    for (tag, count) in tags_with_counts {
        let segs: Vec<&str> = tag.split('/').filter(|s| !s.is_empty()).collect();
        if !segs.is_empty() {
            root.insert(&segs, *count);
        }
    }

    flatten_trie(&root, "", 0, collapsed_paths, &mut rows);
    rows
}

fn flatten_trie(
    node: &TrieNode,
    prefix: &str,
    depth: usize,
    collapsed_paths: &HashSet<String>,
    out: &mut Vec<TagRow>,
) {
    let total_children = node.children.len();
    for (idx, (name, child)) in node.children.iter().enumerate() {
        let full_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", prefix, name)
        };
        let has_children = !child.children.is_empty();
        let collapsed = has_children && collapsed_paths.contains(&full_path);
        let is_last_sibling = idx + 1 == total_children;

        out.push(TagRow {
            full_path: Some(full_path.clone()),
            label: name.clone(),
            depth,
            count: child.subtree_count,
            has_children,
            collapsed,
            is_last_sibling,
        });

        if has_children && !collapsed {
            flatten_trie(child, &full_path, depth + 1, collapsed_paths, out);
        }
    }
}

pub fn format_tag_row(row: &TagRow) -> String {
    if row.full_path.is_none() {
        return format!("◆ {} ({})", row.label, row.count);
    }

    let glyph = if row.has_children {
        if row.collapsed { "▸ " } else { "▾ " }
    } else if row.depth > 0 {
        if row.is_last_sibling {
            "└─ "
        } else {
            "├─ "
        }
    } else {
        "• "
    };

    let indent = if row.depth > 0 {
        "  ".repeat(row.depth.saturating_sub(1))
    } else {
        String::new()
    };

    format!("{}{}{} ({})", indent, glyph, row.label, row.count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_tree_hierarchy_and_collapse() {
        let input = vec![
            ("personal".to_string(), 3),
            ("work/jotun".to_string(), 6),
            ("work/meetings".to_string(), 4),
        ];
        let collapsed = HashSet::new();
        let rows = build_tag_rows(&input, 13, &collapsed);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].label, "All");
        assert_eq!(rows[0].count, 13);
        assert_eq!(rows[1].label, "personal");
        assert_eq!(rows[2].label, "work");
        assert_eq!(rows[2].count, 10);
        assert!(rows[2].has_children);
        assert_eq!(rows[3].label, "jotun");
        assert_eq!(rows[3].depth, 1);
        assert!(format_tag_row(&rows[3]).contains("├─ jotun (6)"));
        assert!(format_tag_row(&rows[4]).contains("└─ meetings (4)"));

        let mut collapsed = HashSet::new();
        collapsed.insert("work".to_string());
        let rows_collapsed = build_tag_rows(&input, 13, &collapsed);
        assert_eq!(rows_collapsed.len(), 3);
        assert!(rows_collapsed[2].collapsed);
        assert!(format_tag_row(&rows_collapsed[2]).contains("▸ work (10)"));
    }
}
