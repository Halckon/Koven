//! Public real-source contract for nonempty invariant While boundaries, not phi.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerObservationStatus,
        UnitOwnerOrigin, UnitOwnerValueEventV1 as Event, check_compilation_unit_ownership,
    },
    parser::Statement,
    source::SourceMap,
    type_checking::{
        UnitExpressionId, UnitStatementId, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

#[test]
fn invariant_while_boundaries_match_actual_sources_in_both_unit_inputs() {
    for rooted in [false, true] {
        for (body, suffix, admitted) in [
            (r#"val local = "inner""#, "val retired = held", true),
            (
                r#"val local = "inner"; val alias = (local)"#,
                "val retired = held",
                true,
            ),
            (
                r#"var root = "old"; val incoming = "next"; val previous = replace(&root, incoming)"#,
                "val retired = held",
                true,
            ),
            (
                r#"val local = "inner""#,
                "val retired: String? = held",
                false,
            ),
            (
                r#"val local = "inner"; continue"#,
                "val retired = held",
                false,
            ),
            (r#"val local = "inner""#, "val retired = (held)", false),
        ] {
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
                super::parser_test_assertions::parse_file_twice(
                    &sources,
                    id,
                    "invariant While owner",
                )
            });
            assert!(parsed.iter().all(|p| p.diagnostics().is_empty()));
            let source_count = if rooted { 2 } else { 1 };
            let inputs = parsed
                .iter()
                .take(source_count)
                .enumerate()
                .map(|(i, p)| {
                    if rooted {
                        SourceUnitInput::new(
                            "root",
                            if i == 0 { "a.ko" } else { "b.ko" },
                            ids[i],
                            p,
                        )
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
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap();
            assert!(owned.is_compatible_with(&typed));
            assert!(owned.clone().validate().is_ok(), "rooted={rooted}: {text}");
            assert_eq!(owned.owner_observations().len(), source_count);
            for (i, legacy) in owned.owner_observations().iter().enumerate() {
                assert!(matches!(
                    legacy.status(),
                    UnitOwnerObservationStatus::Unsupported { .. }
                ));
                assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
                assert!(
                    legacy.value_observation_v1().is_none()
                        && legacy.loop_observation_v1().is_none()
                );
                let Some(observed) = legacy.invariant_loop_observation_v1() else {
                    assert!(!admitted);
                    continue;
                };
                assert!(admitted && observed.is_consistent());
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
                assert_eq!(observed.entry(), observed.backedge());
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
                assert_eq!(
                    observed.edges(),
                    &[
                        Edge::EnterBody(observed.condition(), observed.body()),
                        Edge::Backedge(
                            UnitDropPoint::AfterStatement(observed.body()),
                            observed.condition()
                        ),
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
                for event in graph.prefix() {
                    if let Event::Drop(point, symbol, _) = event {
                        assert_ne!(*symbol, observed.entry().symbol());
                        assert!(
                            owned.drops().iter().any(|d| d.point() == *point
                                && d.target() == UnitDropTarget::Named(*symbol))
                        );
                    }
                }
                if rooted {
                    let foreign = owned.owner_observations()[1 - i]
                        .invariant_loop_observation_v1()
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
    }
}
