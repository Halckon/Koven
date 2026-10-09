//! SPEC-0287 Map/MutableMap 类型系统、Hashable 约束与借用操作测试。

use lang_frontend::{
    name_resolution::{
        NameResolution, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        resolve_names,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{TypedFile, check_compilation_unit_types, check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

fn parsed(text: &str) -> (SourceMap, ParsedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("map_test.ko", text).expect("source");
    let parsed = parse_file_twice(&sources, source, "map type source");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (sources, parsed)
}

fn checked(text: &str) -> (SourceMap, ParsedFile, NameResolution, TypedFile) {
    let (sources, parsed) = parsed(text);
    let (names, types) = standard_environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(
        resolution.diagnostics().is_empty(),
        "{:?}",
        resolution.diagnostics()
    );
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    (sources, parsed, resolution, typed)
}

fn checked_unit(text: &str) -> lang_frontend::type_checking::CompilationUnitTypes {
    checked_unit_with_sources(text).1
}

fn checked_unit_with_sources(
    text: &str,
) -> (
    SourceMap,
    lang_frontend::type_checking::CompilationUnitTypes,
) {
    let (sources, file) = parsed(text);
    let (environment, types) = standard_environments();
    let source = file.source_id();
    let inputs = [SourceUnitInput::new("root", "map_test.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    (sources, typed)
}

#[test]
fn unit_map_signatures_check_hashable_and_storable_arguments() {
    for constructor in ["Map", "MutableMap"] {
        let typed = checked_unit(&format!(
            "fun bad(m: {constructor}<List<Int>, String>): Unit {{}}"
        ));
        assert!(
            typed
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0161"),
            "{:?}",
            typed.diagnostics()
        );
        let typed = checked_unit(&format!("fun bad(m: {constructor}<Int, Any>): Unit {{}}"));
        assert!(
            !typed.diagnostics().is_empty(),
            "Any must not enter Map storage in a signature"
        );
    }
}

#[test]
fn test_map_and_mutable_map_construction_and_members() {
    let text = r#"
        fun test() {
            val m: Map<Int, Int> = mapOf<Int, Int>()
            val mm: MutableMap<String, Int> = mutableMapOf<String, Int>()
            val size: Int = m.size
            val contains: Boolean = m.contains(1)
            val v: Int? = m.get(1)
            val vi: Int? = m[1]
            mm.put("key", 42)
            val rem: Int? = mm.remove("key")
            mm["key2"] = 100
        }
    "#;
    let (_sources, _parsed, _resolution, typed) = checked(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
}

#[test]
fn test_map_key_must_be_hashable() {
    let text = r#"
        fun bad_key(m: Map<List<Int>, String>) {}
    "#;
    let (_sources, _parsed, _resolution, typed) = checked(text);
    let diagnostics = typed.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
    assert_eq!(diagnostics[0].code().to_string(), "L0161");
}

#[test]
fn test_mutable_map_key_must_be_hashable() {
    let text = r#"
        fun bad_key(m: MutableMap<List<Int>, String>) {}
    "#;
    let (_sources, _parsed, _resolution, typed) = checked(text);
    let diagnostics = typed.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
    assert_eq!(diagnostics[0].code().to_string(), "L0161");
}

#[test]
fn test_map_type_argument_arity_check() {
    let text = r#"
        fun bad_arity_1(m: Map<Int>) {}
    "#;
    let (_sources, _parsed, _resolution, typed) = checked(text);
    let diagnostics = typed.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
    assert_eq!(diagnostics[0].code().to_string(), "L0091");
}

#[test]
fn test_map_readonly_mutation_disallowed() {
    let text = r#"
        fun bad_mut(m: Map<Int, String>) {
            m[1] = "val"
            m.put(1, "val")
            m.remove(1)
        }
    "#;
    let (_sources, _parsed, _resolution, typed) = checked(text);
    let diagnostics = typed.diagnostics();
    assert_eq!(diagnostics.len(), 3, "{:?}", diagnostics);
    assert_eq!(diagnostics[0].code().to_string(), "L0129");
    assert_eq!(diagnostics[1].code().to_string(), "L0130");
    assert_eq!(diagnostics[2].code().to_string(), "L0130");
}

#[test]
fn test_map_method_must_be_called() {
    let text = r#"
        fun bad_member(m: Map<Int, String>) {
            val f = m.get
        }
    "#;
    let (_sources, _parsed, _resolution, typed) = checked(text);
    let diagnostics = typed.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
    assert_eq!(diagnostics[0].code().to_string(), "L0130");
}

#[test]
fn test_map_key_and_value_type_mismatches() {
    let text = r#"
        fun test_mismatch(m: Map<Int, Int>, mm: MutableMap<Int, Int>) {
            val v1 = m.get("not_int")
            val v2 = m["not_int"]
            mm.put("not_int", 42)
            mm.put(1, "not_int")
            mm["not_int"] = 42
            mm[1] = "not_int"
        }
    "#;
    let (_sources, _parsed, _resolution, typed) = checked(text);
    let diagnostics = typed.diagnostics();
    assert_eq!(diagnostics.len(), 6, "{:?}", diagnostics);
    for d in diagnostics {
        assert_eq!(d.code().to_string(), "L0084");
    }
}

#[test]
fn test_move_only_value_index_read_disallowed() {
    let text = r#"
        class NonCopyable(val x: Int)

        fun test(m: Map<Int, NonCopyable>) {
            val item = m[1]
        }
    "#;
    let (_sources, _parsed, _resolution, typed) = checked(text);
    let diagnostics = typed.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
    assert_eq!(diagnostics[0].code().to_string(), "L0136");
}

#[test]
fn test_move_only_value_get_disallowed() {
    let text = r#"
        class NonCopyable(val x: Int)

        fun test(m: Map<Int, NonCopyable>) {
            val item = m.get(1)
        }
    "#;
    let (_sources, _parsed, _resolution, typed) = checked(text);
    let diagnostics = typed.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
    assert_eq!(diagnostics[0].code().to_string(), "L0136");
}

#[test]
fn test_string_value_map_get_is_move_only() {
    let text = r#"
        fun test(m: Map<Int, String>) {
            val s = m.get(1)
        }
    "#;
    let (_sources, _parsed, _resolution, typed) = checked(text);
    let diagnostics = typed.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
    assert_eq!(diagnostics[0].code().to_string(), "L0136");
}

#[test]
fn test_map_multifile_compilation_unit() {
    let mut sources = SourceMap::new();
    let file1 = sources
        .add_source(
            "file1.ko",
            r#"
                fun create_map(): Map<String, Int> = mapOf<String, Int>()
            "#,
        )
        .expect("source1");
    let file2 = sources
        .add_source(
            "file2.ko",
            r#"
                fun read_size(): Int {
                    val m = create_map()
                    return m.size
                }
            "#,
        )
        .expect("source2");
    let parsed1 = parse_file_twice(&sources, file1, "unit file 1");
    let parsed2 = parse_file_twice(&sources, file2, "unit file 2");
    assert!(parsed1.diagnostics().is_empty());
    assert!(parsed2.diagnostics().is_empty());

    let (names, types) = standard_environments();
    let inputs = [
        SourceUnitInput::new("unit", "file1.ko", file1, &parsed1),
        SourceUnitInput::new("unit", "file2.ko", file2, &parsed2),
    ];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let unit_names = resolve_compilation_unit_names(&sources, &inputs, &index, &names)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &unit_names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let _validated = typed.validate().unwrap();
}
