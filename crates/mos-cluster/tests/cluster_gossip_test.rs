use m_os_cluster::{
    ClusterNode, ConsistentHashRing, GlobalIngressRouter, GlobalRouteDecision, GossipMembership,
    NodeStatus,
};
use mos_edge::EdgeRouter;
use std::net::SocketAddr;

#[test]
fn test_consistent_hash_ring_distribution() {
    let mut ring = ConsistentHashRing::new(30);
    ring.add_node("node-kr-seoul-1");
    ring.add_node("node-us-east-1");
    ring.add_node("node-eu-frankfurt-1");

    assert_eq!(ring.len(), 90);

    let target1 = ring.get_node("my-nextjs-app.mos.dev").unwrap();
    let target2 = ring.get_node("fastapi-backend.mos.dev").unwrap();
    let target3 = ring.get_node("rust-service.mos.dev").unwrap();

    // Verify deterministic mapping
    assert_eq!(target1, ring.get_node("my-nextjs-app.mos.dev").unwrap());
    assert!(!target1.is_empty());
    assert!(!target2.is_empty());
    assert!(!target3.is_empty());

    // Remove one node
    ring.remove_node("node-eu-frankfurt-1");
    assert_eq!(ring.len(), 60);
}

#[test]
fn test_gossip_membership_lifecycle() {
    let local_addr: SocketAddr = "10.0.0.1:8080".parse().unwrap();
    let membership = GossipMembership::new("node-1", local_addr);

    let node2 = ClusterNode {
        node_id: "node-2".to_string(),
        addr: "10.0.0.2:8080".parse().unwrap(),
        status: NodeStatus::Alive,
        last_seen_epoch: 1787180000,
        capacity_vms: 500,
        active_vms: 10,
    };
    membership.register_or_update(node2);

    let alive = membership.get_alive_nodes();
    assert_eq!(alive.len(), 2);

    membership.mark_suspect("node-2");
    membership.mark_dead("node-2");

    let remaining = membership.get_alive_nodes();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].node_id, "node-1");
}

#[test]
fn test_global_ingress_routing_decision() {
    let local_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    let edge_router = EdgeRouter::new();
    let global_router = GlobalIngressRouter::new("node-1", local_addr, edge_router);

    // Register node-2 in cluster
    let node2 = ClusterNode {
        node_id: "node-2".to_string(),
        addr: "192.168.1.50:8080".parse().unwrap(),
        status: NodeStatus::Alive,
        last_seen_epoch: 1787180000,
        capacity_vms: 500,
        active_vms: 5,
    };
    global_router.membership.register_or_update(node2);
    global_router.sync_ring_from_alive_nodes();

    let decision = global_router.route_decision("demo-app.mos.local");
    match decision {
        GlobalRouteDecision::ServeLocal => {
            // Target is local node
            assert_eq!(global_router.local_node_id, "node-1");
        }
        GlobalRouteDecision::ForwardToNode {
            node_id,
            target_addr,
        } => {
            // Target is remote node
            assert_eq!(node_id, "node-2");
            assert_eq!(
                target_addr,
                "192.168.1.50:8080".parse::<SocketAddr>().unwrap()
            );
        }
        GlobalRouteDecision::ClusterEmpty => panic!("Cluster should not be empty"),
    }
}
