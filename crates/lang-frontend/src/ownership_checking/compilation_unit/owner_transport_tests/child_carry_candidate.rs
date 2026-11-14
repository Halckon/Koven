//! Ordinary source probe for the isolated Carry candidate; no new authority.
use super::*;
mod counter;
mod model;
mod shape;
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

/// Necessary source evidence only: not a body whitelist or a Carry permission.
/// All identities come from one compatible analysis, never root names or ordinals.
#[derive(Debug, PartialEq, Eq)]
struct CarrySourceEvidence {
    replaced: UnitSymbolId,
    carried: UnitSymbolId,
    suffix: UnitExpressionId,
}

fn extract_source_evidence(
    fixture: &Fixture,
    owned: &CompilationUnitOwnership,
) -> Option<CarrySourceEvidence> {
    if fixture.parsed.len() != 1
        || !owned.is_compatible_with(&fixture.typed)
        || owned.clone().validate().is_err()
        || !owned.deferred().is_empty()
    {
        return None;
    }
    let source = fixture.source(0);
    let ast = fixture.parsed[0].ast();
    // Reject additional callable scopes instead of inferring ownership from spans.
    let functions = ast
        .items()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), crate::parser::Item::Function { .. }))
        .collect::<Vec<_>>();
    if functions.len() != 1
        || ast
            .expressions()
            .iter()
            .any(|(_, node)| matches!(node.payload(), crate::parser::Expression::Lambda { .. }))
    {
        return None;
    }
    let function = functions[0].1.span();
    let mut loops = ast
        .statements()
        .iter()
        .filter_map(|(_, node)| {
            matches!(node.payload(), Statement::While { .. }).then_some(node.span())
        })
        .collect::<Vec<_>>();
    loops.sort_by_key(|span| span.start());
    let [parent, child] = loops.as_slice() else {
        return None;
    };
    if !(function.start() < parent.start()
        && parent.end() < function.end()
        && parent.start() < child.start()
        && child.end() < parent.end())
    {
        return None;
    }
    let primitives = owned.ownership_primitives();
    if primitives.len() != 2 {
        return None;
    }
    let mut replaced = None;
    let mut in_child = 0;
    let mut after_child = 0;
    for primitive in primitives {
        let expression = primitive.descriptor().expression();
        if expression.source_unit() != source
            || primitive.descriptor().kind()
                != crate::type_checking::OwnershipPrimitiveKind::Replace
        {
            return None;
        }
        let [place] = primitive.places() else {
            return None;
        };
        if !place.is_root() || place.root().source_unit() != source {
            return None;
        }
        if replaced.is_some_and(|root| root != place.root()) {
            return None;
        }
        replaced = Some(place.root());
        let span = ast.expressions().get(expression.expression()).ok()?.span();
        if child.start() < span.start() && span.end() < child.end() {
            in_child += 1;
        } else if child.end() < span.start() && span.end() < parent.end() {
            after_child += 1;
        } else {
            return None;
        }
    }
    if in_child != 1 || after_child != 1 {
        return None;
    }
    let replaced = replaced?;
    let suffix_loans = owned
        .loans()
        .iter()
        .filter(|loan| {
            loan.call().source_unit() == source
                && ast
                    .expressions()
                    .get(loan.call().expression())
                    .is_ok_and(|node| {
                        parent.end() < node.span().start() && node.span().end() < function.end()
                    })
        })
        .collect::<Vec<_>>();
    let [loan] = suffix_loans.as_slice() else {
        return None;
    };
    if loan.kind() != crate::ownership_checking::LoanKind::Shared
        || loan.argument().source_unit() != source
    {
        return None;
    }
    let UnitLoanTarget::Place(place) = loan.target() else {
        return None;
    };
    let carried = place.root();
    if !place.is_root() || carried == replaced || carried.source_unit() != source {
        return None;
    }
    let resolution = fixture.names.names().source_units()[source.index()].resolution();
    for root in [replaced, carried] {
        let span = resolution.symbols().get(root.symbol().index())?.span();
        let declarations = ast.items().iter().filter(|(_, node)| matches!(node.payload(), crate::parser::Item::Variable { name: crate::parser::NameMarker::Present(marker), .. } if *marker == span)).collect::<Vec<_>>();
        let [(_, declaration)] = declarations.as_slice() else {
            return None;
        };
        if !(functions[0].1.span().start() < declaration.span().start()
            && declaration.span().end() < parent.start())
        {
            return None;
        }
    }
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(carried))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        return None;
    };
    if drop.point() != UnitDropPoint::CallReturn(loan.call()) {
        return None;
    }
    Some(CarrySourceEvidence {
        replaced,
        carried,
        suffix: loan.call(),
    })
}

