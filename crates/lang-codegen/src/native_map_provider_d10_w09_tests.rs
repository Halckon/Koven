//! D10 原 w09 的两条 native 正例；只复用既有 object/link/run helper。
//! helper 没有内置硬超时。执行须在同源 SSA/LLVM 门通过后独立准入。

use super::{assert_result, run_unit_with_entry};
use crate::native_tests::emit_link_and_run;

const PATH: &str = "w09_map_temp_source.ko";
const SOURCE: &str =
    include_str!("../tests/fixtures/d10-unit-migration-0341/w09_map_temp_source.ko");

#[test]
fn single_original_w09_temporary_map_carry_exits_normally() {
    let run = emit_link_and_run(PATH, SOURCE, "main");
    assert_result(PATH, "single", &run, "map-temp\n");
}

#[test]
fn unit_original_w09_temporary_map_carry_exits_normally() {
    let run = run_unit_with_entry(PATH, PATH, SOURCE, "main");
    assert_result(PATH, "unit", &run, "map-temp\n");
}

pub(crate) mod temporary_arguments {
    //! The same selected argument factory executes once, with and without a provider.
    use super::super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::temporary_argument_sources,
    };

    fn run(unit: bool) {
        for case in temporary_argument_sources() {
            for control in [false, true] {
                let (source, golden) = if control {
                    (&case.control, &case.control_golden)
                } else {
                    (&case.source, &case.golden)
                };
                let label = format!("{}-control-{control}", case.label);
                let output = if unit {
                    run_unit(&label, source)
                } else {
                    emit_link_and_run("provider.ko", source, "entry")
                };
                assert_result(
                    &label,
                    if unit { "unit" } else { "single" },
                    &output,
                    golden,
                );
            }
        }
    }

    #[test]
    fn single_selected_argument_factory_exits_normally() {
        run(false);
    }

    #[test]
    fn unit_selected_argument_factory_exits_normally() {
        run(true);
    }
}

pub(crate) mod temporary_delivery {
    //! Actual ordinary delivery, deinit timing and named evaluation order through both entries.
    use super::super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::temporary_delivery_sources,
    };

    fn run(unit: bool) {
        for case in temporary_delivery_sources() {
            for control in [false, true] {
                let (source, golden) = if control {
                    (&case.control, &case.control_golden)
                } else {
                    (&case.source, &case.golden)
                };
                let label = format!("{}-control-{control}", case.label);
                let output = if unit {
                    run_unit(&label, source)
                } else {
                    emit_link_and_run("provider.ko", source, "entry")
                };
                assert_result(
                    &label,
                    if unit { "unit" } else { "single" },
                    &output,
                    golden,
                );
            }
        }
    }

    #[test]
    fn single_ordinary_delivery_preserves_order_and_deinit() {
        run(false);
    }

    #[test]
    fn unit_ordinary_delivery_preserves_order_and_deinit() {
        run(true);
    }
}

pub(crate) mod stack_paths {
    //! The same G8 pipeline sources execute their nonempty and independently empty paths.
    use super::super::{assert_result, run_unit};
    use crate::ssa::map_provider_exits_pipeline_tests::{
        StackCase, stack_owned_resource_sources, stack_runtime_sources, stack_variant_sources,
    };

    fn run(unit: bool, cases: Vec<StackCase>, count: usize) {
        assert_eq!(cases.len(), count);
        for case in cases {
            let result = if unit {
                run_unit(&case.label, &case.source)
            } else {
                crate::native_tests::emit_link_and_run("provider.ko", &case.source, "entry")
            };
            assert_result(
                &case.label,
                if unit { "unit" } else { "single" },
                &result,
                &case.golden,
            );
        }
    }

    #[test]
    fn single_nested_stack_paths_preserve_cleanup_and_caller_permissions() {
        run(false, stack_runtime_sources(), 12);
    }

