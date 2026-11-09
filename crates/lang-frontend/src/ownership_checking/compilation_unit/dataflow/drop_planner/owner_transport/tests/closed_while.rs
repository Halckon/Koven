//! Real While traversal, not a fabricated repeated straight-line trace.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerObservationStatus,
    UnitOwnerOrigin, UnitOwnerUnsupportedReason, UnitOwnerValueEventV1 as Event,
};
use crate::type_checking::UnitStatementId;

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
