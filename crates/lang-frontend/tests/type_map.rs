//! SPEC-0287 Map/MutableMap 类型系统、Hashable 约束与借用操作测试。

use lang_frontend::{
    name_resolution::{
        NameResolution, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::parse_file_twice;

#[path = "type_map/consume.rs"]
mod consume;

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

#[derive(Clone, Copy, Debug)]
enum Entry {
    MemoryDocument,
    Rooted,
}

const ENTRIES: [Entry; 2] = [Entry::MemoryDocument, Entry::Rooted];

impl Entry {
    fn input<'a>(self, file: &'a ParsedFile, path: &'a str) -> SourceUnitInput<'a> {
        match self {
            Self::MemoryDocument => SourceUnitInput::memory_document(file.source_id(), file),
            Self::Rooted => SourceUnitInput::new("root", path, file.source_id(), file),
        }
    }

    fn assert_identity(
        self,
        index: &lang_frontend::name_resolution::CompilationUnitIndex,
        file: &ParsedFile,
        path: &str,
    ) {
        use lang_frontend::name_resolution::SourceUnitKey;
        let [source] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(source.source_id(), file.source_id());
        assert!(match (self, source.key()) {
            (Self::MemoryDocument, SourceUnitKey::MemoryDocument) => true,
            (Self::Rooted, SourceUnitKey::Rooted { root, logical_path }) =>
                root.as_str() == "root" && logical_path.as_str() == path,
            _ => false,
        });
    }
}

fn checked_entries(
    text: &str,
) -> (
    SourceMap,
    [lang_frontend::type_checking::CompilationUnitTypes; 2],
) {
    let (sources, file) = parsed(text);
    let (environment, types) = standard_environments();
    let results = ENTRIES.map(|entry| {
        let inputs = [entry.input(&file, "map_test.ko")];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        entry.assert_identity(&index, &file, "map_test.ko");
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("unit names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid names");
        check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit types")
    });
    (sources, results)
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
fn map_nullable_value_queries_require_scoped_access_with_diagnostic_span() {
    for (query, code) in [("m.get(1)", "L0130"), ("m[1]", "L0130")] {
        let text = format!("fun read(m: Map<Int, Int?>): Unit {{ val result = {query} }}");
        let (sources, entries) = checked_entries(&text);
        for unit in &entries {
            let diagnostics = unit.diagnostics();
            let diagnostic = diagnostics
                .iter()
                .find(|d| d.code().to_string() == code)
                .unwrap_or_else(|| panic!("{text}: {diagnostics:?}"));
            assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), query);
            assert!(diagnostic.message().contains("withValue"));
        }
        assert!(entries.iter().all(|unit| unit.map_gets().is_empty()));
    }
}

#[test]
fn map_move_only_get_contract_stays_rejected_including_nullable_values() {
    for value in ["String", "String?"] {
        for query in ["m.get(1)", "m[1]"] {
            let text = format!("fun read(m: Map<Int, {value}>): Unit {{ val result = {query} }}");
            let (sources, entries) = checked_entries(&text);
            for unit in &entries {
                let diagnostics = unit.diagnostics();
                let diagnostic = diagnostics
                    .iter()
                    .find(|d| d.code().to_string() == "L0136")
                    .unwrap_or_else(|| panic!("{text}: {diagnostics:?}"));
                assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), query);
            }
        }
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
    for typed in checked_entries(text).1 {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    }
}

#[test]
fn test_map_key_must_be_hashable() {
    let text = r#"
        fun bad_key(m: Map<List<Int>, String>) {}
    "#;
    for typed in checked_entries(text).1 {
        let diagnostics = typed.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
        assert_eq!(diagnostics[0].code().to_string(), "L0161");
    }
}

#[test]
fn test_mutable_map_key_must_be_hashable() {
    let text = r#"
        fun bad_key(m: MutableMap<List<Int>, String>) {}
    "#;
    for typed in checked_entries(text).1 {
        let diagnostics = typed.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
        assert_eq!(diagnostics[0].code().to_string(), "L0161");
    }
}

#[test]
fn test_map_type_argument_arity_check() {
    let text = r#"
        fun bad_arity_1(m: Map<Int>) {}
    "#;
    for typed in checked_entries(text).1 {
        let diagnostics = typed.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
        assert_eq!(diagnostics[0].code().to_string(), "L0091");
    }
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
    for typed in checked_entries(text).1 {
        let diagnostics = typed.diagnostics();
        assert_eq!(diagnostics.len(), 3, "{:?}", diagnostics);
        assert_eq!(diagnostics[0].code().to_string(), "L0129");
        assert_eq!(diagnostics[1].code().to_string(), "L0130");
        assert_eq!(diagnostics[2].code().to_string(), "L0130");
    }
}

