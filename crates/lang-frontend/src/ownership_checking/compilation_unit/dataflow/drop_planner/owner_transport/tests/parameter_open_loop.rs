//! Real Parameter P enters the existing open loop through one consuming Name binding.
use super::*;
use crate::ownership_checking::{
    CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitOwnerCallable,
    UnitOwnerOpenLoopObservationV1, UnitOwnerOpenLoopReturnV1, UnitOwnerOrigin,
    UnitOwnerReturnActionV1 as A, UnitOwnerTransportEvent as E,
};
use crate::type_checking::UnitItemId;

const SOURCE: &str = r#"fun run(flag: Boolean, own incoming: String): String {
    var held = incoming
    while(flag) { val previous = replace(&held, "next"); return held }
    return held
}
fun entry(): Unit { val first = run(false, "outer"); val second = run(true, "outer") }
"#;

fn product<'a>(
    fixture: &Fixture,
    file: usize,
    owned: &'a CompilationUnitOwnership,
) -> &'a UnitOwnerOpenLoopReturnV1 {
    let callable = UnitOwnerCallable::Function(UnitItemId::new(
        fixture.source(file),
        fixture.parsed[file].roots()[0],
    ));
    let observation = owned
        .owner_observations()
        .iter()
        .find(|observation| observation.callable() == callable)
        .unwrap();
    let Some(UnitOwnerOpenLoopObservationV1::ReturnDelivered(product)) =
        observation.open_loop_observation_v1()
    else {
        panic!("the actual Parameter entry needs its own ReturnDelivered receipt")
    };
    assert!(observation.replace_break_observation_v1().is_none());
    assert!(observation.loop_observation_v1().is_none());
    assert!(observation.invariant_loop_observation_v1().is_none());
    assert!(observation.value_observation_v1().is_none());
    product
}

fn check_product(fixture: &Fixture, file: usize, owned: &CompilationUnitOwnership) {
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    let product = product(fixture, file, owned);
    assert!(product.is_consistent());
    let parameter = fixture.symbol(file, "incoming");
    let root = fixture.local(file, "held");
    let previous = fixture.local(file, "previous");
    let initial = UnitExpressionId::new(fixture.source(file), fixture.expression(file, "incoming"));
    assert_eq!(product.initial(), initial);
    assert_eq!(product.root(), root);
    assert_eq!(product.previous(), previous);
    let p = product.entry().owner();
    let q = product.body_return().owner();
    assert_ne!(p, q);
    assert!(product.input_producers().is_consistent());
    assert!(product.body_producers().is_consistent());
    assert_eq!(product.input_producers().definitions().len(), 1);
    assert_eq!(product.body_producers().definitions().len(), 1);
    assert_eq!(
        product
            .input_producers()
            .definition(p.clone())
            .unwrap()
            .origin(),
        UnitOwnerOrigin::Parameter(parameter)
    );
    assert_eq!(
        product
            .body_producers()
            .definition(q.clone())
            .unwrap()
            .origin(),
        UnitOwnerOrigin::String(product.argument())
    );
    assert_eq!(product.entry(), product.condition_false_exit());
    assert_eq!(product.condition_false_return().owner(), p);
    assert_eq!(
        product.prefix_events(),
        [
            E::Move {
                expression: initial,
                symbol: parameter,
                owner: p.clone()
            },
            E::Result {
                expression: initial,
                owner: p.clone()
            },
            E::Binding {
                symbol: root,
                initializer: initial,
                owner: p.clone()
            },
        ]
    );
    let body_payload = product.body_return().payload();
    assert_eq!(
        product.body_events(),
        [
            E::Result {
                expression: product.argument(),
                owner: q.clone()
            },
            E::Pending {
                call: product.call(),
                argument: product.argument(),
                owner: q.clone()
            },
            E::Replace {
                call: product.call(),
                root,
                argument: product.argument(),
                previous: p.clone(),
                incoming: q.clone()
            },
            E::Result {
                expression: product.call(),
                owner: p.clone()
            },
            E::Binding {
                symbol: previous,
                initializer: product.call(),
                owner: p.clone()
            },
            E::Drop {
                point: UnitDropPoint::AfterStatement(product.previous_statement()),
                symbol: previous,
                owner: p.clone()
            },
            E::Move {
                expression: body_payload,
                symbol: root,
                owner: q.clone()
            },
            E::Result {
                expression: body_payload,
                owner: q.clone()
            },
        ]
    );
    let body = product.body_return();
    let condition_false = product.condition_false_return();
    assert_ne!(
        body.return_expression(),
        condition_false.return_expression()
    );
    assert_ne!(body.payload(), condition_false.payload());
    for (endpoint, owner) in [(body, q), (condition_false, p)] {
        assert!(endpoint.is_consistent());
        assert_eq!(endpoint.root(), root);
        assert_eq!(
            endpoint.actions(),
            [
                A::Take {
                    expression: endpoint.payload(),
                    root,
                    owner: owner.clone()
                },
                A::Return {
                    expression: endpoint.return_expression(),
                    payload: endpoint.payload(),
                    owner
                },
            ]
        );
        assert!(owned.drops().iter().all(|drop| {
            ![
                UnitDropTarget::Named(root),
                UnitDropTarget::Named(parameter),
                UnitDropTarget::Temporary(initial),
                UnitDropTarget::Temporary(endpoint.payload()),
            ]
            .contains(&drop.target())
        }));
    }
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(previous))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("detached P has exactly one original cleanup")
    };
    assert_eq!(
        drop.point(),
        UnitDropPoint::AfterStatement(product.previous_statement())
    );
}

