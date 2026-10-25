//! SPEC-0318: real-source last-use baseline; no synthesized ownership facts.
use crate::ssa::map_unit_consumer_test_support::assert_source;

#[path = "lower_frontend_map_operand_unit_identity.rs"]
mod unit_identity;

#[path = "lower_frontend_map_nullable_operand_unit_test_support.rs"]
mod nullable_operand;
#[path = "lower_frontend_map_same_guard_unit_test_support.rs"]
mod same_guard;

const ORIGINAL: &str =
    include_str!("../../tests/fixtures/map-conditional-return-0313/updating-operand-boundary.ko");

fn source(kind: &str, live: bool) -> String {
    let text = match kind {
        "remove" => ORIGINAL.to_owned(),
        "contains" => ORIGINAL.replace("cache.remove(item) != null", "cache.contains(item)"),
        "item" => ORIGINAL.replace(
            "if (cache.remove(item) != null) { println(\"removed operand\"); item } else { 0 }",
            "item",
        ),
        "independent-nullable" => {
            ORIGINAL.replace("cache.remove(item) != null", "nullableValue(item) != null")
        }
        "no-map" => ORIGINAL
            .replace("    var cache = mutableMapOf<Int, String>()\n", "")
            .replace("        cache.put(item, \"head\".clone())\n", "")
            .replace("        cache.put(9, \"tail\".clone())\n", "")
            .replace("    cache.contains(9)\n", "")
            .replace("cache.remove(item) != null", "nullableValue(item) != null"),
        _ => unreachable!(),
    };
    let text = if live {
        text
    } else {
        text.replace("    cache.contains(9)\n", "")
    };
    if kind == "independent-nullable" || kind == "no-map" {
        format!(
            "fun nullableValue(item: Int): String? {{ return if (item == 1) {{ \"value\".clone() }} else {{ null }} }}\n{text}"
        )
    } else {
        text
    }
}

fn assert_unit(text: &str, constants: bool, supported: bool) {
    unit(
        text,
        constants,
        supported,
        "\nfun main(): Unit { updatingOperand(2) }",
    );
}

fn unit(text: &str, constants: bool, supported: bool, entry: &str) {
    use crate::ssa::unit_lower_test_support::{declaration, parsed};
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{
            check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        },
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let expected = text.matches("return if (cache.").count();
    let text = format!("package p\n{text}{entry}");
    let mut sources = SourceMap::new();
    let (source_id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source_id, &file)];
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let result = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    let entry = declaration(&names, "p", "main");
    let result = if constants {
        let typed = result.validate_constants().unwrap();
        let owned =
            check_compilation_unit_constant_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap();
        println!(
            "UNIT FACTS constants=true: drops={:#?}",
            owned.ownership().drops()
        );
        crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    } else {
        let typed = result.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        println!(
            "UNIT FACTS constants=false: drops={:#?}",
            owned.ownership().drops()
        );
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    };
    println!(
        "RESULT unit constants={constants}: {:?}",
        result.as_ref().map(|_| "VERIFIED")
    );
    match result {
        Ok((program, entry)) => {
            assert!(supported);
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            assert_eq!(
                super::operand_structure::assert_source(&program, &file),
                expected
            );
        }
        Err(error) => {
            assert!(!supported, "{error:?}");
            assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
            // SPEC-0350: `String?` now has an SSA type, so the rejection moved from the
            // signature into the body of the same unsupported `nullableValue` declaration.
            let span = error.span.unwrap();
            let line_start = "package p\n".len();
            let line = text.lines().nth(1).unwrap();
            assert!(line_start <= span.start() && span.end() <= line_start + line.len());
        }
    }
}

#[test]
fn map_operand_last_use_single_baseline() {
    for live in [false, true] {
        for kind in ["remove", "contains", "item", "independent-nullable"] {
            nullable_operand::check(
                &source(kind, live),
                &format!("{kind}-live={live}"),
                kind != "independent-nullable",
            );
        }
    }
}

