//! Independent AST/capture oracles for environments released on real Unit If paths.
use super::*;
use crate::ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode};

pub(in crate::ownership_checking::compilation_unit) const TWO_ENVIRONMENTS: &str = r#"fun run(flag: Boolean, own first: String, own second: String): Unit {
val one = move { first }
val two = move { second }
if (flag) {
    val left_one = one
    val left_two = two
} else {
    val right_one = one
    val right_two = two
}
}"#;

pub(in crate::ownership_checking::compilation_unit) const TWO_SLOTS: &str = r#"fun run(flag: Boolean, own first: String, own second: String): Unit {
val callback = move { second + first }
if (flag) { val left = callback } else { val right = callback }
}"#;

pub(in crate::ownership_checking::compilation_unit) const NESTED_UNUSED_EDGE: &str = r#"fun run(flag: Boolean, own text: String): Unit {
val callback = move { text }
if (flag) {
    if (flag) { val inner_used = callback } else { val unused = 1 }
} else { val outer_used = callback }
}"#;

fn function(owned: &CompilationUnitOwnership) -> &UnitOwnerObservation {
    let functions = owned
        .owner_observations()
        .iter()
        .filter(|observation| matches!(observation.callable(), UnitOwnerCallable::Function(_)))
        .collect::<Vec<_>>();
    let [observation] = functions.as_slice() else {
        panic!("one real function")
    };
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    observation
}

fn owner_at(observation: &UnitOwnerObservation, origin: UnitOwnerOrigin) -> UnitOwnerId {
    let owners = observation
        .definitions()
        .iter()
        .filter(|definition| definition.origin() == origin)
        .map(UnitOwnerDefinition::id)
        .collect::<Vec<_>>();
    let [owner] = owners.as_slice() else {
        panic!("unique AST producer {origin:?}")
    };
    owner.clone()
}

struct ExpectedCapture {
    lambda: UnitExpressionId,
    environment: UnitOwnerId,
    slots: Vec<(UnitSymbolId, UnitOwnerId)>,
}

fn expected_capture(
    fixture: &Fixture,
    owned: &CompilationUnitOwnership,
    observation: &UnitOwnerObservation,
    lambda: &str,
    names_in_capture_order: &[&str],
) -> ExpectedCapture {
    let lambda = UnitExpressionId::new(fixture.source(0), fixture.expression(0, lambda));
    let symbols = names_in_capture_order
        .iter()
        .map(|name| fixture.symbol(0, name))
        .collect::<Vec<_>>();
    let checked = owned
        .captures()
        .iter()
        .filter(|capture| capture.lambda() == lambda)
        .map(|capture| {
            assert_eq!(capture.mode(), ClosureCaptureMode::Owned);
            assert_eq!(capture.effect(), ClosureCaptureEffect::Move);
            capture.source()
        })
        .collect::<Vec<_>>();
    // Fix the original order independently, before reversing it for cleanup.
    assert_eq!(
        checked,
        symbols
            .iter()
            .copied()
            .map(UnitClosureCaptureSource::Symbol)
            .collect::<Vec<_>>()
    );
    let slots = symbols
        .into_iter()
        .map(|symbol| {
            (
                symbol,
                owner_at(observation, UnitOwnerOrigin::Parameter(symbol)),
            )
        })
        .collect::<Vec<_>>();
    let environment = owner_at(observation, UnitOwnerOrigin::Closure(lambda));
    let formations = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::Capture {
                expression,
                environment,
                inputs,
            } if *expression == lambda => Some((environment, inputs)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(formations, vec![(&environment, &slots)]);
    ExpectedCapture {
        lambda,
        environment,
        slots,
    }
}

fn fork(
    events: &[UnitOwnerTransportEvent],
    expected: UnitExpressionId,
) -> [&[UnitOwnerTransportEvent]; 2] {
    let forks = events
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::If {
                control,
                then_events,
                else_events,
            } => {
                assert_eq!(*control, expected);
                Some([then_events.as_slice(), else_events.as_slice()])
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [edges] = forks.as_slice() else {
        panic!("one source If")
    };
    *edges
}

fn assert_live_at_fork(events: &[UnitOwnerTransportEvent], captures: &[ExpectedCapture]) {
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, UnitOwnerTransportEvent::Capture { .. }))
            .count(),
        captures.len()
    );
    let fork_index = events
        .iter()
        .position(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
        .expect("real fork after formation");
    for (index, event) in events.iter().enumerate() {
        match event {
            UnitOwnerTransportEvent::Capture { .. } => assert!(index < fork_index),
            UnitOwnerTransportEvent::CaptureDrop { .. } => panic!("no top-level slot release"),
            UnitOwnerTransportEvent::Drop { owner, .. } => {
                assert!(captures.iter().all(|capture| &capture.environment != owner));
            }
            _ => {}
        }
    }
}

