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

#[test]
fn missing_nested_classifier_name_preserves_the_enclosing_closer() {
    let text = "class Outer { class }\nclass Sentinel";
    let (sources, parsed) = file(text);
    assert_eq!(codes(parsed.diagnostics()), ["L0077", "L0067"]);
    let [outer, sentinel] = parsed.roots() else {
        panic!("outer class and following file item");
    };
    let Item::Classifier(outer) = parsed.ast().items().get(*outer).unwrap().payload() else {
        panic!("outer class");
    };
    let body = outer.body.as_ref().unwrap();
    let [inner] = body.members.as_slice() else {
        panic!("one nested classifier");
    };
    let Item::Classifier(inner) = parsed.ast().items().get(*inner).unwrap().payload() else {
        panic!("nested classifier");
    };
    let closer = body.right_brace_span.unwrap();
    let NameMarker::Missing(name) = inner.name else {
        panic!("missing name");
    };
    assert!(name.is_empty());
    assert_eq!(name.start(), closer.start());
    assert_eq!(parsed.diagnostics()[1].primary_span(), name);
    assert_eq!(sources.slice(closer).unwrap(), "}");
    assert_eq!(
        sources
            .slice(parsed.ast().items().get(*sentinel).unwrap().span())
            .unwrap(),
        "class Sentinel"
    );
}

#[test]
fn member_recovery_preserves_a_contextual_receiver_only_at_owner_baseline() {
    for (garbage, prefix, expected) in [
        ("@", "own", &["L0071"][..]),
        ("@ (own fun ignored(): Unit {})", "own", &["L0071"][..]),
        (
            r#"@ "${own fun ignored(): Unit {}}""#,
            "own",
            &["L0071"][..],
        ),
        ("@", "own borrow", &["L0071", "L0076"][..]),
        ("@", "own public", &["L0071", "L0076"][..]),
    ] {
        let text = format!("class C {{ {garbage}\n{prefix} fun member(): Unit {{}} }}");
        let (sources, parsed) = declaration(&text);
        assert_eq!(codes(parsed.diagnostics()), expected, "{text}");
        let body = classifier(&parsed).body.as_ref().unwrap();
        let [error, member] = body.members.as_slice() else {
            panic!("error region and recovered member");
        };
        assert!(matches!(item(&parsed, *error), Item::Error));
        let Some(marker @ ParameterModeMarker::Own(_)) = receiver_mode(&parsed, *member) else {
            panic!("recovered receiver");
        };
        assert_eq!(sources.slice(receiver_mode_span(marker)).unwrap(), "own");
        assert!(body.right_brace_span.is_some());
    }
}

#[test]
fn lexer_poison_at_a_supertype_slot_is_reachable_without_losing_the_next_entry() {
    use lang_frontend::parser::TypeRef;

    for (prefix, poison, expected_codes) in [
        ("", "async", &["L0002"][..]),
        ("", "async<A, B>", &["L0002"][..]),
        ("I, ", "β /* gap */ <A<B>, C>", &["L0001"][..]),
        ("", "async await <A, B>", &["L0002", "L0002"][..]),
        ("I, ", "async await", &["L0002", "L0002"][..]),
    ] {
        let text = format!("class C: {prefix}{poison} /* outside */, J {{}}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        assert_eq!(codes(parsed.diagnostics()), expected_codes, "{text}");
        let [first, second] = parsed.roots() else {
            panic!("classifier and following file owner for {text}");
        };
        let Item::Classifier(classifier) = parsed.ast().items().get(*first).unwrap().payload()
        else {
            panic!("recovered classifier for {text}");
        };
        let types = parsed.ast().type_refs();
        let mut entries = classifier.supertypes.iter();
        if !prefix.is_empty() {
            let leading = types.get(entries.next().unwrap().type_ref).unwrap();
            assert_eq!(sources.slice(leading.span()).unwrap(), "I");
            assert!(matches!(leading.payload(), TypeRef::Qualified { .. }));
        }
        let recovered = entries.next().unwrap();
        let error = types.get(recovered.type_ref).unwrap();
        assert_eq!(sources.slice(recovered.span).unwrap(), poison);
        assert_eq!(error.span(), recovered.span);
        assert_eq!(error.payload(), &TypeRef::Error);
        assert!(recovered.delegation.is_none());
        let following = types.get(entries.next().unwrap().type_ref).unwrap();
        assert_eq!(sources.slice(following.span()).unwrap(), "J");
        assert!(matches!(following.payload(), TypeRef::Qualified { .. }));
        assert!(entries.next().is_none());
        assert!(classifier.body.as_ref().unwrap().right_brace_span.is_some());
        let next_root = parsed.ast().items().get(*second).unwrap();
        assert_eq!(sources.slice(next_root.span()).unwrap(), "class Sentinel");
    }
}

#[test]
fn invalid_supertype_slot_stops_at_the_next_file_declaration_without_a_body() {
    use lang_frontend::parser::TypeRef;

    let (sources, parsed) = file("interface C: β\nclass Sentinel");
    assert_eq!(codes(parsed.diagnostics()), ["L0001"]);
    let items = parsed.ast().items();
    let [first, second] = parsed.roots() else {
        panic!("two classifiers");
    };
    let classifier = items.get(*first).unwrap();
    assert_eq!(sources.slice(classifier.span()).unwrap(), "interface C: β");
    let Item::Classifier(classifier) = classifier.payload() else {
        panic!("recovered interface");
    };
    let [entry] = classifier.supertypes.as_slice() else {
        panic!("one error entry");
    };
    let error = parsed.ast().type_refs().get(entry.type_ref).unwrap();
    assert_eq!(error.payload(), &TypeRef::Error);
    assert_eq!(error.span(), entry.span);
    assert_eq!(error.span(), parsed.diagnostics()[0].primary_span());
    assert!(classifier.body.is_none());
    assert_eq!(
        sources.slice(items.get(*second).unwrap().span()).unwrap(),
        "class Sentinel"
    );
}

#[test]
fn an_unclosed_poisoned_type_suffix_leaves_the_next_supertype_available() {
    let (sources, parsed) = declaration("class C: async<A, J {}");
    let recovered = classifier(&parsed);
    let [error, next] = recovered.supertypes.as_slice() else {
        panic!("error region and valid sibling: {:?}", parsed.diagnostics());
    };
    assert_eq!(sources.slice(error.span).unwrap(), "async<A");
    assert_eq!(sources.slice(next.span).unwrap(), "J");
    assert!(recovered.body.as_ref().unwrap().right_brace_span.is_some());
    assert_eq!(codes(parsed.diagnostics()), ["L0002"]);
}
