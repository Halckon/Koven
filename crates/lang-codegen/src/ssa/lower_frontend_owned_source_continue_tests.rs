//! SPEC-0312: real ordinary source, no Map and no synthetic facts.
#[path = "lower_frontend_owned_source_structure_tests.rs"]
mod structure;
use super::{analyze, lower_scalar_file};
use lang_frontend::ownership_checking::{CleanupOwnerValue, IterationPhiBoundary};

fn check(live: bool, factory: &str) {
    check_with_decoy(live, factory, false);
}

fn check_with_decoy(live: bool, factory: &str, decoy: bool) {
    let text = format!(
        "fun main(): Unit {{ val keys = {factory}(1, 2, 3); for (key in keys) {{ {decoy_body}if (key == 2) {{ continue }} }}; {} }}",
        if live { "keys.size" } else { "" },
        decoy_body = if decoy {
            format!("val other = {factory}(9); other.size; ")
        } else {
            String::new()
        }
    );
    let analysis = analyze(&text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let fact = analysis.owned.drops().iter().find(|fact| {
            matches!(fact.owner().and_then(|owner| analysis.owned.cleanup_conditions().owner_value(owner)),
                Some(CleanupOwnerValue::IterationPhi { boundary, .. })
                if *boundary == if live { IterationPhiBoundary::Exit } else { IterationPhiBoundary::Header })
        }).expect("real source phi cleanup");
    assert!(fact.condition().is_some());
    let result = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    );
    if let Err(error) = &result {
        assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
        assert_eq!(analysis.sources.slice(error.span.unwrap()).unwrap(), "keys");
    }
    let program = result.unwrap_or_else(|error| panic!("{factory} live={live}: {error:?}"));
    crate::llvm::render_verified_program(&program).unwrap();
    let plan = &analysis.owned.iterations()[0];
    let entry = plan
        .closure_phi_incomings()
        .iter()
        .find(|edge| {
            edge.kind() == lang_frontend::ownership_checking::IterationPhiIncomingKind::Entry
        })
        .unwrap();
    let source_symbol = match plan.source() {
        lang_frontend::ownership_checking::LoanTarget::Place(place) => place.root(),
        _ => unreachable!(),
    };
    let header = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.symbol() == source_symbol && phi.boundary() == IterationPhiBoundary::Header)
        .unwrap();
    let seed = entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == header.owner())
        .unwrap()
        .values()[0]
        .source();
    let Some(CleanupOwnerValue::Expression { expression, .. }) =
        analysis.owned.cleanup_conditions().owner_value(seed)
    else {
        panic!("real source seed")
    };
    let seed_span = analysis
        .parsed
        .ast()
        .expressions()
        .get(*expression)
        .unwrap()
        .span();
    structure::check(&program, live, seed_span);
}

#[test]
fn owned_source_continue_array_live_without_map() {
    check(true, "arrayOf");
}
#[test]
fn owned_source_continue_array_dead_without_map() {
    check(false, "arrayOf");
}
#[test]
fn owned_source_continue_list_live_without_map() {
    check(true, "listOf");
}
#[test]
fn owned_source_continue_list_dead_without_map() {
    check(false, "listOf");
}
#[test]
fn owned_source_continue_mutable_list_live_without_map() {
    check(true, "mutableListOf");
}
#[test]
fn owned_source_continue_mutable_list_dead_without_map() {
    check(false, "mutableListOf");
}
fn assert_unit_boundary(live: bool, constants: bool) {
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
    let text = format!(
        "package p\nfun main(): Unit {{ val keys = arrayOf(1, 2, 3); for (key in keys) {{ if (key == 2) {{ continue }} }}; {} }}",
        if live { "keys.size" } else { "" }
    );
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
        crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    } else {
        let typed = result.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    };
    let (program, entry) =
        result.unwrap_or_else(|error| panic!("unit constants={constants}, live={live}: {error:?}"));
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
}
#[test]
fn owned_source_continue_unit_basic_dead() {
    assert_unit_boundary(false, false);
}
#[test]
fn owned_source_continue_unit_basic_live() {
    assert_unit_boundary(true, false);
}
#[test]
fn owned_source_continue_unit_constant_dead() {
    assert_unit_boundary(false, true);
}
#[test]
fn owned_source_continue_unit_constant_live() {
    assert_unit_boundary(true, true);
}

// Exact outcomes; the original baseline observations remain outside the repository.
#[test]
fn owned_source_continue_nested_scope_boundaries() {
    for (label, text) in [
        (
            "inner_fresh",
            "fun main(): Unit { for (outer in arrayOf(1, 2)) { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { continue } }; keys.size } }",
        ),
        (
            "outer_independent_inner",
            "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { for (other in arrayOf(3)) { other }; if (key == 2) { continue } }; keys.size }",
        ),
        (
            "same_source_nested",
            "fun main(): Unit { val keys = arrayOf(1, 2); for (outer in keys) { for (key in keys) { if (key == 2) { continue } } }; keys.size }",
        ),
        (
            "sequential_same_source",
            "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { continue } }; for (key in keys) { if (key == 2) { continue } }; keys.size }",
        ),
    ] {
        let analysis = analyze(text);
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert!(analysis.owned.diagnostics().is_empty());
        let result = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        );
        if label == "inner_fresh" {
            crate::llvm::render_verified_program(&result.expect("independent fresh inner owner"))
                .unwrap();
        } else {
            let error = result
                .err()
                .expect("cross-loop owner proof remains unsupported");
            assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
            assert_eq!(analysis.sources.slice(error.span.unwrap()).unwrap(), "keys");
        }
    }
}

