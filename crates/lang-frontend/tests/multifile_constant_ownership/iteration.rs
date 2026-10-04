use super::*;
use lang_frontend::ownership_checking::UnitIterationExitKind;

#[test]
fn constant_iteration_capability_keeps_only_reachable_provider_facts() {
    with_unit(
        "package a\nimport b.Labels\nfun read(flag: Boolean): Unit { val skipped = false && if(flag) { for (_ in listOf(Labels.TEXT)) {}\ntrue } else false\nfor (item in listOf(Labels.TEXT)) { println(item)\nif(flag) {continue}\nreturn } }",
        "package b\nobject Labels { const val TEXT = \"hi\" }",
        |sources, inputs, names, te, typed| {
            assert_eq!(typed.types().sequential_iterations().len(), 2);
            let recovery =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap();
            assert!(recovery.is_compatible_with(typed));
            assert!(
                recovery.ownership().clone().validate().is_err(),
                "constant facts cannot become ordinary native capability"
            );
            let owned = recovery.validate().unwrap();
            assert_eq!(
                owned.materializations().len(),
                1,
                "unreachable constant use has no owner"
            );
            assert_eq!(
                owned.ownership().iterations().len(),
                1,
                "unreachable provider is not acquired"
            );
            let exits = owned.ownership().iterations()[0].exits();
            assert!(
                exits
                    .iter()
                    .any(|exit| matches!(exit.kind(), UnitIterationExitKind::Continue(_)))
            );
            assert!(
                exits
                    .iter()
                    .any(|exit| matches!(exit.kind(), UnitIterationExitKind::Return(_)))
            );
            let retyped = check_compilation_unit_types(sources, inputs, names, te)
                .unwrap()
                .validate_constants()
                .unwrap();
            assert!(
                !owned.is_compatible_with(&retyped),
                "same text in a fresh type analysis has a different capability identity"
            );
        },
    );
}

#[test]
fn grouped_constant_pending_owner_keeps_one_identity_in_iteration_cleanup() {
    with_unit(
        "package a\nimport b.Labels\nfun use(text: String, n: Int) {}\nfun run(flag: Boolean) { for (_ in listOf(1)) { use((Labels.TEXT), if(flag) {return} else 1) } }",
        "package b\nobject Labels { const val TEXT = \"hi\" }",
        |sources, inputs, names, te, typed| {
            let owned =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            let owner = owned.materializations()[0].descriptor().expression();
            let exit = owned.ownership().iterations()[0]
                .exits()
                .iter()
                .find(|exit| matches!(exit.kind(), UnitIterationExitKind::Return(_)))
                .unwrap();
            assert!(exit.actions().iter().any(|action| matches!(action, lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(fact) if fact.target() == UnitDropTarget::Temporary(owner))));
        },
    );
}

#[test]
fn constant_capability_preserves_conditional_receiver_cleanup_position() {
    with_unit(
        "package a\nimport b.Labels\ninterface Relay { own fun consume(n: Int): Unit {}\nown fun run(xs: List<Int>, flag: Boolean): Unit { for (_ in xs) { consume(if(flag) {val n = Labels.N\nreturn} else {return}) } } }",
        "package b\nobject Labels { const val N = 1 }",
        |sources, inputs, names, te, typed| {
            let owned =
                check_compilation_unit_constant_ownership(sources, inputs, names, te, typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            let plan = &owned.ownership().iterations()[0];
            assert_eq!(
                plan.exits()
                    .iter()
                    .filter(|exit| matches!(exit.kind(), UnitIterationExitKind::Return(_)))
                    .count(),
                2,
                "both return branches must retain their cleanup plans through constant handoff"
            );
            for exit in plan
                .exits()
                .iter()
                .filter(|exit| matches!(exit.kind(), UnitIterationExitKind::Return(_)))
            {
                use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
                let drop = exit
                    .actions()
                    .iter()
                    .position(|action| matches!(action, Action::DropConditionalReceiver(_)))
                    .unwrap();
                let end = exit
                    .actions()
                    .iter()
                    .position(|action| matches!(action, Action::EndElement(_)))
                    .unwrap();
                assert!(drop < end);
            }
        },
    );
}
