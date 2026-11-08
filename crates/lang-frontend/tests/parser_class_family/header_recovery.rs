use super::*;

#[test]
fn unfinished_classifier_headers_keep_the_body_and_next_file_owner() {
    for (head, expected) in [
        ("class C(val x: Int", &["L0010"][..]),
        ("value class C(val x: Int", &["L0010"][..]),
        ("interface C<T", &["L0010"][..]),
        ("interface C: Parent<Int,", &["L0010", "L0014"][..]),
    ] {
        let text = format!("{head}   {{ fun member(): Unit {{}} }}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        assert_eq!(codes(parsed.diagnostics()), expected, "{text}");
        let [class, sentinel] = parsed.roots() else {
            panic!("classifier and next file owner for {text}");
        };
        let classifier = parsed.ast().items().get(*class).unwrap();
        let Item::Classifier(classifier) = classifier.payload() else {
            panic!("recovered classifier");
        };
        if let Some(constructor) = &classifier.primary_constructor {
            assert_eq!(constructor.fields.len(), 1);
            assert!(constructor.right_paren_span.is_none());
            assert_eq!(
                sources.slice(constructor.fields[0].span).unwrap(),
                "val x: Int"
            );
        }
        if let Some(list) = classifier.type_parameter_list_span {
            assert_eq!(sources.slice(list).unwrap(), "<T");
        }
        if let [parent] = classifier.supertypes.as_slice() {
            assert_eq!(sources.slice(parent.span).unwrap(), "Parent<Int,");
        }
        let body = classifier.body.as_ref().unwrap();
        let [member] = body.members.as_slice() else {
            panic!("classifier member for {text}");
        };
        let member = parsed.ast().items().get(*member).unwrap();
        assert_eq!(
            sources.slice(member.span()).unwrap(),
            "fun member(): Unit {}"
        );
        assert_eq!(sources.slice(body.right_brace_span.unwrap()).unwrap(), "}");
        let sentinel = parsed.ast().items().get(*sentinel).unwrap();
        assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
        assert!(parsed.diagnostics().iter().all(|diagnostic| {
            let span = diagnostic.primary_span();
            span.is_empty() && span.start() == body.left_brace_span.start()
        }));
    }
}

#[test]
fn classifier_header_recovery_distinguishes_nested_braces_and_function_types() {
    let (_, parsed) = declaration("class C(public val x: move (Int) -> Int)");
    assert!(parsed.diagnostics().is_empty());
    assert_eq!(
        classifier(&parsed)
            .primary_constructor
            .as_ref()
            .unwrap()
            .fields
            .len(),
        1
    );

    for (field, code) in [
        ("val x: Int @ ({ 1 })", "L0069"),
        ("val x: Int = { -> 1 }", "L0077"),
        ("val x: Int = f({ -> 1 })", "L0077"),
    ] {
        let text = format!("class C({field}, val y: Int) {{ fun member(): Unit {{}} }}");
        let (_, parsed) = declaration(&text);
        assert_eq!(codes(parsed.diagnostics()), [code], "{text}");
        let classifier = classifier(&parsed);
        let constructor = classifier.primary_constructor.as_ref().unwrap();
        assert_eq!(constructor.fields.len(), 2);
        assert!(constructor.right_paren_span.is_some());
        let body = classifier.body.as_ref().unwrap();
        assert_eq!(body.members.len(), 1);
        assert!(body.right_brace_span.is_some());
    }

    for (inner, expected) in [
        ("class Inner(val x: Int", &["L0077", "L0010"][..]),
        (
            "class Inner(val x: Int = 1",
            &["L0077", "L0077", "L0010"][..],
        ),
        ("class Inner: I by", &["L0077", "L0078"][..]),
        (
            "interface Inner: Parent<Int,",
            &["L0077", "L0010", "L0014"][..],
        ),
    ] {
        let text = format!("class Outer {{ {inner} }}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        assert_eq!(codes(parsed.diagnostics()), expected, "{text}");
        let [outer, sentinel] = parsed.roots() else {
            panic!("outer and next classifier");
        };
        let Item::Classifier(outer) = parsed.ast().items().get(*outer).unwrap().payload() else {
            panic!("outer classifier");
        };
        let body = outer.body.as_ref().unwrap();
        let closer = body.right_brace_span.unwrap();
        assert_eq!(body.members.len(), 1);
        assert_eq!(sources.slice(closer).unwrap(), "}");
        assert!(
            parsed
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.code().to_string() != "L0077")
                .all(|diagnostic| {
                    let primary = diagnostic.primary_span();
                    primary.is_empty() && primary.start() == closer.start()
                })
        );
        let sentinel = parsed.ast().items().get(*sentinel).unwrap();
        assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
    }
}

#[test]
fn recovered_classifier_span_uses_its_last_consumed_token() {
    for source in [
        "class C(val x: Int,",
        "interface I :",
        "class C { fun member(): Unit {}",
    ] {
        let text = format!("{source}   ");
        let (sources, parsed) = declaration(&text);
        assert!(!parsed.diagnostics().is_empty());
        let classifier = parsed.ast().items().get(parsed.root()).unwrap();
        assert_eq!(sources.slice(classifier.span()).unwrap(), source);
    }
}

#[test]
fn unsupported_unclosed_supertype_calls_keep_the_existing_body_boundary() {
    for header in ["class C: I(", "class C: I by make("] {
        let text = format!("{header}\n{{ fun member(): Unit {{}} }}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        assert_eq!(codes(parsed.diagnostics()), ["L0077"], "{text}");
        let [first, sentinel] = parsed.roots() else {
            panic!("class and later classifier");
        };
        let Item::Classifier(class) = parsed.ast().items().get(*first).unwrap().payload() else {
            panic!("class with recovered body");
        };
        let body = class.body.as_ref().unwrap();
        assert_eq!(body.members.len(), 1);
        assert!(body.right_brace_span.is_some());
        let sentinel = parsed.ast().items().get(*sentinel).unwrap();
        assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
    }
}
