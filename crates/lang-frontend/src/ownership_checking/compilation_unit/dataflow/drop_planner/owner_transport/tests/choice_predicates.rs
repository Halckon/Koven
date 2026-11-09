//! Owner-free Boolean predicates keep actual If edges and output transport intact.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerObservationStatus, UnitOwnerUnsupportedReason,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};

#[test]
fn pure_two_parameter_short_circuit_conditions_keep_actual_if_edges() {
    let dual = "flag: Boolean, other: Boolean";
    for (parameters, condition) in [
        ("flag: Boolean", "flag && flag"),
        ("flag: Boolean", "!(flag || flag)"),
        (dual, "flag"),
        (dual, "other"),
        (dual, "flag && other"),
        (dual, "flag || other"),
        (dual, "!(flag && !other)"),
        (dual, "(!flag || (other && flag))"),
        (dual, "(flag && other) || (!flag && !other)"),
        (dual, "other && flag"),
        (dual, "!((flag || other) && (other || !flag))"),
    ] {
        for arms in [
            r#""left" else "right""#,
            r#"{ val local = "left"; ((local)) } else "right""#,
            r#""left" else { val local = "right"; (local) }"#,
            r#"{ val local = "left"; local } else { val local = "right"; local }"#,
        ] {
            let control = format!("if ({condition}) {arms}");
            let source = format!(
                "fun run({parameters}): Unit {{ val chosen = {control} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}"
            );
            let fixture = Fixture::new(&[("root.ko", &source)]);
            let owned = fixture.shared();
            assert!(owned.diagnostics().is_empty(), "{source}");
            assert!(owned.clone().validate().is_ok(), "{source}");
            let legacy = &owned.owner_observations()[0];
            assert_eq!(
                legacy.status(),
                UnitOwnerObservationStatus::Unsupported {
                    point: UnitDropPoint::AfterExpression(UnitExpressionId::new(
                        fixture.source(0),
                        fixture.expression(0, &control)
                    )),
                    reason: UnitOwnerUnsupportedReason::Syntax,
                }
            );
            assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
            let graph = legacy
                .value_observation_v1()
                .unwrap_or_else(|| panic!("pure predicate Choice: {source}"));
            // Full edge events and Bind/Drop identities come from this actual AST.
            choice_branch_local::assert_ast_graph(&fixture, &control, &graph);
            assert_eq!(graph.producers().definitions().len(), 2);
            assert_eq!(
                owned.drops().len(),
                2,
                "predicate parameters own no resources"
            );
            assert_eq!(
                owned.owner_observations()[1].status(),
                UnitOwnerObservationStatus::Complete
            );
            compare(&fixture, &fixture.parts(&owned));
        }
    }
}