// Unused pure aliases die at their own declaration, not a point read from either product.
pub(super) fn local_drop_point(fixture: &Fixture, binding: UnitSymbolId) -> UnitDropPoint {
    use crate::parser::{Item, NameMarker, Statement};
    let resolution =
        fixture.names.names().source_units()[binding.source_unit().index()].resolution();
    let marker = resolution.symbols()[binding.symbol().index()].span();
    let ast = fixture.parsed[0].ast();
    let statements = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            let Statement::LocalVariable { declaration } = node.payload() else {
                return None;
            };
            matches!(ast.items().get(*declaration).unwrap().payload(),
            Item::Variable { name: NameMarker::Present(span), .. } if *span == marker)
            .then_some(id)
        })
        .collect::<Vec<_>>();
    let [statement] = statements.as_slice() else {
        panic!("one AST declaration for the binding")
    };
    UnitDropPoint::AfterStatement(UnitStatementId::new(binding.source_unit(), *statement))
}

fn assert_released(
    owned: &CompilationUnitOwnership,
    edge: &[UnitOwnerTransportEvent],
    capture: &ExpectedCapture,
    binding: UnitSymbolId,
    expected_point: UnitDropPoint,
) {
    assert!(
        edge.iter().all(|event| !matches!(
            event,
            UnitOwnerTransportEvent::Capture { .. } | UnitOwnerTransportEvent::If { .. }
        )),
        "a leaf path cannot hide formation or another fork"
    );
    let releases = edge
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            UnitOwnerTransportEvent::CaptureDrop {
                point,
                environment,
                source,
                owner,
            } if environment == &capture.environment => {
                Some((index, *point, *source, owner.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let expected_slots = capture.slots.iter().rev().cloned().collect::<Vec<_>>();
    assert_eq!(
        releases
            .iter()
            .map(|(_, _, source, owner)| (*source, owner.clone()))
            .collect::<Vec<_>>(),
        expected_slots,
        "exactly once, reverse original slot order"
    );
    let drops = edge
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            UnitOwnerTransportEvent::Drop {
                point,
                symbol,
                owner,
            } if owner == &capture.environment => {
                assert_eq!(*symbol, binding);
                Some((index, *point))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(drop_index, point)] = drops.as_slice() else {
        panic!("one environment drop per edge")
    };
    assert_eq!(
        *point, expected_point,
        "environment cleanup has the AST-derived point"
    );
    for (index, slot_point, _, _) in &releases {
        assert!(index < drop_index, "all slots precede environment");
        assert_eq!(
            *slot_point, expected_point,
            "slot cleanup has the AST-derived point"
        );
    }
    let legacy = owned
        .drops()
        .iter()
        .enumerate()
        .filter_map(|(index, fact)| {
            if fact.point() != expected_point {
                return None;
            }
            match fact.target() {
                UnitDropTarget::Captured { closure, source } if closure == capture.lambda => {
                    Some((index, source))
                }
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        legacy.iter().map(|(_, source)| *source).collect::<Vec<_>>(),
        expected_slots
            .iter()
            .map(|(source, _)| UnitClosureCaptureSource::Symbol(*source))
            .collect::<Vec<_>>(),
        "legacy cleanup has the same independently fixed reverse order"
    );
    let environment_drops = owned
        .drops()
        .iter()
        .enumerate()
        .filter(|(_, fact)| {
            fact.point() == expected_point && fact.target() == UnitDropTarget::Named(binding)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let [environment_drop] = environment_drops.as_slice() else {
        panic!("one legacy environment drop")
    };
    assert!(legacy.iter().all(|(index, _)| index < environment_drop));
}

#[test]
fn two_live_capture_environments_keep_independent_slots_on_both_if_edges() {
    let fixture = Fixture::new(&[("root.ko", TWO_ENVIRONMENTS)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = function(&owned);
    let captures = [
        expected_capture(&fixture, &owned, observation, "move { first }", &["first"]),
        expected_capture(
            &fixture,
            &owned,
            observation,
            "move { second }",
            &["second"],
        ),
    ];
    assert_ne!(captures[0].environment, captures[1].environment);
    assert_ne!(captures[0].slots[0], captures[1].slots[0]);
    assert_live_at_fork(observation.events(), &captures);
    let control = UnitExpressionId::new(fixture.source(0), fixture.expression(0,
        "if (flag) {\n    val left_one = one\n    val left_two = two\n} else {\n    val right_one = one\n    val right_two = two\n}"));
    for (edge, bindings) in fork(observation.events(), control)
        .into_iter()
        .zip([["left_one", "left_two"], ["right_one", "right_two"]])
    {
        assert_eq!(
            edge.iter()
                .filter(|event| matches!(event, UnitOwnerTransportEvent::CaptureDrop { .. }))
                .count(),
            2
        );
        for (capture, binding) in captures.iter().zip(bindings) {
            let binding = fixture.local(0, binding);
            assert_released(
                &owned,
                edge,
                capture,
                binding,
                local_drop_point(&fixture, binding),
            );
        }
    }
}

#[test]
fn two_owned_string_slots_release_in_reverse_checked_order_on_each_path() {
    let fixture = Fixture::new(&[("root.ko", TWO_SLOTS)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = function(&owned);
    // Parameter declaration order is deliberately different from first reference order.
    let captures = [expected_capture(
        &fixture,
        &owned,
        observation,
        "move { second + first }",
        &["second", "first"],
    )];
    assert_live_at_fork(observation.events(), &captures);
    let control = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(
            0,
            "if (flag) { val left = callback } else { val right = callback }",
        ),
    );
    for (edge, binding) in fork(observation.events(), control)
        .into_iter()
        .zip(["left", "right"])
    {
        assert_eq!(
            edge.iter()
                .filter(|event| matches!(event, UnitOwnerTransportEvent::CaptureDrop { .. }))
                .count(),
            2
        );
        let binding = fixture.local(0, binding);
        assert_released(
            &owned,
            edge,
            &captures[0],
            binding,
            local_drop_point(&fixture, binding),
        );
    }
}

#[test]
fn nested_capture_release_cannot_claim_its_ancestor_control() {
    let fixture = Fixture::new(&[("root.ko", NESTED_UNUSED_EDGE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = function(&owned);
    let captures = [expected_capture(
        &fixture,
        &owned,
        observation,
        "move { text }",
        &["text"],
    )];
    assert_live_at_fork(observation.events(), &captures);
    let outer = UnitExpressionId::new(fixture.source(0), fixture.expression(0,
        "if (flag) {\n    if (flag) { val inner_used = callback } else { val unused = 1 }\n} else { val outer_used = callback }"));
    let inner = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(
            0,
            "if (flag) { val inner_used = callback } else { val unused = 1 }",
        ),
    );
    assert_ne!(inner, outer);
    let [outer_then, outer_else] = fork(observation.events(), outer);
    let [inner_then, inner_else] = fork(outer_then, inner);
    for (edge, binding) in [
        (inner_then, "inner_used"),
        (inner_else, "callback"),
        (outer_else, "outer_used"),
    ] {
        let symbol = fixture.local(0, binding);
        let expected_point = if binding == "callback" {
            UnitDropPoint::BranchExit {
                control: inner,
                branch: 1,
            }
        } else {
            local_drop_point(&fixture, symbol)
        };
        assert_released(&owned, edge, &captures[0], symbol, expected_point);
    }
    let mut damaged = observation.clone();
    let outer_then = damaged
        .events
        .iter_mut()
        .find_map(|event| match event {
            UnitOwnerTransportEvent::If {
                control,
                then_events,
                ..
            } if *control == outer => Some(then_events),
            _ => None,
        })
        .unwrap();
    let inner_else = outer_then
        .iter_mut()
        .find_map(|event| match event {
            UnitOwnerTransportEvent::If {
                control,
                else_events,
                ..
            } if *control == inner => Some(else_events),
            _ => None,
        })
        .unwrap();
    let points = inner_else
        .iter_mut()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::CaptureDrop { point, .. } => Some(point),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(points.len(), 1);
    for point in points {
        assert_eq!(
            *point,
            UnitDropPoint::BranchExit {
                control: inner,
                branch: 1
            }
        );
        let UnitDropPoint::BranchExit { control, .. } = point else {
            unreachable!()
        };
        // Only control changes: preserve branch, slot, environment, owner and position.
        *control = outer;
    }
    assert!(!damaged.is_consistent());
}
