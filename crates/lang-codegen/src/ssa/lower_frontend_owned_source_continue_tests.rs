//! SPEC-0312: real ordinary source, no Map and no synthetic facts.
#[path = "lower_frontend_owned_source_structure_tests.rs"]
mod structure;
use super::owned_source_return_tests::unit_contract::{self, Control, Expected};
use super::{analyze, lower_scalar_file};
use lang_frontend::type_checking::SequentialContainerKind;

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
    let container = match factory {
        "arrayOf" => SequentialContainerKind::Array,
        "listOf" => SequentialContainerKind::List,
        "mutableListOf" => SequentialContainerKind::MutableList,
        _ => panic!("original owned-source factory"),
    };
    let (program, origins) = unit_contract::observe(
        &text,
        "main",
        &[Expected {
            function: "main",
            live,
            container,
            own: false,
        }],
        Control::Continue,
        false,
    );
    let [origin] = origins.as_slice() else {
        panic!("one original provider");
    };
    structure::check(&program, live, origin.seed.unwrap());
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
    let text = include_str!("../../tests/fixtures/owned-source-continue-0312/fresh-inner.ko");
    let (program, origins) = unit_contract::observe(
        text,
        "main",
        &[
            Expected {
                function: "freshLive",
                live: true,
                container: SequentialContainerKind::Array,
                own: false,
            },
            Expected {
                function: "freshDead",
                live: false,
                container: SequentialContainerKind::Array,
                own: false,
            },
        ],
        Control::Continue,
        false,
    );
    assert_eq!(origins.len(), 2);
    for (origin, live) in origins.iter().zip([true, false]) {
        structure::check(&program, live, origin.seed.unwrap());
    }
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
        if label == "return" {
            crate::llvm::render_verified_program(
                &result.expect("SPEC-0314 independent return proof"),
            )
            .unwrap();
            continue;
        }
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

