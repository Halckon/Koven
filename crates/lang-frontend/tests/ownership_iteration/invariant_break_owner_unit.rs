//! Public real-source contract for actual invariant Break exits, never backedges.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerObservationStatus,
        UnitOwnerOrigin, UnitOwnerValueEventV1 as Event, check_compilation_unit_ownership,
    },
    parser::{Expression, Statement},
    source::SourceMap,
    type_checking::{
        UnitExpressionId, UnitStatementId, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

fn check_break_inputs(rooted: bool, body: &str, suffix: &str, admitted: bool, old_backedge: bool) {
    let text = format!(
        r#"fun first(flag: Boolean) {{ val held = "outer"; while((flag)) {{ {body} }}; {suffix} }}"#
    );
    let second = text.replace("first", "second");
    let mut sources = SourceMap::new();
    let ids = [
        sources.add_source("a.ko", &text).unwrap(),
        sources.add_source("b.ko", &second).unwrap(),
    ];
    let parsed = ids.map(|id| {
        super::parser_test_assertions::parse_file_twice(&sources, id, "invariant While owner")
    });
    assert!(parsed.iter().all(|p| p.diagnostics().is_empty()));
    let source_count = if rooted { 2 } else { 1 };
    let inputs = parsed
        .iter()
        .take(source_count)
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
    assert_eq!(owned.owner_observations().len(), source_count);
    for (i, legacy) in owned.owner_observations().iter().enumerate() {
        assert!(matches!(
            legacy.status(),
            UnitOwnerObservationStatus::Unsupported { .. }
        ));
        assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
        assert!(legacy.value_observation_v1().is_none() && legacy.loop_observation_v1().is_none());
        let Some(observed) = legacy.invariant_break_observation_v1() else {
            assert!(!admitted);
            assert_eq!(
                legacy.invariant_loop_observation_v1().is_some(),
                old_backedge
            );
            continue;
        };
        assert!(admitted && observed.is_consistent());
        assert!(legacy.invariant_loop_observation_v1().is_none());
        let source = index.source_units()[i].id();
        assert_eq!(index.source_units()[i].source_id(), ids[i]);
        assert_eq!(observed.statement().source_unit(), source);
        let (statement, condition, body) = parsed[i]
            .ast()
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
            observed.statement(),
            UnitStatementId::new(source, statement)
        );
        assert_eq!(
            observed.condition(),
            UnitExpressionId::new(source, condition)
        );
        assert_eq!(observed.body(), UnitStatementId::new(source, body));
        let held = names.names().source_units()[source.index()]
            .resolution()
            .symbols()
            .iter()
            .find(|s| s.name() == "held")
            .unwrap();
        assert_eq!(observed.entry().symbol().source_unit(), source);
        assert_eq!(observed.entry().symbol().symbol(), held.id());
        assert_eq!(observed.entry(), observed.body_completion());
        assert_eq!(observed.entry(), observed.exit());
        let literal = parsed[i]
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                (sources.slice(node.span()).unwrap() == "\"outer\"")
                    .then_some(UnitExpressionId::new(source, id))
            })
            .unwrap();
        assert_eq!(observed.input_producers().definitions().len(), 1);
        assert_eq!(
            observed
                .input_producers()
                .definition(observed.entry().owner())
                .unwrap()
                .origin(),
            UnitOwnerOrigin::String(literal)
        );
        let jump = parsed[i]
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                matches!(node.payload(), Expression::Break { .. })
                    .then_some(UnitExpressionId::new(source, id))
            })
            .expect("actual Break AST");
        assert_eq!(observed.break_boundary(), Some((jump, observed.entry())));
        let control = UnitDropPoint::ControlTransfer(jump);
        assert!(owned.drops().iter().all(|drop| drop.point() != control));
        let break_exit = Edge::BreakExit(control, UnitDropPoint::LoopExit(observed.statement()));
        assert_eq!(
            observed.edges(),
            &[
                Edge::EnterBody(observed.condition(), observed.body()),
                break_exit,
                Edge::ConditionFalseExit(
                    observed.condition(),
                    UnitDropPoint::LoopExit(observed.statement())
                )
            ]
        );
        let graph = observed.body_transport();
        assert!(graph.is_consistent() && !graph.producers().definitions().is_empty());
        assert!(
            graph
                .producers()
                .definition(observed.entry().owner())
                .is_none()
        );
        assert!(
            observed
                .edges()
                .iter()
                .all(|edge| !matches!(edge, Edge::Backedge(..) | Edge::ContinueBackedge(..)))
        );
        for event in graph.prefix() {
            if let Event::Drop(point, symbol, _) = event {
                assert_ne!(*point, control);
                assert_ne!(*symbol, observed.entry().symbol());
                assert!(
                    owned.drops().iter().any(
                        |d| d.point() == *point && d.target() == UnitDropTarget::Named(*symbol)
                    )
                );
            }
        }
        if rooted {
            let foreign = owned.owner_observations()[1 - i]
                .invariant_break_observation_v1()
                .unwrap();
            assert!(
                observed
                    .input_producers()
                    .definition(foreign.entry().owner())
                    .is_none()
            );
        }
    }
}

#[test]
fn invariant_break_exits_have_no_backedge_in_both_unit_inputs() {
    for rooted in [false, true] {
        for (body, suffix, admitted, old_backedge) in [
            (
                r#"val local = "inner"; break"#,
                "val retired = held",
                true,
                false,
            ),
            (
                r#"val local = "inner"; val alias = (local); break"#,
                "val retired = held",
                true,
                false,
            ),
            (
                r#"var root = "old"; val incoming = "next"; val previous = replace(&root, incoming); break"#,
                "val retired = held",
                true,
                false,
            ),
            (
                r#"var left = "left"; var right = "right"; val previous = replace(&left, replace(&right, "next")); break"#,
                "val retired = held",
                true,
                false,
            ),
            (
                r#"val local = "inner"; (break)"#,
                "val retired = held",
                false,
                false,
            ),
            (
                r#"val local = "inner"; continue"#,
                "val retired = held",
                false,
                true,
            ),
            (
                r#"val local = "inner"; if(flag) { break } else { break }"#,
                "val retired = held",
                false,
                false,
            ),
            (
                r#"val local = held + "inner"; break"#,
                "val retired = held",
                false,
                false,
            ),
            (
                r#"val local = "inner"; break"#,
                "val retired: String? = held",
                false,
                false,
            ),
        ] {
            check_break_inputs(rooted, body, suffix, admitted, old_backedge);
        }
    }
}
