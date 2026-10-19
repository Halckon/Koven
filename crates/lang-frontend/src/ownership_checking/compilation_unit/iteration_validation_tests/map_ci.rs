//! CI-only unit Map facts; preserve original qualified identities and reject before view publication.
use super::super::*;

#[test]
fn unit_map_private_fact_damage_cannot_publish_owned_capability() {
    let valid = super::super::constants_tests::analyze(
        "package a\nfun run(first:Map<Int,String>, second:Map<Int,String>) {for((key,value) in first){println(value)};for((key,value) in second){println(value)}}",
    );
    assert!(valid.diagnostics().is_empty() && valid.deferred().is_empty());
    assert_eq!(valid.map_iterations().len(), 2);
    assert!(valid.map_iteration_headers_are_valid());
    assert!(valid.clone().validate().is_ok());
    for fault in 0..8 {
        let mut damaged = valid.clone();
        match fault {
            0 => {
                damaged.map_iterations.remove(0);
            }
            1 => damaged.map_iterations[0].source = valid.map_iterations[1].source.clone(),
            2 => damaged.map_iterations[0].loans[0] = valid.map_iterations[1].loans[0].clone(),
            3 => {
                damaged.map_iterations[0].descriptor.value_type =
                    valid.map_iterations[0].descriptor.key_type
            }
            4 => damaged.map_iterations[0].bindings.swap(0, 1),
            5 => damaged.map_iterations[0]
                .exits
                .retain(|e| e.kind() != UnitIterationExitKind::Exhaustion),
            6 => damaged.map_iterations[0]
                .exits
                .iter_mut()
                .find(|e| e.kind() == UnitIterationExitKind::Fallthrough)
                .unwrap()
                .actions
                .swap(0, 1),
            7 => {
                let statement = damaged.map_iterations[0].descriptor.statement();
                damaged.map_iterations[0]
                    .exits
                    .iter_mut()
                    .find(|e| e.kind() == UnitIterationExitKind::Fallthrough)
                    .unwrap()
                    .actions
                    .insert(0, UnitIterationCleanupAction::EndSource(statement));
            }
            _ => unreachable!(),
        }
        if fault < 5 {
            assert!(
                !damaged.map_iteration_headers_are_valid(),
                "fault={fault}: header guard"
            );
        } else {
            assert!(damaged.map_iteration_headers_are_valid());
            assert!(
                !damaged.iterations_are_valid(),
                "fault={fault}: complete cleanup guard"
            );
        }
        let before = format!("{damaged:?}");
        let recovery = damaged
            .validate()
            .expect_err("bad Map cannot publish owned unit");
        assert_eq!(
            before,
            format!("{:?}", *recovery),
            "reject preserves original recovery"
        );
        println!("SPEC0331 CI MAP FACT unit fault={fault} capability=rejected");
    }
}