// These original sources have different Legacy/Unit alias outcomes. Keep both oracles.
fn assert_continue_identity_unit(text: &str, expected: &[(&str, SequentialContainerKind, usize)]) {
    use crate::{
        native_tests::boxed_enum_tests::unit_source::UnitSource,
        ssa::model::{Operation, Origin},
    };
    use lang_frontend::{
        name_resolution::{SourceUnitKey, SymbolKind},
        ownership_checking::{UnitIterationSourceAccess, UnitLoanTarget},
        type_checking::IterationProvider,
    };

    let unit = UnitSource::analyze("lowering.ko", text).expect("strict original identity source");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    let source = unit.parsed().source_id();
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), source);
    assert_eq!(unit.sources().source_name(source), Ok("lowering.ko"));
    assert_eq!(unit.sources().source_text(source), Ok(text));
    assert_eq!(unit.parsed().roots().len(), expected.len());
    assert_eq!(
        unit.basic_types().types().sequential_iterations().len(),
        expected.len()
    );
    assert_eq!(unit.ownership().iterations().len(), expected.len());
    let main = unit.declaration("main", SymbolKind::Function);
    let (program, entry) = unit
        .lower_entry(main)
        .expect("the unchanged identity source lowers");
    let [module] = program.modules.as_slice() else {
        panic!("one source module");
    };
    assert_eq!(module.functions.len(), expected.len());
    assert_eq!(
        module.function(entry).unwrap().origin,
        Origin::Source(unit.declaration_span(main))
    );
    let mut roots = Vec::new();
    for &(name, container, keys_start) in expected {
        let declaration = unit.declaration(name, SymbolKind::Function);
        let span = unit.declaration_span(declaration);
        let functions = module
            .functions
            .iter()
            .filter(|function| function.origin == Origin::Source(span))
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            panic!("one lowered original declaration {name}");
        };
        let plans = unit
            .ownership()
            .iterations()
            .iter()
            .filter(|plan| {
                let statement = plan.descriptor().statement();
                assert_eq!(statement.source_unit(), document.id());
                let loop_span = unit
                    .parsed()
                    .ast()
                    .statements()
                    .get(statement.statement())
                    .unwrap()
                    .span();
                assert_eq!(loop_span.source_id(), source);
                span.start() <= loop_span.start() && loop_span.end() <= span.end()
            })
            .collect::<Vec<_>>();
        let [plan] = plans.as_slice() else {
            panic!("one actual loop in {name}");
        };
        let descriptor = plan.descriptor();
        assert_eq!(
            unit.types().sequential_iteration(descriptor.statement()),
            Some(descriptor)
        );
        assert_eq!(
            descriptor.provider(),
            IterationProvider::Sequential(container)
        );
        assert_eq!(plan.source_access(), UnitIterationSourceAccess::Owned);
        let Some(UnitLoanTarget::Place(place)) = plan.source() else {
            panic!("the original named source root");
        };
        assert!(place.is_root());
        assert_eq!(place.root().source_unit(), document.id());
        assert!(
            !roots.contains(&place.root()),
            "same-spelled roots in distinct functions cannot collapse"
        );
        roots.push(place.root());
        let symbols = unit.names().names().source_units()[0]
            .resolution()
            .symbols()
            .iter()
            .filter(|symbol| symbol.id() == place.root().symbol())
            .collect::<Vec<_>>();
        let [symbol] = symbols.as_slice() else {
            panic!("one actual keys declaration");
        };
        let keys = unit
            .sources()
            .span(source, keys_start, keys_start + 4)
            .unwrap();
        assert_eq!(unit.sources().slice(keys), Ok("keys"));
        assert_eq!(symbol.span(), keys);
        assert_eq!(symbol.name(), "keys");
        assert!(span.start() <= keys.start() && keys.end() <= span.end());
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|instruction| matches!(
                    instruction.operation,
                    Operation::ContainerElementPlace { .. }
                ))
                .count(),
            1
        );
    }
    let calls = module
        .functions
        .iter()
        .flat_map(|function| &function.instructions)
        .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), expected.len() - 1);
    if expected.len() == 2 {
        let first = unit.declaration("first", SymbolKind::Function);
        let first = module
            .functions
            .iter()
            .find(|function| function.origin == Origin::Source(unit.declaration_span(first)))
            .unwrap();
        let [call] = calls.as_slice() else {
            panic!("one original main-to-first call");
        };
        assert!(
            module
                .function(entry)
                .unwrap()
                .instructions
                .iter()
                .any(|instruction| std::ptr::eq(instruction, *call))
        );
        let Operation::DirectCall {
            callee,
            receiver,
            arguments,
        } = &call.operation
        else {
            unreachable!();
        };
        assert_eq!(*callee, first.id());
        assert!(receiver.is_none() && arguments.is_empty());
        let Origin::Source(span) = call.origin else {
            panic!("original call origin");
        };
        assert_eq!(unit.sources().slice(span), Ok("first()"));
    }
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("original roots and actual call verify through LLVM");
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
        let expected = if text.starts_with("fun first") {
            vec![
                ("first", SequentialContainerKind::Array, 24),
                ("main", SequentialContainerKind::List, 131),
            ]
        } else {
            vec![("main", SequentialContainerKind::Array, 23)]
        };
        assert_continue_identity_unit(text, &expected);
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
    let source = analysis.parsed.source_id();
    assert_eq!(analysis.sources.source_name(source), Ok("lowering.ko"));
    assert_eq!(analysis.sources.source_text(source), Ok(text));
    let expected_span = analysis.sources.span(source, 53, 57).unwrap();
    assert_eq!(analysis.sources.slice(expected_span), Ok("keys"));
    assert_eq!(
        error,
        crate::ssa::LoweringError {
            kind: crate::ssa::LoweringErrorKind::UnsupportedNode,
            span: Some(expected_span),
        },
        "the original alias boundary stays at keys, not original or a later use"
    );
    assert_continue_identity_unit(text, &[("main", SequentialContainerKind::Array, 53)]);
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
