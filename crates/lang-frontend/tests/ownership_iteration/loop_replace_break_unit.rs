//! Independent public MemoryDocument and rooted inputs consume the new bounded product.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerOrigin,
        UnitOwnerTransportEvent as E, check_compilation_unit_ownership,
    },
    parser::{Expression, Item, NameMarker, Statement},
    source::SourceMap,
    type_checking::{
        UnitExpressionId, UnitStatementId, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};
const SOURCE: &str = r#"fun run(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; val retired = held }"#;
fn check(rooted: bool, text: &str, admitted: bool) {
    let second = text.replacen("fun run(", "fun second(", 1);
    let mut sources = SourceMap::new();
    let ids = [
        sources.add_source("a.ko", text).unwrap(),
        sources.add_source("b.ko", &second).unwrap(),
    ];
    let parsed = ids.map(|id| {
        super::parser_test_assertions::parse_file_twice(&sources, id, "replace Break owner")
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
    let (names_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &names_environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{text}");
    let complete =
        complete_compilation_unit_types(&sources, &inputs, &names, &environment, &typed).unwrap();
    assert!(complete.is_same_analysis(&typed));
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed).unwrap();
    assert!(
        owned.is_compatible_with(&typed) && owned.clone().validate().is_ok(),
        "{text}"
    );
    assert_eq!(owned.owner_observations().len(), count);
    for (i, o) in owned.owner_observations().iter().enumerate() {
        let value = o.replace_break_observation_v1();
        assert_eq!(value.is_some(), admitted, "{rooted}: {text}");
        if !admitted {
            continue;
        }
        let value = value.unwrap();
        assert!(value.is_consistent());
        let source = index.source_units()[i].id();
        assert_eq!(index.source_units()[i].source_id(), ids[i]);
        let ast = parsed[i].ast();
        let declaration = |name| {
            let matches: Vec<_> = ast
                .statements()
                .iter()
                .filter_map(|(id, n)| {
                    let Statement::LocalVariable { declaration } = n.payload() else {
                        return None;
                    };
                    let Item::Variable {
                        name: NameMarker::Present(marker),
                        initializer,
                        ..
                    } = ast.items().get(*declaration).unwrap().payload()
                    else {
                        return None;
                    };
                    (sources.slice(*marker).unwrap() == name).then_some((
                        UnitStatementId::new(source, id),
                        UnitExpressionId::new(source, *initializer),
                        *marker,
                    ))
                })
                .collect();
            assert_eq!(matches.len(), 1);
            matches[0]
        };
        let symbol = |name| {
            let found: Vec<_> = names.names().source_units()[source.index()]
                .resolution()
                .symbols()
                .iter()
                .filter(|s| s.name() == name)
                .map(|s| s.id())
                .collect();
            assert_eq!(found.len(), 1);
            found[0]
        };
        let (_, initial, root_span) = declaration("held");
        let (previous_statement, call, _) = declaration("previous");
        let (suffix, _, _) = declaration("retired");
        let held = value.entry().symbol();
        assert_eq!(
            (held.source_unit(), held.symbol()),
            (source, symbol("held"))
        );
        // Obtain qualified local IDs from independent legacy cleanup, then verify the
        // complete source and local identity against the real name-resolution oracle.
        let dropped_symbol = |name| {
            let matches: Vec<_> = owned
                .drops()
                .iter()
                .filter_map(|d| match d.target() {
                    UnitDropTarget::Named(s)
                        if s.source_unit() == source && s.symbol() == symbol(name) =>
                    {
                        Some(s)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(matches.len(), 1);
            matches[0]
        };
        let previous = dropped_symbol("previous");
        let call_node = ast.expressions().get(call.expression()).unwrap();
        let Expression::Call { arguments, .. } = call_node.payload() else {
            panic!("real replace call")
        };
        assert_eq!(arguments.len(), 2);
        let argument = UnitExpressionId::new(source, arguments[1].value);
        let loops: Vec<_> = ast
            .statements()
            .iter()
            .filter_map(|(id, n)| {
                let Statement::While {
                    condition, body, ..
                } = n.payload()
                else {
                    return None;
                };
                Some((
                    UnitStatementId::new(source, id),
                    UnitExpressionId::new(source, *condition),
                    UnitStatementId::new(source, *body),
                ))
            })
            .collect();
        assert_eq!(loops.len(), 1);
        let (statement, condition, body) = loops[0];
        let jumps: Vec<_> = ast
            .expressions()
            .iter()
            .filter_map(|(id, n)| {
                matches!(n.payload(), Expression::Break { .. })
                    .then_some(UnitExpressionId::new(source, id))
            })
            .collect();
        assert_eq!(jumps.len(), 1);
        let jump = jumps[0];
        assert_eq!(
            (
                value.statement(),
                value.condition(),
                value.body(),
                value.break_expression()
            ),
            (statement, condition, body, jump)
        );
        assert_eq!(value.callable(), o.callable());
        let consumer = value.suffix_consumer_v1().expect("complete bounded suffix");
        assert_eq!(
            consumer.incoming(),
            &[
                value.condition_false_exit().clone(),
                value.break_boundary().clone(),
            ]
        );
        use lang_frontend::ownership_checking::UnitOwnerReplaceBreakConsumerActionV1 as A;
        let (_, used, _) = declaration("retired");
        assert_eq!(
            consumer.actions(),
            [
                A::Take(used, value.entry().symbol()),
                A::Bind(used, dropped_symbol("retired")),
                A::Drop(
                    UnitDropPoint::AfterStatement(suffix),
                    dropped_symbol("retired")
                ),
            ]
        );
        let p = value.entry().owner();
        let q = value.break_boundary().owner();
        assert_ne!(p, q);
        assert_eq!(value.input_producers().definitions().len(), 1);
        assert_eq!(value.body_producers().definitions().len(), 1);
        assert_eq!(
            value
                .input_producers()
                .definition(p.clone())
                .unwrap()
                .origin(),
            UnitOwnerOrigin::String(initial)
        );
        assert_eq!(
            value
                .body_producers()
                .definition(q.clone())
                .unwrap()
                .origin(),
            UnitOwnerOrigin::String(argument)
        );
        assert!(
            value.input_producers().definition(q.clone()).is_none()
                && value.body_producers().definition(p.clone()).is_none()
        );
        assert_eq!(value.entry(), value.condition_false_exit());
        assert_eq!(value.break_boundary(), value.body_completion());
        for s in [
            value.entry(),
            value.condition_false_exit(),
            value.break_boundary(),
            value.body_completion(),
        ] {
            assert_eq!(s.symbol(), held);
            assert_eq!(s.declaration(), root_span);
            assert_eq!(s.scope_depth(), 1);
        }
        assert_eq!(value.entry().origin(), root_span);
        assert_eq!(value.break_boundary().origin(), call_node.span());
        assert_eq!(
            value.body_events(),
            &[
                E::Result {
                    expression: argument,
                    owner: q.clone()
                },
                E::Pending {
                    call,
                    argument,
                    owner: q.clone()
                },
                E::Replace {
                    call,
                    root: held,
                    argument,
                    previous: p.clone(),
                    incoming: q
                },
                E::Result {
                    expression: call,
                    owner: p.clone()
                },
                E::Binding {
                    symbol: previous,
                    initializer: call,
                    owner: p.clone()
                },
                E::Drop {
                    point: UnitDropPoint::AfterStatement(previous_statement),
                    symbol: previous,
                    owner: p
                },
            ]
        );
        assert_eq!(
            value.edges(),
            &[
                Edge::EnterBody(condition, body),
                Edge::BreakExit(
                    UnitDropPoint::ControlTransfer(jump),
                    UnitDropPoint::LoopExit(statement)
                ),
                Edge::ConditionFalseExit(condition, UnitDropPoint::LoopExit(statement))
            ]
        );
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|d| matches!(d.target(),UnitDropTarget::Named(s) if s.source_unit()==source))
            .map(|d| (d.point(), d.target()))
            .collect();
        assert_eq!(
            drops,
            vec![
                (
                    UnitDropPoint::AfterStatement(previous_statement),
                    UnitDropTarget::Named(previous)
                ),
                (
                    UnitDropPoint::AfterStatement(suffix),
                    UnitDropTarget::Named(dropped_symbol("retired"))
                )
            ]
        );
        let plan = owned.ownership_primitive(call).unwrap();
        assert_eq!(plan.places().len(), 1);
        assert_eq!(plan.places()[0].root(), held);
        assert!(plan.places()[0].is_root());
        assert_eq!(plan.descriptor().operands()[1], argument);
        assert!(o.definitions().is_empty() && o.events().is_empty());
        assert!(o.loop_observation_v1().is_none() && o.value_observation_v1().is_none());
        assert!(
            o.invariant_loop_observation_v1().is_none()
                && o.invariant_break_observation_v1().is_none()
        );
        assert!(
            o.invariant_normal_join_observation_v1().is_none()
                && o.invariant_mixed_join_observation_v1().is_none()
        );
        assert!(
            o.invariant_split_continue_observation_v1().is_none()
                && o.invariant_split_exit_observation_v1().is_none()
        );
    }
    if admitted && rooted {
        let a = owned.owner_observations()[0]
            .replace_break_observation_v1()
            .unwrap();
        let b = owned.owner_observations()[1]
            .replace_break_observation_v1()
            .unwrap();
        for token in [a.entry().owner(), a.break_boundary().owner()] {
            assert!(
                b.input_producers().definition(token.clone()).is_none()
                    && b.body_producers().definition(token).is_none()
            );
        }
    }
}
#[test]
fn replace_break_matches_both_public_input_routes() {
    for explicit in [false, true] {
        for same_text in [false, true] {
            let mut text = SOURCE.to_owned();
            if explicit {
                text = text.replace("flag: Boolean)", "flag: Boolean): Unit");
            }
            if same_text {
                text = text.replace("\"next\"", "\"outer\"");
            }
            for rooted in [false, true] {
                check(rooted, &text, true);
            }
        }
    }
    for text in [
        SOURCE.replace("break", "continue"),
        SOURCE.replace("; break", ""),
        SOURCE.replace("while(flag)", "while(flag && flag)"),
        SOURCE.replace("while(flag)", "while((flag == flag))"),
        SOURCE.replace("\"next\"", "(\"next\")"),
        SOURCE.replace("val previous", "var previous"),
        SOURCE.replace("val retired =", "val retired: String? ="),
        SOURCE.replace("retired = held", "retired = (held)"),
    ] {
        for rooted in [false, true] {
            check(rooted, &text, false);
        }
    }
}