#[test]
fn owned_source_continue_frontend_source_loan_blocks_move_replace_and_mutation() {
    for (factory, operation) in [
        ("arrayOf", "consume(keys)"),
        ("arrayOf", "keys[0] = 9"),
        ("mutableListOf", "keys.add(4)"),
        ("mutableListOf", "keys.removeLast()"),
    ] {
        let text = format!(
            "fun consume(own keys: Array<Int>): Unit {{}}\nfun main(): Unit {{ var keys = {factory}(1, 2, 3); for (key in keys) {{ {operation}; if (key == 2) {{ continue }} }} }}"
        );
        let analysis = analyze(&text);
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert!(
            analysis
                .owned
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0135"),
            "{:?}",
            analysis.owned.diagnostics()
        );
    }
}

#[test]
fn owned_source_continue_same_type_decoy_has_distinct_owner_lineage() {
    check_with_decoy(true, "arrayOf", true);
    check_with_decoy(false, "arrayOf", true);
}

#[test]
fn owned_source_continue_fresh_inner_owner_structure() {
    use lang_frontend::ownership_checking::{DropTarget, IterationPhiIncomingKind, LoanTarget};
    let text = include_str!("../../tests/fixtures/owned-source-continue-0312/fresh-inner.ko");
    let analysis = analyze(text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    let mut count = 0;
    for plan in analysis.owned.iterations() {
        let LoanTarget::Place(place) = plan.source() else {
            continue;
        };
        let symbol = place.root();
        if analysis.names.symbols()[symbol.index()].name() != "keys" {
            continue;
        }
        count += 1;
        let header = plan
            .closure_phis()
            .iter()
            .find(|phi| phi.symbol() == symbol && phi.boundary() == IterationPhiBoundary::Header)
            .unwrap();
        let entry = plan
            .closure_phi_incomings()
            .iter()
            .find(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
            .unwrap();
        let seed = entry
            .bindings()
            .iter()
            .find(|binding| binding.target() == header.owner())
            .unwrap()
            .values()[0]
            .source();
        let Some(CleanupOwnerValue::Expression { expression, .. }) =
            analysis.owned.cleanup_conditions().owner_value(seed)
        else {
            panic!("fresh expression seed, never outer phi")
        };
        let span = analysis
            .parsed
            .ast()
            .expressions()
            .get(*expression)
            .unwrap()
            .span();
        let fact = analysis
            .owned
            .drops()
            .iter()
            .find(|fact| fact.target() == DropTarget::Named(symbol))
            .unwrap();
        let live = matches!(
            analysis
                .owned
                .cleanup_conditions()
                .owner_value(fact.owner().unwrap()),
            Some(CleanupOwnerValue::IterationPhi {
                boundary: IterationPhiBoundary::Exit,
                ..
            })
        );
        structure::check(&program, live, span);
    }
    assert_eq!(count, 2);
}

#[test]
fn owned_source_continue_existing_boundaries_remain_explicit() {
    for (label, text) in [
        (
            "index_without_loop",
            "fun main(): Unit { var keys = arrayOf(1, 2, 3); keys[0] = 9 }",
        ),
        (
            "resource_source",
            "class Item(val id: Int) { deinit() {} }\nfun main(): Unit { val keys = arrayOf(Item(1), Item(2)); for (key in keys) { if (key.id == 2) { continue } }; keys.size }",
        ),
        (
            "two_continue",
            "fun main(): Unit { val keys = arrayOf(1, 2, 3); for (key in keys) { if (key == 1) { continue }; if (key == 2) { continue } }; keys.size }",
        ),
        (
            "break",
            "fun main(): Unit { val keys = arrayOf(1, 2, 3); for (key in keys) { if (key == 2) { break } }; keys.size }",
        ),
        (
            "return",
            "fun main(): Unit { val keys = arrayOf(1, 2, 3); for (key in keys) { if (key == 2) { return } }; keys.size }",
        ),
        (
            "inout",
            "fun scan(inout keys: Array<Int>): Unit { for (key in keys) { if (key == 2) { continue } }; keys.size }",
        ),
        (
            "field",
            "class Holder(val keys: Array<Int>) { fun scan(): Unit { for (key in this.keys) { if (key == 2) { continue } } } }",
        ),
        (
            "replacement_seed",
            "fun main(): Unit { var keys = arrayOf(1, 2); keys = arrayOf(2, 3); for (key in keys) { if (key == 2) { continue } }; keys.size }",
        ),
    ] {
        let analysis = analyze(text);
        assert!(
            analysis.parsed.diagnostics().is_empty(),
            "{label}: {:?}",
            analysis.parsed.diagnostics()
        );
        assert!(
            analysis.names.diagnostics().is_empty(),
            "{label}: {:?}",
            analysis.names.diagnostics()
        );
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{label}: {:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{label}: {:?}",
            analysis.owned.diagnostics()
        );
        let result = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        );
        let error = result
            .err()
            .unwrap_or_else(|| panic!("{label} remains outside the source proof"));
        assert_eq!(
            error.kind,
            if label == "field" {
                crate::ssa::LoweringErrorKind::MissingFact
            } else {
                crate::ssa::LoweringErrorKind::UnsupportedNode
            }
        );
        let actual = analysis.sources.slice(error.span.unwrap()).unwrap();
        match label {
            "index_without_loop" => assert_eq!(actual, "keys[0]"),
            "inout" => assert_eq!(actual, text),
            "field" => assert!(actual.starts_with("for (key in this.keys)")),
            _ => assert_eq!(actual, "keys"),
        }
    }
}

#[test]
fn owned_source_continue_declaration_identity_and_groups() {
    for text in [
        "fun main(): Unit { var keys = ((arrayOf(1, 2))); for (key in keys) { if (key == 2) { continue } }; keys.size }",
        "fun first(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { continue } }; keys.size }\nfun main(): Unit { val keys = listOf(1, 2); for (key in keys) { if (key == 2) { continue } }; keys.size; first() }",
    ] {
        let analysis = analyze(text);
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert!(analysis.owned.diagnostics().is_empty());
        let program = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
    }
    let text = "fun main(): Unit { val original = arrayOf(1, 2); val keys = original; for (key in keys) { if (key == 2) { continue } }; keys.size }";
    let analysis = analyze(text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let error = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .err()
    .expect("alias is not a fresh initializer seed");
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
}

#[test]
fn owned_source_continue_borrowed_element_cannot_be_owned() {
    let text = "fun consume(own text: String): Unit {}\nfun main(): Unit { val keys = arrayOf(\"one\", \"two\"); for (key in keys) { consume(key); if (key == \"two\") { continue } } }";
    let analysis = analyze(text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(
        analysis
            .owned
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0133")
    );
}

#[test]
fn owned_source_continue_external_owner_and_short_circuit_boundaries() {
    for (expected, text) in [
        (
            "keys",
            "fun main(): Unit { val keys = arrayOf(9); keys.size; { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { continue } }; keys.size } }",
        ),
        (
            "",
            "fun main(): Unit { val keys = arrayOf(1, 2, 3); var first = mutableMapOf<Int, String>(); var second = mutableMapOf<Int, String>(); for (key in keys) { first.put(key, \"first\".clone()); second.put(key, \"second\".clone()); if (key == 2) { continue }; println(\"tail\") } }",
        ),
        (
            "second",
            "fun main(): Unit { val keys = arrayOf(1, 2, 3); var first = mutableMapOf<Int, String>(); var second = mutableMapOf<Int, String>(); for (key in keys) { first.put(key, \"first\".clone()); second.put(key, \"second\".clone()); if (key == 2) { continue }; first.put(9, \"tail one\".clone()); second.put(9, \"tail two\".clone()) }; if (first.contains(3) && second.contains(3)) { println(\"maps\") } }",
        ),
    ] {
        let analysis = analyze(text);
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert!(analysis.owned.diagnostics().is_empty());
        let result = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        );
        if expected.is_empty() {
            crate::llvm::render_verified_program(
                &result.expect("dead Map without post-control use stays supported"),
            )
            .unwrap();
            continue;
        }
        let error = result
            .err()
            .expect("unrelated conditional owner remains a boundary");
        assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
        assert_eq!(
            analysis.sources.slice(error.span.unwrap()).unwrap(),
            expected
        );
    }
}

#[test]
fn owned_source_continue_range_view_remains_explicit() {
    use lang_frontend::{
        lexer::lex,
        name_resolution::resolve_names,
        ownership_checking::check_ownership,
        parser::parse_file,
        source::SourceMap,
        type_checking::{check_types, standard_environments},
    };
    let mut sources = SourceMap::new();
    let text = format!(
        "{}\nfun main(): Unit {{ val keys = listOf(1, 2); for (key in take(keys, 1)) {{ if (key == 2) {{ continue }} }}; keys.size }}",
        include_str!("../../../lang-std/koven/algorithms/ranges.ko")
            .strip_prefix("package koven.algorithms\n")
            .unwrap()
    );
    let source = sources.add_source("trusted.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    types
        .authorize_range_extension_source(&sources, source)
        .unwrap();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let error = lower_scalar_file(&sources, &parsed, &names, &typed, &owned)
        .err()
        .expect("RangeView is not a Sequential source proof");
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
    assert_eq!(sources.slice(error.span.unwrap()).unwrap(), "keys");
}