#[test]
fn map_operand_last_use_unit_basic_baseline() {
    for live in [false, true] {
        for kind in [
            "remove",
            "contains",
            "item",
            "independent-nullable",
            "no-map",
        ] {
            println!("CASE unit-basic {kind}-live={live}");
            assert_unit(
                &source(kind, live),
                false,
                !matches!(kind, "independent-nullable" | "no-map"),
            );
        }
    }
}

#[test]
fn map_operand_last_use_unit_constant_baseline() {
    for live in [false, true] {
        for kind in [
            "remove",
            "contains",
            "item",
            "independent-nullable",
            "no-map",
        ] {
            println!("CASE unit-constant {kind}-live={live}");
            assert_unit(
                &source(kind, live),
                true,
                !matches!(kind, "independent-nullable" | "no-map"),
            );
        }
    }
}

#[test]
fn map_operand_last_use_no_map_nullable_control() {
    nullable_operand::check(&source("no-map", false), "no-map-nullable", false);
}

#[test]
fn map_operand_last_use_remove_live_red() {
    unit_identity::check(ORIGINAL, "updatingOperand", 1);
}
#[test]
fn map_operand_last_use_remove_dead_red() {
    unit_identity::check(&source("remove", false), "updatingOperand", 1);
}
#[test]
fn map_operand_last_use_contains_live_red() {
    unit_identity::check(&source("contains", true), "updatingOperand", 1);
}
#[test]
fn map_operand_last_use_contains_dead_red() {
    unit_identity::check(&source("contains", false), "updatingOperand", 1);
}

#[test]
fn map_operand_last_use_same_guard_outside_return_is_rejected() {
    let text = ORIGINAL.replace(
        "return if (cache.remove(item) != null) { println(\"removed operand\"); item } else { 0 }",
        "cache.contains(item); return item",
    );
    same_guard::check(&text);
}

#[test]
fn map_operand_last_use_checked_operand_abort_path_is_rejected() {
    let text = ORIGINAL.replace("cache.remove(item)", "cache.remove(item + 100)");
    assert_source(&text, &["updatingOperand"]);
}

#[test]
fn map_operand_last_use_extended_provider_and_result_shapes_are_rejected() {
    for text in [
        ORIGINAL.replace("arrayOf(1, 2, 3)", "listOf(1, 2, 3)"),
        ORIGINAL.replace(
            "    for (item in arrayOf(1, 2, 3))",
            "    val keys = arrayOf(1, 2, 3)\n    for (item in keys)",
        ),
        ORIGINAL
            .replace(
                "updatingOperand(stop: Int): Int",
                "updatingOperand(stop: Int): Boolean",
            )
            .replace("; item } else { 0 }", "; true } else { false }")
            .replace("return 0", "return false"),
    ] {
        assert_source(&text, &["updatingOperand"]);
    }
}

#[test]
fn map_operand_last_use_two_maps_and_borrowed_array_are_rejected() {
    let two_maps = ORIGINAL
        .replace(
            "    for (item",
            "    var spare = mutableMapOf<Int, String>()\n    for (item",
        )
        .replace(
            "        cache.put(item",
            "        spare.put(item, \"spare\".clone())\n        cache.put(item",
        )
        .replace(
            "        cache.put(9",
            "        spare.put(9, \"spare-tail\".clone())\n        cache.put(9",
        )
        .replace(
            "    cache.contains(9)",
            "    spare.contains(9)\n    cache.contains(9)",
        );
    let borrowed = ORIGINAL
        .replace("stop: Int", "stop: Int, keys: Array<Int>")
        .replace("arrayOf(1, 2, 3)", "keys");
    for text in [two_maps, borrowed] {
        assert_source(&text, &["updatingOperand"]);
    }
}

#[test]
fn map_operand_last_use_cardinality_fixture_ssa() {
    let text = include_str!("../../tests/fixtures/map-return-last-use-0318/cardinality.ko");
    unit_identity::check(text, "main", 4);
    unit(text, false, true, "");
    unit(text, true, true, "");
}
