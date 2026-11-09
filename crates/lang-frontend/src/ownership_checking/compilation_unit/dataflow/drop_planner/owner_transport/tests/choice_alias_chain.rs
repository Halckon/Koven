//! Two actual named takes of one Choice, with no intermediate cleanup.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerObservationStatus, UnitOwnerUnsupportedReason,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::parser::{Expression, Item, Statement};
use crate::type_checking::UnitStatementId;

fn binding(fixture: &Fixture, symbol: UnitSymbolId) -> (UnitDropPoint, Vec<UnitExpressionId>) {
    let ast = fixture.parsed[0].ast();
    let resolution = fixture.names.names().source_units()[fixture.source(0).index()].resolution();
    let (statement, mut inner) = ast
        .statements()
        .iter()
        .find_map(|(id, node)| {
            let Statement::LocalVariable { declaration } = node.payload() else {
                return None;
            };
            let Item::Variable {
                name, initializer, ..
            } = ast.items().get(*declaration).unwrap().payload()
            else {
                return None;
            };
            let crate::parser::NameMarker::Present(span) = name else {
                return None;
            };
            (resolution.symbols()[symbol.symbol().index()].span() == *span)
                .then_some((id, *initializer))
        })
        .unwrap();
    let mut ids = vec![UnitExpressionId::new(fixture.source(0), inner)];
    while let Expression::Group { expression } = ast.expressions().get(inner).unwrap().payload() {
        inner = *expression;
        ids.push(UnitExpressionId::new(fixture.source(0), inner));
    }
    ids.reverse();
    (
        UnitDropPoint::AfterStatement(UnitStatementId::new(fixture.source(0), statement)),
        ids,
    )
}

fn assert_chain(source: &str, control: &str) {
    assert_chain_names(source, control, "forwarded", "again");
}

fn assert_chain_names(source: &str, control: &str, middle: &str, last: &str) {
    let text = format!("{source}\nfun sibling(): Unit {{ val safe = \"safe\" }}");
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty(), "{text}");
    assert!(owned.clone().validate().is_ok(), "{text}");
    let unit_control = UnitExpressionId::new(fixture.source(0), fixture.expression(0, control));
    let legacy = &owned.owner_observations()[0];
    assert_eq!(
        legacy.status(),
        UnitOwnerObservationStatus::Unsupported {
            point: UnitDropPoint::AfterExpression(unit_control),
            reason: UnitOwnerUnsupportedReason::Syntax,
        }
    );
    assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
    let graph = legacy
        .value_observation_v1()
        .unwrap_or_else(|| panic!("two real alias takes: {text}"));
    choice_branch_local::assert_ast_edges(&fixture, control, &graph);
    assert_eq!(graph.producers().definitions().len(), 2);
    let chosen = fixture.symbol(0, "chosen");
    let forwarded = fixture.symbol(0, middle);
    let again = fixture.symbol(0, last);
    let (_, result) = binding(&fixture, chosen);
    let (_, first) = binding(&fixture, forwarded);
    let (point, second) = binding(&fixture, again);
    assert_eq!(result[0], unit_control);
    for (ids, name) in [(&first, "chosen"), (&second, middle)] {
        let node = fixture.parsed[0]
            .ast()
            .expressions()
            .get(ids[0].expression())
            .unwrap();
        assert!(matches!(node.payload(), Expression::Name));
        assert_eq!(fixture.sources.slice(node.span()).unwrap(), name);
    }
    let value = Value::Choice(graph.choice().unwrap().id().clone());
    let mut expected = Vec::new();
    for pair in result.windows(2) {
        expected.push(Event::Forward(pair[0], pair[1], value.clone()));
    }
    expected.push(Event::Bind(chosen, *result.last().unwrap(), value.clone()));
    for (source, target, ids) in [(chosen, forwarded, &first), (forwarded, again, &second)] {
        expected.push(Event::Move(ids[0], source, value.clone()));
        for pair in ids.windows(2) {
            expected.push(Event::Forward(pair[0], pair[1], value.clone()));
        }
        expected.push(Event::Bind(target, *ids.last().unwrap(), value.clone()));
    }
    expected.push(Event::Drop(point, again, value));
    assert_eq!(graph.suffix().len(), expected.len());
    for (actual, expected) in graph.suffix().iter().zip(&expected) {
        let same = match (actual, expected) {
            (Event::Forward(a, b, Value::Choice(v)), Event::Forward(x, y, Value::Choice(w))) => {
                a == x && b == y && v == w
            }
            (Event::Move(a, b, Value::Choice(v)), Event::Move(x, y, Value::Choice(w))) => {
                a == x && b == y && v == w
            }
            (Event::Bind(a, b, Value::Choice(v)), Event::Bind(x, y, Value::Choice(w))) => {
                a == x && b == y && v == w
            }
            (Event::Drop(a, b, Value::Choice(v)), Event::Drop(x, y, Value::Choice(w))) => {
                a == x && b == y && v == w
            }
            _ => false,
        };
        assert!(same, "actual {actual:?}, expected {expected:?}");
    }
    assert_eq!(owned.drops().len(), 2, "only terminal alias and sibling");
    for symbol in [chosen, forwarded] {
        assert!(
            !owned
                .drops()
                .iter()
                .any(|drop| drop.target() == UnitDropTarget::Named(symbol))
        );
    }
    let drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(again))
        .collect();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].point(), point);
    assert_eq!(
        owned.owner_observations()[1].status(),
        UnitOwnerObservationStatus::Complete
    );
    assert!(
        owned.owner_observations()[1]
            .value_observation_v1()
            .unwrap()
            .choice()
            .is_none()
    );
    for definition in graph.producers().definitions() {
        assert!(legacy.definition(definition.id()).is_none());
    }
    compare(&fixture, &fixture.parts(&owned));
}

