//! Private metadata algebra, not a claim of public support for unknown sources.
use super::{Coverage, EscapeWitnesses};
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::type_checking::UnitExpressionId;

#[test]
fn unknown_branch_keeps_other_reachable_positive_evidence_at_join() {
    let fixture = Fixture::new(&[(
        "join.ko",
        "fun run(flag: Boolean) { val f: () -> Unit = {}; var optional = if (flag) f else null }",
    )]);
    let symbol = fixture.symbol(0, "optional");
    let source = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "{}"));
    let mut known = EscapeWitnesses::default();
    known.mutable_locals.insert(symbol);
    known.known_bindings.insert(symbol);
    known.bindings.insert(symbol, [source].into());
    known.coverage.insert(source, Coverage::Known);
    let mut unknown = EscapeWitnesses::default();
    unknown.mutable_locals.insert(symbol);
    unknown.coverage.insert(source, Coverage::Unknown);
    for (mut left, right) in [(known.clone(), unknown.clone()), (unknown, known)] {
        left.merge(right);
        assert!(left.mutable_locals.contains(&symbol));
        assert!(!left.known_bindings.contains(&symbol));
        assert_eq!(left.bindings.get(&symbol), Some(&[source].into()));
        assert_eq!(left.coverage.get(&source), Some(&Coverage::Unknown));
    }
}

#[test]
fn disjoint_branch_expression_coverage_is_not_a_binding_intersection() {
    let fixture = Fixture::new(&[(
        "branches.ko",
        "fun run(flag: Boolean) { val one = {}; val two = {}; var optional = if (flag) one else two }",
    )]);
    let one = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "one"));
    let two = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "two"));
    let mut left = EscapeWitnesses::default();
    let mut right = EscapeWitnesses::default();
    left.coverage.insert(one, Coverage::Known);
    right.coverage.insert(two, Coverage::Known);
    left.merge(right);
    assert_eq!(left.coverage.get(&one), Some(&Coverage::Known));
    assert_eq!(left.coverage.get(&two), Some(&Coverage::Known));
}

#[test]
fn missing_syntactic_eligibility_is_not_unknown_value_coverage() {
    let fixture = Fixture::new(&[(
        "eligibility.ko",
        "fun run(flag:Boolean) { val f = {}; var optional=if(flag) f else null }",
    )]);
    let symbol = fixture.symbol(0, "optional");
    let source = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "{}"));
    let mut eligible = EscapeWitnesses::default();
    eligible.mutable_locals.insert(symbol);
    eligible.known_bindings.insert(symbol);
    eligible.bindings.insert(symbol, [source].into());
    for (mut left, right) in [
        (eligible.clone(), EscapeWitnesses::default()),
        (EscapeWitnesses::default(), eligible),
    ] {
        left.merge(right);
        assert!(!left.mutable_locals.contains(&symbol));
        assert!(!left.known_bindings.contains(&symbol));
        assert!(!left.bindings.contains_key(&symbol));
    }
}
