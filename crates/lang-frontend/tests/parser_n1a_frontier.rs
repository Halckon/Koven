//! N1a 首片的声明前置；不把新描述符当作普通借用对象返回。

use lang_frontend::{
    lexer::lex,
    parser::{
        BorrowReturnSource, FunctionForm, FunctionResultSource, Item, NameMarker,
        ParameterModeMarker, parse_file,
    },
    source::SourceMap,
};

fn diagnostics(text: &str) -> Vec<String> {
    let mut sources = SourceMap::new();
    let source = sources.add_source("n1a_declaration.ko", text).unwrap();
    let tokens = lex(&sources, source).unwrap();
    let parsed = parse_file(&sources, &tokens).unwrap();
    parsed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn range_carrier_result_keeps_a_unique_source_without_borrowing_local_descriptor() {
    let text =
        "fun <T> view(source: List<T>): View<T> from source = rangeView(source, 0, source.size)";
    assert!(diagnostics(text).is_empty(), "{:?}", diagnostics(text));
}

#[test]
fn standard_extension_uses_the_existing_receiver_mode_spelling() {
    for text in [
        "package koven\nborrow fun <T> List<T>.take(n: Int): View<T> from this = rangeView(this, 0, n)",
        "package koven\nown fun <T> List<T>.consume(): ConsumingSeq<T> = consumingSequence(this)",
    ] {
        assert!(
            diagnostics(text).is_empty(),
            "{text}: {:?}",
            diagnostics(text)
        );
    }
}

#[test]
fn nested_nullable_remains_illegal() {
    assert!(!diagnostics("fun reject(source: String??): Unit {}").is_empty());
}

#[test]
fn receiver_type_and_mode_keep_their_own_source_spans() {
    for (prefix, owned) in [("borrow ", false), ("own ", true)] {
        let text = format!(
            "{prefix}fun <T> pkg.List<Pair<Int, T>>.inspect(source: T): View<T> from this {{}}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("receiver.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        let Item::Modified {
            modifiers,
            declaration,
        } = parsed
            .ast()
            .items()
            .get(parsed.roots()[0])
            .unwrap()
            .payload()
        else {
            panic!("mode wrapper")
        };
        let mode = modifiers.receiver_mode.unwrap();
        assert_eq!(matches!(mode, ParameterModeMarker::Own(_)), owned);
        let mode_span = match mode {
            ParameterModeMarker::Own(s)
            | ParameterModeMarker::Borrow(s)
            | ParameterModeMarker::Inout(s) => s,
        };
        assert_eq!(sources.slice(mode_span).unwrap(), prefix.trim());
        let Item::Function {
            name: NameMarker::Present(name),
            extension_receiver: Some(receiver),
            form:
                FunctionForm::Explicit {
                    result_source: Some(FunctionResultSource::Carrier(origin)),
                    ..
                },
            ..
        } = parsed.ast().items().get(*declaration).unwrap().payload()
        else {
            panic!("extension function")
        };
        assert_eq!(sources.slice(*name).unwrap(), "inspect");
        assert_eq!(sources.slice(receiver.dot_span).unwrap(), ".");
        assert_eq!(sources.slice(origin.from_span).unwrap(), "from");
        let BorrowReturnSource::Receiver(this_span) = origin.source else {
            panic!("receiver origin")
        };
        assert_eq!(sources.slice(this_span).unwrap(), "this");
        assert_eq!(
            sources
                .slice(
                    parsed
                        .ast()
                        .type_refs()
                        .get(receiver.type_ref)
                        .unwrap()
                        .span()
                )
                .unwrap(),
            "pkg.List<Pair<Int, T>>"
        );
    }
}

#[test]
fn inout_extension_is_rejected_at_its_marker_until_its_contract_is_enabled() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "inout_extension.ko",
            "inout fun List<Int>.inspect(): Unit {}",
        )
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let rejection = parsed
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code().to_string() == "L0076")
        .expect("N1a does not enable inout extension receivers");
    assert_eq!(sources.slice(rejection.primary_span()).unwrap(), "inout");
}

#[test]
fn carrier_and_ordinary_borrow_results_preserve_distinct_modes_and_source_spans() {
    for (marker, carrier) in [("", true), ("borrow ", false)] {
        let text = format!("fun view(source: Int): {marker}Int from source = source");
        let mut sources = SourceMap::new();
        let source = sources.add_source("source_mode.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        let Item::Function {
            extension_receiver: None,
            form:
                FunctionForm::Explicit {
                    result_source: Some(mode),
                    ..
                },
            ..
        } = parsed
            .ast()
            .items()
            .get(parsed.roots()[0])
            .unwrap()
            .payload()
        else {
            panic!("source result")
        };
        let (from_span, origin) = match mode {
            FunctionResultSource::Carrier(syntax) => {
                assert!(carrier);
                (syntax.from_span, syntax.source)
            }
            FunctionResultSource::Borrow(syntax) => {
                assert!(!carrier);
                assert_eq!(sources.slice(syntax.borrow_span).unwrap(), "borrow");
                (syntax.from_span, syntax.source)
            }
        };
        assert_eq!(sources.slice(from_span).unwrap(), "from");
        let BorrowReturnSource::Parameter(NameMarker::Present(origin)) = origin else {
            panic!("parameter source")
        };
        assert_eq!(sources.slice(origin).unwrap(), "source");
        assert!(origin.start() > from_span.end());
    }
}

#[test]
fn illegal_declaration_forms_still_produce_structured_diagnostics() {
    for text in [
        "borrow fun ordinary(): Unit {}",
        "own fun ordinary(): Unit {}",
        "borrow own fun List<Int>.inspect(): Unit {}",
        "fun own List<Int>.inspect(): Unit {}",
        "class Owner { fun List<Int>.inspect(): Unit {} }",
        "val List<Int>.size: Int = 1",
        "fun view(source: Int): Int from = source",
        "fun view(source: Int): Int from? source = source",
        "fun view(a: Int, b: Int): Int from a, b = a",
        "fun view(a: Int, b: Int): Int from a from b = a",
        "fun view(source: Int): List<Int from source> = listOf(1)",
    ] {
        assert!(!diagnostics(text).is_empty(), "{text}");
    }
}

#[test]
fn ordinary_soft_names_and_qualified_parameter_types_are_unchanged() {
    for text in [
        "fun from(borrow: Int): Int = borrow",
        "fun borrow(from: Int): Int = from",
        "fun inspect(source: pkg.List<Int>): Unit {}",
        "fun inspect(source: (pkg.Input) -> pkg.Output): Unit {}",
    ] {
        assert!(
            diagnostics(text).is_empty(),
            "{text}: {:?}",
            diagnostics(text)
        );
    }
}