#[test]
fn pure_predicates_compose_with_all_bounded_choice_consumers() {
    for condition in [
        "flag && other",
        "!(other || flag)",
        "(!flag || other) && (flag || !other)",
        "other",
    ] {
        for (body, choice_moves, definition_moves, producers, named_drops) in [
            ("val chosen = ({control})", 0, 0, 2, 1),
            (
                "val chosen = (({control})); val forwarded = ((chosen))",
                1,
                0,
                2,
                1,
            ),
            (
                "val chosen = ({control}); val forwarded = (chosen); val again = ((forwarded))",
                2,
                0,
                2,
                1,
            ),
            (
                "var chosen = ({control}); val previous = ((replace(&chosen, (\"next\"))))",
                0,
                0,
                3,
                2,
            ),
            (
                "var chosen = ({control}); val previous = (replace(&chosen, \"next\")); val forwarded = ((previous))",
                1,
                0,
                3,
                2,
            ),
            (
                "var chosen = ({control}); val previous = ((replace(&chosen, \"next\"))); val forwarded = (chosen)",
                0,
                1,
                3,
                2,
            ),
            (
                "var chosen = ({control}); val previous = ((replace(&chosen, (\"next\")))); val old = (previous); val installed = ((chosen))",
                1,
                1,
                3,
                2,
            ),
            (
                "var chosen = ({control}); val previous = ((replace(&chosen, (\"next\")))); val installed = ((chosen)); val old = (previous)",
                1,
                1,
                3,
                2,
            ),
        ] {
            let control =
                format!("if ({condition}) {{ val local = \"left\"; ((local)) }} else \"right\"");
            let body = body.replace("{control}", &control);
            let source = format!(
                "fun run(flag: Boolean, other: Boolean): Unit {{ {body} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}"
            );
            let fixture = Fixture::new(&[("root.ko", &source)]);
            let owned = fixture.shared();
            assert!(owned.diagnostics().is_empty(), "{source}");
            assert!(owned.clone().validate().is_ok(), "{source}");
            let legacy = &owned.owner_observations()[0];
            let graph = legacy
                .value_observation_v1()
                .unwrap_or_else(|| panic!("predicate/consumer composition: {source}"));
            assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
            choice_branch_local::assert_ast_edges(&fixture, &control, &graph);
            assert_eq!(graph.producers().definitions().len(), producers);
            assert_eq!(
                graph
                    .suffix()
                    .iter()
                    .filter(|event| matches!(event, Event::Move(_, _, Value::Choice(_))))
                    .count(),
                choice_moves
            );
            assert_eq!(
                graph
                    .suffix()
                    .iter()
                    .filter(|event| matches!(event, Event::Move(_, _, Value::Definition(_))))
                    .count(),
                definition_moves
            );
            let drops: Vec<_> = graph
                .suffix()
                .iter()
                .filter_map(|event| match event {
                    Event::Drop(point, symbol, _) => Some((*point, *symbol)),
                    _ => None,
                })
                .collect();
            assert_eq!(drops.len(), named_drops);
            for (point, symbol) in drops {
                assert_eq!(
                    owned
                        .drops()
                        .iter()
                        .filter(|drop| drop.point() == point
                            && drop.target() == UnitDropTarget::Named(symbol))
                        .count(),
                    1
                );
            }
            assert_eq!(owned.drops().len(), named_drops + 1);
            for parameter in [fixture.symbol(0, "flag"), fixture.symbol(0, "other")] {
                assert!(!graph.suffix().iter().any(|event| matches!(event,
                    Event::Move(_, symbol, _) | Event::Bind(symbol, _, _) | Event::Drop(_, symbol, _) if *symbol == parameter)));
            }
            assert_eq!(
                owned.owner_observations()[1].status(),
                UnitOwnerObservationStatus::Complete
            );
            compare(&fixture, &fixture.parts(&owned));
        }
    }
}

#[test]
fn predicate_admission_rejects_impure_leaves_modes_and_wider_signatures() {
    for (parameters, condition) in [
        ("flag: Boolean, other: Boolean", "flag && true"),
        ("flag: Boolean, other: Boolean", "false || other"),
        ("flag: Boolean, other: Boolean", "flag && decision()"),
        ("flag: Boolean, other: Boolean", "decision() || other"),
        (
            "flag: Boolean, other: Boolean",
            "flag || (other && !decision())",
        ),
        ("flag: Boolean, other: Boolean", "flag && (other == flag)"),
        ("flag: Boolean, other: Boolean", "!(flag != other) || other"),
        (
            "flag: Boolean, other: Boolean",
            "flag && (if (other) flag else other)",
        ),
        ("flag: Boolean, other: Boolean?", "flag && other!!"),
        ("flag: Boolean, other: Int", "flag && (other > 0)"),
        ("flag: Boolean, own other: Boolean", "flag && other"),
        ("flag: Boolean, borrow other: Boolean", "flag && other"),
        ("flag: Boolean, inout other: Boolean", "flag && other"),
        (
            "flag: Boolean, other: Boolean, third: Boolean",
            "flag && (other || third)",
        ),
        ("own flag: Boolean, other: Boolean", "flag || other"),
        ("", "true"),
    ] {
        let source = format!(
            "fun run({parameters}): Unit {{ val chosen = if ({condition}) \"left\" else \"right\" }}\nfun decision(): Boolean = true\nfun sibling(): Unit {{ val safe = \"safe\" }}"
        );
        let fixture = Fixture::new(&[("root.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{source}");
        assert!(owned.clone().validate().is_ok(), "{source}");
        let rejected = &owned.owner_observations()[0];
        assert!(rejected.value_observation_v1().is_none(), "{source}");
        assert!(rejected.events().is_empty() && rejected.definitions().is_empty());
        assert_eq!(
            owned.owner_observations()[2].status(),
            UnitOwnerObservationStatus::Complete
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}
