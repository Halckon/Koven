//! Public input routes preserve the actual seed move and each distinct loop edge.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge,
        UnitOwnerOrigin, UnitOwnerValueEventV1 as Event, UnitOwnerValueObservationV1,
        UnitOwnerValueV1 as Value, check_compilation_unit_ownership,
    },
    parser::Statement,
    source::SourceMap,
    type_checking::{
        UnitExpressionId, UnitStatementId, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

const SOURCE: &str = r#"fun run(flag: Boolean) { val seed = "outer"; val held = seed; while(flag) { val local = "inner" }; val retired = held }"#;
fn transport<'a>(
    owned: &'a CompilationUnitOwnership,
    index: usize,
    jump: &str,
) -> Option<&'a UnitOwnerValueObservationV1> {
    let o = &owned.owner_observations()[index];
    if jump == "break" {
        o.invariant_break_observation_v1()?.post_loop_transport_v1()
    } else {
        o.invariant_loop_observation_v1()?.post_loop_transport_v1()
    }
}
fn check(rooted: bool, text: &str, admitted: bool, jump: &str) {
    let second = text.replace("run", "second");
    let mut sources = SourceMap::new();
    let ids = [
        sources.add_source("a.ko", text).unwrap(),
        sources.add_source("b.ko", &second).unwrap(),
    ];
    let parsed = ids.map(|id| {
        super::parser_test_assertions::parse_file_twice(&sources, id, "named loop input")
    });
    assert!(parsed.iter().all(|p| p.diagnostics().is_empty()));
    let count = if rooted { 2 } else { 1 };
    let inputs = parsed
        .iter()
        .take(count)
        .enumerate()
        .map(|(i, p)| {
            if rooted {
                SourceUnitInput::new("root", if i == 0 { "a.ko" } else { "b.ko" }, ids[i], p)
            } else {
                SourceUnitInput::memory_document(ids[i], p)
            }
        })
        .collect::<Vec<_>>();
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let completed =
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(completed.is_same_analysis(&typed));
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(
        owned.is_compatible_with(&typed) && owned.clone().validate().is_ok(),
        "{text}"
    );
    assert_eq!(owned.owner_observations().len(), count);
    for (i, o) in owned.owner_observations().iter().enumerate() {
        assert!(
            o.definitions().is_empty()
                && o.events().is_empty()
                && o.value_observation_v1().is_none()
                && o.loop_observation_v1().is_none()
        );
        let outer = transport(&owned, i, jump);
        assert_eq!(outer.is_some(), admitted, "rooted={rooted}: {text}");
        if !admitted {
            assert!(
                o.invariant_loop_observation_v1().is_none()
                    && o.invariant_break_observation_v1().is_none()
                    && o.invariant_normal_join_observation_v1().is_none()
                    && o.invariant_mixed_join_observation_v1().is_none()
                    && o.invariant_split_continue_observation_v1().is_none()
                    && o.invariant_split_exit_observation_v1().is_none()
            );
            continue;
        }
        let outer = outer.unwrap();
        let source = index.source_units()[i].id();
        assert_eq!(index.source_units()[i].source_id(), ids[i]);
        let ast = parsed[i].ast();
        let expression = |text| {
            ast.expressions()
                .iter()
                .find_map(|(id, node)| {
                    (sources.slice(node.span()).unwrap() == text)
                        .then_some(UnitExpressionId::new(source, id))
                })
                .unwrap()
        };
        let symbol = |name| {
            names.names().source_units()[source.index()]
                .resolution()
                .symbols()
                .iter()
                .find(|s| s.name() == name)
                .unwrap()
                .id()
        };
        let initializer = expression("\"outer\"");
        let alias = expression("seed");
        let used = expression("held");
        let suffix = ast
            .statements()
            .iter()
            .find_map(|(id, node)| {
                sources
                    .slice(node.span())
                    .unwrap()
                    .starts_with("val retired")
                    .then_some(UnitStatementId::new(source, id))
            })
            .unwrap();
        let (statement, condition, body) = ast
            .statements()
            .iter()
            .find_map(|(id, node)| {
                if let Statement::While {
                    condition, body, ..
                } = node.payload()
                {
                    Some((
                        UnitStatementId::new(source, id),
                        UnitExpressionId::new(source, *condition),
                        UnitStatementId::new(source, *body),
                    ))
                } else {
                    None
                }
            })
            .unwrap();
        let (entry, body_graph, catalog, edges) = if jump == "break" {
            assert!(o.invariant_loop_observation_v1().is_none());
            let b = o.invariant_break_observation_v1().unwrap();
            assert!(b.is_consistent());
            assert_eq!(
                (b.statement(), b.condition(), b.body()),
                (statement, condition, body)
            );
            assert_eq!(b.entry(), b.body_completion());
            assert_eq!(b.entry(), b.exit());
            assert_eq!(b.break_boundary(), Some((expression(jump), b.entry())));
            (
                b.entry(),
                b.body_transport(),
                b.input_producers(),
                b.edges(),
            )
        } else {
            assert!(o.invariant_break_observation_v1().is_none());
            let b = o.invariant_loop_observation_v1().unwrap();
            assert!(b.is_consistent());
            assert_eq!(
                (b.statement(), b.condition(), b.body()),
                (statement, condition, body)
            );
            assert_eq!(b.entry(), b.backedge());
            assert_eq!(b.entry(), b.exit());
            assert_eq!(
                b.continue_boundary(),
                (!jump.is_empty()).then(|| (expression(jump), b.entry()))
            );
            (
                b.entry(),
                b.body_transport(),
                b.input_producers(),
                b.edges(),
            )
        };
        let completion = match jump {
            "continue" => {
                Edge::ContinueBackedge(UnitDropPoint::ControlTransfer(expression(jump)), condition)
            }
            "break" => Edge::BreakExit(
                UnitDropPoint::ControlTransfer(expression(jump)),
                UnitDropPoint::LoopExit(statement),
            ),
            _ => Edge::Backedge(UnitDropPoint::AfterStatement(body), condition),
        };
        assert_eq!(
            edges,
            [
                Edge::EnterBody(condition, body),
                completion,
                Edge::ConditionFalseExit(condition, UnitDropPoint::LoopExit(statement))
            ]
        );
        assert!(outer.is_consistent() && outer.choice().is_none() && outer.suffix().is_empty());
        assert_eq!(outer.producers().definitions().len(), 1);
        let [
            Event::Create(created, owner),
            Event::Bind(first, bound, Value::Definition(first_owner)),
            Event::Move(moved_at, moved_from, Value::Definition(moved)),
            Event::Bind(input, received, Value::Definition(input_owner)),
            Event::Move(used_at, used_from, Value::Definition(used_owner)),
            Event::Bind(output, installed_at, Value::Definition(installed)),
            Event::Drop(point, dropped, Value::Definition(consumed)),
        ] = outer.prefix()
        else {
            panic!("exact seven actual events")
        };
        assert_eq!(
            (*created, first.symbol(), *bound),
            (initializer, symbol("seed"), initializer)
        );
        assert_eq!(
            (*moved_at, moved_from.symbol(), input.symbol(), *received),
            (alias, symbol("seed"), symbol("held"), alias)
        );
        assert_eq!(
            (*used_at, used_from.symbol(), output.symbol(), *installed_at),
            (used, symbol("held"), symbol("retired"), used)
        );
        assert_eq!(
            (*point, dropped.symbol()),
            (UnitDropPoint::AfterStatement(suffix), symbol("retired"))
        );
        assert_eq!(entry.symbol(), *input);
        for symbol in [first, moved_from, input, used_from, output, dropped] {
            assert_eq!(symbol.source_unit(), source);
        }
        for token in [
            owner,
            first_owner,
            moved,
            input_owner,
            used_owner,
            installed,
            consumed,
        ] {
            assert_eq!(*token, entry.owner());
            assert_eq!(
                outer
                    .producers()
                    .definition(token.clone())
                    .unwrap()
                    .origin(),
                UnitOwnerOrigin::String(initializer)
            );
            assert!(
                catalog.definition(token.clone()).is_some()
                    && body_graph.producers().definition(token.clone()).is_none()
            );
            for other in 0..count {
                if other != i {
                    assert!(
                        transport(&owned, other, jump)
                            .unwrap()
                            .producers()
                            .definition(token.clone())
                            .is_none()
                    );
                }
            }
        }
        assert!(
            body_graph
                .producers()
                .definitions()
                .iter()
                .all(|d| outer.producers().definition(d.id()).is_none())
        );
        assert_eq!(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(*output))
                .count(),
            1
        );
        assert!(
            owned
                .drops()
                .iter()
                .any(|d| d.target() == UnitDropTarget::Named(*output) && d.point() == *point)
        );
        for symbol in [first, input] {
            assert!(
                owned
                    .drops()
                    .iter()
                    .all(|d| d.target() != UnitDropTarget::Named(*symbol))
            );
        }
    }
}