#[test]
fn actual_choice_crosses_two_aliases_without_an_intermediate_drop() {
    for condition in ["flag", "!flag", "(!flag)"] {
        for arms in [
            r#""left" else "right""#,
            r#"{ val local = "left"; ((local)) } else "right""#,
            r#""left" else { val local = "right"; (local) }"#,
            r#"{ val local = "left"; local } else { val local = "right"; local }"#,
        ] {
            let control = format!("if ({condition}) {arms}");
            for initializer in [control.clone(), format!("(({control}))")] {
                for first in ["chosen", "(chosen)", "((chosen))"] {
                    for second in ["forwarded", "(forwarded)", "((forwarded))"] {
                        assert_chain(
                            &format!(
                                "fun run(flag: Boolean): Unit {{ val chosen = {initializer}; val forwarded: String = {first}; val again = {second} }}"
                            ),
                            &control,
                        );
                    }
                }
            }
        }
    }
    // The exact source previously rejected by the single-transfer boundary test.
    assert_chain(
        "fun run(flag: Boolean): Unit { val chosen = if (flag) \"left\" else \"right\"; val forwarded = chosen; val again = forwarded }",
        "if (flag) \"left\" else \"right\"",
    );
    assert_chain(
        "fun run(flag: Boolean): Unit { val chosen = ((if (flag) \"left\" else \"right\")); val forwarded = chosen; val again = forwarded }",
        "if (flag) \"left\" else \"right\"",
    );
    assert_chain_names(
        "fun run(flag: Boolean): Unit { val chosen = if (flag) \"left\" else \"right\"; val previous = chosen; val forwarded = previous }",
        "if (flag) \"left\" else \"right\"",
        "previous",
        "forwarded",
    );
}

