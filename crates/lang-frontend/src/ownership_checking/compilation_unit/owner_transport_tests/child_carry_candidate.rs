//! Ordinary source probe for the isolated Carry candidate; no new authority.
use super::*;
use crate::parser::Statement;

const SOURCE: &str = r#"fun run(outerCount: Int, innerCount: Int): Unit {
var a = "initial a"
var b = "initial b"
var outer = outerCount
while (outer > 0) {
    var inner = innerCount
    while (inner > 0) {
        val oldChild = replace(&a, "child")
        inner = inner - 1
    }
    val oldParent = replace(&a, "parent")
    outer = outer - 1
}
}"#;

#[test]
fn ordinary_unused_carry_is_retired_before_parent_entry() {
    let fixture = Fixture::new(&[("carry.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert!(owned.is_compatible_with(&fixture.typed));
    assert!(owned.deferred().is_empty());
    let replaced = fixture.local(0, "a");
    let primitives = owned.ownership_primitives();
    assert_eq!(primitives.len(), 2);
    for primitive in primitives {
        assert_eq!(
            primitive.descriptor().kind(),
            crate::type_checking::OwnershipPrimitiveKind::Replace
        );
        let [place] = primitive.places() else {
            panic!("one actual root place")
        };
        assert!(place.is_root());
        assert_eq!(place.root(), replaced);
    }
    let carry = fixture.local(0, "b");
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(carry))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    let UnitDropPoint::AfterStatement(statement) = drops[0].point() else {
        panic!("unused root must retain its actual declaration cleanup")
    };
    assert_eq!(statement.source_unit(), carry.source_unit());
    let parsed = &fixture.parsed[0];
    let node = parsed
        .ast()
        .statements()
        .get(statement.statement())
        .unwrap();
    let Statement::LocalVariable { declaration } = node.payload() else {
        panic!("cleanup must belong to the original binding")
    };
    let item = parsed.ast().items().get(*declaration).unwrap();
    assert_eq!(
        fixture.sources.slice(item.span()).unwrap(),
        "var b = \"initial b\""
    );
    let whiles = parsed
        .ast()
        .statements()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Statement::While { .. }))
        .collect::<Vec<_>>();
    assert_eq!(whiles.len(), 2);
    assert!(
        whiles
            .iter()
            .all(|(_, loop_node)| node.span().end() <= loop_node.span().start())
    );
}

#[test]
fn ordinary_real_suffix_use_keeps_candidate_root_past_parent() {
    let text = SOURCE.strip_suffix("}\n}").unwrap().to_owned() + "}\nprintln(b)\n}";
    let fixture = Fixture::new(&[("carry-used.ko", &text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert!(owned.is_compatible_with(&fixture.typed));
    assert!(owned.deferred().is_empty());
    let carry = fixture.local(0, "b");
    let drops = owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(carry))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    let UnitDropPoint::CallReturn(call) = drops[0].point() else {
        panic!("real final shared use must retain its call-return cleanup")
    };
    assert_eq!(call.source_unit(), carry.source_unit());
    let parsed = &fixture.parsed[0];
    let call_node = parsed.ast().expressions().get(call.expression()).unwrap();
    assert_eq!(
        fixture.sources.slice(call_node.span()).unwrap(),
        "println(b)"
    );
    let whiles = parsed
        .ast()
        .statements()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Statement::While { .. }))
        .collect::<Vec<_>>();
    assert_eq!(whiles.len(), 2);
    assert!(
        whiles
            .iter()
            .all(|(_, node)| node.span().end() < call_node.span().start())
    );
    let loans = owned
        .loans()
        .iter()
        .filter(|loan| loan.call() == call)
        .collect::<Vec<_>>();
    let [loan] = loans.as_slice() else {
        panic!("one actual suffix loan")
    };
    assert_eq!(loan.kind(), crate::ownership_checking::LoanKind::Shared);
    let UnitLoanTarget::Place(place) = loan.target() else {
        panic!("same original root")
    };
    assert!(place.is_root());
    assert_eq!(place.root(), carry);
    assert_eq!(
        fixture
            .sources
            .slice(
                parsed
                    .ast()
                    .expressions()
                    .get(loan.argument().expression())
                    .unwrap()
                    .span()
            )
            .unwrap(),
        "b"
    );
    assert!(
        owned
            .owner_observations()
            .iter()
            .all(|observation| observation.nested_normal_body_v1().is_none()),
        "a real final use does not authorize the old all-root child rule"
    );
}