    #[test]
    fn unit_nested_stack_paths_preserve_cleanup_and_caller_permissions() {
        run(true, stack_runtime_sources(), 12);
    }

    #[test]
    fn single_owned_resource_same_root_preserves_exact_cleanup() {
        run(false, stack_owned_resource_sources(), 4);
    }

    #[test]
    fn unit_owned_resource_same_root_preserves_exact_cleanup() {
        run(true, stack_owned_resource_sources(), 4);
    }

    #[test]
    fn single_temporary_nullable_and_three_layers_preserve_each_exit() {
        run(false, stack_variant_sources(), 18);
    }

    #[test]
    fn unit_temporary_nullable_and_three_layers_preserve_each_exit() {
        run(true, stack_variant_sources(), 18);
    }
}

pub(crate) mod local_arguments {
    //! Core and generic-wrapper native groups use their exact public pipeline inputs.
    use super::super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::{
            local_argument_sources, local_generic_argument_sources,
        },
    };
    fn run_core(unit: bool) {
        for c in local_argument_sources() {
            if c.unit_only && !unit {
                continue;
            }
            let output = if unit {
                run_unit(&c.label, &c.source)
            } else {
                emit_link_and_run("provider.ko", &c.source, "entry")
            };
            assert_result(
                &c.label,
                if unit { "unit" } else { "single" },
                &output,
                &c.golden,
            );
        }
    }
    fn run_wrapper(unit: bool) {
        for (label, source, golden) in local_generic_argument_sources() {
            let output = if unit {
                run_unit(&label, &source)
            } else {
                emit_link_and_run("provider.ko", &source, "entry")
            };
            assert_result(
                &label,
                if unit { "unit" } else { "single" },
                &output,
                &golden,
            );
        }
    }
    #[test]
    fn single_local_argument_sources_keep_exact_native_order_and_cleanup() {
        run_core(false);
    }
    #[test]
    fn unit_local_argument_sources_keep_exact_native_order_and_cleanup() {
        run_core(true);
    }
    #[test]
    fn single_generic_wrapper_sources_keep_exact_native_order_and_cleanup() {
        run_wrapper(false);
    }
    #[test]
    fn unit_generic_wrapper_sources_keep_exact_native_order_and_cleanup() {
        run_wrapper(true);
    }
}

pub(crate) mod ordinary_control_flow {
    //! Real ordinary return operands, empty inputs and lexical Guard cleanup.
    use super::super::{assert_result, run_unit};
    use crate::ssa::map_provider_exits_pipeline_tests::{
        ordinary_control_flow_neighbor_sources, ordinary_control_flow_sources,
        ordinary_return_sources,
    };

    fn run(unit: bool) {
        for case in ordinary_return_sources() {
            let run = if unit {
                run_unit(&case.label, &case.source)
            } else {
                crate::native_tests::emit_link_and_run("provider.ko", &case.source, "entry")
            };
            assert_result(
                &case.label,
                if unit { "unit" } else { "single" },
                &run,
                &case.golden,
            );
        }
    }

    #[test]
    fn single_ordinary_returns_preserve_delivery_and_cleanup() {
        run(false);
    }

    #[test]
    fn unit_ordinary_returns_preserve_delivery_and_cleanup() {
        run(true);
    }

    fn run_cfg(unit: bool, neighbors: bool) {
        let cases = if neighbors {
            ordinary_control_flow_neighbor_sources()
        } else {
            ordinary_control_flow_sources()
        };
        assert_eq!(cases.len(), if neighbors { 14 } else { 24 });
        for case in cases {
            let result = if unit {
                run_unit(&case.label, &case.source)
            } else {
                crate::native_tests::emit_link_and_run("provider.ko", &case.source, "entry")
            };
            assert_result(
                &case.label,
                if unit { "unit" } else { "single" },
                &result,
                &case.golden,
            );
        }
    }

