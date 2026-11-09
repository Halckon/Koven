//! Real While traversal, not a fabricated repeated straight-line trace.
use super::*;
use crate::ownership_checking::{
    OwnershipDeferredReason, UnitDropPoint, UnitDropTarget, UnitOwnerCallable,
    UnitOwnerLoopEdgeV1 as Edge, UnitOwnerObservationStatus, UnitOwnerOrigin,
    UnitOwnerUnsupportedReason, UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::type_checking::{UnitItemId, UnitStatementId};

#[test]
fn real_while_body_publishes_closed_transport_and_actual_backedge() {
    for condition in ["flag", "(flag)", "((flag))"] {
        for (body, definitions, replaces, moves) in [
            (r#"val local = "local""#, 1, 0, 0),
            (
                r#"val local = "local"; val first = (local); val last = ((first))"#,
                1,
                0,
                2,
            ),
            (
                r#"var root = "old"; val incoming = "next"; val previous = replace(&root, incoming); val forwarded = (previous)"#,
                2,
                1,
                2,
            ),
            (
                r#"var left = "left"; var right = "right"; val previous = replace(&left, replace(&right, "next"))"#,
                3,
                2,
                0,
            ),
        ] {
            for result in ["", ": Unit"] {
                let control = format!("while ({condition}) {{ {body} }}");
                let text = format!(
                    "fun run(flag: Boolean){result} {{ {control} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}"
                );
                let fixture = Fixture::new(&[("root.ko", &text)]);
                let owned = fixture.shared();
                assert!(owned.clone().validate().is_ok(), "{text}");
                let observation = &owned.owner_observations()[0];
                let statement = fixture.parsed[0]
                    .ast()
                    .statements()
                    .iter()
                    .find_map(|(id, node)| {
                        (fixture.sources.slice(node.span()).unwrap() == control).then_some(id)
                    })
                    .unwrap();
                let statement = UnitStatementId::new(fixture.source(0), statement);
                assert_eq!(
                    observation.status(),
                    UnitOwnerObservationStatus::Unsupported {
                        point: UnitDropPoint::AfterStatement(statement),
                        reason: UnitOwnerUnsupportedReason::Syntax
                    }
                );
                assert!(observation.events().is_empty() && observation.definitions().is_empty());
                assert!(observation.value_observation_v1().is_none());
                let observed = observation
                    .loop_observation_v1()
                    .unwrap_or_else(|| panic!("missing real While body: {text}"));
                assert_eq!(observed.statement(), statement);
                assert_eq!(
                    observed.condition(),
                    UnitExpressionId::new(fixture.source(0), fixture.expression(0, condition))
                );
                assert_eq!(
                    observed.edges(),
                    &[
                        Edge::EnterBody(observed.condition(), observed.body()),
                        Edge::Backedge(
                            UnitDropPoint::AfterStatement(observed.body()),
                            observed.condition()
                        ),
                        Edge::ConditionFalseExit(
                            observed.condition(),
                            UnitDropPoint::LoopExit(statement)
                        ),
                    ]
                );
                let actual_body = fixture.parsed[0]
                    .ast()
                    .statements()
                    .iter()
                    .find_map(|(id, node)| {
                        (fixture.sources.slice(node.span()).unwrap() == format!("{{ {body} }}"))
                            .then_some(id)
                    })
                    .unwrap();
                assert_eq!(
                    observed.body(),
                    UnitStatementId::new(fixture.source(0), actual_body)
                );
                assert!(observed.is_consistent());
                let graph = observed.body_transport();
                assert_eq!(graph.producers().definitions().len(), definitions);
                assert_eq!(
                    graph
                        .prefix()
                        .iter()
                        .filter(|e| matches!(e, Event::Replace { .. }))
                        .count(),
                    replaces
                );
                assert_eq!(
                    graph
                        .prefix()
                        .iter()
                        .filter(|e| matches!(e, Event::Move(..)))
                        .count(),
                    moves
                );
                let creates = graph
                    .prefix()
                    .iter()
                    .filter_map(|e| match e {
                        Event::Create(expression, owner) => Some((*expression, owner)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(creates.len(), definitions);
                for (expression, owner) in creates {
                    assert_eq!(
                        graph
                            .producers()
                            .definition(owner.clone())
                            .unwrap()
                            .origin(),
                        UnitOwnerOrigin::String(expression)
                    );
                }
                for definition in graph.producers().definitions() {
                    assert_eq!(
                        graph
                            .prefix()
                            .iter()
                            .filter(|event| matches!(event,
                        Event::Drop(_, _, Value::Definition(owner)) if *owner == definition.id()))
                            .count(),
                        1
                    );
                }
                let drops = graph
                    .prefix()
                    .iter()
                    .filter_map(|e| match e {
                        Event::Drop(point, symbol, _) => Some((*point, *symbol)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(drops.len(), definitions);
                for (point, symbol) in drops {
                    assert!(owned.drops().iter().any(|drop| drop.point() == point
                        && drop.target() == UnitDropTarget::Named(symbol)));
                }
                assert!(
                    owned.owner_observations()[1]
                        .loop_observation_v1()
                        .is_none()
                );
                assert_eq!(
                    owned.owner_observations()[1].status(),
                    UnitOwnerObservationStatus::Complete
                );
                compare(&fixture, &fixture.parts(&owned));
            }
        }
    }
}

#[test]
fn closed_while_rejects_open_boundaries_and_unsupported_suffixes() {
    for run in [
        r#"fun run(flag: Boolean) { while (flag) {} }"#,
        r#"fun run(flag: Boolean) { while (false) { val local = "x" } }"#,
        r#"fun run(flag: Boolean) { while (true) { val local = "x" } }"#,
        r#"fun run(flag: Boolean) { while (!flag) { val local = "x" } }"#,
        r#"fun run(flag: Boolean) { while (flag && flag) { val local = "x" } }"#,
        r#"fun run(flag: Boolean) { while (flag == flag) { val local = "x" } }"#,
        r#"fun run(flag: Boolean) { while (test()) { val local = "x" } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "x"; return } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "x"; break } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "x"; continue } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "x"; while (flag) { val nested = "y" } } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "x"; if (flag) { val left = "y" } else { val right = "z" } } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "x"; val callback = move { local } } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "x"; val other = make() } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "x"; consume(local) } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "x"; "temporary" } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "x"; var other = "old"; other = "new" } }"#,
        r#"fun run(flag: Boolean) { val prefix = "x"; while (flag) { val local = "y" } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "y" }; val suffix = "x" }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "y" }; while (flag) { val next = "z" } }"#,
        r#"fun run(flag: Boolean) { var root = "x"; while (flag) { val previous = replace(&root, "y") } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local: String? = "x" } }"#,
        r#"fun run(flag: Boolean) { while (flag) { val local = "x"; val other = local + "y" } }"#,
        r#"fun run(flag: Boolean) { while (flag) { var root = "x"; val other = replace(&root, make()) } }"#,
        r#"fun run(flag: Boolean) { loop { val local = "x"; break } }"#,
        r#"fun run(flag: Boolean, other: Boolean) { while (flag) { val local = "x" } }"#,
        r#"fun run(inout flag: Boolean) { while (flag) { val local = "x" } }"#,
    ] {
        let text = format!(
            "{run}\nfun test(): Boolean = false\nfun make(): String = \"opaque\"\nfun consume(own value: String) {{}}\nfun sibling() {{ val safe = \"safe\" }}"
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(
            owned.clone().validate().is_ok(),
            "{run}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.owner_observations()[0]
                .loop_observation_v1()
                .is_none(),
            "{run}"
        );
        let sibling = UnitOwnerCallable::Function(UnitItemId::new(
            fixture.source(0),
            fixture.parsed[0].roots()[4],
        ));
        let sibling = owned
            .owner_observations()
            .iter()
            .find(|observation| observation.callable() == sibling)
            .unwrap();
        assert_eq!(sibling.status(), UnitOwnerObservationStatus::Complete);
        assert!(sibling.loop_observation_v1().is_none());
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn closed_while_catalogs_are_local_to_callable_and_analysis() {
    let text = "fun first(flag: Boolean) { while (flag) { val a = \"a\" } }\nfun second(flag: Boolean) { while (flag) { val b = \"b\" } }";
    let fixture = Fixture::new(&[("root.ko", text)]);
    let left = fixture.shared();
    let right = fixture.shared();
    for product in [&left, &right] {
        assert!(product.clone().validate().is_ok());
        assert_eq!(product.owner_observations().len(), 2);
        let catalogs = product
            .owner_observations()
            .iter()
            .map(|observation| {
                observation
                    .loop_observation_v1()
                    .unwrap()
                    .body_transport()
                    .producers()
            })
            .collect::<Vec<_>>();
        for (index, catalog) in catalogs.iter().enumerate() {
            assert!(catalog.is_consistent());
            let owner = catalog.definitions()[0].id();
            assert!(catalogs[1 - index].definition(owner.clone()).is_none());
            assert!(
                product.owner_observations()[index]
                    .definition(owner)
                    .is_none()
            );
        }
    }
    for index in 0..2 {
        let l = left.owner_observations()[index]
            .loop_observation_v1()
            .unwrap();
        let r = right.owner_observations()[index]
            .loop_observation_v1()
            .unwrap();
        assert_eq!(l.statement(), r.statement());
        assert!(
            r.body_transport()
                .producers()
                .definition(l.body_transport().producers().definitions()[0].id())
                .is_none()
        );
    }
    compare(&fixture, &fixture.parts(&left));
}

// Deliberate capability corruption: compile locally, execute only in formal CI.
#[test]
fn closed_while_seal_rejects_foreign_transport_and_missing_edges_or_drops() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "fun run(flag: Boolean) { while (flag) { val local = \"local\" } }",
    )]);
    let left = fixture.shared();
    let right = fixture.shared();
    let original = left.owner_observations()[0].loop_observation_v1().unwrap();
    let mut changed = original.clone();
    changed.transport = right.owner_observations()[0]
        .loop_observation_v1()
        .unwrap()
        .transport
        .clone();
    assert!(
        !changed.is_consistent(),
        "whole foreign catalog must not cross the loop seal"
    );
    let mut changed = original.clone();
    changed.edges[1] = changed.edges[0];
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed
        .transport
        .prefix
        .retain(|event| !matches!(event, Event::Drop(..)));
    assert!(!changed.is_consistent());
    let mut changed = left.owner_observations()[0].clone();
    changed.loop_value = right.owner_observations()[0].loop_value.clone();
    assert!(changed.loop_observation_v1().is_none());
}

#[test]
fn ownership_error_withholds_an_otherwise_admitted_loop() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "fun run(flag: Boolean) { while (flag) { val local = \"local\" } }\nfun bad(input: String) { val moved = input }",
    )]);
    let owned = fixture.shared();
    assert!(!owned.diagnostics().is_empty());
    assert!(owned.clone().validate().is_err());
    assert!(owned.owner_observations().is_empty());
}

// Deliberate final-publication deferred injection; compile locally, execute only in CI.
#[test]
fn deferred_product_withholds_sealed_loop_capability() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "fun run(flag: Boolean) { while (flag) { val local = \"local\" } }",
    )]);
    let baseline = fixture.shared();
    let parts = fixture.parts(&baseline);
    let mut raw = fixture.dataflow(&parts);
    assert!(raw.deferred.is_empty());
    assert!(raw.owner_observations[0].loop_observation_v1().is_some());
    raw.deferred.push(UnitOwnershipDeferredFact::new(
        UnitExpressionId::new(fixture.source(0), fixture.expression(0, "flag")),
        OwnershipDeferredReason::MemberReceiver,
    ));
    let owned = crate::ownership_checking::compilation_unit::CompilationUnitOwnership::new(
        fixture.typed.types(),
        parts.bindings.into_values().collect(),
        parts.arguments,
        parts.receivers,
        parts.capture,
        raw,
    );
    assert!(!owned.deferred().is_empty());
    assert_eq!(owned.owner_observations().len(), 1);
    assert!(
        owned.owner_observations()[0]
            .loop_observation_v1()
            .is_none()
    );
}