#[test]
fn two_aliases_keep_replacement_chains_and_unbounded_consumers_closed() {
    for body in [
        "val chosen = if (flag) \"left\" else \"right\"; val forwarded = chosen; val again = forwarded; val third = again",
        "val chosen = if (flag) \"left\" else \"right\"; var forwarded = chosen; val again = forwarded",
        "val chosen = if (flag) \"left\" else \"right\"; val forwarded = chosen; var again = forwarded",
        "val chosen = if (flag) \"left\" else \"right\"; val forwarded = chosen; val again: String? = forwarded",
        "val chosen = if (flag) \"left\" else \"right\"; val forwarded = chosen; val again = forwarded + \"suffix\"",
        "val chosen = if (flag) \"left\" else \"right\"; val forwarded = chosen; consume(forwarded)",
        "val chosen = if (flag) \"left\" else \"right\"; val forwarded = chosen; val again = move { forwarded }",
        "val chosen = if (flag) \"left\" else \"right\"; val forwarded = chosen; loop { val again = forwarded; break }",
        "var chosen = if (flag) \"left\" else \"right\"; val previous = replace(&chosen, \"new\"); val forwarded = previous; val again = forwarded",
        "var chosen = if (flag) \"left\" else \"right\"; val previous = replace(&chosen, \"new\"); val forwarded = chosen; val again = forwarded",
    ] {
        let source = format!(
            "fun run(flag: Boolean): Unit {{ {body} }}\nfun consume(own value: String): Unit {{}}"
        );
        let fixture = Fixture::new(&[("root.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{source}");
        assert!(owned.clone().validate().is_ok(), "{source}");
        let rejected = &owned.owner_observations()[0];
        assert!(rejected.value_observation_v1().is_none(), "{source}");
        assert!(rejected.definitions().is_empty() && rejected.events().is_empty());
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn alias_chain_requires_real_intermediate_binding_and_second_take() {
    let control = "if (flag) \"left\" else \"right\"";
    let source = format!(
        "fun run(flag: Boolean): Unit {{ val chosen = {control}; val forwarded = ((chosen)); val again = ((forwarded)) }}"
    );
    let fixture = Fixture::new(&[("root.ko", &source)]);
    let owned = fixture.shared();
    let chosen = fixture.symbol(0, "chosen");
    let forwarded = fixture.symbol(0, "forwarded");
    let again = fixture.symbol(0, "again");
    let (first_drop, first) = binding(&fixture, forwarded);
    let (last_drop, second) = binding(&fixture, again);
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let unit = |text| checker.unit_expression(fixture.expression(0, text));
        // Supplementary manual guard traces, not proof of actual planner ordering.
        for mutation in 0..26 {
            let mut tx = super::super::choice::ChoiceTransaction::begin(
                checker,
                checker.parsed.roots()[0],
                std::sync::Arc::new(()),
            )
            .unwrap();
            for (edge, text) in ["\"left\"", "\"right\""].into_iter().enumerate() {
                tx.string_completed(unit(text));
                tx.expression_completed(unit(text), true, true);
                tx.edge_completed(unit(control), edge, Some(unit(text)), true);
            }
            tx.expression_completed(unit(control), true, true);
            tx.bound(chosen, unit(control));
            if mutation == 1 {
                tx.moved(second[0], Some(forwarded));
            }
            if mutation == 2 {
                tx.expression_completed(second[1], true, true);
            }
            tx.moved(first[0], Some(chosen));
            for id in &first {
                tx.expression_completed(*id, true, true);
            }
            if mutation == 3 {
                tx.moved(second[0], Some(forwarded));
            }
            if mutation != 4 {
                tx.bound(forwarded, *first.last().unwrap());
            }
            if mutation == 5 {
                tx.bound(forwarded, *first.last().unwrap());
            }
            if mutation == 6 {
                tx.dropped(first_drop, forwarded);
            }
            if mutation == 7 {
                tx.dropped(first_drop, chosen);
            }
            if mutation == 8 {
                tx.expression_completed(first[1], true, true);
            }
            if mutation == 9 {
                tx.moved(first[0], Some(chosen));
            }
            if mutation == 10 {
                tx.bound(again, *second.last().unwrap());
            }
            if mutation != 11 {
                tx.moved(
                    second[0],
                    match mutation {
                        12 => None,
                        13 => Some(chosen),
                        _ => Some(forwarded),
                    },
                );
            }
            if mutation == 14 {
                tx.moved(second[0], Some(forwarded));
            }
            if mutation == 15 {
                tx.expression_completed(second[1], true, true);
            }
            for (index, id) in second.iter().enumerate() {
                if mutation == 16 && index == 1 {
                    continue;
                }
                tx.expression_completed(*id, mutation != 17, mutation != 18);
            }
            if mutation == 19 {
                tx.expression_completed(*second.last().unwrap(), true, true);
            }
            if mutation != 20 {
                tx.bound(again, *second.last().unwrap());
            }
            if mutation == 21 {
                tx.bound(again, *second.last().unwrap());
            }
            if mutation != 22 {
                tx.dropped(
                    if mutation == 23 {
                        first_drop
                    } else {
                        last_drop
                    },
                    if mutation == 24 { forwarded } else { again },
                );
            }
            if mutation == 25 {
                tx.dropped(last_drop, again);
            }
            assert_eq!(tx.finish().is_some(), mutation == 0, "mutation {mutation}");
        }
    });
}