#[test]
fn parameter_owner_enters_open_loop_return_without_redefinition() {
    let fixture = Fixture::new(&[("parameter-open-loop.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    assert_eq!(
        owned
            .owner_observations()
            .iter()
            .filter(|observation| observation.open_loop_value.is_some())
            .count(),
        1,
        "the real parameter Move and Binding need an open ReturnDelivered product"
    );
    check_product(&fixture, 0, &owned);
}

#[test]
fn parameter_owner_keeps_source_and_analysis_catalog_identity() {
    let second = SOURCE
        .replace("fun run(", "fun another(")
        .replace("fun entry()", "fun other_entry()")
        .replace("= run(", "= another(")
        .replace("\"next\"", "\"outer\"");
    let fixture = Fixture::new(&[("first.ko", SOURCE), ("second.ko", &second)]);
    let first = fixture.shared();
    let repeated = fixture.shared();
    for file in 0..2 {
        check_product(&fixture, file, &first);
        check_product(&fixture, file, &repeated);
    }
    assert_ne!(
        product(&fixture, 0, &first).entry().owner(),
        product(&fixture, 1, &first).entry().owner()
    );
    assert_ne!(
        product(&fixture, 0, &first).entry().owner(),
        product(&fixture, 0, &repeated).entry().owner()
    );
}

#[test]
fn parameter_owner_declines_legal_neighbor_shapes() {
    for text in [
        SOURCE.replace("var held = incoming", "var held = (incoming)"),
        SOURCE.replace(
            "var held = incoming",
            "val alias = incoming; var held = alias",
        ),
        SOURCE.replace("var held = incoming", "var held = \"outer\""),
        SOURCE.replace("while(flag)", "while((flag))"),
        SOURCE.replace("while(flag)", "while(!flag)"),
        SOURCE.replace("return held }", "break }"),
        SOURCE.replace(
            "val previous = replace(&held, \"next\"); return held",
            "val local = \"local\"; break",
        ),
        SOURCE.replace("flag: Boolean", "borrow flag: Boolean"),
        SOURCE
            .replace("flag: Boolean,", "flag: Boolean, extra: Boolean,")
            .replace("run(false,", "run(false, true,")
            .replace("run(true,", "run(true, true,"),
        SOURCE
            .replace(
                "own incoming: String",
                "own incoming: String, own spare: String",
            )
            .replace("\"outer\")", "\"outer\", \"unused\")"),
    ] {
        let fixture = Fixture::new(&[("neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        let callable = UnitOwnerCallable::Function(UnitItemId::new(
            fixture.source(0),
            fixture.parsed[0].roots()[0],
        ));
        let observation = owned
            .owner_observations()
            .iter()
            .find(|observation| observation.callable() == callable)
            .unwrap();
        assert!(observation.open_loop_observation_v1().is_none(), "{text}");
        assert!(observation.replace_break_observation_v1().is_none());
        assert!(observation.loop_observation_v1().is_none());
        assert!(observation.invariant_loop_value.is_none());
        assert!(observation.value_observation_v1().is_none());
    }
}

#[test]
fn parameter_owner_does_not_enter_legacy_closed_catalog_factory() {
    let fixture = Fixture::new(&[(
        "parameter-closed.ko",
        "fun run(own incoming: String): Unit { val held = incoming }",
    )]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.deferred().is_empty());
    assert!(owned.clone().validate().is_ok());
    let [observation] = owned.owner_observations() else {
        panic!("one ordinary function")
    };
    assert!(observation.is_consistent());
    let [definition] = observation.definitions() else {
        panic!("one actual entry definition")
    };
    assert_eq!(
        definition.origin(),
        UnitOwnerOrigin::Parameter(fixture.symbol(0, "incoming"))
    );
    assert!(
        crate::ownership_checking::UnitOwnerProducerCatalogV1::from_legacy(observation).is_none()
    );
    assert!(observation.value_observation_v1().is_none());
}

#[test]
fn parameter_catalog_extension_preserves_string_loop_families() {
    let check_strings = |catalog: &crate::ownership_checking::UnitOwnerProducerCatalogV1| {
        assert!(catalog.is_consistent());
        assert!(!catalog.definitions().is_empty());
        for definition in catalog.definitions() {
            assert!(matches!(definition.origin(), UnitOwnerOrigin::String(_)));
            assert_eq!(catalog.definition(definition.id()), Some(definition));
        }
    };
    let cases = [
        r#"fun run(flag: Boolean) { val chosen = if(flag) "left" else "right" }"#,
        r#"fun run(flag: Boolean) { while(flag) { val local = "inner"; break } }"#,
        r#"fun run(flag: Boolean): String { var held = "outer"; while(flag) { val previous = replace(&held, "next"); return held }; return held }"#,
        r#"fun run(flag: Boolean): String { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; return held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; val retired = held }"#,
        r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner"; break }; val retired = held }"#,
        r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) { continue } else { continue } }; val retired = held }"#,
        r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) { continue } else { break } }; val retired = held }"#,
        r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) {} else {} }; val retired = held }"#,
        r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) {} else { continue } }; val retired = held }"#,
    ];
    for (index, text) in cases.into_iter().enumerate() {
        let fixture = Fixture::new(&[("old-family.ko", text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());
        assert!(owned.clone().validate().is_ok());
        let [observation] = owned.owner_observations() else {
            panic!("one ordinary function")
        };
        match index {
            0 => {
                let value = observation.value_observation_v1().unwrap();
                assert!(value.is_consistent());
                check_strings(value.producers());
            }
            1 => check_strings(
                observation
                    .loop_observation_v1()
                    .unwrap()
                    .body_transport()
                    .producers(),
            ),
            2 | 3 => {
                let value = observation.open_loop_observation_v1().unwrap();
                assert!(value.is_consistent());
                let prefix: &crate::ownership_checking::UnitOwnerOpenLoopPrefixV1 = match value {
                    UnitOwnerOpenLoopObservationV1::BreakLive(v) => v,
                    UnitOwnerOpenLoopObservationV1::ReturnDelivered(v) => v,
                    UnitOwnerOpenLoopObservationV1::NormalBackedge(_)
                    | UnitOwnerOpenLoopObservationV1::MixedBackedge(_)
                    | UnitOwnerOpenLoopObservationV1::ContinueBackedge(_) => {
                        panic!("old terminal shape changed")
                    }
                };
                assert_eq!(prefix.prefix_events().len(), 2);
                check_strings(prefix.input_producers());
                check_strings(prefix.body_producers());
                assert_eq!(
                    matches!(value, UnitOwnerOpenLoopObservationV1::ReturnDelivered(_)),
                    index == 2
                );
                assert_eq!(
                    observation.replace_break_observation_v1().is_some(),
                    index == 3
                );
            }
            4 => check_strings(
                observation
                    .invariant_loop_observation_v1()
                    .unwrap()
                    .input_producers(),
            ),
            5 => check_strings(
                observation
                    .invariant_break_observation_v1()
                    .unwrap()
                    .input_producers(),
            ),
            6 => check_strings(
                observation
                    .invariant_split_continue_observation_v1()
                    .unwrap()
                    .input_producers(),
            ),
            7 => check_strings(
                observation
                    .invariant_split_exit_observation_v1()
                    .unwrap()
                    .input_producers(),
            ),
            8 => check_strings(
                observation
                    .invariant_normal_join_observation_v1()
                    .unwrap()
                    .input_producers(),
            ),
            9 => check_strings(
                observation
                    .invariant_mixed_join_observation_v1()
                    .unwrap()
                    .input_producers(),
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn ci_only_parameter_open_loop_rejects_foreign_and_damaged_inputs() {
    let fixture = Fixture::new(&[("parameter-guard.ko", SOURCE)]);
    let owned = fixture.shared();
    let repeated = fixture.shared();
    let original = product(&fixture, 0, &owned);
    let foreign = product(&fixture, 0, &repeated);
    assert!(
        original
            .input_producers()
            .definition(foreign.entry().owner())
            .is_none()
    );
    assert!(
        original
            .input_producers()
            .definition(original.body_return().owner())
            .is_none()
    );
    for damage in 0..4 {
        let mut value = original.clone();
        match damage {
            0 => {
                value.prefix.prefix.remove(0);
            }
            1 => {
                value.prefix.inputs.definitions[0].origin = UnitOwnerOrigin::String(value.initial())
            }
            2 => {
                value.prefix.inputs.definitions[0].origin = UnitOwnerOrigin::Parameter(value.root())
            }
            _ => value.prefix.inputs = foreign.input_producers().clone(),
        }
        assert!(!value.is_consistent());
    }
}
