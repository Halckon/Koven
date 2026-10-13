//! Ordinary source observations isolating source and returned-owner boundaries.
use super::{analyze, lower_scalar_file};

fn observe(name: &str, text: &str) {
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
            crate::llvm::render_verified_program(&program).unwrap();
            println!("CONTROL {name}: VERIFIED_LLVM");
        }
        Err(error) => println!("CONTROL {name}: {error:?}"),
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
                );
            }
        }
    }
}
