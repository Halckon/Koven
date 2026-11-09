//! Both public Unit input routes retain exact post-loop source and consumption identities.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitOwnerOrigin, UnitOwnerValueEventV1 as Event,
        UnitOwnerValueV1 as Value, check_compilation_unit_ownership,
    },
    parser::{Item, Statement},
    source::SourceMap,
    type_checking::{
        UnitExpressionId, UnitStatementId, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

const SOURCE: &str = r#"fun run(flag: Boolean) { val held = "outer"; while(flag) { val local = "inner" }; val retired = held }"#;
fn check(rooted: bool, text: &str, admitted: bool) {
    let second = text.replace("run", "second");
    let mut sources = SourceMap::new();
    let ids = [
        sources.add_source("a.ko", text).unwrap(),
        sources.add_source("b.ko", &second).unwrap(),
    ];
    let parsed = ids.map(|id| {
        super::parser_test_assertions::parse_file_twice(&sources, id, "invariant suffix")
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
    assert!(owned.is_compatible_with(&typed));
    assert!(owned.clone().validate().is_ok(), "rooted={rooted}: {text}");
    assert_eq!(owned.owner_observations().len(), count);
    for (i, legacy) in owned.owner_observations().iter().enumerate() {
        assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
        assert!(legacy.value_observation_v1().is_none() && legacy.loop_observation_v1().is_none());
        let outer = legacy
            .invariant_loop_observation_v1()
            .and_then(|b| b.post_loop_transport_v1());
        assert_eq!(outer.is_some(), admitted, "rooted={rooted}: {text}");
        let Some(outer) = outer else { continue };
        let boundaries = legacy.invariant_loop_observation_v1().unwrap();
        assert!(outer.is_consistent() && boundaries.is_consistent());
        assert!(outer.choice().is_none() && outer.suffix().is_empty());
        assert_eq!(outer.producers().definitions().len(), 1);
        let source = index.source_units()[i].id();
        assert_eq!(index.source_units()[i].source_id(), ids[i]);
        let symbol = |name| {
            names.names().source_units()[source.index()]
                .resolution()
                .symbols()
                .iter()
                .find(|s| s.name() == name)
                .unwrap()
                .id()
        };
        let held = symbol("held");
        let retired = symbol("retired");
        let expression = |text| {
            parsed[i]
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, node)| {
                    (sources.slice(node.span()).unwrap() == text)
                        .then_some(UnitExpressionId::new(source, id))
                })
                .unwrap()
        };
        let initializer = expression("\"outer\"");
        let used_expression = expression("held");
        let ast = parsed[i].ast();
        let suffix = ast
            .statements()
            .iter()
            .find_map(|(id, node)| {
                let Statement::LocalVariable { declaration } = node.payload() else {
                    return None;
                };
                let Item::Variable { initializer, .. } =
                    ast.items().get(*declaration).unwrap().payload()
                else {
                    return None;
                };
                (*initializer == used_expression.expression())
                    .then_some(UnitStatementId::new(source, id))
            })
            .unwrap();
        let (while_id, condition, body) = ast
            .statements()
            .iter()
            .find_map(|(id, node)| {
                if let Statement::While {
                    condition, body, ..
                } = node.payload()
                {
                    Some((id, *condition, *body))
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(
            (
                boundaries.statement(),
                boundaries.condition(),
                boundaries.body()
            ),
            (
                UnitStatementId::new(source, while_id),
                UnitExpressionId::new(source, condition),
                UnitStatementId::new(source, body)
            )
        );
        let [
            Event::Create(created_at, owner),
            Event::Bind(input, bound, Value::Definition(bound_owner)),
            Event::Move(moved_at, moved_from, Value::Definition(moved)),
            Event::Bind(output, used, Value::Definition(installed)),
            Event::Drop(point, dropped, Value::Definition(consumed)),
        ] = outer.prefix()
        else {
            panic!("exact outer events")
        };
        assert_eq!(
            (*created_at, input.symbol(), *bound),
            (initializer, held, initializer)
        );
        assert_eq!(
            (*moved_at, moved_from.symbol(), output.symbol(), *used),
            (used_expression, held, retired, used_expression)
        );
        assert_eq!(
            (*point, dropped.symbol()),
            (UnitDropPoint::AfterStatement(suffix), retired)
        );
        assert_eq!(boundaries.entry().symbol().symbol(), held);
        for symbol in [input, moved_from, output, dropped] {
            assert_eq!(symbol.source_unit(), source);
        }
        assert_eq!(boundaries.entry(), boundaries.backedge());
        assert_eq!(boundaries.entry(), boundaries.exit());
        for token in [owner, bound_owner, moved, installed, consumed] {
            assert_eq!(*token, boundaries.entry().owner());
            assert_eq!(
                outer
                    .producers()
                    .definition(token.clone())
                    .unwrap()
                    .origin(),
                UnitOwnerOrigin::String(initializer)
            );
            assert!(
                boundaries
                    .input_producers()
                    .definition(token.clone())
                    .is_some()
            );
            assert!(
                boundaries
                    .body_transport()
                    .producers()
                    .definition(token.clone())
                    .is_none()
            );
            for (other, observation) in owned.owner_observations().iter().enumerate() {
                if other != i {
                    assert!(
                        observation
                            .invariant_loop_observation_v1()
                            .unwrap()
                            .post_loop_transport_v1()
                            .unwrap()
                            .producers()
                            .definition(token.clone())
                            .is_none()
                    );
                }
            }
        }
        assert_eq!(
            owned
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(*output))
                .count(),
            1
        );
        assert!(
            owned
                .drops()
                .iter()
                .any(|drop| drop.target() == UnitDropTarget::Named(*output)
                    && drop.point() == *point)
        );
        assert!(
            !owned
                .drops()
                .iter()
                .any(|drop| drop.target() == UnitDropTarget::Named(*input))
        );
    }
}

#[test]
fn actual_suffix_consumption_matches_both_public_unit_inputs() {
    let mut cases = Vec::new();
    for body in [
        r#"val local = "inner""#,
        r#"val local = "inner"; val alias = (local)"#,
        r#"var root = "old"; val incoming = "next"; val previous = replace(&root, incoming)"#,
        r#"var left = "left"; var right = "right"; val previous = replace(&left, replace(&right, "next"))"#,
    ] {
        cases.push((SOURCE.replace("val local = \"inner\"", body), true));
    }
    for text in [
        SOURCE.replace("val held", "var held"),
        SOURCE.replace("val retired", "var retired"),
        SOURCE.replace("= held", "= (held)"),
        SOURCE.replace("= \"outer\"", "= (\"outer\")"),
        SOURCE.replace("val retired =", "val retired: String? ="),
        SOURCE.replace(
            "val retired = held",
            "val retired = held; val extra = \"extra\"",
        ),
        SOURCE.replace("val local = \"inner\"", ""),
        SOURCE.replace("val local = \"inner\"", "val local = held + \"inner\""),
        SOURCE.replace("val local = \"inner\"", "val local = \"inner\"; continue"),
        SOURCE.replace("val local = \"inner\"", "val local = \"inner\"; break"),
    ] {
        cases.push((text, false));
    }
    for tail in [
        "if(other) {} else {}",
        "if(other) { continue } else { continue }",
        "if(other) {} else { break }",
    ] {
        cases.push((format!(r#"fun run(flag: Boolean, other: Boolean) {{ val held = "outer"; while(flag) {{ val local = "inner"; {tail} }}; val retired = held }}"#),false));
    }
    assert_eq!(cases.len(), 17);
    for (text, admitted) in cases {
        for rooted in [false, true] {
            check(rooted, &text, admitted);
        }
    }
}
