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
    let text = format!("package p\n{text}\nfun main(): Unit {{ updatingOperand(2) }}");
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
                kind == "item",
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
