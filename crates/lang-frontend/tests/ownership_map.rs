//! SPEC-0288: Map 参数交付与 receiver 权限的单文件/unit 一致性。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipCheckedFile, check_compilation_unit_ownership,
        check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn checked(text: &str) -> (OwnershipCheckedFile, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("map.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let single = check_ownership(&sources, &file, &names, &typed).unwrap();
    let inputs = [SourceUnitInput::new("root", "map.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    (single, unit)
}

#[test]
fn map_queries_borrow_string_keys_and_allow_reuse() {
    let (single, unit) = checked(
        "fun run(m: Map<String, Int>, key: String): Unit {\n m.contains(key)\n m.get(key)\n val v = m[key]\n println(key)\n}",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
}

#[test]
fn map_subscript_releases_a_temporary_string_key_after_query() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget, UnitDropPoint, UnitDropTarget};
    let (single, unit) =
        checked("fun run(m: Map<String, Int>): Unit {\n val result = m[\"key\"]\n}");
    assert!(
        single
            .drops()
            .iter()
            .any(|drop| matches!(drop.point(), DropPoint::CallReturn(_))
                && matches!(drop.target(), DropTarget::Temporary(_)))
    );
    assert!(
        unit.drops()
            .iter()
            .any(|drop| matches!(drop.point(), UnitDropPoint::CallReturn(_))
                && matches!(drop.target(), UnitDropTarget::Temporary(_)))
    );
}

#[test]
fn map_mutation_requires_mutable_receiver() {
    for operation in ["m.put(1, 2)", "m.remove(1)", "m[1] = 2"] {
        let (single, unit) = checked(&format!(
            "fun run(): Unit {{\n val m = mutableMapOf<Int, Int>()\n {operation}\n}}"
        ));
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0134"),
            "{operation}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0134"),
            "{operation}: {:?}",
            unit.diagnostics()
        );
    }
}

#[test]
fn map_put_owns_string_key_and_move_only_value() {
    for operation in ["m.put(key, value)", "m[key] = value"] {
        let (single, unit) = checked(&format!(
            "class Resource {{}}\nfun run(key: String, value: Resource): Unit {{\n var m = mutableMapOf<String, Resource>()\n {operation}\n}}"
        ));
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0133"),
            "{operation}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0133"),
            "{operation}: {:?}",
            unit.diagnostics()
        );
    }
}

#[test]
fn map_remove_borrows_string_key() {
    let (single, unit) = checked(
        "fun run(key: String): Unit {\n var m = mutableMapOf<String, Int>()\n m.remove(key)\n println(key)\n}",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
}
