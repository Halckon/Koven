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
