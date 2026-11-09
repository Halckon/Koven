//! One post-Choice named transfer, with source-derived events and cleanup points.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerObservationStatus, UnitOwnerUnsupportedReason,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::parser::{Expression, Item, Statement};
use crate::type_checking::UnitStatementId;

fn assert_forwarding(source: &str, control: &str, target: &str) {
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
        .unwrap_or_else(|| panic!("real post-Choice transfer: {text}"));
    choice_branch_local::assert_ast_edges(&fixture, control, &graph);
    let choice = graph.choice().unwrap();
    let chosen = fixture.symbol(0, "chosen");
    let target = fixture.symbol(0, target);
    let ast = fixture.parsed[0].ast();
    let declarations: Vec<_> = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| {
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
            let resolution =
                fixture.names.names().source_units()[fixture.source(0).index()].resolution();
            (resolution.symbols()[target.symbol().index()].span() == *span)
                .then_some((UnitStatementId::new(fixture.source(0), id), *initializer))
        })
        .collect();
    let [(statement, initializer)] = declarations.as_slice() else {
        panic!("one real final binding");
    };
    let mut chain = vec![UnitExpressionId::new(fixture.source(0), *initializer)];
    let mut inner = *initializer;
    while let Expression::Group { expression } = ast.expressions().get(inner).unwrap().payload() {
        inner = *expression;
        chain.push(UnitExpressionId::new(fixture.source(0), inner));
    }
    assert!(matches!(
        ast.expressions().get(inner).unwrap().payload(),
        Expression::Name
    ));
    assert_eq!(
        fixture
            .sources
            .slice(ast.expressions().get(inner).unwrap().span())
            .unwrap(),
        "chosen"
    );
    chain.reverse();
    let suffix = graph.suffix();
    assert_eq!(suffix.len(), chain.len() + 3);
    assert!(
        matches!(&suffix[0], Event::Bind(symbol, expression, Value::Choice(value))
        if *symbol == chosen && *expression == unit_control && value == choice.id())
    );
    assert!(
        matches!(&suffix[1], Event::Move(expression, symbol, Value::Choice(value))
        if *expression == chain[0] && *symbol == chosen && value == choice.id())
    );
    for (index, pair) in chain.windows(2).enumerate() {
        assert!(
            matches!(&suffix[index + 2], Event::Forward(from, to, Value::Choice(value))
            if *from == pair[0] && *to == pair[1] && value == choice.id())
        );
    }
    assert!(
        matches!(&suffix[chain.len() + 1], Event::Bind(symbol, expression, Value::Choice(value))
        if *symbol == target && Some(expression) == chain.last() && value == choice.id())
    );
    let drop_point = UnitDropPoint::AfterStatement(*statement);
    assert!(
        matches!(&suffix[chain.len() + 2], Event::Drop(point, symbol, Value::Choice(value))
        if *point == drop_point && *symbol == target && value == choice.id())
    );
    assert_eq!(
        owned.drops().len(),
        2,
        "only final binding and sibling are cleaned"
    );
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(chosen))
            .count(),
        0
    );
    let final_drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(target))
        .collect();
    assert_eq!(final_drops.len(), 1);
    assert_eq!(final_drops[0].point(), drop_point);
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
fn actual_choice_moves_once_through_name_and_group_to_final_binding() {
    for condition in ["flag", "!flag", "(!flag)"] {
        for arms in [
            r#""left" else "right""#,
            r#"{ val local = "left"; ((local)) } else "right""#,
            r#""left" else { val local = "right"; (local) }"#,
            r#"{ val local = "left"; local } else { val local = "right"; local }"#,
        ] {
            for initializer in ["chosen", "(chosen)", "((chosen))"] {
                let control = format!("if ({condition}) {arms}");
                let source = format!(
                    "fun run(flag: Boolean): Unit {{ val chosen = {control}; val forwarded: String = {initializer} }}"
                );
                assert_forwarding(&source, &control, "forwarded");
            }
        }
    }
}

#[test]
fn original_post_choice_rejections_become_verified_single_transfers() {
    // Preserve both original source strings when migrating their old rejection assertions.
    assert_forwarding(
        "fun run(flag: Boolean): Unit { val chosen = if (flag) \"left\" else \"right\"; val used = chosen }",
        "if (flag) \"left\" else \"right\"",
        "used",
    );
    assert_forwarding(
        "fun run(flag: Boolean): Unit { val chosen = if (!flag) \"left\" else \"right\"; val moved = chosen }",
        "if (!flag) \"left\" else \"right\"",
        "moved",
    );
}

