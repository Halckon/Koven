use super::*;

#[test]
fn recursive_loop_carried_capture_does_not_publish_a_truncated_phi() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;
    let (_, _, owned) = checked(
        "fun run(flags: List<Int>) { var f: move () -> Unit = move {}\nfor (_ in flags) { f = move { f() } }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1);
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::RecursiveClosureCapture
    );
    assert!(
        owned.iterations().is_empty(),
        "recursive captured environments need an unbounded transport representation"
    );
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn recursive_owned_chain_with_shared_child_stays_atomically_deferred() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(xs: List<Int>, flags: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in flags) {\n    val g: () -> Unit = { read(xs) }\n    f = move { val old = f()\nval borrowed = g() }\n}\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1);
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::RecursiveClosureCapture
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn recursive_chain_keeps_scoped_shared_source_atomically_deferred() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(first: List<Int>, flags: List<Int>) {
var f: move () -> Unit = move {}
{
    val xs = listOf(1)
    val borrowed: () -> Unit = { read(xs) }
    for (_ in first) { f = move { f() } }
    f = move { val old = f()\nval used = borrowed() }
}
for (_ in flags) { f = move { f() } }
val used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(
        owned
            .deferred()
            .iter()
            .any(|deferred| deferred.reason() == OwnershipDeferredReason::RecursiveClosureCapture)
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn closure_holding_two_recursive_loop_roots_stays_atomically_deferred() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, OwnershipDeferredReason,
    };

    let (sources, _, owned) = checked(
        "fun run(first: List<Int>, second: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in first) { f = move { f() } }\nvar g: move () -> Unit = move {}\nfor (_ in second) { g = move { g() } }\nval outer: move () -> Unit = move { val x = f()\nval y = g() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let outer_captures = owned
        .captures()
        .iter()
        .filter(|capture| {
            owned
                .captures()
                .iter()
                .filter(|other| other.lambda() == capture.lambda())
                .count()
                == 2
        })
        .collect::<Vec<_>>();
    assert_eq!(outer_captures.len(), 2);
    assert_eq!(
        outer_captures
            .iter()
            .map(|capture| sources.slice(capture.reference_span()).unwrap())
            .collect::<Vec<_>>(),
        ["f", "g"]
    );
    assert!(outer_captures.iter().all(|capture| {
        capture.mode() == ClosureCaptureMode::Owned
            && capture.effect() == ClosureCaptureEffect::Move
    }));
    assert!(
        owned.deferred().iter().any(|deferred| {
            deferred.reason() == OwnershipDeferredReason::RecursiveClosureCapture
        })
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn mutually_recursive_loop_captures_do_not_publish_a_truncated_phi() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun run(flags: List<Int>) { var f: move () -> Unit = move {}\nvar g: move () -> Unit = move {}\nfor (_ in flags) { { f = move { g() } }\ng = move { f() } }\nval used = g() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1);
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::RecursiveClosureCapture
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn recursive_capture_discards_earlier_iteration_facts_atomically() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun run(first: List<Int>, second: List<Int>) { for (_ in first) {}\nvar f: move () -> Unit = move {}\nfor (_ in second) { f = move { f() } }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1);
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::RecursiveClosureCapture
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn iteration_plan_exposes_finite_owned_capture_graph() {
    use lang_frontend::ownership_checking::{ClosureCaptureMode, ClosureCaptureSource};

    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Int>) { var g: move () -> Unit = move {}\nvar f: move () -> Unit = move {}\nfor (_ in flags) { { f = move { g() } }\ng = move {} }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let graph = owned.iterations()[0].capture_graph();
    let mut closures = std::collections::BTreeSet::new();
    for node in graph.nodes() {
        assert!(
            closures.insert(node.closure().index()),
            "one node per lambda identity"
        );
        for source in node.sources() {
            for &target in source.captured() {
                assert!(target < graph.nodes().len(), "edge stays inside the graph");
            }
        }
    }
    let body_f = graph
        .nodes()
        .iter()
        .find(|node| {
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                == "move { g() }"
        })
        .unwrap();
    let g_source = body_f
        .sources()
        .iter()
        .find(|source| matches!(source.capture().source(), ClosureCaptureSource::Symbol(_)))
        .unwrap();
    assert_eq!(g_source.capture().mode(), ClosureCaptureMode::Owned);
    assert!(
        !g_source.captured().is_empty(),
        "owned g keeps its possible prior environments"
    );
    let plan = &owned.iterations()[0];
    let table = owned.cleanup_conditions();
    let mut all_slots = std::collections::BTreeSet::new();
    for phi in plan.closure_phis() {
        let mut seen = std::collections::BTreeSet::new();
        let mut reachable = std::collections::BTreeSet::new();
        let tree_roots = phi
            .root_origins()
            .map(|origin| origin.node())
            .collect::<Vec<_>>();
        assert_eq!(phi.root_nodes(), tree_roots);
        let mut pending = phi.root_nodes().to_vec();
        while let Some(node) = pending.pop() {
            if !reachable.insert(node) {
                continue;
            }
            for source in graph.nodes()[node].sources() {
                pending.extend_from_slice(source.captured());
            }
        }
        let expected = reachable
            .into_iter()
            .flat_map(|node| {
                graph.nodes()[node]
                    .sources()
                    .iter()
                    .map(move |source| (node, source.position()))
            })
            .collect::<std::collections::BTreeSet<_>>();
        for capture in phi.capture_layout() {
            assert!(seen.insert((capture.node(), capture.position())));
            assert!(
                all_slots.insert(capture.slot()),
                "phi roots cannot share capture slots"
            );
            let node = &graph.nodes()[capture.node()];
            let edge = node
                .sources()
                .iter()
                .find(|source| source.position() == capture.position())
                .unwrap();
            let slot = table.capture_slot_value(capture.slot()).unwrap();
            assert_eq!(slot.environment(), phi.owner());
            assert_eq!(slot.closure(), node.closure());
            assert_eq!(slot.source(), edge.capture().source());
            assert_eq!(slot.position(), capture.position());
        }
        assert_eq!(
            seen, expected,
            "each phi must cover its full reachable graph"
        );
        for origin in phi.origins() {
            assert_eq!(graph.nodes()[origin.node()].closure(), origin.closure());
            for source in origin.sources() {
                let edge = graph.nodes()[origin.node()]
                    .sources()
                    .iter()
                    .find(|edge| edge.capture().source() == source.source())
                    .unwrap();
                for &index in source.captured() {
                    let nested = &phi.origins()[index];
                    assert!(edge.captured().contains(&nested.node()));
                    assert_eq!(graph.nodes()[nested.node()].closure(), nested.closure());
                }
            }
        }
    }
    let mut nested_incomings = 0;
    for incoming in plan.closure_phi_incomings() {
        for binding in incoming.bindings() {
            let phi = plan
                .closure_phis()
                .iter()
                .find(|phi| phi.owner() == binding.target())
                .unwrap();
            assert_eq!(
                binding.capture_slots_to_clear(),
                phi.capture_layout()
                    .iter()
                    .map(|capture| capture.slot())
                    .collect::<Vec<_>>()
            );
            for origin in binding.origins() {
                let layout = phi
                    .origins()
                    .iter()
                    .find(|layout| layout.selector() == origin.target())
                    .unwrap();
                assert_eq!(origin.node(), layout.node());
                for environment in origin.environments() {
                    for source in environment.sources() {
                        let Some(slot) = layout
                            .sources()
                            .iter()
                            .find(|slot| slot.source() == source.input().source())
                        else {
                            assert!(source.captured().is_empty());
                            continue;
                        };
                        for nested in source.captured() {
                            nested_incomings += 1;
                            let target = slot
                                .captured()
                                .iter()
                                .map(|&index| &phi.origins()[index])
                                .find(|target| target.selector() == nested.target())
                                .unwrap();
                            assert_eq!(nested.node(), target.node());
                        }
                    }
                }
            }
        }
    }
    assert!(
        nested_incomings > 0,
        "fixture must exercise nested capture transport"
    );
}

#[test]
fn alternative_owned_captures_defer_when_phi_loses_exclusive_paths() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (sources, parsed, owned) = checked(
        r#"fun read(xs: List<Int>) {}
fun run(own xs: List<Int>, flag: Boolean, flags: List<Int>) {
    var base: move () -> Unit = move { read(xs) }
    var f: move () -> Unit = move {}
    var g: move () -> Unit = move {}
    if (flag) { f = move { base() } } else { g = move { base() } }
    var outer: move () -> Unit = move { val first = f()
val second = g() }
    for (_ in flags) {}
    val used = outer()
}"#,
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let [deferred] = owned.deferred() else {
        panic!("forwarded phi must not publish independent nested presence bits");
    };
    assert_eq!(
        deferred.reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert_eq!(
        sources.slice(
            parsed
                .ast()
                .expressions()
                .get(deferred.expression())
                .unwrap()
                .span()
        ),
        Ok("move { read(xs) }")
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn same_lambda_parent_paths_defer_until_instance_transport() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>, next: List<Int>) {
            var first: move () -> Unit = move {}
            var second: move () -> Unit = move {}
            for (_ in flags) {
                second = first
                val xs = listOf(1)
                { first = move { read(xs) } }
            }
            var outer: move () -> Unit = move { val a = first()\nval b = second() }
            for (_ in next) {}
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let [deferred] = owned.deferred() else {
        panic!("coexisting captures of one lambda need instance-qualified transport");
    };
    assert_eq!(
        deferred.reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert_eq!(
        sources.slice(
            parsed
                .ast()
                .expressions()
                .get(deferred.expression())
                .unwrap()
                .span()
        ),
        Ok("move { read(xs) }")
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn same_lambda_shared_capture_paths_defer_until_instance_loan_ends() {
    use lang_frontend::ownership_checking::{ClosureCaptureMode, OwnershipDeferredReason};

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>, next: List<Int>) {
            val xs = listOf(1)
            var first: () -> Unit = {}
            var second: () -> Unit = {}
            for (_ in flags) {
                second = first
                { first = { read(xs) } }
            }
            var outer: move () -> Unit = move { val a = first()\nval b = second() }
            for (_ in next) {}
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(
        owned
            .captures()
            .iter()
            .filter(|capture| capture.mode() == ClosureCaptureMode::Shared)
            .count(),
        1,
        "the repeated inner lambda must borrow its source"
    );
    let [deferred] = owned.deferred() else {
        panic!("two inner instances cannot share one static loan-end target");
    };
    assert_eq!(
        deferred.reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert_eq!(
        sources.slice(
            parsed
                .ast()
                .expressions()
                .get(deferred.expression())
                .unwrap()
                .span()
        ),
        Ok("{ read(xs) }")
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}