    #[test]
    fn single_ordinary_cfg_preserves_branches_and_cleanup() {
        run_cfg(false, false);
    }

    #[test]
    fn unit_ordinary_cfg_preserves_branches_and_cleanup() {
        run_cfg(true, false);
    }

    #[test]
    fn single_cfg_neighbors_preserve_current_provider_and_cleanup() {
        run_cfg(false, true);
    }

    #[test]
    fn unit_cfg_neighbors_preserve_current_provider_and_cleanup() {
        run_cfg(true, true);
    }
}

pub(crate) mod resource_continuation_tests {
    //! Strict normal-source Resource continuations, using the independently gated source/golden set.
    use super::super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::resource_continuation_sources,
    };
    #[test]
    fn single_resource_continuations_exit_with_lexical_drops() {
        for case in resource_continuation_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(case.label, "single", &run, &case.golden);
        }
    }
    #[test]
    fn unit_resource_continuations_exit_with_lexical_drops() {
        for case in resource_continuation_sources() {
            let run = run_unit(case.label, &case.source);
            assert_result(case.label, "unit", &run, &case.golden);
        }
    }

    #[test]
    fn single_resource_continue_exits_with_lexical_drops() {
        for case in crate::ssa::map_provider_exits_pipeline_tests::resource_continue_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(case.label, "single", &run, &case.golden);
        }
    }
    #[test]
    fn unit_resource_continue_exits_with_lexical_drops() {
        for case in crate::ssa::map_provider_exits_pipeline_tests::resource_continue_sources() {
            let run = run_unit(case.label, &case.source);
            assert_result(case.label, "unit", &run, &case.golden);
        }
    }

    #[test]
    fn single_nullable_resource_components_exit_with_exact_presence_and_drops() {
        for case in crate::ssa::map_provider_exits_pipeline_tests::resource_nullable_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(case.label, "single", &run, &case.golden);
        }
    }
    #[test]
    fn unit_nullable_resource_components_exit_with_exact_presence_and_drops() {
        for case in crate::ssa::map_provider_exits_pipeline_tests::resource_nullable_sources() {
            let run = run_unit(case.label, &case.source);
            assert_result(case.label, "unit", &run, &case.golden);
        }
    }

    #[test]
    fn single_resource_break_exits_with_lexical_drops() {
        for case in crate::ssa::map_provider_exits_pipeline_tests::resource_break_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(case.label, "single", &run, &case.golden);
        }
    }
    #[test]
    fn unit_resource_break_exits_with_lexical_drops() {
        for case in crate::ssa::map_provider_exits_pipeline_tests::resource_break_sources() {
            let run = run_unit(case.label, &case.source);
            assert_result(case.label, "unit", &run, &case.golden);
        }
    }

    #[test]
    fn single_resource_return_delivers_before_complete_cleanup() {
        for case in crate::ssa::map_provider_exits_pipeline_tests::resource_return_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(case.label, "single", &run, &case.golden);
        }
    }
    #[test]
    fn unit_resource_return_delivers_before_complete_cleanup() {
        for case in crate::ssa::map_provider_exits_pipeline_tests::resource_return_sources() {
            let run = run_unit(case.label, &case.source);
            assert_result(case.label, "unit", &run, &case.golden);
        }
    }
}