#[test]
fn post_choice_transfer_keeps_other_consumers_and_multiple_statements_closed() {
    for suffix in [
        "var forwarded = chosen",
        "val forwarded: String? = chosen",
        "val forwarded = chosen; val again = forwarded",
        "val unrelated = \"other\"",
        "val forwarded = chosen + \"suffix\"",
        "val callback = move { chosen }",
        "(chosen)",
        "consume(chosen)",
        "if (flag) { val forwarded = chosen } else { val forwarded = chosen }",
        "loop { val forwarded = chosen; break }",
        "return",
    ] {
        let source = format!(
            "fun run(flag: Boolean): Unit {{ val chosen = if (flag) \"left\" else \"right\"; {suffix} }}\nfun consume(own value: String): Unit {{}}"
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
fn transfer_transaction_rejects_missing_or_misordered_handoff_evidence() {
    let control = "if (flag) \"left\" else \"right\"";
    let source = format!(
        "fun run(flag: Boolean): Unit {{ val chosen = {control}; val forwarded = ((chosen)) }}"
    );
    let fixture = Fixture::new(&[("root.ko", &source)]);
    let owned = fixture.shared();
    let parts = fixture.parts(&owned);
    compare_and_inspect(&fixture, &parts, |checker| {
        let unit = |text| checker.unit_expression(fixture.expression(0, text));
        let control = unit(control);
        let left = unit("\"left\"");
        let right = unit("\"right\"");
        let name = unit("chosen");
        let group = unit("(chosen)");
        let initializer = unit("((chosen))");
        let chosen = fixture.symbol(0, "chosen");
        let forwarded = fixture.symbol(0, "forwarded");
        let graph = owned.owner_observations()[0]
            .value_observation_v1()
            .unwrap();
        let Event::Drop(point, _, _) = graph.suffix().last().unwrap() else {
            panic!("actual final cleanup");
        };
        // Guard traces are separate from the real source tests above. Do not
        // substitute these hand-written transitions for planner execution proof.
        for mutation in 0..20 {
            let mut tx = super::super::choice::ChoiceTransaction::begin(
                checker,
                checker.parsed.roots()[0],
                std::sync::Arc::new(()),
            )
            .unwrap();
            tx.string_completed(left);
            tx.expression_completed(left, true, true);
            tx.edge_completed(control, 0, Some(left), true);
            tx.string_completed(right);
            tx.expression_completed(right, true, true);
            tx.edge_completed(control, 1, Some(right), true);
            tx.expression_completed(control, true, true);
            if mutation == 1 {
                tx.moved(name, Some(chosen));
            }
            if mutation != 2 {
                tx.bound(chosen, control);
            }
            if mutation == 3 {
                tx.bound(forwarded, initializer);
            }
            tx.moved(
                name,
                match mutation {
                    4 => None,
                    5 => Some(forwarded),
                    _ => Some(chosen),
                },
            );
            if mutation == 6 {
                tx.moved(name, Some(chosen));
            }
            if mutation == 7 {
                tx.expression_completed(group, true, true);
            }
            if mutation != 8 {
                tx.expression_completed(name, mutation != 9, mutation != 10);
            }
            if mutation == 11 {
                tx.expression_completed(name, true, true);
            }
            tx.expression_completed(group, true, true);
            if mutation != 12 {
                tx.expression_completed(initializer, true, true);
            }
            tx.bound(
                if mutation == 13 { chosen } else { forwarded },
                if mutation == 14 { name } else { initializer },
            );
            if mutation == 15 {
                tx.bound(forwarded, initializer);
            }
            if mutation != 16 {
                tx.dropped(
                    if mutation == 17 {
                        UnitDropPoint::AfterExpression(control)
                    } else {
                        *point
                    },
                    if mutation == 18 { chosen } else { forwarded },
                );
            }
            if mutation == 19 {
                tx.dropped(*point, forwarded);
            }
            assert_eq!(tx.finish().is_some(), mutation == 0, "mutation {mutation}");
        }
    });
}
