//! Ordinary source observations isolating source and returned-owner boundaries.
use super::{analyze, lower_scalar_file};

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
                observe(
                    &format!("owned-source-own={own}-live={live}-map={map}"),
                    &text,
                    true,
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
                observe(
                    &format!("operand-{kind}-map={map}-grouped={grouped}"),
                    &text,
                    kind != "removed",
                );
            }
        }
    }
}

#[test]
fn ordinary_source_controls_outer_resource_and_updating_operand() {
    let outer =
        include_str!("../../tests/fixtures/map-conditional-return-0313/outer-resource-boundary.ko");
    observe("outer-resource-map", outer, false);
    let without_map = outer
        .replace("    var cache = mutableMapOf<Int, String>()\n", "")
        .replace("        cache.put(1, \"head\".clone())\n", "")
        .replace("        cache.put(9, \"tail\".clone())\n", "")
        .replace(
            "    if (cache.contains(9)) { println(\"tail kept\") }\n",
            "",
        );
    observe("outer-resource-no-map", &without_map, false);
    observe(
        "updating-return-operand",
        include_str!(
            "../../tests/fixtures/map-conditional-return-0313/updating-operand-boundary.ko"
        ),
        true,
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
        observe("unsupported-return-shape", &text, false);
    }
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
        let analysis = analyze(text);
        assert!(
            analysis.parsed.diagnostics().is_empty(),
            "{:?}",
            analysis.parsed.diagnostics()
        );
        assert!(
            analysis.names.diagnostics().is_empty(),
            "{:?}",
            analysis.names.diagnostics()
        );
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis
                .owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == code),
            "{:?}",
            analysis.owned.diagnostics()
        );
    }
}
