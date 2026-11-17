//! 受限来源中可复用范围原语的真实 typed 操作数；不执行 runtime。
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        CallableResultSource, CarrierSourceMarker, CompilationUnitTypes, RangeSourceKind,
        check_compilation_unit_types, standard_environments,
    },
};

fn checked(
    text: &str,
    authorized: bool,
) -> (SourceMap, CompilationUnitTypes, CompilationUnitTypes) {
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
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().unwrap();
    let single = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
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

#[test]
fn range_bounds_preserve_operand_completion() {
    for (begin, end, completes) in [
        ("0", "1", true),
        ("return 7", "1", false),
        ("0", "return 7", false),
        ("stop()", "1", false),
        ("0", "stop()", false),
        ("if (flag) { return 7 } else { 0 }", "1", true),
    ] {
        let text = format!(
            "fun stop(): Nothing = stop()\nfun run(source: List<Int>, flag: Boolean): Int {{ borrow val part = rangeView(source, {begin}, {end}); return 0 }}"
        );
        let (_, document, rooted) = checked(&text, true);
        for typed in [&document, &rooted] {
            assert!(
                typed.diagnostics().is_empty(),
                "{text}: {:?}",
                typed.diagnostics()
            );
            let descriptor = typed
                .calls()
                .iter()
                .find_map(|call| call.range_construction())
                .expect("static range identity");
            assert_eq!(
                typed.expression_type(descriptor.expression()),
                Some(descriptor.result_type())
            );
            assert_eq!(
                typed.expression_falls_through(descriptor.expression()),
                Some(completes),
                "{text}"
            );
        }
    }
}

#[test]
fn range_bounds_do_not_publish_unreachable_loop_break() {
    for (begin, end, missing_return) in [
        ("return 7", "break", false),
        ("stop()", "break", false),
        ("continue", "break", false),
        ("0", "break", true),
    ] {
        let text = format!(
            "fun stop(): Nothing = stop()\nfun run(source: List<Int>): Int {{ loop {{ rangeView(source, {begin}, {end}) }} }}"
        );
        let (_, document, rooted) = checked(&text, true);
        for typed in [&document, &rooted] {
            let codes: Vec<_> = typed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            assert_eq!(
                codes,
                if missing_return {
                    vec!["L0088"]
                } else {
                    vec![]
                },
                "{text}"
            );
        }
    }
}

#[test]
fn range_bounds_keep_unreachable_suffix_diagnostics() {
    for begin in ["0", "return 7", "stop()"] {
        let text = format!(
            "fun stop(): Nothing = stop()\nfun run(source: List<Int>): Int {{ rangeView(source, {begin}, \"bad\"); return 0 }}"
        );
        let (sources, document, rooted) = checked(&text, true);
        for typed in [&document, &rooted] {
            let codes: Vec<_> = typed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            assert_eq!(codes, ["L0084"], "{text}");
            assert_eq!(
                sources.slice(typed.diagnostics()[0].primary_span()),
                Ok("\"bad\"")
            );
            assert!(
                typed
                    .calls()
                    .iter()
                    .all(|call| call.range_construction().is_none())
            );
        }
    }
}

fn assert_rejected_range_recovery(sources: &SourceMap, typed: &CompilationUnitTypes) {
    let arity = typed
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code().to_string() == "L0121")
        .expect("original argument arity rejection");
    let source = arity.primary_span().source_id();
    let parsed = parse_file(sources, &lex(sources, source).unwrap()).unwrap();
    let calls = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| {
            node.span() == arity.primary_span()
                && matches!(
                    node.payload(),
                    lang_frontend::parser::Expression::Call { .. }
                )
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1, "unique reparsed rejected call");
    let expression = calls[0].0;
    let expression = typed
        .expression_types()
        .keys()
        .find(|id| id.expression() == expression)
        .copied()
        .unwrap();
    assert_eq!(typed.expression_falls_through(expression), Some(true));
    assert!(
        typed
            .calls()
            .iter()
            .all(|call| call.range_construction().is_none())
    );
}

#[test]
fn rejected_range_arity_does_not_publish_unreachable_loop_exits() {
    for (prefix, missing_return) in [
        ("0", true),
        ("return 7", false),
        ("stop()", false),
        ("continue", false),
    ] {
        let text = format!(
            "fun stop(): Nothing = stop()\nfun run(source: List<Int>): Int {{ loop {{ rangeView(source, {prefix}, break, 0) }} }}"
        );
        let (sources, document, rooted) = checked(&text, true);
        for typed in [&document, &rooted] {
            assert_rejected_range_recovery(&sources, typed);
            let codes = typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>();
            assert_eq!(
                codes,
                if missing_return {
                    vec!["L0121", "L0088"]
                } else {
                    vec!["L0121"]
                },
                "{text}"
            );
        }
    }
}

#[test]
fn rejected_range_arity_keeps_unreachable_suffix_diagnostics() {
    for prefix in ["0", "return 7", "stop()"] {
        let text = format!(
            "fun stop(): Nothing = stop()\nfun run(source: List<Int>): Int {{ rangeView(source, {prefix}, if (true) {{ val invalid: Int = \"bad\"; 0 }} else 0, 0); return 0 }}"
        );
        let (sources, document, rooted) = checked(&text, true);
        for typed in [&document, &rooted] {
            assert_rejected_range_recovery(&sources, typed);
            let codes = typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>();
            assert_eq!(codes, ["L0121", "L0084"], "{text}");
            assert_eq!(
                sources.slice(typed.diagnostics()[1].primary_span()),
                Ok("\"bad\"")
            );
        }
    }
}
