use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};

pub struct ConsistentHashRing {
    vnodes_per_node: usize,
    ring: BTreeMap<u64, String>, // Hash -> NodeID
}

impl ConsistentHashRing {
    pub fn new(vnodes_per_node: usize) -> Self {
        Self {
            vnodes_per_node,
            ring: BTreeMap::new(),
        }
    }

    pub fn add_node(&mut self, node_id: &str) {
        for v in 0..self.vnodes_per_node {
            let key = format!("{}-vnode-{}", node_id, v);
            let hash = hash_str(&key);
            self.ring.insert(hash, node_id.to_string());
        }
    }

    pub fn remove_node(&mut self, node_id: &str) {
        for v in 0..self.vnodes_per_node {
            let key = format!("{}-vnode-{}", node_id, v);
            let hash = hash_str(&key);
            self.ring.remove(&hash);
        }
    }

    pub fn get_node(&self, resource_key: &str) -> Option<String> {
        if self.ring.is_empty() {
            return None;
        }

        let hash = hash_str(resource_key);
        // Find the first node with hash >= resource_key hash, or wrap around
        match self.ring.range(hash..).next() {
            Some((_, node_id)) => Some(node_id.clone()),
            None => self.ring.values().next().cloned(),
        }
    }

    pub fn len(&self) -> usize {
        self.ring.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}

fn hash_str(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}
