use m_os_edge::{
    CanaryPipelineConfig, CanaryPipelineManager, EdgeRouter, PipelineEvaluation, RouteTarget,
};
use mos_core::InstanceId;

#[test]
fn test_canary_pipeline_step_promotion_and_full_rollout() {
    let router = EdgeRouter::new();
    let stable_target = RouteTarget {
        instance_id: InstanceId::new(),
        host: "172.16.0.2".to_string(),
        port: 8080,
        is_suspended: false,
    };
    router.register("app.mos.local", stable_target);

    let config = CanaryPipelineConfig {
        step_weights: vec![10, 50, 100],
        min_requests_per_step: 10,
        max_error_rate_percent: 5.0,
    };
    let manager = CanaryPipelineManager::new(router.clone(), config);

    let canary_target = RouteTarget {
        instance_id: InstanceId::new(),
        host: "172.16.0.3".to_string(),
        port: 8080,
        is_suspended: false,
    };

    // 1. Start Canary at 10%
    manager.start_canary_deployment("app.mos.local", canary_target, "v2-next");
    let routes = router.inspect_routes("app.mos.local").unwrap();
    assert_eq!(routes.canary.as_ref().unwrap().weight, 10);
    assert_eq!(routes.stable.weight, 90);

    // 2. Feed 10 successful requests -> Advance to 50%
    for _ in 0..10 {
        manager.record_result("app.mos.local", false);
    }
    let eval_1 = manager.evaluate_and_advance("app.mos.local");
    assert_eq!(
        eval_1,
        PipelineEvaluation::Promoted {
            new_step: 1,
            new_weight: 50
        }
    );

    let routes = router.inspect_routes("app.mos.local").unwrap();
    assert_eq!(routes.canary.as_ref().unwrap().weight, 50);
    assert_eq!(routes.stable.weight, 50);

    // 3. Feed 10 successful requests -> Advance to 100% (Full rollout)
    for _ in 0..10 {
        manager.record_result("app.mos.local", false);
    }
    let eval_2 = manager.evaluate_and_advance("app.mos.local");
    assert_eq!(
        eval_2,
        PipelineEvaluation::FullyPromoted {
            version_tag: "v2-next".to_string()
        }
    );

    let routes = router.inspect_routes("app.mos.local").unwrap();
    assert_eq!(routes.stable.version_tag, "v2-next");
    assert!(routes.canary.is_none());
}

#[test]
fn test_canary_pipeline_automatic_rollback_on_high_error_rate() {
    let router = EdgeRouter::new();
    let stable_target = RouteTarget {
        instance_id: InstanceId::new(),
        host: "172.16.0.2".to_string(),
        port: 8080,
        is_suspended: false,
    };
    router.register("faulty.mos.local", stable_target);

    let config = CanaryPipelineConfig {
        step_weights: vec![10, 50, 100],
        min_requests_per_step: 20,
        max_error_rate_percent: 5.0, // 5%
    };
    let manager = CanaryPipelineManager::new(router.clone(), config);

    let faulty_canary = RouteTarget {
        instance_id: InstanceId::new(),
        host: "172.16.0.9".to_string(),
        port: 8080,
        is_suspended: false,
    };

    manager.start_canary_deployment("faulty.mos.local", faulty_canary, "v2-buggy");

    // Feed 5 requests with 2 errors (40% error rate > 5.0% threshold)
    manager.record_result("faulty.mos.local", false);
    manager.record_result("faulty.mos.local", false);
    manager.record_result("faulty.mos.local", false);
    manager.record_result("faulty.mos.local", true);
    manager.record_result("faulty.mos.local", true);

    let eval = manager.evaluate_and_advance("faulty.mos.local");
    match eval {
        PipelineEvaluation::RolledBack { reason } => {
            assert!(reason.contains("exceeded threshold"));
        }
        _ => panic!("Expected automatic rollback"),
    }

    let routes = router.inspect_routes("faulty.mos.local").unwrap();
    assert_eq!(routes.stable.weight, 100);
    assert!(routes.canary.is_none());
}
