//! SPEC-0288: Map 参数交付与 receiver 权限的双入口 Unit 合同。
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

// Original bytes and callback/loan contracts are checked through both Unit entries.
fn checked_unit(text: &str) -> (CompilationUnitOwnership, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("map.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let [memory, rooted] = [
        SourceUnitInput::memory_document(source, &file),
        SourceUnitInput::new("root", "map.ko", source, &file),
    ]
    .map(|input| {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let [indexed] = index.source_units() else {
            panic!("one original Map source")
        };
        assert_eq!(indexed.source_id(), source);
        match indexed.key() {
            lang_frontend::name_resolution::SourceUnitKey::MemoryDocument => {}
            lang_frontend::name_resolution::SourceUnitKey::Rooted { root, logical_path } => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "map.ko");
            }
        }
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(
            typed.map_with_values().len(),
            1,
            "one original Map operation"
        );
        for descriptor in typed.map_with_values() {
            let Some(lang_frontend::type_checking::UnitTypeKind::Function {
                parameters,
                return_type,
                ..
            }) = typed.types().get(descriptor.action_type())
            else {
                panic!("callback type missing");
            };
            assert_eq!(parameters.len(), 1);
            assert_eq!(
                parameters[0].mode(),
                lang_frontend::type_checking::ParameterMode::Borrow
            );
            assert_eq!(parameters[0].ty(), descriptor.value_type());
            assert_eq!(
                Some(*return_type),
                typed
                    .types()
                    .builtin(lang_frontend::type_checking::BuiltinType::Unit)
            );
        }
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(complete.is_same_analysis(&typed));
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        owned
    });
    (memory, rooted)
}

#[test]
fn scoped_map_action_borrows_string_and_restores_receiver() {
    let text = "fun run() { var m = mutableMapOf<String, String>(); m.put(\"key\", \"value\"); val hit = m.withValue(\"key\", { value -> println(value) }); m.put(\"next\", \"after\") }";
    let (memory, rooted) = checked_unit(text);
    for owned in [&memory, &rooted] {
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    }
}

#[test]
fn scoped_map_action_rejects_source_invalidation_and_value_escape() {
    for action in [
        "{ value -> m.put(\"next\",\"bad\");println(value) }",
        "{ value -> m.remove(\"key\");println(value) }",
        "{ value -> consumeMap(m);println(value) }",
        "{ value -> consume(value) }",
        "{ value -> val stored=Holder(value) }",
        "{ value -> val escaped=move {println(value)};sink(escaped) }",
        "{ value -> val escaped:()->Unit={println(value)};save(escaped) }",
    ] {
        let text = format!(
            "class Holder(val value:String)\nfun consume(own value:String) {{}}\nfun consumeMap(own m:MutableMap<String,String>) {{}}\nfun sink(own action:move ()->Unit) {{}}\nfun save(own action:()->Unit) {{}}\nfun run() {{var m=mutableMapOf<String,String>();m.withValue(\"key\",{action})}}"
        );
        let (memory, rooted) = checked_unit(&text);
        for diagnostics in [memory.diagnostics(), rooted.diagnostics()] {
            assert!(!diagnostics.is_empty(), "must reject {text}");
            for d in diagnostics {
                assert!(
                    d.code().to_string().starts_with("L013"),
                    "actual ownership diagnostic: {text}: {d:?}"
                );
                let span = d.primary_span();
                assert!(
                    span.start() < span.end() && span.end() <= text.len(),
                    "{d:?}"
                );
            }
        }
    }
}

#[test]
fn scoped_map_action_generic_borrow_ends_before_owned_map_delivery() {
    for value in ["String", "Packet", "Resource", "Token?"] {
        let text = format!(
            "value class Packet(val text:String)\nclass Resource {{deinit() {{}}}}\nclass Token(val n:Int)\nfun observe(value:{value}) {{}}\nfun consumeMap(own m:MutableMap<String,{value}>) {{}}\nfun run() {{val key=\"key\";var m=mutableMapOf<String,{value}>();m.withValue(key,{{value -> observe(value);return}});println(key);consumeMap(m)}}"
        );
        let (memory, rooted) = checked_unit(&text);
        for owned in [&memory, &rooted] {
            assert!(
                owned.diagnostics().is_empty(),
                "{text}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.loans().len() >= 4);
        }
    }
}

#[test]
fn scoped_map_action_rejects_returned_payload_at_type_boundary() {
    let text =
        "fun run() {val m=mapOf<String,String>();m.withValue(\"key\",{value -> return value})}";
    let mut sources = SourceMap::new();
    let source = sources.add_source("map.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "map.ko", source, &file),
        SourceUnitInput::memory_document(source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let unit = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        let diagnostics = unit.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let d = &diagnostics[0];
        assert_eq!(d.code().to_string(), "L0087");
        let span = d.primary_span();
        assert_eq!(&text[span.start()..span.end()], "value");
    }
}
