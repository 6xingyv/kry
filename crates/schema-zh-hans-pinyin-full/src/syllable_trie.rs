use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default)]
pub(super) struct SyllableTrieNode {
    pub(super) children: HashMap<char, usize>,
    pub(super) syllable: Option<String>,
}

pub(super) fn build_syllable_trie(syllables: &HashSet<String>) -> Vec<SyllableTrieNode> {
    let mut nodes = vec![SyllableTrieNode::default()];
    let mut ordered = syllables.iter().collect::<Vec<_>>();
    ordered.sort();
    for syllable in ordered {
        let mut node_index = 0usize;
        for ch in syllable.chars() {
            let next = if let Some(existing) = nodes[node_index].children.get(&ch) {
                *existing
            } else {
                let created = nodes.len();
                nodes.push(SyllableTrieNode::default());
                nodes[node_index].children.insert(ch, created);
                created
            };
            node_index = next;
        }
        nodes[node_index].syllable = Some(syllable.clone());
    }
    nodes
}
