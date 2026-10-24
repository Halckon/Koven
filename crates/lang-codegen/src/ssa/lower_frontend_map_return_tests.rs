#[path = "lower_frontend_map_return_structure_tests.rs"]
mod structure;
// Original conditional return source, admitted through the same Unit analysis.
use super::unit_iteration::{Control, assert_facts};
// Unchanged mixed and negative children still inspect their original Legacy route.
use super::{analyze, lower_scalar_file};

fn source(live: bool) -> String {
    format!(
        "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2, 3)) {{ cache.put(key, \"head\".clone()); if (key == 1) {{ return }}; cache.put(9, \"tail\".clone()) }}; {} }}",
        if live { "cache.contains(9)" } else { "" }
    )
}

fn check(live: bool) {
    let text = source(live);
    let (program, analysis) = super::lower_map_unit(&text, "main");
    let origins = assert_facts(&analysis, Control::Return, live);
    structure::assert_return_structure(
        &program,
        origins.control,
        origins.statement,
        origins.source,
        live,
    );
}

#[test]
fn single_map_return_live_source_red() {
    check(true);
}
#[test]
fn single_map_return_dead_source_red() {
    check(false);
}

fn assert_unit_lowers(live: bool, constants: bool) {
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
    let text = format!("package p\n{}", source(live));
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
fn unit_map_return_basic_live_source_red() {
    assert_unit_lowers(true, false);
}

#[test]
fn unit_map_return_basic_dead_source_red() {
    assert_unit_lowers(false, false);
}

#[test]
fn unit_map_return_constant_live_source_red() {
    assert_unit_lowers(true, true);
}

#[test]
fn unit_map_return_constant_dead_source_red() {
    assert_unit_lowers(false, true);
}

#[path = "lower_frontend_map_return_controls_tests.rs"]
mod ordinary_controls;

#[path = "lower_frontend_map_return_matrix_tests.rs"]
mod source_matrix;

#[path = "lower_frontend_map_operand_last_use_tests.rs"]
mod operand_last_use;

#[path = "lower_frontend_map_operand_structure_tests.rs"]
mod operand_structure;
