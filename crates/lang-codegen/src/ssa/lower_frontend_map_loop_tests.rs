//! SPEC-0300: real source owner facts and normal-exhaustion SSA transport.
// The unmigrated operand children retain their exact Legacy contracts.
use super::{analyze, lower_map_unit, lower_scalar_file};
use crate::ssa::map_unit_consumer_test_support::{assert_source, assert_trusted_range};
#[path = "lower_frontend_map_loop_unit_identity.rs"]
pub(super) mod unit_identity;
#[path = "lower_frontend_map_unit_iteration_tests.rs"]
mod unit_iteration;

const ORIGINAL: &str =
    include_str!("../../tests/fixtures/owned-null-cleanup-0296/single-loop-boundary.ko");
const NORMAL: &str = include_str!("../../tests/fixtures/map-loop-owner-0300/normal-exhaustion.ko");

#[test]
fn single_map_loop_original_remove_uses_published_header_owner() {
    unit_identity::check_original(ORIGINAL);
}

#[test]
fn unit_map_loop_original_source_is_an_independent_control() {
    assert_unit_lowers(ORIGINAL);
}

#[test]
fn single_map_loop_normal_exhaustion_preserves_dead_and_live_owners() {
    unit_identity::check_normal(NORMAL);
}

#[test]
fn unit_map_loop_normal_source_is_an_independent_control() {
    assert_unit_lowers(NORMAL);
}

#[test]
fn single_map_loop_abort_guard_combination_remains_explicit() {
    let source = include_str!("../../tests/fixtures/map-loop-owner-0300/guarded-abort-boundary.ko");
    assert_source(
        source,
        &["removeDead", "removeLive", "putDead", "putLive", "main"],
    );
}

fn assert_control_exit_boundary(exit: &str) {
    let source = format!(
        "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) {{ cache.put(key, \"value\".clone()); if (key == 1) {{ {exit} }} }}; cache.contains(1) }}"
    );
    assert_source(&source, &["main"]);
}

#[test]
fn single_map_continue_with_break_remains_explicit() {
    assert_control_exit_boundary("if (key == 2) { break }; continue");
}

#[test]
fn single_map_continue_unconditional_and_multiple_remain_explicit() {
    for exit in [
        "continue",
        "if (key == 1) { continue }; if (key == 2) { continue }",
    ] {
        let source = format!(
            "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) {{ cache.put(key, \"value\".clone()); {exit} }}; cache.contains(1) }}"
        );
        assert_source(&source, &["main"]);
    }
}

#[test]
fn single_map_continue_return_replacement_and_abort_remain_explicit() {
    assert_control_exit_boundary("if (key == 2) { return }; continue");
    for prefix in [
        "cache = mutableMapOf<Int, String>();",
        "if (key == 2) { error(\"stop\") };",
    ] {
        let source = format!(
            "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) {{ {prefix} cache.put(key, \"value\".clone()); if (key == 1) {{ continue }} }}; cache.contains(1) }}"
        );
        assert_source(&source, &["main"]);
    }
}

#[test]
fn single_map_loop_unconditional_return_remains_explicit() {
    assert_source(
        "fun main(): Unit { var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) { cache.put(key, \"value\".clone()); return }; cache.contains(1) }",
        &["main"],
    );
}

#[test]
fn single_map_loop_replaced_seed_remains_explicit() {
    let source = "fun main(): Unit { var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) { cache = mutableMapOf<Int, String>(); cache.put(key, \"value\".clone()) }; cache.contains(1) }";
    assert_source(source, &["main"]);
}

#[test]
fn single_map_loop_returned_map_seed_remains_explicit() {
    assert_source(
        "fun make(): MutableMap<Int, String> = mutableMapOf<Int, String>()\nfun main(): Unit { var cache = make(); for (key in arrayOf(1)) { cache.put(key, \"value\".clone()) }; cache.contains(1) }",
        &["make", "main"],
    );
}