#[test]
fn named_loop_input_matches_both_public_routes() {
    for condition in ["flag", "(flag)", "!flag", "(!flag)", "!(!flag)"] {
        for result in ["", ": Unit"] {
            for body in [
                r#"val local = "inner""#,
                r#"val local = "inner"; val alias = (local)"#,
                r#"var root = "old"; val incoming = "next"; val old = replace(&root, incoming)"#,
                r#"var left = "left"; var right = "right"; val old = replace(&left, replace(&right, "next"))"#,
            ] {
                for jump in ["", "continue", "break"] {
                    let tail = if jump.is_empty() {
                        String::new()
                    } else {
                        format!("; {jump}")
                    };
                    let text = format!(
                        r#"fun run(flag: Boolean){result} {{ val seed = "outer"; val held = seed; while({condition}) {{ {body}{tail} }}; val retired = held }}"#
                    );
                    for rooted in [false, true] {
                        check(rooted, &text, true, jump);
                    }
                }
            }
        }
    }
    let mut negatives = vec![
        SOURCE.replace("= seed", "= (seed)"),
        SOURCE.replace("= held", "= (held)"),
        SOURCE.replace("= \"outer\"", "= (\"outer\")"),
        SOURCE.replace("val seed", "var seed"),
        SOURCE.replace("val held", "var held"),
        SOURCE.replace("val retired", "var retired"),
        SOURCE.replace("while(flag)", "while(false)"),
        SOURCE.replace("val held = seed", "val middle = seed; val held = middle"),
        SOURCE.replace("val local = \"inner\"", ""),
    ];
    for (left, right) in [
        ("", ""),
        ("continue", "continue"),
        ("continue", "break"),
        ("break", "continue"),
        ("break", "break"),
        ("", "continue"),
        ("continue", ""),
        ("", "break"),
        ("break", ""),
    ] {
        negatives.push(
            SOURCE
                .replace("flag: Boolean", "flag: Boolean, other: Boolean")
                .replace(
                    "val local = \"inner\"",
                    &format!("val local = \"inner\"; if(other) {{ {left} }} else {{ {right} }}"),
                ),
        );
    }
    assert_eq!(negatives.len(), 18);
    for text in negatives {
        for rooted in [false, true] {
            check(rooted, &text, false, "");
        }
    }
}
