use crate::gossip::GossipMembership;
use crate::hash_ring::ConsistentHashRing;
use mos_edge::EdgeRouter;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq)]
pub enum GlobalRouteDecision {
    ServeLocal,
    ForwardToNode {
        node_id: String,
        target_addr: SocketAddr,
    },
    ClusterEmpty,
}

pub struct GlobalIngressRouter {
    pub local_node_id: String,
    pub membership: GossipMembership,
    pub ring: Arc<RwLock<ConsistentHashRing>>,
    pub local_router: EdgeRouter,
}

impl GlobalIngressRouter {
    pub fn new(
        local_node_id: impl Into<String>,
        local_addr: SocketAddr,
        local_router: EdgeRouter,
    ) -> Self {
        let id = local_node_id.into();
        let membership = GossipMembership::new(id.clone(), local_addr);
        let mut ring = ConsistentHashRing::new(30);
        ring.add_node(&id);

        Self {
            local_node_id: id,
            membership,
            ring: Arc::new(RwLock::new(ring)),
            local_router,
        }
    }

    pub fn sync_ring_from_alive_nodes(&self) {
        let alive = self.membership.get_alive_nodes();
        if let Ok(mut ring) = self.ring.write() {
            *ring = ConsistentHashRing::new(30);
            for node in alive {
                ring.add_node(&node.node_id);
            }
        }
    }

    pub fn route_decision(&self, domain: &str) -> GlobalRouteDecision {
        let ring = match self.ring.read() {
            Ok(r) => r,
            Err(_) => return GlobalRouteDecision::ClusterEmpty,
        };

        let target_node_id = match ring.get_node(domain) {
            Some(id) => id,
            None => return GlobalRouteDecision::ClusterEmpty,
        };

        if target_node_id == self.local_node_id {
            GlobalRouteDecision::ServeLocal
        } else {
            let alive = self.membership.get_alive_nodes();
            if let Some(target_node) = alive.iter().find(|n| n.node_id == target_node_id) {
                GlobalRouteDecision::ForwardToNode {
                    node_id: target_node_id,
                    target_addr: target_node.addr,
                }
            } else {
                GlobalRouteDecision::ServeLocal
            }
        }
    }
}