pub(crate) mod formation_paths {
    //! Original legal formation/transfer sources; native output does not replace owner identity queries.
    use super::super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::{
            alternate_local_sources, formation_runtime_sources, future_transfer_sources,
            populated_future_transfer_sources,
        },
    };

    fn cases() -> Vec<(String, String, &'static str)> {
        let mut cases = alternate_local_sources()
            .into_iter()
            .map(|(label, source)| {
                let golden = match label {
                    "reassigned" | "parameter-move" => "gap\n",
                    "old-alias-rebound" => "gap\nzero\n",
                    "prefix-cfg" => "zero\ngap\n",
                    "prefix-put" => "gap\nkey\nvalue\n",
                    other => panic!("unexpected original formation case: {other}"),
                };
                (label.to_owned(), source, golden)
            })
            .collect::<Vec<_>>();
        cases.extend(
            future_transfer_sources()
                .into_iter()
                .map(|(mutable, source)| {
                    (
                        format!(
                            "future-owner-transfer-{}",
                            if mutable { "MutableMap" } else { "Map" }
                        ),
                        source,
                        "sink\n",
                    )
                }),
        );
        assert_eq!(cases.len(), 7);
        cases
    }

    #[test]
    fn single_original_formations_and_owner_transfers_exit_normally() {
        for (label, source, golden) in cases() {
            let run = emit_link_and_run("provider.ko", &source, "entry");
            assert_result(&label, "single", &run, golden);
        }
    }

    #[test]
    fn unit_original_formations_and_owner_transfers_exit_normally() {
        for (label, source, golden) in cases() {
            let run = run_unit(&label, &source);
            assert_result(&label, "unit", &run, golden);
        }
    }

    #[test]
    fn single_resource_formations_and_owner_transfers_preserve_cleanup() {
        for case in formation_runtime_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }

    #[test]
    fn unit_resource_formations_and_owner_transfers_preserve_cleanup() {
        for case in formation_runtime_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }

    #[test]
    fn single_populated_future_transfers_execute_original_conditional_body() {
        for (mutable, source) in populated_future_transfer_sources() {
            let label = format!(
                "populated-future-transfer-{}",
                if mutable { "MutableMap" } else { "Map" }
            );
            let run = emit_link_and_run("provider.ko", &source, "entry");
            assert_result(&label, "single", &run, "key\nsink\n");
        }
    }

    #[test]
    fn unit_populated_future_transfers_execute_original_conditional_body() {
        for (mutable, source) in populated_future_transfer_sources() {
            let label = format!(
                "populated-future-transfer-{}",
                if mutable { "MutableMap" } else { "Map" }
            );
            let run = run_unit(&label, &source);
            assert_result(&label, "unit", &run, "key\nsink\n");
        }
    }
}