#[test]
fn test_map_method_must_be_called() {
    let text = r#"
        fun bad_member(m: Map<Int, String>) {
            val f = m.get
        }
    "#;
    for typed in checked_entries(text).1 {
        let diagnostics = typed.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
        assert_eq!(diagnostics[0].code().to_string(), "L0130");
    }
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
    for typed in checked_entries(text).1 {
        let diagnostics = typed.diagnostics();
        assert_eq!(diagnostics.len(), 6, "{:?}", diagnostics);
        for d in diagnostics {
            assert_eq!(d.code().to_string(), "L0084");
        }
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
    for typed in checked_entries(text).1 {
        let diagnostics = typed.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
        assert_eq!(diagnostics[0].code().to_string(), "L0136");
    }
}

#[test]
fn test_move_only_value_get_disallowed() {
    let text = r#"
        class NonCopyable(val x: Int)

        fun test(m: Map<Int, NonCopyable>) {
            val item = m.get(1)
        }
    "#;
    for typed in checked_entries(text).1 {
        let diagnostics = typed.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
        assert_eq!(diagnostics[0].code().to_string(), "L0136");
    }
}

#[test]
fn test_string_value_map_get_is_move_only() {
    let text = r#"
        fun test(m: Map<Int, String>) {
            val s = m.get(1)
        }
    "#;
    for typed in checked_entries(text).1 {
        let diagnostics = typed.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{:?}", diagnostics);
        assert_eq!(diagnostics[0].code().to_string(), "L0136");
    }
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

#[test]
fn map_nullable_borrow_argument_requires_a_non_null_proof() {
    for query in ["values.get(1)", "values.remove(1)"] {
        let text = format!(
            "fun observe(value: Int): Unit {{}}\nfun entry(): Unit {{\n var values = mutableMapOf<Int, Int>()\n val result = {query}\n observe(result)\n}}"
        );
        let (sources, entries) = checked_entries(&text);
        for unit in &entries {
            let diagnostics = unit.diagnostics();
            let error = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code().to_string() == "L0084")
                .unwrap_or_else(|| panic!("{query}: {diagnostics:?}"));
            assert_eq!(sources.slice(error.primary_span()).unwrap(), "result");
        }
        assert!(entries.iter().all(|unit| unit.non_null_uses().is_empty()));
    }
}

#[test]
fn single_map_queries_publish_only_with_a_checked_key() {
    use lang_frontend::{
        name_resolution::resolve_names,
        type_checking::{BuiltinType, TypeKind, check_types},
    };
    let declarations = "fun reject(inout target: Int, input: Int): Int = input\nfun outer(inout target: Int?, value: Int?): Boolean { target = null; return true }";
    for (key, rejected, resolved) in [
        ("1", false, true),
        ("produce<Int>()", false, false),
        ("reject(&y, produce<Int>())", true, false),
    ] {
        for query in [format!("table[{key}]"), format!("table.get({key})")] {
            let outer = format!("outer(&x, ({query}))");
            let text = format!(
                "{declarations}\nfun run(produce: () -> Int, table: Map<Int, Int>): Unit {{ var x: Int? = 7; var y: Int? = 7; if (x != null) {{ {outer}; val after: Int = /*read*/x }} }}"
            );
            let (sources, file) = parsed(&text);
            let (environment, types) = standard_environments();
            let names = resolve_names(&sources, &file, &environment).unwrap();
            let typed = check_types(&sources, &file, &names, &types).unwrap();
            let expression = |expected| {
                file.ast()
                    .expressions()
                    .iter()
                    .find(|(_, node)| sources.slice(node.span()) == Ok(expected))
                    .unwrap_or_else(|| panic!("expression not found: {expected}"))
                    .0
            };
            let query_id = expression(&query);
            assert_eq!(typed.map_get(query_id).is_some(), resolved, "{text}");
            assert_eq!(typed.call(expression(&outer)).is_some(), resolved, "{text}");
            let actual = typed.types().get(typed.expression_type(query_id).unwrap());
            if rejected {
                assert_eq!(actual, Some(&TypeKind::Error), "{text}");
            } else {
                assert!(
                    matches!(actual, Some(TypeKind::Nullable(inner))
                    if typed.types().get(*inner) == Some(&TypeKind::Builtin(BuiltinType::Int))),
                    "{text}"
                );
            }
            let [diagnostic] = typed.diagnostics() else {
                panic!("expected one mismatch: {text}: {:?}", typed.diagnostics());
            };
            assert_eq!(diagnostic.code().to_string(), "L0084");
            let offset = if rejected {
                text.find("reject(&y").unwrap() + "reject(&".len()
            } else {
                text.find("/*read*/").unwrap() + "/*read*/".len()
            };
            assert_eq!(diagnostic.primary_span().start(), offset, "{text}");
        }
    }
}
