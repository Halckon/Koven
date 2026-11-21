//! Full-source formation identity, not a complete callback lifecycle capability.
use super::*;
use crate::ownership_checking::{
    UnitOwnerCallable, UnitOwnerObservationStatus, UnitOwnerTransportEvent as Event,
};

#[test]
fn callback_formation_reads_actual_replacement_owners_with_invocation_suffix() {
    for capture in ["move { a + old }", "{ a + old }"] {
        let text = format!(
            "fun run(): Unit {{\nvar a = \"old\"\nval old = replace(&a, \"new\")\nval callback = {capture}\ncallback()\n}}"
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.clone().validate().is_ok());
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.owner_hook_probe = Some(Vec::new());
            let planner = planner.run().unwrap();
            let observation = planner
                .owner_observations
                .iter()
                .find(|o| matches!(o.callable(), UnitOwnerCallable::Function(_)))
                .unwrap();
            assert!(matches!(
                observation.status(),
                UnitOwnerObservationStatus::Unsupported { .. }
            ));
            assert!(observation.definitions().is_empty());
            assert!(observation.events().is_empty());
            let events = planner.owner_hook_probe.as_ref().unwrap();
            let replacements = events
                .iter()
                .enumerate()
                .filter_map(|(position, event)| {
                    if let Event::Replace {
                        call,
                        root,
                        argument,
                        previous,
                        incoming,
                    } = event
                    {
                        Some((position, *call, *root, *argument, previous, incoming))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let [(replace_position, call, root, argument, previous, incoming)] =
                replacements.as_slice()
            else {
                panic!("one actual replacement");
            };
            assert_eq!(*root, fixture.symbol(0, "a"));
            assert_eq!(
                *call,
                UnitExpressionId::new(
                    fixture.source(0),
                    fixture.expression(0, "replace(&a, \"new\")")
                )
            );
            assert_eq!(
                *argument,
                UnitExpressionId::new(fixture.source(0), fixture.expression(0, "\"new\""))
            );
            assert_ne!(previous, incoming);
            let captures = events
                .iter()
                .enumerate()
                .filter_map(|(position, event)| match event {
                    Event::Capture {
                        expression,
                        environment,
                        inputs,
                    } if capture.starts_with("move") => {
                        Some((position, expression, environment, inputs))
                    }
                    Event::SharedCapture {
                        expression,
                        environment,
                        inputs,
                    } if !capture.starts_with("move") => {
                        Some((position, expression, environment, inputs))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(capture_position, expression, environment, inputs)] = captures.as_slice() else {
                panic!("one actual capture formation");
            };
            assert!(replace_position < capture_position);
            assert_eq!(
                **expression,
                UnitExpressionId::new(fixture.source(0), fixture.expression(0, capture))
            );
            assert_eq!(inputs.len(), 2);
            for (symbol, owner) in [
                (fixture.symbol(0, "a"), *incoming),
                (fixture.symbol(0, "old"), *previous),
            ] {
                assert_eq!(
                    inputs
                        .iter()
                        .filter(|(s, o)| *s == symbol && o == owner)
                        .count(),
                    1
                );
                assert_ne!(*environment, owner);
            }
        });
    }
}
