//! Ordinary source observations isolating source and returned-owner boundaries.
use super::{analyze, lower_scalar_file};
use crate::ssa::map_original_observation_test_support as original;

fn observe(name: &str, text: &str, supported: bool) {
    let analysis = analyze(text);
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{name}: {:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{name}: {:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{name}: {:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{name}: {:?}",
        analysis.owned.diagnostics()
    );
    match lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) {
        Ok(program) => {
            assert!(supported, "{name}: unexpectedly accepted");
            crate::llvm::render_verified_program(&program).unwrap();
            use crate::ssa::model::{Operation, TerminatorKind};
            for function in program.modules.iter().flat_map(|m| &m.functions) {
                for block in &function.blocks {
                    if let TerminatorKind::Return { values } =
                        &block.terminator.as_ref().unwrap().kind
                    {
                        for value in values {
                            assert!(!function.instructions.iter().any(|i| matches!(i.operation, Operation::Drop { owner } if owner == *value)), "returned owner is never also dropped");
                        }
                    }
                }
            }
            println!("CONTROL {name}: VERIFIED_LLVM");
        }
        Err(error) => {
            assert!(!supported, "{name}: {error:?}");
            assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
            let span = error.span.unwrap();
            let expected = if name.starts_with("owned-source") {
                "keys"
            } else if name.starts_with("outer-resource") {
                "outer"
            } else {
                "cache"
            };
            assert_eq!(
                &text[span.start()..span.end()],
                expected,
                "{name}: precise rejected owner"
            );
            println!("CONTROL {name}: {error:?}");
        }
    }
}

// Only this all-success whole uses Unit. Mixed and negative observations keep
// the original observe path, including their exact rejection owner and phase.
fn observe_owned_source_unit(name: &str, text: &str) {
    use crate::{
        native_tests::boxed_enum_tests::unit_source::UnitSource,
        ssa::model::{Operation, Origin, TerminatorKind},
    };
    use lang_frontend::name_resolution::SymbolKind;

    let analysis = UnitSource::analyze("lowering.ko", text)
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    analysis.assert_frontend_clean();
    let declaration = analysis.declaration("work", SymbolKind::Function);
    let (program, entry) = analysis
        .lower_entry(declaration)
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    assert_eq!(
        program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap()
            .origin,
        Origin::Source(analysis.declaration_span(declaration))
    );
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    for function in program.modules.iter().flat_map(|module| &module.functions) {
        for block in &function.blocks {
            if let TerminatorKind::Return { values } = &block.terminator.as_ref().unwrap().kind {
                for value in values {
                    assert!(
                        !function.instructions.iter().any(|instruction| {
                            matches!(instruction.operation, Operation::Drop { owner } if owner == *value)
                        }),
                        "returned owner is never also dropped"
                    );
                }
            }
        }
    }
    println!("CONTROL {name}: VERIFIED_LLVM");
}

#[test]
fn ordinary_source_controls_owned_source_return() {
    for own in [false, true] {
        for live in [false, true] {
            for map in [false, true] {
                let parameter = if own { "own keys: Array<Int>" } else { "" };
                let source = if own {
                    ""
                } else {
                    "val keys = arrayOf(1, 2, 3);"
                };
                let before = if map {
                    "var cache = mutableMapOf<Int, String>();"
                } else {
                    ""
                };
                let update = if map {
                    "cache.put(key, \"value\".clone());"
                } else {
                    ""
                };
                let tail = if live { "keys.size;" } else { "" };
                let map_tail = if map { "cache.contains(1);" } else { "" };
                let text = format!(
                    "fun work({parameter}): Unit {{ {source} {before} for (key in keys) {{ {update} if (key == 2) {{ return }}; println(\"tail\") }}; {tail} {map_tail} }}"
                );
                observe_owned_source_unit(
                    &format!("owned-source-own={own}-live={live}-map={map}"),
                    &text,
                );
            }
        }
    }
}

#[test]
fn ordinary_source_controls_owned_return_operand() {
    let mut cases = Vec::new();
    for map in [false, true] {
        for grouped in [false, true] {
            for kind in ["string", "value", "removed", "map"] {
                if (kind == "removed" || kind == "map") && !map {
                    continue;
                }
                let (ty, binding, operand, fallback) = match kind {
                    "string" => (
                        "String",
                        "val result = \"result\".clone();",
                        "result",
                        "\"fallback\".clone()",
                    ),
                    "value" => (
                        "Text",
                        "val result = Text(\"result\".clone());",
                        "result",
                        "Text(\"fallback\".clone())",
                    ),
                    "removed" => ("String?", "", "cache.remove(1)", "null"),
                    "map" => ("MutableMap<Int, String>", "", "cache", "cache"),
                    _ => unreachable!(),
                };
                let before = if map {
                    "var cache = mutableMapOf<Int, String>();"
                } else {
                    ""
                };
                let update = if map {
                    "cache.put(key, \"value\".clone());"
                } else {
                    ""
                };
                let operand = if grouped {
                    format!("({operand})")
                } else {
                    operand.to_owned()
                };
                let text = format!(
                    "value class Text(val word: String)\nfun work(): {ty} {{ {before} for (key in arrayOf(1, 2, 3)) {{ {update} {binding} if (key == 2) {{ return {operand} }}; println(\"tail\") }}; return {fallback} }}"
                );
                cases.push((
                    format!("operand-{kind}-map={map}-grouped={grouped}"),
                    text,
                    kind != "removed",
                ));
            }
        }
    }
    let inputs = cases
        .iter()
        .map(|(name, text, _)| original::Input::new(name, text, &["work"]))
        .collect::<Vec<_>>();
    let observation = original::observe_with(
        "M24",
        concat!(
            module_path!(),
            "::ordinary_source_controls_owned_return_operand"
        ),
        &inputs,
        |_, _, program, _| {
            use crate::ssa::model::{Operation, TerminatorKind};
            Some(program.modules.iter().flat_map(|module| &module.functions).all(|function| {
            function.blocks.iter().all(|block| match &block.terminator.as_ref().unwrap().kind {
                TerminatorKind::Return { values } => values.iter().all(|value| {
                    !function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::Drop { owner } if owner == *value))
                }),
                _ => true,
            })
        }))
        },
    );
    for (name, text, supported) in &cases {
        observe(name, text, *supported);
    }
    observation.finish();
}

