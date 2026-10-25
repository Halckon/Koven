//! SPEC-0314: independently named ordinary-source baseline cells.
use super::{analyze, lower_scalar_file};
#[path = "lower_frontend_owned_source_return_structure_tests.rs"]
mod structure;
#[path = "lower_frontend_owned_source_unit_contract_tests.rs"]
pub(super) mod unit_contract;
use lang_frontend::type_checking::SequentialContainerKind;
use unit_contract::{Control, Expected};

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
    original_text(&text, "work", ty, own, live, map);
}

fn single_text(text: &str, live: bool, map: bool) {
    original_text(text, "main", "Array", false, live, map);
}

fn original_text(text: &str, root: &str, ty: &str, own: bool, live: bool, map: bool) {
    let container = match ty {
        "Array" => SequentialContainerKind::Array,
        "List" => SequentialContainerKind::List,
        "MutableList" => SequentialContainerKind::MutableList,
        _ => panic!("original owned-source type"),
    };
    let (program, origins) = unit_contract::observe(
        text,
        root,
        &[Expected {
            function: root,
            live,
            container,
            own,
        }],
        Control::Return,
        map,
    );
    let [origin] = origins.as_slice() else {
        panic!("one original provider");
    };
    structure::check(&program, live, origin.seed, origin.control, map);
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

#[path = "lower_frontend_owned_source_return_native_tests.rs"]
mod native;

#[path = "lower_frontend_owned_source_return_boundary_tests.rs"]
mod boundaries;
