//! SPEC-0299: ordinary-source non-consuming Map result Borrow, without IR mutation.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, resolve_names},
    ownership_checking::check_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

fn source(payload: &str, grouped: bool) -> String {
    let (declaration, value, read) = match payload {
        "String" => ("", "\"value\".clone()", "value"),
        "Text" => (
            "value class Text(val n: Int, val text: String)",
            "Text(7, \"value\".clone())",
            "\"borrowed inline\"",
        ),
        "Resource" => (
            "class Resource(val text: String) { deinit() { println(\"drop\") } }",
            "Resource(\"value\".clone())",
            "value.text",
        ),
        _ => panic!("selected payload"),
    };
    let condition = if grouped { "(removed != null)" } else { "removed != null" };
    let after = if payload == "Text" { "if (taken.n == 7) { println(\"owned inline\") } else { error(\"wrong payload\") }" } else { "observe(taken)" };
    format!(
        "{declaration}\nfun observe(value: {payload}) {{ println({read}) }}\nfun entry() {{\n var values = mutableMapOf<Int, {payload}>()\n values.put(1, {value})\n val removed = values.remove(1)\n if ({condition}) {{ observe(removed); observe((removed)) }}\n val taken = removed!!\n {after}\n println(\"end\")\n}}"
    )
}

fn check(payload: &str, grouped: bool, compilation_unit: bool) {
    let source = source(payload, grouped);
    let mut sources = SourceMap::new();
    let (environment, types) = standard_environments();
    let result = if compilation_unit {
        let (provider, entry) = source.split_once("fun entry()").unwrap();
        let (p, provider) = super::unit_lower_test_support::parsed(&mut sources, "provider.ko", &format!("package p\n{provider}"));
        let (q, entry) = super::unit_lower_test_support::parsed(&mut sources, "entry.ko", &format!("package q\nimport p.*\nfun entry(){entry}"));
        let inputs = [SourceUnitInput::new("root", "p/provider.ko", p, &provider), SourceUnitInput::new("root", "q/entry.ko", q, &entry)];
        let (names, typed, owned) = super::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        super::unit_lower::lower_scalar_unit_with_entry(&sources, &inputs, &names, &types, &typed, &owned, super::unit_lower_test_support::declaration(&names, "q", "entry")).map(|(program, _)| program)
    } else {
        let id = sources.add_source("result.ko", source.clone()).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        assert!(names.diagnostics().is_empty());
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        super::lower_frontend::orchestrate::lower_scalar_file(&sources, &parsed, &names, &typed, &owned)
    };
    let program = result.unwrap_or_else(|error| {
        let origin = error.span.map(|span| (sources.source_name(span.source_id()).unwrap(), sources.slice(span).unwrap()));
        panic!("payload={payload}, grouped={grouped}, unit={compilation_unit}: {error:?}; origin={origin:?}")
    });
    super::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
}

macro_rules! case {
    ($name:ident, $payload:literal, $grouped:literal, $unit:literal) => {
        #[test]
        fn $name() { check($payload, $grouped, $unit); }
    };
}
case!(owned_map_borrow_single_string_plain, "String", false, false);
case!(owned_map_borrow_single_string_group, "String", true, false);
case!(owned_map_borrow_unit_string_plain, "String", false, true);
case!(owned_map_borrow_unit_string_group, "String", true, true);
case!(owned_map_borrow_single_inline_plain, "Text", false, false);
case!(owned_map_borrow_single_inline_group, "Text", true, false);
case!(owned_map_borrow_unit_inline_plain, "Text", false, true);
case!(owned_map_borrow_unit_inline_group, "Text", true, true);
case!(owned_map_borrow_single_resource_plain, "Resource", false, false);
case!(owned_map_borrow_single_resource_group, "Resource", true, false);
case!(owned_map_borrow_unit_resource_plain, "Resource", false, true);
case!(owned_map_borrow_unit_resource_group, "Resource", true, true);
