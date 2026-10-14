//! SPEC-0314: independently named ordinary-source baseline cells.
use super::{analyze, lower_scalar_file};

fn source(factory: &str, ty: &str, own: bool, live: bool, map: bool) -> String {
    let parameter = if own {
        format!("own keys: {ty}<Int>")
    } else {
        String::new()
    };
    let seed = if own {
        String::new()
    } else {
        format!("val keys = {factory}(1, 2, 3);")
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
    format!(
        "fun work({parameter}): Unit {{ {seed} {before} for (key in keys) {{ {update} if (key == 2) {{ return }}; println(\"tail\") }}; {tail} {map_tail} }}"
    )
}

fn single(factory: &str, ty: &str, own: bool, live: bool, map: bool) {
    let text = source(factory, ty, own, live, map);
    let a = analyze(&text);
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
    use lang_frontend::ownership_checking::{IterationExitKind, IterationPhiIncomingKind};
    let plan = &a.owned.iterations()[0];
    assert_eq!(plan.exits().len(), 3);
    assert_eq!(
        plan.exits()
            .iter()
            .filter(|e| matches!(e.kind(), IterationExitKind::Return(_)))
            .count(),
        1
    );
    assert_eq!(plan.closure_phi_incomings().len(), 3);
    assert!(plan.closure_phi_incomings().iter().all(|e| matches!(
        e.kind(),
        IterationPhiIncomingKind::Entry
            | IterationPhiIncomingKind::Fallthrough
            | IterationPhiIncomingKind::Exhaustion
    )));
    let lowered = lower_scalar_file(&a.sources, &a.parsed, &a.names, &a.typed, &a.owned);
    if let Err(error) = &lowered {
        assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
        assert_eq!(a.sources.slice(error.span.unwrap()).unwrap(), "keys");
        println!("BASELINE {factory} own={own} live={live} map={map}: {error:?}");
    }
    let program = lowered.unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
}
fn unit(factory: &str, ty: &str, own: bool, live: bool, constants: bool) {
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
    let body = source(factory, ty, own, live, false);
    let argument = if own {
        format!("{factory}(1, 2, 3)")
    } else {
        String::new()
    };
    let text = format!("package p\n{body}\nfun main(): Unit {{ work({argument}) }}");
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
fn owned_source_return_single_array_local_dead_no_map() {
    single("arrayOf", "Array", false, false, false);
}

#[test]
fn owned_source_return_single_array_local_dead_map() {
    single("arrayOf", "Array", false, false, true);
}

#[test]
fn owned_source_return_single_array_local_live_no_map() {
    single("arrayOf", "Array", false, true, false);
}

#[test]
fn owned_source_return_single_array_local_live_map() {
    single("arrayOf", "Array", false, true, true);
}

#[test]
fn owned_source_return_single_array_own_dead_no_map() {
    single("arrayOf", "Array", true, false, false);
}

#[test]
fn owned_source_return_single_array_own_dead_map() {
    single("arrayOf", "Array", true, false, true);
}

#[test]
fn owned_source_return_single_array_own_live_no_map() {
    single("arrayOf", "Array", true, true, false);
}

#[test]
fn owned_source_return_single_array_own_live_map() {
    single("arrayOf", "Array", true, true, true);
}

#[test]
fn owned_source_return_single_list_local_dead_no_map() {
    single("listOf", "List", false, false, false);
}

#[test]
fn owned_source_return_single_list_local_dead_map() {
    single("listOf", "List", false, false, true);
}

#[test]
fn owned_source_return_single_list_local_live_no_map() {
    single("listOf", "List", false, true, false);
}

#[test]
fn owned_source_return_single_list_local_live_map() {
    single("listOf", "List", false, true, true);
}

#[test]
fn owned_source_return_single_list_own_dead_no_map() {
    single("listOf", "List", true, false, false);
}

#[test]
fn owned_source_return_single_list_own_dead_map() {
    single("listOf", "List", true, false, true);
}

#[test]
fn owned_source_return_single_list_own_live_no_map() {
    single("listOf", "List", true, true, false);
}

#[test]
fn owned_source_return_single_list_own_live_map() {
    single("listOf", "List", true, true, true);
}

#[test]
fn owned_source_return_single_mutable_list_local_dead_no_map() {
    single("mutableListOf", "MutableList", false, false, false);
}

#[test]
fn owned_source_return_single_mutable_list_local_dead_map() {
    single("mutableListOf", "MutableList", false, false, true);
}

#[test]
fn owned_source_return_single_mutable_list_local_live_no_map() {
    single("mutableListOf", "MutableList", false, true, false);
}

#[test]
fn owned_source_return_single_mutable_list_local_live_map() {
    single("mutableListOf", "MutableList", false, true, true);
}

#[test]
fn owned_source_return_single_mutable_list_own_dead_no_map() {
    single("mutableListOf", "MutableList", true, false, false);
}

#[test]
fn owned_source_return_single_mutable_list_own_dead_map() {
    single("mutableListOf", "MutableList", true, false, true);
}

#[test]
fn owned_source_return_single_mutable_list_own_live_no_map() {
    single("mutableListOf", "MutableList", true, true, false);
}

#[test]
fn owned_source_return_single_mutable_list_own_live_map() {
    single("mutableListOf", "MutableList", true, true, true);
}

#[test]
fn owned_source_return_unit_array_local_dead_basic() {
    unit("arrayOf", "Array", false, false, false);
}

#[test]
fn owned_source_return_unit_array_local_dead_constant() {
    unit("arrayOf", "Array", false, false, true);
}

#[test]
fn owned_source_return_unit_array_local_live_basic() {
    unit("arrayOf", "Array", false, true, false);
}

#[test]
fn owned_source_return_unit_array_local_live_constant() {
    unit("arrayOf", "Array", false, true, true);
}

#[test]
fn owned_source_return_unit_array_own_dead_basic() {
    unit("arrayOf", "Array", true, false, false);
}

#[test]
fn owned_source_return_unit_array_own_dead_constant() {
    unit("arrayOf", "Array", true, false, true);
}

#[test]
fn owned_source_return_unit_array_own_live_basic() {
    unit("arrayOf", "Array", true, true, false);
}

#[test]
fn owned_source_return_unit_array_own_live_constant() {
    unit("arrayOf", "Array", true, true, true);
}

#[test]
fn owned_source_return_unit_list_local_dead_basic() {
    unit("listOf", "List", false, false, false);
}

#[test]
fn owned_source_return_unit_list_local_dead_constant() {
    unit("listOf", "List", false, false, true);
}

#[test]
fn owned_source_return_unit_list_local_live_basic() {
    unit("listOf", "List", false, true, false);
}

#[test]
fn owned_source_return_unit_list_local_live_constant() {
    unit("listOf", "List", false, true, true);
}

#[test]
fn owned_source_return_unit_list_own_dead_basic() {
    unit("listOf", "List", true, false, false);
}

#[test]
fn owned_source_return_unit_list_own_dead_constant() {
    unit("listOf", "List", true, false, true);
}

#[test]
fn owned_source_return_unit_list_own_live_basic() {
    unit("listOf", "List", true, true, false);
}

#[test]
fn owned_source_return_unit_list_own_live_constant() {
    unit("listOf", "List", true, true, true);
}

#[test]
fn owned_source_return_unit_mutable_list_local_dead_basic() {
    unit("mutableListOf", "MutableList", false, false, false);
}

#[test]
fn owned_source_return_unit_mutable_list_local_dead_constant() {
    unit("mutableListOf", "MutableList", false, false, true);
}

#[test]
fn owned_source_return_unit_mutable_list_local_live_basic() {
    unit("mutableListOf", "MutableList", false, true, false);
}

#[test]
fn owned_source_return_unit_mutable_list_local_live_constant() {
    unit("mutableListOf", "MutableList", false, true, true);
}

#[test]
fn owned_source_return_unit_mutable_list_own_dead_basic() {
    unit("mutableListOf", "MutableList", true, false, false);
}

#[test]
fn owned_source_return_unit_mutable_list_own_dead_constant() {
    unit("mutableListOf", "MutableList", true, false, true);
}

#[test]
fn owned_source_return_unit_mutable_list_own_live_basic() {
    unit("mutableListOf", "MutableList", true, true, false);
}

#[test]
fn owned_source_return_unit_mutable_list_own_live_constant() {
    unit("mutableListOf", "MutableList", true, true, true);
}
