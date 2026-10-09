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
    for descriptor in typed.map_with_values() {
        let Some(lang_frontend::type_checking::TypeKind::Function {
            parameters,
            return_type,
            ..
        }) = typed.types().get(descriptor.action_type())
        else {
            panic!("callback type missing")
        };
        assert_eq!(parameters.len(), 1);
        assert_eq!(
            parameters[0].mode,
            lang_frontend::type_checking::ParameterMode::Borrow
        );
        assert_eq!(parameters[0].ty, descriptor.value_type());
        assert_eq!(
            Some(*return_type),
            typed
                .types()
                .builtin(lang_frontend::type_checking::BuiltinType::Unit)
        );
    }
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
fn scoped_map_action_borrows_string_and_restores_receiver() {
    let text = "fun run() { var m = mutableMapOf<String, String>(); m.put(\"key\", \"value\"); val hit = m.withValue(\"key\", { value -> println(value) }); m.put(\"next\", \"after\") }";
    let (single, unit) = checked(text);
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
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
        let (single, unit) = checked(&text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
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
        let (single, unit) = checked(&text);
        assert!(
            single.diagnostics().is_empty(),
            "{text}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics().is_empty(),
            "{text}: {:?}",
            unit.diagnostics()
        );
        assert!(single.loans().len() >= 4 && unit.loans().len() >= 4);
    }
}

#[test]
fn scoped_map_action_rejects_returned_payload_at_type_boundary() {
    let text =
        "fun run() {val m=mapOf<String,String>();m.withValue(\"key\",{value -> return value})}";
    let mut sources = SourceMap::new();
    let source = sources.add_source("map.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    let single = check_types(&sources, &file, &names, &types).unwrap();
    let inputs = [SourceUnitInput::new("root", "map.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let d = &diagnostics[0];
        assert_eq!(d.code().to_string(), "L0087");
        let span = d.primary_span();
        assert_eq!(&text[span.start()..span.end()], "value");
    }
}
