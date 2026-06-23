use std::collections::HashMap;

use data_core::LexiconEntry;

#[derive(Clone, Debug, Default)]
pub(super) struct EnglishPrefixIndex {
    nodes: Vec<PrefixNode>,
}

#[derive(Clone, Debug, Default)]
struct PrefixNode {
    children: HashMap<char, usize>,
    entry_indices: Vec<usize>,
}

impl EnglishPrefixIndex {
    pub(super) fn build(
        entries: impl Iterator<Item = LexiconEntry>,
        prefix_limit: usize,
    ) -> Self {
        let mut nodes = vec![PrefixNode::default()];
        for (entry_index, entry) in entries.enumerate() {
            if !is_ascii_word(&entry.reading) {
                continue;
            }
            let mut node_index = 0usize;
            push_limited(
                &mut nodes[node_index].entry_indices,
                entry_index,
                prefix_limit,
            );
            for ch in entry.reading.chars() {
                let next = if let Some(existing) = nodes[node_index].children.get(&ch) {
                    *existing
                } else {
                    let created = nodes.len();
                    nodes.push(PrefixNode::default());
                    nodes[node_index].children.insert(ch, created);
                    created
                };
                node_index = next;
                push_limited(
                    &mut nodes[node_index].entry_indices,
                    entry_index,
                    prefix_limit,
                );
            }
        }
        Self { nodes }
    }

    pub(super) fn lookup(&self, prefix: &str, limit: usize) -> &[usize] {
        if limit == 0 || self.nodes.is_empty() {
            return &[];
        }
        let mut node_index = 0usize;
        for ch in prefix.chars() {
            let Some(next) = self.nodes[node_index].children.get(&ch) else {
                return &[];
            };
            node_index = *next;
        }
        let entries = &self.nodes[node_index].entry_indices;
        &entries[..entries.len().min(limit)]
    }

    pub(super) fn root(&self) -> usize {
        0
    }

    pub(super) fn advance_char(&self, node_index: usize, ch: char) -> Option<usize> {
        self.nodes.get(node_index)?.children.get(&ch).copied()
    }

    pub(super) fn has_children(&self, node_index: usize) -> bool {
        self.nodes
            .get(node_index)
            .map(|n| !n.children.is_empty())
            .unwrap_or(false)
    }
}

fn push_limited(values: &mut Vec<usize>, value: usize, limit: usize) {
    if values.len() < limit {
        values.push(value);
    }
}

fn is_ascii_word(value: &str) -> bool {
    value.chars().all(|ch| ch.is_ascii_lowercase())
}
