//! Ordinary finite source requires a closed selected old-value Return receipt.
use super::*;

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); break }
    return replace(&held, "last")
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn suffix_replace_return_requires_closed_path_receipt() {
    let fixture = Fixture::new(&[("suffix-replace-return.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.clone().validate().is_ok());
    let boundaries = owned
        .owner_observations()
        .iter()
        .filter_map(|observation| observation.replace_break_observation_v1())
        .collect::<Vec<_>>();
    assert_eq!(
        boundaries.len(),
        1,
        "real selected old payload requires a closed path receipt"
    );
    use crate::ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitOwnerOrigin, UnitOwnerReplaceBreakReturnActionV1 as A,
    };
    let boundary = boundaries[0];
    let consumer = boundary.suffix_return_consumer_v1().unwrap();
    assert_eq!(
        consumer.incoming(),
        &[
            boundary.condition_false_exit().clone(),
            boundary.break_boundary().clone()
        ]
    );
    assert_eq!(boundary.input_producers().definitions().len(), 1);
    assert_eq!(boundary.body_producers().definitions().len(), 1);
    let catalog = consumer.replacement_producers().unwrap();
    let [definition] = catalog.definitions() else {
        panic!("one real R producer");
    };
    let argument = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "\"last\""));
    let call = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, "replace(&held, \"last\")"),
    );
    let expression = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, "return replace(&held, \"last\")"),
    );
    let root = fixture.local(0, "held");
    let owner = definition.id();
    assert_eq!(definition.origin(), UnitOwnerOrigin::String(argument));
    assert_eq!(
        catalog.definition(owner.clone()).unwrap().origin(),
        definition.origin()
    );
    assert_eq!(
        consumer.actions(),
        [
            A::Create(argument, owner.clone()),
            A::Pending(call, argument, owner.clone()),
            A::Replace(call, root, owner.clone()),
            A::Drop(UnitDropPoint::CallReturn(call), root, owner),
            A::Result(call),
            A::Return {
                expression,
                payload: call
            }
        ]
    );
    let drops = owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one actual installed root cleanup");
    };
    assert_eq!(drop.point(), UnitDropPoint::CallReturn(call));
    assert_eq!(
        drop.value_origin(),
        fixture.parsed[0]
            .ast()
            .expressions()
            .get(call.expression())
            .unwrap()
            .span()
    );
}
