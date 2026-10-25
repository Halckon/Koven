//! Ordinary source observations isolating source and returned-owner boundaries.
use crate::ssa::map_unit_consumer_test_support::{assert_outer_resource, assert_return_controls};

// The pre-existing all-success whole retains its original Unit oracle.
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
                assert_return_controls(
                    &format!("operand-{kind}-map={map}-grouped={grouped}"),
                    &text,
                    &["work"],
                );
            }
        }
    }
}

#[test]
fn ordinary_source_controls_outer_resource_and_updating_operand() {
    let outer =
        include_str!("../../tests/fixtures/map-conditional-return-0313/outer-resource-boundary.ko");
    assert_outer_resource("outer-resource-map", outer);
    let without_map = outer
        .replace("    var cache = mutableMapOf<Int, String>()\n", "")
        .replace("        cache.put(1, \"head\".clone())\n", "")
        .replace("        cache.put(9, \"tail\".clone())\n", "")
        .replace(
            "    if (cache.contains(9)) { println(\"tail kept\") }\n",
            "",
        );
    assert_outer_resource("outer-resource-no-map", &without_map);
    assert_return_controls(
        "updating-return-operand",
        include_str!(
            "../../tests/fixtures/map-conditional-return-0313/updating-operand-boundary.ko"
        ),
        &["updatingOperand"],
    );
}

#[test]
fn ordinary_source_controls_return_shapes_remain_bounded() {
    for exit in [
        "return",
        "if (key == 1) { return }; if (key == 2) { return }",
        "if (key == 1) { return }; if (key == 2) { break }",
        "if (key == 1) { return }; if (key == 2) { continue }",
    ] {
        let text = format!(
            "fun work(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2, 3)) {{ cache.put(key, \"head\".clone()); {exit} }}; cache.contains(1) }}"
        );
        assert_return_controls("unsupported-return-shape", &text, &["work"]);
    }
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
