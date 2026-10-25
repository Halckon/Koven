#[path = "lower_frontend_map_break_structure_tests.rs"]
mod structure;
// Original conditional break source, admitted through the same Unit analysis.
use super::unit_iteration::{Control, assert_facts};

fn source(live: bool) -> String {
    format!(
        "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) {{ cache.put(key, \"value\".clone()); if (key == 1) {{ break }} }}; {} }}",
        if live { "cache.contains(1)" } else { "" }
    )
}

fn check(live: bool) {
    let text = source(live);
    let (program, analysis) = super::lower_map_unit(&text, "main");
    let origins = assert_facts(&analysis, Control::Break, live);
    structure::assert_exit_structure(&program, origins.statement, origins.source, live, true);
}

#[test]
fn single_map_break_live_source_red() {
    check(true);
}

#[test]
fn single_map_break_dead_source_red() {
    check(false);
}

fn assert_unit_boundary(live: bool, constants: bool, remove: bool) {
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
    let body = source(live);
    let body = if remove {
        body.replace("cache.put(key, \"value\".clone())", "cache.remove(key)")
    } else {
        body
    };
    let text = format!("package p\n{body}");
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
    let (program, entry) = result.unwrap_or_else(|error| {
        panic!("unit constants={constants}, live={live}, remove={remove}: {error:?}")
    });
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let (loop_span, source_expression) = file
        .ast()
        .statements()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            lang_frontend::parser::Statement::For { source, .. } => Some((node.span(), *source)),
            _ => None,
        })
        .unwrap();
    let source_span = file
        .ast()
        .expressions()
        .get(source_expression)
        .unwrap()
        .span();
    // Unit EndElement/EndSource carry the provider statement's actual origin.
    structure::assert_exit_structure(&program, loop_span, source_span, live, true);
}

#[test]
fn unit_map_break_basic_live_is_independent() {
    assert_unit_boundary(true, false, false);
}
#[test]
fn unit_map_break_basic_dead_has_exit_cleanup() {
    assert_unit_boundary(false, false, false);
}
#[test]
fn unit_map_break_constant_live_is_independent() {
    assert_unit_boundary(true, true, false);
}
#[test]
fn unit_map_break_constant_dead_has_exit_cleanup() {
    assert_unit_boundary(false, true, false);
}

#[test]
fn single_map_break_unconditional_and_multiple_exits_remain_explicit() {
    for body in ["break", "if (key == 1) { break }; if (key == 2) { break }"] {
        let text = source(true).replace("if (key == 1) { break }", body);
        crate::ssa::map_unit_consumer_test_support::assert_source(&text, &["main"]);
    }
}

#[test]
fn single_map_break_matrix_has_real_ssa_and_separate_owner_drops() {
    for (text, counts) in [
        (
            include_str!("../../tests/fixtures/map-conditional-break-0310/live-dead.ko"),
            vec![
                ("removeDead", 2),
                ("removeLive", 1),
                ("putDead", 2),
                ("putLive", 1),
            ],
        ),
        (
            include_str!("../../tests/fixtures/map-conditional-break-0310/distinct-owners.ko"),
            vec![("parameterOwner", 1), ("separateOwners", 2)],
        ),
    ] {
        let (program, analysis) = super::lower_map_unit(text, "main");
        super::assert_unit_map_drop_counts(&program, &analysis, &counts);
    }
}

#[test]
fn unit_map_break_remove_uses_current_owner_on_each_exit() {
    for constants in [false, true] {
        for live in [false, true] {
            assert_unit_boundary(live, constants, true);
        }
    }
}

#[test]
fn unit_map_break_original_full_fixture_has_separate_owner_drops() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    let source = include_str!("../../tests/fixtures/map-conditional-break-0310/live-dead.ko");
    let analysis = UnitSource::analyze("map-break-live-dead.ko", source).unwrap();
    analysis.assert_frontend_clean();
    let entry = analysis.declaration("main", SymbolKind::Function);
    let (program, entry) = analysis.lower_entry(entry).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let expected = [
        ("removeDead", 2),
        ("removeLive", 1),
        ("putDead", 2),
        ("putLive", 1),
    ]
    .into_iter()
    .map(|(name, count)| {
        let functions = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| {
                let span = function.origin.span();
                matches!(&function.origin, crate::ssa::model::Origin::Source(_))
                    && source[span.start()..span.end()].starts_with(&format!("fun {name}("))
            })
            .collect::<Vec<_>>();
        assert_eq!(functions.len(), 1, "one real source definition for {name}");
        (functions[0].name.as_str(), count)
    })
    .collect::<Vec<_>>();
    super::assert_map_drop_counts(&program, &expected);
}