#[test]
fn ordinary_source_evidence_rejects_missing_premises() {
    let used = SOURCE.strip_suffix("}\n}").unwrap().to_owned() + "}\nprintln(b)\n}";
    let fixture = Fixture::new(&[("carry.ko", &used)]);
    let owned = fixture.shared();
    let evidence = extract_source_evidence(&fixture, &owned).unwrap();
    assert_eq!(evidence.replaced, fixture.local(0, "a"));
    assert_eq!(evidence.carried, fixture.local(0, "b"));
    assert_eq!(
        evidence.suffix,
        UnitExpressionId::new(fixture.source(0), fixture.expression(0, "println(b)"))
    );
    for text in [
        SOURCE.to_owned(),
        used.replace("println(b)", "println(a)"),
        used.replace("replace(&a, \"parent\")", "replace(&b, \"parent\")"),
        used.replace("println(b)", "println(b)\nprintln(b)"),
        used.replace("var b = \"initial b\"\n", "")
            .replace("println(b)", "var b = \"initial b\"\nprintln(b)"),
    ] {
        let rejected = Fixture::new(&[("reject.ko", &text)]);
        assert!(extract_source_evidence(&rejected, &rejected.shared()).is_none());
    }
}

#[test]
fn ordinary_closed_source_shape_pairs_old_results() {
    let used = SOURCE.strip_suffix("}\n}").unwrap().to_owned() + "}\nprintln(b)\n}";
    let fixture = Fixture::new(&[("shape.ko", &used)]);
    let owned = fixture.shared();
    let evidence = extract_source_evidence(&fixture, &owned).unwrap();
    assert!(shape::matches(&fixture, &owned, &evidence).is_some());
    for text in [
        used.replace("var b =", "val b ="),
        used.replace("val oldChild", "var oldChild"),
        used.replace("inner = inner - 1", "println(oldChild)\ninner = inner - 1"),
        used.replace("var b = \"initial b\"", "var b = \"initial b\" + \"extra\""),
    ] {
        let fixture = Fixture::new(&[("reject.ko", &text)]);
        let owned = fixture.shared();
        let accepted = extract_source_evidence(&fixture, &owned)
            .and_then(|evidence| shape::matches(&fixture, &owned, &evidence));
        assert!(accepted.is_none());
    }
}

#[test]
fn ordinary_counter_relationships_are_not_name_or_scalar_shape_guesses() {
    let used = SOURCE.strip_suffix("}\n}").unwrap().to_owned() + "}\nprintln(b)\n}";
    let renamed = used
        .replace("outer", "remaining")
        .replace("inner", "nested");
    let fixture = Fixture::new(&[("rename.ko", &renamed)]);
    let owned = fixture.shared();
    let evidence = extract_source_evidence(&fixture, &owned).unwrap();
    assert!(shape::matches(&fixture, &owned, &evidence).is_some());
    for text in [
        used.replace("inner > 0", "outer > 0"),
        used.replace("inner = inner - 1", "outer = outer - 1"),
        used.replace("inner = inner - 1", "inner = outer - 1"),
        used.replace("inner > 0", "inner >= 0"),
        used.replace("inner - 1", "inner + 1"),
        used.replace("inner - 1", "inner - 2"),
        used.replace("var inner = innerCount", "var inner = outerCount"),
    ] {
        let fixture = Fixture::new(&[("counter-reject.ko", &text)]);
        let owned = fixture.shared();
        let evidence = extract_source_evidence(&fixture, &owned)
            .expect("counter variant retains necessary owner evidence");
        assert!(shape::matches(&fixture, &owned, &evidence).is_none());
    }
}

#[test]
fn ordinary_recursive_suffix_is_not_the_bound_line_output_target() {
    let text = SOURCE.strip_suffix("}\n}").unwrap().to_owned() + "}\nrun(0, 0, b)\n}";
    let text = text.replace("innerCount: Int)", "innerCount: Int, input: String)");
    let fixture = Fixture::new(&[("target-reject.ko", &text)]);
    let owned = fixture.shared();
    let call = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "run(0, 0, b)"));
    assert!(!fixture.typed.types().call(call).unwrap().prints_line());
    assert!(shape::suffix_target(&fixture, call).is_none());
    assert!(
        extract_source_evidence(&fixture, &owned)
            .and_then(|evidence| shape::matches(&fixture, &owned, &evidence))
            .is_none()
    );
}

#[test]
fn ordinary_final_root_cleanup_uses_actual_parent_exit_and_suffix_return() {
    let text = SOURCE.strip_suffix("}\n}").unwrap().to_owned() + "}\nprintln(b)\n}";
    let fixture = Fixture::new(&[("cleanup.ko", &text)]);
    let owned = fixture.shared();
    let evidence = extract_source_evidence(&fixture, &owned).unwrap();
    assert!(shape::matches(&fixture, &owned, &evidence).is_some());
    let ast = fixture.parsed[0].ast();
    let mut whiles = ast
        .statements()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Statement::While { .. }))
        .collect::<Vec<_>>();
    whiles.sort_by_key(|(_, node)| node.span().start());
    assert_eq!(whiles.len(), 2);
    let parent = UnitStatementId::new(fixture.source(0), whiles[0].0);
    for (root, point) in [
        (evidence.replaced, UnitDropPoint::LoopExit(parent)),
        (evidence.carried, UnitDropPoint::CallReturn(evidence.suffix)),
    ] {
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(root))
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), point);
    }
    assert!(
        owned
            .owner_observations()
            .iter()
            .all(|observation| observation.nested_normal_body_v1().is_none())
    );
}
