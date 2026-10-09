//! 受限来源中可复用范围原语的真实 typed 操作数；不执行 runtime。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        CallableResultSource, CarrierSourceMarker, CompilationUnitTypes, RangeSourceKind,
        TypedFile, check_compilation_unit_types, check_types, standard_environments,
    },
};

fn checked(text: &str, authorized: bool) -> (SourceMap, TypedFile, CompilationUnitTypes) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("algorithms.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, mut types) = standard_environments();
    if authorized {
        types.authorize_range_source(&sources, source).unwrap();
    }
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let single = check_types(&sources, &parsed, &names, &types).unwrap();
    let inputs = [SourceUnitInput::new(
        "std",
        "algorithms.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    (sources, single, unit)
}

#[test]
fn range_constructor_records_source_and_relative_bounds_without_fake_from() {
    for (source_type, kind) in [
        ("List<Int>", RangeSourceKind::List),
        ("List<String>", RangeSourceKind::List),
        ("View<String>", RangeSourceKind::View),
        ("List<Item>", RangeSourceKind::List),
    ] {
        let text = format!(
            "class Item(val text: String) {{}}\nfun produce(source: {source_type}, begin: Int, end: Int): Unit {{ borrow val part = rangeView(source, begin, end) }}"
        );
        let (sources, single, unit) = checked(&text, true);
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        let call = single
            .calls()
            .iter()
            .find(|c| c.range_construction().is_some())
            .expect("real range construction fact");
        let descriptor = call.range_construction().unwrap();
        assert_eq!(descriptor.source_kind(), kind);
        assert_eq!(descriptor.result_type(), call.return_type());
        assert_eq!(sources.slice(descriptor.source_span()).unwrap(), "source");
        assert_eq!(
            call.arguments()[0].mode(),
            lang_frontend::type_checking::ParameterMode::Borrow
        );
        let CallableResultSource::Carrier(contract) = call.result_source() else {
            panic!("new carrier delivery")
        };
        assert!(contract.from_span().is_none());
        assert!(matches!(
            contract.marker(),
            CarrierSourceMarker::PrimitiveCall(_)
        ));
        let call = unit
            .calls()
            .iter()
            .find(|c| c.range_construction().is_some())
            .expect("unit range construction fact");
        let descriptor = call.range_construction().unwrap();
        assert_eq!(descriptor.source_kind(), kind);
        assert_eq!(descriptor.result_type(), call.return_type());
        assert_eq!(sources.slice(descriptor.source_span()).unwrap(), "source");
        assert_eq!(
            descriptor.source().source_unit(),
            descriptor.expression().source_unit()
        );
        assert_eq!(
            call.result_source(),
            CallableResultSource::Carrier(contract)
        );
        assert!(call.borrow_return().is_none());
    }
}

#[test]
fn unauthorized_source_cannot_call_the_bound_range_constructor() {
    let (_, single, unit) = checked(
        "fun run(source: List<Int>): Unit { borrow val part = rangeView(source, 0, 1) }",
        false,
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0164"),
            "{diagnostics:?}"
        );
    }
    assert!(
        single
            .calls()
            .iter()
            .all(|c| c.range_construction().is_none())
    );
    assert!(
        unit.calls()
            .iter()
            .all(|c| c.range_construction().is_none())
    );
}

#[test]
fn source_function_with_the_constructor_name_has_no_intrinsic_facts() {
    let (_, single, unit) = checked(
        "fun rangeView(source: List<Int>, begin: Int, end: Int): Int = 0\nfun run(source: List<Int>): Unit { val result = rangeView(source, 0, 1) }",
        true,
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert!(
        single
            .calls()
            .iter()
            .all(|c| c.range_construction().is_none())
    );
    assert!(
        unit.calls()
            .iter()
            .all(|c| c.range_construction().is_none())
    );
}
