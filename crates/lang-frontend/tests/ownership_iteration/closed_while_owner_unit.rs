//! Public two-input contract for real While body collection; no dynamic replay.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerObservationStatus,
        UnitOwnerValueEventV1 as Event, check_compilation_unit_ownership,
    },
    source::SourceMap,
    type_checking::{
        UnitExpressionId, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

#[test]
fn closed_while_owner_template_has_the_same_contract_in_both_unit_inputs() {
    for rooted in [false, true] {
        for suffix in ["", "; return", "; break", "; continue"] {
            let text = format!(
                "fun run(flag: Boolean) {{ while ((flag)) {{ var root = \"old\"; val next = \"next\"; val previous = replace(&root, next); val delivered = (previous){suffix} }} }}"
            );
            let mut sources = SourceMap::new();
            let source = sources.add_source("loop.ko", &text).unwrap();
            let parsed = super::parser_test_assertions::parse_file_twice(
                &sources,
                source,
                "closed While owner",
            );
            assert!(parsed.diagnostics().is_empty());
            let inputs = [if rooted {
                SourceUnitInput::new("root", "loop.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
            let (environment, types) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty());
            let complete =
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
            assert!(complete.is_same_analysis(&typed));
            let typed = typed.validate().unwrap();
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap();
            assert!(owned.is_compatible_with(&typed));
            assert!(owned.clone().validate().is_ok(), "rooted={rooted}: {text}");
            let [legacy] = owned.owner_observations() else {
                panic!("one real callable")
            };
            assert!(matches!(
                legacy.status(),
                UnitOwnerObservationStatus::Unsupported { .. }
            ));
            assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
            assert!(legacy.value_observation_v1().is_none());
            if suffix != "; return" {
                let observation = legacy
                    .loop_observation_v1()
                    .expect("actual closed body completion");
                assert!(observation.is_consistent());
                assert!(matches!(observation.edges()[0], Edge::EnterBody(..)));
                assert!(matches!(
                    observation.edges()[2],
                    Edge::ConditionFalseExit(..)
                ));
                let expected = if suffix.is_empty() {
                    Edge::Backedge(
                        UnitDropPoint::AfterStatement(observation.body()),
                        observation.condition(),
                    )
                } else {
                    let jump = parsed
                        .ast()
                        .expressions()
                        .iter()
                        .find_map(|(id, node)| {
                            (sources.slice(node.span()).unwrap() == suffix.trim_start_matches("; "))
                                .then_some(id)
                        })
                        .expect("actual terminal jump");
                    let point = UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                        observation.condition().source_unit(),
                        jump,
                    ));
                    if suffix == "; continue" {
                        Edge::ContinueBackedge(point, observation.condition())
                    } else {
                        Edge::BreakExit(point, UnitDropPoint::LoopExit(observation.statement()))
                    }
                };
                assert_eq!(observation.edges()[1], expected);
                let graph = observation.body_transport();
                assert_eq!(graph.producers().definitions().len(), 2);
                assert_eq!(
                    graph
                        .prefix()
                        .iter()
                        .filter(|e| matches!(e, Event::Replace { .. }))
                        .count(),
                    1
                );
                assert_eq!(
                    graph
                        .prefix()
                        .iter()
                        .filter(|e| matches!(e, Event::Drop(..)))
                        .count(),
                    2
                );
                assert_eq!(owned.drops().len(), 2);
            } else {
                assert!(
                    legacy.loop_observation_v1().is_none(),
                    "Return is not a loop completion"
                );
            }
        }
    }
}
