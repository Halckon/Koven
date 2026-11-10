//! Ordinary finite source: selected owner lives through one real immutable binding.
use super::*;
use crate::ownership_checking::{UnitDropTarget, UnitOwnerReplaceBreakReturnActionV1 as A};
use crate::parser::{Expression, Item, NameMarker};

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); break }
    val forwarded = held
    return forwarded
}"#;

#[test]
fn alias_return_requires_live_selected_owner_receipt() {
    let fixture = Fixture::new(&[("alias-return.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.clone().validate().is_ok());
    let boundaries = owned
        .owner_observations()
        .iter()
        .filter_map(|o| o.replace_break_observation_v1())
        .collect::<Vec<_>>();
    assert_eq!(
        boundaries.len(),
        1,
        "one-hop alias must retain a real P/Q return receipt"
    );
    let boundary = boundaries[0];
    let consumer = boundary
        .suffix_return_consumer_v1()
        .expect("actual alias Return handoff");
    assert_eq!(
        consumer.actions().len(),
        4,
        "Take root, Bind alias, Take alias, Return"
    );
    assert_eq!(
        consumer.incoming(),
        &[
            boundary.condition_false_exit().clone(),
            boundary.break_boundary().clone()
        ]
    );
    assert!(
        boundary
            .input_producers()
            .definition(consumer.incoming()[0].owner())
            .is_some()
    );
    assert!(
        boundary
            .body_producers()
            .definition(consumer.incoming()[1].owner())
            .is_some()
    );
    assert_eq!(boundary.input_producers().definitions().len(), 1);
    assert_eq!(boundary.body_producers().definitions().len(), 1);
    let bindings = fixture.parsed[0]
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Variable {
                name: NameMarker::Present(name),
                initializer,
                ..
            } if fixture.sources.slice(*name).unwrap() == "forwarded" => Some(*initializer),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [initializer] = bindings.as_slice() else {
        panic!("one real binding");
    };
    let initializer = UnitExpressionId::new(fixture.source(0), *initializer);
    let expression = fixture.expression(0, "return forwarded");
    let Expression::Return {
        value: Some(payload),
        ..
    } = fixture.parsed[0]
        .ast()
        .expressions()
        .get(expression)
        .unwrap()
        .payload()
    else {
        panic!("real explicit Return");
    };
    let payload = UnitExpressionId::new(fixture.source(0), *payload);
    let expression = UnitExpressionId::new(fixture.source(0), expression);
    let alias = fixture.local(0, "forwarded");
    assert_eq!(
        consumer.actions(),
        [
            A::Take(initializer, fixture.local(0, "held")),
            A::Bind(initializer, alias),
            A::Take(payload, alias),
            A::Return {
                expression,
                payload
            }
        ]
    );
    let drops = owned.drops().iter().map(|d| d.target()).collect::<Vec<_>>();
    assert_eq!(drops, [UnitDropTarget::Named(fixture.local(0, "previous"))]);
    compare(&fixture, &fixture.parts(&owned));
}

#[test]
fn alias_return_keeps_unproved_legal_suffixes_outside_receipt() {
    let cases = [
        SOURCE.replace("val forwarded = held", "var forwarded = held"),
        SOURCE.replace("val forwarded = held", "val forwarded = (held)"),
        SOURCE.replace("return forwarded", "return (forwarded)"),
        SOURCE.replace("return forwarded", "val second = forwarded; return second"),
        SOURCE.replace("return forwarded", "println(forwarded); return forwarded"),
        SOURCE.replace("return forwarded", "return \"other\""),
    ];
    for text in cases {
        let fixture = Fixture::new(&[("alias-neighbor.ko", &text)]);
        let owned = fixture.shared();
        assert!(
            owned.diagnostics().is_empty(),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(owned.clone().validate().is_ok());
        assert!(
            owned
                .owner_observations()
                .iter()
                .all(|o| o.replace_break_observation_v1().is_none())
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}
