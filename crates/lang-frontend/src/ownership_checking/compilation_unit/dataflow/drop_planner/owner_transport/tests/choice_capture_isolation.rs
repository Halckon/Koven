//! Different callables share a planner, not an observation transaction or capability.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitOwnerCallable, UnitOwnerId, UnitOwnerObservation,
    UnitOwnerObservationStatus, UnitOwnerOrigin, UnitOwnerTransportEvent,
    UnitOwnerUnsupportedReason, UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::type_checking::UnitItemId;

fn choice_source(suffix: &str) -> String {
    format!(
        "fun choice_{suffix}(choice_flag_{suffix}: Boolean): Unit {{
val chosen_{suffix} = if (choice_flag_{suffix}) \"left_{suffix}\" else (\"right_{suffix}\")
}}"
    )
}

fn capture_source(suffix: &str) -> String {
    // The existing asymmetric-release shape: formation is top-level, never inside If.
    format!(
        "fun capture_{suffix}(capture_flag_{suffix}: Boolean, own source_{suffix}: String): Unit {{
val callback_{suffix} = move {{ source_{suffix} }}
if (capture_flag_{suffix}) {{ val moved_{suffix} = callback_{suffix} }} else {{ val number_{suffix} = 1 }}
}}"
    )
}

fn assert_choice(
    fixture: &Fixture,
    observation: &UnitOwnerObservation,
    suffix: &str,
) -> Vec<UnitOwnerId> {
    let control = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(
            0,
            &format!("if (choice_flag_{suffix}) \"left_{suffix}\" else (\"right_{suffix}\")"),
        ),
    );
    let declarations = fixture.parsed[0]
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| match node.payload() {
            crate::parser::Statement::LocalVariable { declaration }
                if matches!(
                    fixture.parsed[0].ast().items().get(*declaration).unwrap().payload(),
                    crate::parser::Item::Variable { initializer, .. }
                        if *initializer == control.expression()
                ) =>
            {
                Some(UnitStatementId::new(fixture.source(0), id))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [declaration] = declarations.as_slice() else {
        panic!("one real declaration initialized by this callable's If");
    };
    assert_eq!(
        observation.status(),
        UnitOwnerObservationStatus::Unsupported {
            point: UnitDropPoint::AfterExpression(control),
            reason: UnitOwnerUnsupportedReason::Syntax,
        }
    );
    assert!(observation.definitions().is_empty() && observation.events().is_empty());
    let graph = observation.value_observation_v1().unwrap();
    assert!(graph.is_consistent());
    assert_eq!(graph.producers().callable(), observation.callable());
    let choice = graph.choice().unwrap();
    assert_eq!(choice.id().control(), control);
    assert_eq!(graph.producers().definitions().len(), 2);
    for (edge, literal) in [format!("\"left_{suffix}\""), format!("\"right_{suffix}\"")]
        .iter()
        .enumerate()
    {
        let owner = choice.inputs()[edge].owner();
        assert_eq!(
            graph
                .producers()
                .definition(owner.clone())
                .unwrap()
                .origin(),
            UnitOwnerOrigin::String(UnitExpressionId::new(
                fixture.source(0),
                fixture.expression(0, literal)
            ))
        );
        assert!(observation.definition(owner).is_none());
    }
    let chosen = fixture.symbol(0, &format!("chosen_{suffix}"));
    let [
        Event::Bind(symbol, initializer, Value::Choice(bound)),
        Event::Drop(point, dropped, Value::Choice(released)),
    ] = graph.suffix()
    else {
        panic!("this callable has exactly its own binding and cleanup");
    };
    assert_eq!(*symbol, chosen);
    assert_eq!(*dropped, chosen);
    assert_eq!(*initializer, control);
    assert_eq!(*point, UnitDropPoint::AfterStatement(*declaration));
    assert_eq!(bound, choice.id());
    assert_eq!(released, choice.id());
    graph
        .producers()
        .definitions()
        .iter()
        .map(|definition| definition.id())
        .collect()
}

fn assert_capture(
    fixture: &Fixture,
    observation: &UnitOwnerObservation,
    suffix: &str,
) -> Vec<UnitOwnerId> {
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    assert!(observation.value_observation_v1().is_none());
    let lambda = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, &format!("move {{ source_{suffix} }}")),
    );
    let source = fixture.symbol(0, &format!("source_{suffix}"));
    assert_eq!(observation.definitions().len(), 2);
    let formations = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::Capture {
                expression,
                environment,
                inputs,
            } => Some((*expression, environment, inputs)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(expression, environment, inputs)] = formations.as_slice() else {
        panic!("one real top-level capture in this callable");
    };
    assert_eq!(*expression, lambda);
    assert_eq!(
        observation
            .definition((*environment).clone())
            .unwrap()
            .origin(),
        UnitOwnerOrigin::Closure(lambda)
    );
    let [(captured_source, captured)] = inputs.as_slice() else {
        panic!("exactly this function's owned String parameter");
    };
    assert_eq!(*captured_source, source);
    assert_eq!(
        observation.definition(captured.clone()).unwrap().origin(),
        UnitOwnerOrigin::Parameter(source)
    );
    let expected_control = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(
            0,
            &format!(
                "if (capture_flag_{suffix}) {{ val moved_{suffix} = callback_{suffix} }} else {{ val number_{suffix} = 1 }}"
            ),
        ),
    );
    let controls = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::If { control, .. } => Some(*control),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(controls, vec![expected_control]);
    observation
        .definitions()
        .iter()
        .map(|definition| definition.id())
        .collect()
}