#[test]
fn single_map_loop_range_provider_remains_explicit() {
    let text = format!(
        "{}\nfun main(): Unit {{ var cache = mutableMapOf<Int, String>(); val keys = listOf(1, 2); for (key in take(keys, 1)) {{ cache.put(key, \"value\".clone()) }}; cache.contains(1) }}",
        include_str!("../../../lang-std/koven/algorithms/ranges.ko")
            .strip_prefix("package koven.algorithms\n")
            .unwrap()
    );
    assert_trusted_range(&text);
}

fn assert_unit_lowers(source: &str) {
    use crate::ssa::{
        unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::{analyze, declaration, parsed},
    };
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "p/main.ko", &format!("package p\n{source}"));
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let (program, entry) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "p", "main"),
    )
    .unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
}

#[test]
fn single_map_loop_parameter_seed_and_same_typed_owners_stay_distinct() {
    let source = include_str!("../../tests/fixtures/map-loop-owner-0300/distinct-owners.ko");
    let (program, analysis) = lower_map_unit(source, "main");
    assert_unit_map_drop_counts(
        &program,
        &analysis,
        &[("parameterOwner", 1), ("separateOwners", 2)],
    );
    assert_unit_lowers(source);
}

#[test]
fn single_map_loop_resource_payload_keeps_existing_classification() {
    let source = "class Resource { deinit() { println(\"drop\") } }\nfun main(): Unit { var resources = mutableMapOf<Int, Resource>(); for (key in arrayOf(1, 2)) { resources.put(key, Resource()) }; println(\"after\") }";
    unit_identity::check_resource(source);
}

#[test]
fn single_map_loop_multiple_iteration_phi_seeds_remain_explicit() {
    for loops in [
        "for (key in arrayOf(1)) { cache.put(key, \"first\".clone()) }; for (key in arrayOf(2)) { cache.put(key, \"second\".clone()) }",
        "for (key in arrayOf(1)) { for (other in arrayOf(2)) { cache.put(other, \"nested\".clone()) }; cache.put(key, \"outer\".clone()) }",
    ] {
        let source = format!(
            "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); {loops}; cache.contains(1) }}"
        );
        assert_source(&source, &["main"]);
    }
}

fn assert_map_drop_counts(program: &crate::ssa::model::Program, expected: &[(&str, usize)]) {
    use crate::ssa::model::{EntityId, EntityType, Operation, SsaTypeKind};
    for (name, count) in expected {
        let (module, function) = program
            .modules
            .iter()
            .find_map(|module| {
                module
                    .functions
                    .iter()
                    .find(|function| function.name == *name)
                    .map(|function| (module, function))
            })
            .expect(name);
        let drops = function
            .instructions
            .iter()
            .filter(|instruction| match instruction.operation {
                Operation::Drop { owner } => {
                    match function.entity(EntityId::Value(owner)).unwrap().ty {
                        EntityType::Value(ty) => {
                            matches!(module.types[ty.index()], SsaTypeKind::MapContainer { .. })
                        }
                        _ => false,
                    }
                }
                _ => false,
            })
            .count();
        assert_eq!(
            drops, *count,
            "{name}: expected static Map drops for the selected exits"
        );
    }
}

fn assert_unit_map_drop_counts(
    program: &crate::ssa::model::Program,
    analysis: &crate::native_tests::boxed_enum_tests::unit_source::UnitSource,
    expected: &[(&str, usize)],
) {
    use crate::ssa::model::Origin;
    use lang_frontend::name_resolution::SymbolKind;

    let expected = expected
        .iter()
        .map(|(name, count)| {
            let declaration = analysis.declaration(name, SymbolKind::Function);
            let span = analysis.declaration_span(declaration);
            let functions = program
                .modules
                .iter()
                .flat_map(|module| &module.functions)
                .filter(|function| function.origin == Origin::Source(span))
                .collect::<Vec<_>>();
            let [function] = functions.as_slice() else {
                panic!("one original source function for {name}");
            };
            (function.name.as_str(), *count)
        })
        .collect::<Vec<_>>();
    assert_map_drop_counts(program, &expected);
}

#[path = "lower_frontend_map_break_tests.rs"]
mod conditional_break;

#[path = "lower_frontend_map_continue_tests.rs"]
mod conditional_continue;

#[path = "lower_frontend_map_return_tests.rs"]
mod conditional_return;