#[test]
fn ordinary_source_controls_outer_resource_and_updating_operand() {
    let outer =
        include_str!("../../tests/fixtures/map-conditional-return-0313/outer-resource-boundary.ko");
    let without_map = outer
        .replace("    var cache = mutableMapOf<Int, String>()\n", "")
        .replace("        cache.put(1, \"head\".clone())\n", "")
        .replace("        cache.put(9, \"tail\".clone())\n", "")
        .replace(
            "    if (cache.contains(9)) { println(\"tail kept\") }\n",
            "",
        );
    let updating = include_str!(
        "../../tests/fixtures/map-conditional-return-0313/updating-operand-boundary.ko"
    );
    let observation = original::observe_with(
        "M25",
        concat!(
            module_path!(),
            "::ordinary_source_controls_outer_resource_and_updating_operand"
        ),
        &[
            original::Input::new("outer-resource-map", outer, &["operand", "cleanup"]),
            original::Input::new(
                "outer-resource-no-map",
                &without_map,
                &["operand", "cleanup"],
            ),
            original::Input::new("updating-return-operand", updating, &["updatingOperand"]),
        ],
        |_, _, program, _| {
            use crate::ssa::model::{Operation, TerminatorKind};
            Some(program.modules.iter().flat_map(|module| &module.functions).all(|function| {
            function.blocks.iter().all(|block| match &block.terminator.as_ref().unwrap().kind {
                TerminatorKind::Return { values } => values.iter().all(|value| {
                    !function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::Drop { owner } if owner == *value))
                }),
                _ => true,
            })
        }))
        },
    );
    observe("outer-resource-map", outer, false);
    observe("outer-resource-no-map", &without_map, false);
    observe("updating-return-operand", updating, true);
    observation.finish();
}

#[test]
fn ordinary_source_controls_return_shapes_remain_bounded() {
    let mut sources = Vec::new();
    for exit in [
        "return",
        "if (key == 1) { return }; if (key == 2) { return }",
        "if (key == 1) { return }; if (key == 2) { break }",
        "if (key == 1) { return }; if (key == 2) { continue }",
    ] {
        let text = format!(
            "fun work(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2, 3)) {{ cache.put(key, \"head\".clone()); {exit} }}; cache.contains(1) }}"
        );
        sources.push(text);
    }
    let observation = original::observe(
        "M26",
        concat!(
            module_path!(),
            "::ordinary_source_controls_return_shapes_remain_bounded"
        ),
        &[
            original::Input::new("unconditional-return", &sources[0], &["work"]),
            original::Input::new("two-conditional-returns", &sources[1], &["work"]),
            original::Input::new("return-then-break", &sources[2], &["work"]),
            original::Input::new("return-then-continue", &sources[3], &["work"]),
        ],
    );
    for text in &sources {
        observe("unsupported-return-shape", text, false);
    }
    observation.finish();
}

// This FE-only observer keeps the original single source and ownership phase.
// Its diagnostics remain a contains-code contract, not a unique-diagnostic oracle.
fn active_source_unit_ownership(
    text: &str,
) -> lang_frontend::ownership_checking::CompilationUnitOwnership {
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::check_compilation_unit_ownership,
        source::SourceMap,
        type_checking::{
            check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
        },
    };

    let mut sources = SourceMap::new();
    let (source, parsed) =
        crate::ssa::unit_lower_test_support::parsed(&mut sources, "lowering.ko", text);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().unwrap();
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    {
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        assert!(complete.is_same_analysis(&raw));
        assert!(std::ptr::eq(complete.types(), &raw));
        assert!(raw.is_compatible_with(&sources, &inputs, &names, &types));
    }
    let typed = raw.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(owned.is_compatible_with(&typed));
    owned
}

#[test]
fn ordinary_source_controls_active_source_and_element_diagnostics() {
    for (text, code) in [
        (
            "fun work(own keys: Array<Int>): Array<Int> { for (key in keys) { if (key == 1) { return keys } }; return keys }",
            "L0135",
        ),
        (
            "fun work(keys: Array<String>): String { for (key in keys) { if (key == \"stop\") { return key } }; return \"fallback\".clone() }",
            "L0133",
        ),
        (
            "fun work(): Unit { var keys = mutableListOf(1, 2); for (key in keys) { if (key == 1) { keys.add(2); return } } }",
            "L0135",
        ),
    ] {
        let owned = active_source_unit_ownership(text);
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == code),
            "{:?}",
            owned.diagnostics()
        );
    }
}