#[test]
fn alternating_choice_and_capture_callables_keep_transactions_and_capabilities_local() {
    let functions = [
        (true, "a", choice_source("a")),
        (false, "a", capture_source("a")),
        (true, "b", choice_source("b")),
        (false, "b", capture_source("b")),
    ];
    for order in [[0, 1, 2, 3], [1, 0, 3, 2]] {
        let text = order.map(|index| functions[index].2.as_str()).join("\n");
        let fixture = Fixture::new(&[("root.ko", &text)]);
        assert_eq!(fixture.parsed[0].roots().len(), 4);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.clone().validate().is_ok());
        assert_eq!(
            owned.owner_observations().len(),
            6,
            "four functions and two lambdas"
        );
        let observations = fixture.parsed[0]
            .roots()
            .iter()
            .map(|item| {
                let callable =
                    UnitOwnerCallable::Function(UnitItemId::new(fixture.source(0), *item));
                let matches = owned
                    .owner_observations()
                    .iter()
                    .filter(|observation| observation.callable() == callable)
                    .collect::<Vec<_>>();
                let [observation] = matches.as_slice() else {
                    panic!("one observation for this exact AST function");
                };
                *observation
            })
            .collect::<Vec<_>>();
        assert_eq!(observations.len(), 4);
        let mut tokens = Vec::new();
        for (position, index) in order.into_iter().enumerate() {
            let observation = observations[position];
            assert_eq!(
                observation.callable(),
                UnitOwnerCallable::Function(UnitItemId::new(
                    fixture.source(0),
                    fixture.parsed[0].roots()[position],
                ))
            );
            let (is_choice, suffix, _) = &functions[index];
            let token = if *is_choice {
                assert_choice(&fixture, observation, suffix)
            } else {
                assert_capture(&fixture, observation, suffix)
            };
            tokens.push(token);
        }
        for (index, observation) in observations.iter().enumerate() {
            for (other, owners) in tokens.iter().enumerate() {
                if index != other {
                    for token in owners {
                        assert!(observation.definition(token.clone()).is_none());
                        if let Some(graph) = observation.value_observation_v1() {
                            assert!(graph.producers().definition(token.clone()).is_none());
                        }
                    }
                }
            }
        }
        let lambdas = owned
            .owner_observations()
            .iter()
            .filter_map(|observation| match observation.callable() {
                UnitOwnerCallable::Lambda(lambda) => Some((observation, lambda)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(lambdas.len(), 2);
        for suffix in ["a", "b"] {
            let expected = UnitExpressionId::new(
                fixture.source(0),
                fixture.expression(0, &format!("move {{ source_{suffix} }}")),
            );
            assert_eq!(
                lambdas
                    .iter()
                    .filter(|(_, lambda)| *lambda == expected)
                    .count(),
                1
            );
        }
        for (observation, lambda) in lambdas {
            assert_eq!(
                observation.status(),
                UnitOwnerObservationStatus::Unsupported {
                    point: UnitDropPoint::LambdaEntry(lambda),
                    reason: UnitOwnerUnsupportedReason::Callable,
                }
            );
            assert!(observation.definitions().is_empty() && observation.events().is_empty());
            assert!(observation.value_observation_v1().is_none());
        }
        let parts = fixture.parts(&owned);
        compare(&fixture, &parts);
    }
}