pub(crate) mod local_reassignment {
    //! Normal reassignment products; exact exit/stdout/stderr contracts use real source.
    use super::super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::local_reassignment_sources,
    };

    #[test]
    fn single_local_reassignments_preserve_value_histories() {
        for case in local_reassignment_sources() {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }

    #[test]
    fn unit_local_reassignments_preserve_value_histories() {
        for case in local_reassignment_sources() {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }

    #[test]
    fn single_grouped_replacements_preserve_value_histories() {
        let cases =
            crate::ssa::map_provider_exits_pipeline_tests::local_grouped_reassignment_sources();
        assert_eq!(cases.len(), 6);
        for case in cases {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }
    #[test]
    fn unit_grouped_replacements_preserve_value_histories() {
        let cases =
            crate::ssa::map_provider_exits_pipeline_tests::local_grouped_reassignment_sources();
        assert_eq!(cases.len(), 6);
        for case in cases {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }

    #[test]
    fn single_ordinary_epoch_arguments_preserve_replacement_order() {
        for (label, source, golden) in
            crate::ssa::map_provider_exits_pipeline_tests::local_ordinary_epoch_sources()
        {
            let run = emit_link_and_run("provider.ko", &source, "entry");
            assert_result(label, "single", &run, golden);
        }
    }
    #[test]
    fn unit_ordinary_epoch_arguments_preserve_replacement_order() {
        for (label, source, golden) in
            crate::ssa::map_provider_exits_pipeline_tests::local_ordinary_epoch_sources()
        {
            let run = run_unit(label, &source);
            assert_result(label, "unit", &run, golden);
        }
    }

    #[test]
    fn unit_ordinary_epoch_routes_preserve_replacement_order() {
        let cases = crate::ssa::map_provider_exits_pipeline_tests::local_ordinary_route_sources();
        assert_eq!(cases.len(), 6);
        for (label, source, golden) in cases {
            let run = run_unit(&label, &source);
            assert_result(&label, "unit", &run, &golden);
        }
    }
    #[test]
    fn unit_ordinary_route_controls_preserve_existing_delivery() {
        let cases = crate::ssa::map_provider_exits_pipeline_tests::local_ordinary_route_controls();
        assert_eq!(cases.len(), 3);
        for (label, source, golden) in cases {
            let run = run_unit(&label, &source);
            assert_result(&label, "unit", &run, &golden);
        }
    }

    #[test]
    fn single_completed_replacements_drop_the_installed_owner_asap() {
        let cases = crate::ssa::map_provider_exits_pipeline_tests::local_completed_epoch_sources();
        assert_eq!(cases.len(), 4);
        for case in cases {
            let run = emit_link_and_run("provider.ko", &case.source, "entry");
            assert_result(&case.label, "single", &run, &case.golden);
        }
    }
    #[test]
    fn unit_completed_replacements_drop_the_installed_owner_asap() {
        let cases = crate::ssa::map_provider_exits_pipeline_tests::local_completed_epoch_sources();
        assert_eq!(cases.len(), 4);
        for case in cases {
            let run = run_unit(&case.label, &case.source);
            assert_result(&case.label, "unit", &run, &case.golden);
        }
    }
}

pub(crate) mod scalar_routing_tests {
    //! Ordinary Map-for composition with retired integer/range families, without another
    //! construct forcing refinement. Native cases only take successful runtime paths.

    use crate::test_support::diagnostics::assert_text_eq;
    use lang_frontend::name_resolution::SymbolKind;

    use crate::{
        native_tests::boxed_enum_tests::unit_source::UnitSource,
        ssa::{model::Operation, render_program, verify::verify_program},
    };

    const MODES: [&str; 6] = [
        "borrow-map",
        "own-map",
        "borrow-mutable",
        "own-mutable",
        "temporary",
        "temporary-mutable",
    ];

    fn source(mode: &str, before: &str, body: &str, after: &str) -> String {
        let mutable = mode.contains("mutable");
        let ty = if mutable { "MutableMap" } else { "Map" };
        let result = if mutable { "m" } else { "m.consume()" };
        let (parameter, iterable, argument) = if mode.starts_with("temporary") {
            (String::new(), "make()", "")
        } else {
            let own = if mode.starts_with("own-") { "own " } else { "" };
            (format!("{own}m: {ty}<Int, Int>"), "m", "make()")
        };
        format!(
            r#"fun make(): {ty}<Int, Int> {{
    var m = mutableMapOf<Int, Int>()
    m.put(1, 2)
    m.put(3, 4)
    return {result}
}}
fun scan({parameter}): Unit {{
    {before}
    for ((k, v) in {iterable}) {{ {body} }}
    {after}
}}
fun entry(): Unit {{ scan({argument}) }}
"#
        )
    }

    fn verified(mode: &str, text: &str, expected: fn(&Operation) -> bool) {
        let analysis = UnitSource::analyze("map_scalar.ko", text).expect("strict source admission");
        analysis.assert_frontend_clean();
        let declaration = analysis.declaration("entry", SymbolKind::Function);
        let (program, entry) = analysis
            .lower_entry(declaration)
            .expect("Map/scalar composition lowers");
        verify_program(&program).expect("SSA verification");
        let (again, _) = analysis
            .lower_entry(declaration)
            .expect("deterministic second lowering");
        assert_text_eq!(render_program(&program), render_program(&again));
        let scan_span =
            analysis.declaration_span(analysis.declaration("scan", SymbolKind::Function));
        let scan = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.origin == crate::ssa::model::Origin::Source(scan_span))
            .expect("actual source scan is reachable");
        let operations: Vec<_> = scan
            .instructions
            .iter()
            .map(|instruction| &instruction.operation)
            .collect();
        assert!(
            operations
                .iter()
                .any(|op| matches!(op, Operation::MapIterationValueLoan { .. })),
            "{mode}: provider must remain present"
        );
        assert!(
            operations.iter().any(|op| expected(op)),
            "{mode}: scalar operation must remain in scan"
        );
        crate::llvm::render_verified_program_with_entry(&program, entry)
            .expect("LLVM verification");
    }

    fn native(mode: &str, text: &str, stdout: &str) {
        let memory = super::super::emit_link_and_run("map_scalar.ko", text, "entry");
        super::super::assert_result(mode, "memory", &memory, stdout);
        let rooted = super::super::run_unit_with_entry(mode, "map_scalar.ko", text, "entry");
        super::super::assert_result(mode, "rooted", &rooted, stdout);
    }

    #[test]
    fn map_for_scalar_invert_reaches_verified_llvm() {
        for mode in MODES {
            let text = source(
                mode,
                "",
                "val x = v.inv(); if (x == -3 || x == -5) { println(\"inv\") }",
                "",
            );
            verified(mode, &text, |op| matches!(op, Operation::IntegerNot { .. }));
        }
    }

    #[test]
    fn map_for_scalar_invert_runs_natively() {
        for mode in MODES {
            let text = source(
                mode,
                "",
                "val x = v.inv(); if (x == -3 || x == -5) { println(\"inv\") }",
                "",
            );
            native(mode, &text, "inv\ninv\n");
        }
    }

    #[test]
    fn map_for_scalar_conversion_reaches_verified_llvm() {
        for mode in MODES {
            let text = source(
                mode,
                "",
                "val x = v.toLong(); if (x == 2L || x == 4L) { println(\"long\") }",
                "",
            );
            verified(mode, &text, |op| {
                matches!(op, Operation::CheckedIntegerConvert { .. })
            });
        }
    }

    #[test]
    fn map_for_scalar_conversion_runs_natively() {
        for mode in MODES {
            let text = source(
                mode,
                "",
                "val x = v.toLong(); if (x == 2L || x == 4L) { println(\"long\") }",
                "",
            );
            native(mode, &text, "long\nlong\n");
        }
    }

    #[test]
    fn map_for_scalar_range_before_loop_reaches_verified_llvm() {
        for mode in MODES {
            let text = source(
                mode,
                "val range = 0..<3",
                "val x = v + 1; if (x == 3 || x == 5) { println(\"add\") }",
                "if (1 in range) { println(\"range\") }",
            );
            verified(mode, &text, |op| {
                matches!(op, Operation::IntegerRangeConstruct { .. })
            });
        }
    }

    #[test]
    fn map_for_scalar_range_before_loop_runs_natively() {
        for mode in MODES {
            let text = source(
                mode,
                "val range = 0..<3",
                "val x = v + 1; if (x == 3 || x == 5) { println(\"add\") }",
                "if (1 in range) { println(\"range\") }",
            );
            native(mode, &text, "add\nadd\nrange\n");
        }
    }

    #[test]
    fn map_for_scalar_plain_add_keeps_existing_native_behavior() {
        for mode in MODES {
            let text = source(
                mode,
                "",
                "val x = v + 1; if (x == 3 || x == 5) { println(\"add\") }",
                "",
            );
            native(mode, &text, "add\nadd\n");
        }
    }

    #[test]
    fn map_for_scalar_invert_control_exits_run_natively() {
        for mode in MODES {
            for (exit, stdout) in [
                ("continue", "inv\ninv\nafter\n"),
                ("break", "inv\nafter\n"),
                ("return", "inv\n"),
            ] {
                let body = format!(
                    "val x = v.inv(); if (x == -3 || x == -5) {{ println(\"inv\") }}; {exit}"
                );
                native(mode, &source(mode, "", &body, "println(\"after\")"), stdout);
            }
        }
    }
}
