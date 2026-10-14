//! SPEC-0318: real-source last-use baseline; no synthesized ownership facts.
use super::{analyze, lower_scalar_file};
use lang_frontend::ownership_checking::{DropPoint, DropTarget, IterationExitKind};

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

fn assert_single(text: &str, name: &str, supported: bool) {
    let a = analyze(text);
    assert!(
        a.parsed.diagnostics().is_empty(),
        "{name}: {:?}",
        a.parsed.diagnostics()
    );
    assert!(
        a.names.diagnostics().is_empty(),
        "{name}: {:?}",
        a.names.diagnostics()
    );
    assert!(
        a.typed.diagnostics().is_empty(),
        "{name}: {:?}",
        a.typed.diagnostics()
    );
    assert!(
        a.owned.diagnostics().is_empty(),
        "{name}: {:?}",
        a.owned.diagnostics()
    );
    let plan = &a.owned.iterations()[0];
    assert_eq!(plan.exits().len(), 3);
    if name.starts_with("no-map") {
        assert!(plan.closure_phis().is_empty());
        assert!(
            plan.closure_phi_incomings()
                .iter()
                .all(|edge| edge.bindings().is_empty())
        );
    } else {
        assert_eq!(
            plan.closure_phi_incomings().len(),
            3,
            "Return never feeds phi"
        );
    }
    let exit = plan
        .exits()
        .iter()
        .find(|e| matches!(e.kind(), IterationExitKind::Return(_)))
        .unwrap();
    let IterationExitKind::Return(return_expression) = exit.kind() else {
        unreachable!()
    };
    let return_node = a.parsed.ast().expressions().get(return_expression).unwrap();
    println!(
        "FACTS {name}: return={return_expression:?} {:?} {:?}; plan={plan:#?}; conditions={:#?}",
        return_node.span(),
        return_node.payload(),
        a.owned.cleanup_conditions()
    );
    for fact in a.owned.drops() {
        println!(
            "DROP {name}: {fact:?}; point_source={}",
            match fact.point() {
                DropPoint::CallReturn(id)
                | DropPoint::AfterExpression(id)
                | DropPoint::ControlTransfer(id) => {
                    let span = a.parsed.ast().expressions().get(id).unwrap().span();
                    &text[span.start()..span.end()]
                }
                _ => "other",
            }
        );
    }
    let result = lower_scalar_file(&a.sources, &a.parsed, &a.names, &a.typed, &a.owned);
    println!(
        "RESULT single {name}: {:?}",
        result.as_ref().map(|_| "VERIFIED")
    );
    match result {
        Ok(program) => {
            assert!(supported, "{name}: unexpectedly accepted");
            crate::llvm::render_verified_program(&program).unwrap();
            assert_eq!(
                super::operand_structure::assert_source(&program, &a.parsed),
                usize::from(name.starts_with("remove") || name.starts_with("contains"))
            );
        }
        Err(error) => {
            assert!(!supported, "{name}: {error:?}");
            assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
            let span = error.span.unwrap();
            if name.starts_with("no-map") {
                assert_eq!(span.start(), 0);
                assert_eq!(
                    &text[span.start()..span.end()],
                    text.lines().next().unwrap()
                );
            } else {
                assert_eq!(&text[span.start()..span.end()], "cache");
                assert!(
                    a.owned
                        .drops()
                        .iter()
                        .any(|fact| matches!(fact.target(), DropTarget::Named(_))
                            && fact.value_origin() == span)
                );
            }
        }
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
            let span = error.span.unwrap();
            assert_eq!(span.start(), "package p\n".len());
            assert_eq!(
                &text[span.start()..span.end()],
                text.lines().nth(1).unwrap()
            );
        }
    }
}

#[test]
fn map_operand_last_use_single_baseline() {
    for live in [false, true] {
        for kind in ["remove", "contains", "item", "independent-nullable"] {
            assert_single(
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
    assert_single(&source("no-map", false), "no-map-nullable", false);
}

#[test]
fn map_operand_last_use_remove_live_red() {
    assert_single(ORIGINAL, "remove-live-goal", true);
}
#[test]
fn map_operand_last_use_remove_dead_red() {
    assert_single(&source("remove", false), "remove-dead-goal", true);
}
#[test]
fn map_operand_last_use_contains_live_red() {
    assert_single(&source("contains", true), "contains-live-goal", true);
}
#[test]
fn map_operand_last_use_contains_dead_red() {
    assert_single(&source("contains", false), "contains-dead-goal", true);
}

fn assert_boundary(text: &str, kind: crate::ssa::LoweringErrorKind, expected: &str) {
    let a = analyze(text);
    assert!(
        a.parsed.diagnostics().is_empty(),
        "{:?}",
        a.parsed.diagnostics()
    );
    assert!(
        a.names.diagnostics().is_empty(),
        "{:?}",
        a.names.diagnostics()
    );
    assert!(
        a.typed.diagnostics().is_empty(),
        "{:?}",
        a.typed.diagnostics()
    );
    assert!(
        a.owned.diagnostics().is_empty(),
        "{:?}",
        a.owned.diagnostics()
    );
    let error = lower_scalar_file(&a.sources, &a.parsed, &a.names, &a.typed, &a.owned)
        .err()
        .expect("bounded source remains rejected");
    println!("BOUNDARY {expected}: {error:?}");
    assert_eq!(error.kind, kind);
    let span = error.span.unwrap();
    assert_eq!(&text[span.start()..span.end()], expected);
}

#[test]
fn map_operand_last_use_same_guard_outside_return_is_rejected() {
    let text = ORIGINAL.replace(
        "return if (cache.remove(item) != null) { println(\"removed operand\"); item } else { 0 }",
        "cache.contains(item); return item",
    );
    assert_boundary(
        &text,
        crate::ssa::LoweringErrorKind::UnsupportedNode,
        "cache",
    );
}

#[test]
fn map_operand_last_use_checked_operand_abort_path_is_rejected() {
    let text = ORIGINAL.replace("cache.remove(item)", "cache.remove(item + 100)");
    assert_boundary(&text, crate::ssa::LoweringErrorKind::MissingFact, "cache");
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
        assert_boundary(
            &text,
            crate::ssa::LoweringErrorKind::UnsupportedNode,
            "cache",
        );
    }
}

#[test]
fn map_operand_last_use_cardinality_fixture_ssa() {
    let text = include_str!("../../tests/fixtures/map-return-last-use-0318/cardinality.ko");
    let a = analyze(text);
    assert!(
        a.parsed.diagnostics().is_empty(),
        "{:?}",
        a.parsed.diagnostics()
    );
    assert!(
        a.names.diagnostics().is_empty(),
        "{:?}",
        a.names.diagnostics()
    );
    assert!(
        a.typed.diagnostics().is_empty(),
        "{:?}",
        a.typed.diagnostics()
    );
    assert!(
        a.owned.diagnostics().is_empty(),
        "{:?}",
        a.owned.diagnostics()
    );
    let program = lower_scalar_file(&a.sources, &a.parsed, &a.names, &a.typed, &a.owned).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    assert_eq!(
        super::operand_structure::assert_source(&program, &a.parsed),
        4
    );
    unit(text, false, true, "");
    unit(text, true, true, "");
}
