use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Alive,
    Suspect,
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClusterNode {
    pub node_id: String,
    pub addr: SocketAddr,
    pub status: NodeStatus,
    pub last_seen_epoch: u64,
    pub capacity_vms: u32,
    pub active_vms: u32,
}

#[derive(Clone)]
pub struct GossipMembership {
    pub local_node_id: String,
    nodes: Arc<DashMap<String, ClusterNode>>,
}

impl GossipMembership {
    pub fn new(local_node_id: impl Into<String>, local_addr: SocketAddr) -> Self {
        let id = local_node_id.into();
        let nodes = Arc::new(DashMap::new());
        let local_node = ClusterNode {
            node_id: id.clone(),
            addr: local_addr,
            status: NodeStatus::Alive,
            last_seen_epoch: current_epoch(),
            capacity_vms: 500,
            active_vms: 0,
        };
        nodes.insert(id.clone(), local_node);

        Self {
            local_node_id: id,
            nodes,
        }
    }

    pub fn register_or_update(&self, node: ClusterNode) {
        self.nodes.insert(node.node_id.clone(), node);
    }

    pub fn get_alive_nodes(&self) -> Vec<ClusterNode> {
        self.nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Alive)
            .map(|n| n.value().clone())
            .collect()
    }

    pub fn mark_suspect(&self, node_id: &str) {
        if let Some(mut node) = self.nodes.get_mut(node_id) {
            node.status = NodeStatus::Suspect;
            warn!(
                node_id = node_id,
                "Node marked as SUSPECT (heartbeat missed)"
            );
        }
    }

    pub fn mark_dead(&self, node_id: &str) {
        if let Some(mut node) = self.nodes.get_mut(node_id) {
            node.status = NodeStatus::Dead;
            info!(node_id = node_id, "Node marked as DEAD");
        }
    }
}

fn current_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
