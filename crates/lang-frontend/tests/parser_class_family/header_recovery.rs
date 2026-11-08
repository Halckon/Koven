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

#[test]
fn a_lexical_error_after_a_complete_supertype_keeps_its_siblings_and_body() {
    use lang_frontend::parser::TypeRef;

    for parent in ["I", "pkg.I<A, B>?", "move (A) -> (B) -> I"] {
        let text = format!("class C: {parent} async β  , J {{}}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        assert_eq!(codes(parsed.diagnostics()), ["L0002", "L0001"], "{text}");
        let [class, sentinel] = parsed.roots() else {
            panic!("expected both file roots for {text}");
        };
        let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
            panic!("recovered classifier");
        };
        let [first, following] = class.supertypes.as_slice() else {
            panic!("original and following supertypes for {text}");
        };
        let type_node = parsed.ast().type_refs().get(first.type_ref).unwrap();
        assert!(matches!(
            type_node.payload(),
            TypeRef::Qualified { .. } | TypeRef::Function { .. }
        ));
        assert_eq!(sources.slice(type_node.span()).unwrap(), parent);
        assert_eq!(
            sources.slice(first.span).unwrap(),
            format!("{parent} async β")
        );
        assert_eq!(
            sources
                .slice(first.trailing_lexical_error.unwrap())
                .unwrap(),
            "async β"
        );
        assert_eq!(sources.slice(following.span).unwrap(), "J");
        assert!(class.body.as_ref().unwrap().right_brace_span.is_some());
        let sentinel = parsed.ast().items().get(*sentinel).unwrap();
        assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
    }
}

#[test]
fn malformed_type_arguments_do_not_create_parent_entries_or_delegation() {
    for (parent, expected_diagnostics, error, first_argument) in [
        ("I<A async, B>", &["L0002"][..], "async", "A"),
        ("I<async by field, B>", &["L0002"][..], "by field", "async"),
        ("I<(async)?, B>", &["L0002", "L0014"][..], "?", "(async)"),
        ("I<Inner<A async, B>, D>", &["L0002"][..], "async", "A"),
    ] {
        let text = format!("class C: {parent}, J {{ fun member(): Unit {{}} }}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        assert_eq!(codes(parsed.diagnostics()), expected_diagnostics, "{text}");
        let [class, sentinel] = parsed.roots() else {
            panic!("expected both file roots for {text}");
        };
        let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
            panic!("recovered classifier");
        };
        let [first, following] = class.supertypes.as_slice() else {
            panic!("original and following supertypes for {text}");
        };
        assert_eq!(sources.slice(first.span).unwrap(), parent);
        assert!(first.delegation.is_none(), "{text}");
        assert!(first.trailing_lexical_error.is_none(), "{text}");
        let types = parsed.ast().type_refs();
        let get_segment = |id| {
            let lang_frontend::parser::TypeRef::Qualified { segments, .. } =
                types.get(id).unwrap().payload()
            else {
                panic!("qualified type for {text}");
            };
            segments.last().unwrap()
        };
        let outer = get_segment(first.type_ref);
        let segment = if parent.starts_with("I<Inner") {
            assert!(outer.argument_errors.is_empty(), "{text}");
            assert_eq!(
                sources
                    .slice(types.get(outer.arguments[1]).unwrap().span())
                    .unwrap(),
                "D"
            );
            get_segment(outer.arguments[0])
        } else {
            outer
        };
        assert_eq!(segment.arguments.len(), 2, "{text}");
        assert_eq!(sources.slice(segment.argument_errors[0]).unwrap(), error);
        assert_eq!(segment.argument_errors.len(), 1, "{text}");
        assert_eq!(
            sources
                .slice(types.get(segment.arguments[0]).unwrap().span())
                .unwrap(),
            first_argument
        );
        assert_eq!(
            sources
                .slice(types.get(segment.arguments[1]).unwrap().span())
                .unwrap(),
            "B"
        );

        assert_eq!(sources.slice(following.span).unwrap(), "J");
        assert_eq!(class.body.as_ref().unwrap().members.len(), 1);
        assert_eq!(
            sources
                .slice(parsed.ast().items().get(*sentinel).unwrap().span())
                .unwrap(),
            "class Sentinel"
        );
    }
}

#[test]
fn a_proven_nested_suffix_group_cannot_claim_the_outer_supertype_boundary() {
    for suffix in [
        "async Junk<A, B>",
        "async Junk(A, { -> 1 })",
        "async Junk(A, { \"bad\n })",
    ] {
        let text =
            format!("class C: I {suffix}  , J {{ fun member(): Unit {{}} }}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        let expected = if suffix.contains("\"bad") {
            vec!["L0002", "L0004"]
        } else {
            vec!["L0002"]
        };
        assert_eq!(codes(parsed.diagnostics()), expected, "{text}");
        let [class, sentinel] = parsed.roots() else {
            panic!("expected both file roots for {text}");
        };
        let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
            panic!("recovered classifier");
        };
        let [first, following] = class.supertypes.as_slice() else {
            panic!("original and following supertypes for {text}");
        };
        assert_eq!(
            sources
                .slice(first.trailing_lexical_error.unwrap())
                .unwrap(),
            suffix
        );
        assert_eq!(sources.slice(following.span).unwrap(), "J");
        let body = class.body.as_ref().unwrap();
        assert_eq!(body.members.len(), 1);
        assert!(body.right_brace_span.is_some());
        let sentinel = parsed.ast().items().get(*sentinel).unwrap();
        assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
    }
}

#[test]
fn only_a_paired_suffix_group_keeps_a_nested_file_starter() {
    for suffix in ["async(\nclass Hidden)", "async ("] {
        let text = format!("class C: I {suffix}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        assert_eq!(codes(parsed.diagnostics()), ["L0002"], "{text}");
        let [class, sentinel] = parsed.roots() else {
            panic!("expected both file roots for {text}");
        };
        let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
            panic!("recovered classifier");
        };
        let [entry] = class.supertypes.as_slice() else {
            panic!("one original supertype");
        };
        assert_eq!(
            sources
                .slice(entry.trailing_lexical_error.unwrap())
                .unwrap(),
            suffix
        );
        let sentinel = parsed.ast().items().get(*sentinel).unwrap();
        assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
    }
}

#[test]
fn an_unproved_suffix_angle_does_not_turn_its_argument_into_a_parent() {
    for suffix in ["Junk<A, B", "Junk<1, B>, J"] {
        let text =
            format!("class C: I async {suffix} {{ fun member(): Unit {{}} }}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        assert_eq!(codes(parsed.diagnostics()), ["L0002"]);
        let [class, sentinel] = parsed.roots() else {
            panic!("expected both file roots");
        };
        let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
            panic!("recovered classifier");
        };
        let [entry] = class.supertypes.as_slice() else {
            panic!("ambiguous inner comma cannot establish a parent");
        };
        assert_eq!(
            sources
                .slice(entry.trailing_lexical_error.unwrap())
                .unwrap(),
            format!("async {suffix}")
        );
        assert_eq!(class.body.as_ref().unwrap().members.len(), 1);
        let sentinel = parsed.ast().items().get(*sentinel).unwrap();
        assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
    }
}

#[test]
fn a_lexically_poisoned_string_after_a_complete_supertype_keeps_its_owner() {
    for (suffix, diagnostic, separator) in [("\"bad", "L0004", "\n,"), (r#""bad\q""#, "L0006", ",")]
    {
        let text = format!("class C: I {suffix}{separator} J {{}}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        assert_eq!(codes(parsed.diagnostics()), [diagnostic], "{text}");
        let [class, sentinel] = parsed.roots() else {
            panic!("expected both file roots for {text}");
        };
        let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
            panic!("recovered classifier");
        };
        let [first, following] = class.supertypes.as_slice() else {
            panic!("original and following supertypes for {text}");
        };
        assert_eq!(
            sources
                .slice(first.trailing_lexical_error.unwrap())
                .unwrap(),
            suffix
        );
        assert_eq!(sources.slice(following.span).unwrap(), "J");
        assert!(class.body.as_ref().unwrap().right_brace_span.is_some());
        let sentinel = parsed.ast().items().get(*sentinel).unwrap();
        assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
    }
}

#[test]
fn a_nested_classifier_suffix_preserves_the_enclosing_members() {
    for member in [
        "deinit() {}",
        "companion { fun keep(): Unit {} }",
        "own fun keep(): Unit {}",
    ] {
        for (suffix, separator) in [
            ("async", "\n"),
            ("async", "; "),
            ("async (", "\n"),
            ("async (deinit() {})", "\n"),
        ] {
            let text = format!("class Outer {{\nclass Inner: I {suffix}{separator}{member}\n}}");
            let mut sources = SourceMap::new();
            let source = add_source(&mut sources, "nested-suffix.ko", &text);
            let file = parse_file_twice(&sources, source, &text);
            let declaration = parse_declaration_twice(&sources, source, &text);
            for (ast, root, diagnostics) in [
                (file.ast(), file.roots()[0], file.diagnostics()),
                (
                    declaration.ast(),
                    declaration.root(),
                    declaration.diagnostics(),
                ),
            ] {
                assert_eq!(codes(diagnostics), ["L0077", "L0002"], "{text}");
                let Item::Classifier(outer) = ast.items().get(root).unwrap().payload() else {
                    panic!("outer classifier for {text}");
                };
                let [inner, sibling] = outer.body.as_ref().unwrap().members.as_slice() else {
                    panic!("nested classifier and its outer sibling for {text}");
                };
                let inner = ast.items().get(*inner).unwrap();
                let Item::Classifier(inner) = inner.payload() else {
                    panic!("nested classifier for {text}");
                };
                assert!(inner.body.is_none(), "{text}");
                assert_eq!(
                    sources
                        .slice(inner.supertypes[0].trailing_lexical_error.unwrap())
                        .unwrap(),
                    suffix
                );
                let sibling = ast.items().get(*sibling).unwrap();
                assert_eq!(sources.slice(sibling.span()).unwrap(), member, "{text}");
            }
        }
    }
}

#[test]
fn function_parent_parameters_keep_their_comma_after_lexical_errors() {
    let parent = "(A async, B) -> I";
    let text = format!("class C: {parent}, J {{}}\nclass Sentinel");
    let (sources, parsed) = file(&text);
    assert_eq!(codes(parsed.diagnostics()), ["L0002"]);
    let [class, sentinel] = parsed.roots() else {
        panic!("class and following declaration");
    };
    let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
        panic!("recovered classifier");
    };
    let [first, following] = class.supertypes.as_slice() else {
        panic!("two original supertypes");
    };
    assert_eq!(sources.slice(first.span).unwrap(), parent);
    let types = parsed.ast().type_refs();
    let lang_frontend::parser::TypeRef::Function {
        parameters,
        return_type,
        ..
    } = types.get(first.type_ref).unwrap().payload()
    else {
        panic!("function TypeRef");
    };
    let [damaged, intact] = parameters.as_slice() else {
        panic!("two original function parameters");
    };
    assert_eq!(
        sources.slice(damaged.trailing_error.unwrap()).unwrap(),
        "async"
    );
    assert_eq!(sources.slice(damaged.span).unwrap(), "A async");
    assert!(intact.trailing_error.is_none());

    assert_eq!(
        sources
            .slice(types.get(damaged.type_ref).unwrap().span())
            .unwrap(),
        "A"
    );
    assert_eq!(
        sources
            .slice(types.get(intact.type_ref).unwrap().span())
            .unwrap(),
        "B"
    );
    assert_eq!(
        sources
            .slice(types.get(*return_type).unwrap().span())
            .unwrap(),
        "I"
    );
    assert_eq!(sources.slice(following.span).unwrap(), "J");
    assert!(class.body.as_ref().unwrap().right_brace_span.is_some());
    assert_eq!(
        sources
            .slice(parsed.ast().items().get(*sentinel).unwrap().span())
            .unwrap(),
        "class Sentinel"
    );
}

#[test]
fn delegation_requires_the_actual_closing_angle_of_its_parent() {
    let text = "class C: I<A async, B> by delegate, J {}\nclass Sentinel";
    let (sources, parsed) = file(text);
    assert_eq!(codes(parsed.diagnostics()), ["L0002"]);
    let [class, _] = parsed.roots() else {
        panic!("two original roots");
    };
    let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
        panic!("recovered classifier");
    };
    let [first, following] = class.supertypes.as_slice() else {
        panic!("two original parents");
    };
    let clause = first.delegation.unwrap();
    assert_eq!(sources.slice(clause.span).unwrap(), "by delegate");
    assert_eq!(sources.slice(following.span).unwrap(), "J");

    let (sources, parsed) = file("class C: I<async by field\nclass Sentinel");
    let [class, sentinel] = parsed.roots() else {
        panic!("unfinished parent and following class");
    };
    let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
        panic!("recovered classifier");
    };
    assert!(class.supertypes[0].delegation.is_none());
    assert_eq!(codes(parsed.diagnostics()), ["L0002", "L0010"]);
    assert_eq!(
        sources
            .slice(parsed.ast().items().get(*sentinel).unwrap().span())
            .unwrap(),
        "class Sentinel"
    );
}

#[test]
fn damaged_type_elements_only_obey_delimiters_with_a_current_owner() {
    for parent in [
        "I<A async ], B>",
        "I<A async < 1, B>",
        "(A async ], B) -> I",
        "(A async < 1, B) -> I",
        "(A async (Y) -> I, B) -> J",
        "I<(A async >, B) -> R, D>",
        "(A async { -> 1 }, B) -> I",
        "I<[A, B], C>",
        "I<<A, B>, C>",
        "([A, B], C) -> I",
    ] {
        let text = format!("class C: {parent}, J {{}}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        let [class, sentinel] = parsed.roots() else {
            panic!("class and sentinel for {text}");
        };
        let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
            panic!("classifier for {text}");
        };
        let [first, following] = class.supertypes.as_slice() else {
            panic!("two parent entries for {text}");
        };
        assert_eq!(sources.slice(first.span).unwrap(), parent);
        assert_eq!(sources.slice(following.span).unwrap(), "J");
        assert!(class.body.as_ref().unwrap().right_brace_span.is_some());
        assert_eq!(
            sources
                .slice(parsed.ast().items().get(*sentinel).unwrap().span())
                .unwrap(),
            "class Sentinel"
        );
    }
}

#[test]
fn a_missing_type_closer_after_a_comma_preserves_the_next_declaration() {
    for parent in ["I<A async,", "(A async,"] {
        let text = format!("class C: {parent}\nfun next(): Unit {{}}\nclass Sentinel");
        let (sources, parsed) = file(&text);
        let [class, next, sentinel] = parsed.roots() else {
            panic!("class, function and sentinel for {text}");
        };
        let items = parsed.ast().items();
        let Item::Classifier(class) = items.get(*class).unwrap().payload() else {
            panic!("classifier for {text}");
        };
        assert!(class.body.is_none());
        assert_eq!(
            sources.slice(items.get(*next).unwrap().span()).unwrap(),
            "fun next(): Unit {}"
        );
        assert_eq!(
            sources.slice(items.get(*sentinel).unwrap().span()).unwrap(),
            "class Sentinel"
        );
    }

    let (sources, parsed) = declaration("class Outer {\nclass Inner: I<A async,\ndeinit() {}\n}");
    let [inner, deinit] = classifier(&parsed)
        .body
        .as_ref()
        .unwrap()
        .members
        .as_slice()
    else {
        panic!("nested classifier and original deinit");
    };
    let Item::Classifier(inner) = item(&parsed, *inner) else {
        panic!("nested classifier");
    };
    assert!(inner.body.is_none());
    assert_eq!(
        sources
            .slice(parsed.ast().items().get(*deinit).unwrap().span())
            .unwrap(),
        "deinit() {}"
    );
}

#[test]
fn a_contextual_member_name_can_still_be_a_nested_type_name() {
    let (sources, parsed) =
        declaration("class Outer { class Inner: I<A, deinit>, (deinit) -> I {} }");
    let [inner] = classifier(&parsed)
        .body
        .as_ref()
        .unwrap()
        .members
        .as_slice()
    else {
        panic!("one nested classifier");
    };
    let Item::Classifier(inner) = item(&parsed, *inner) else {
        panic!("nested classifier");
    };
    assert_eq!(codes(parsed.diagnostics()), ["L0077"]);
    let [qualified, function] = inner.supertypes.as_slice() else {
        panic!("two original supertypes");
    };
    assert_eq!(sources.slice(qualified.span).unwrap(), "I<A, deinit>");
    assert_eq!(sources.slice(function.span).unwrap(), "(deinit) -> I");
}

#[test]
fn type_element_start_respects_contextual_names_and_real_file_prefixes() {
    for ty in ["value", "() -> value"] {
        let (sources, parsed) = file(&format!("class C: {ty}\nclass Sentinel"));
        assert!(parsed.diagnostics().is_empty(), "{ty}");
        let [class, _] = parsed.roots() else {
            panic!("original class and sentinel");
        };
        let Item::Classifier(class) = parsed.ast().items().get(*class).unwrap().payload() else {
            panic!("classifier");
        };
        assert_eq!(sources.slice(class.supertypes[0].span).unwrap(), ty);
    }

    for ty in ["deinit", "() -> deinit", "(borrow own) -> I"] {
        let (sources, parsed) = declaration(&format!("class Outer {{ class Inner: {ty} {{}} }}"));
        assert_eq!(codes(parsed.diagnostics()), ["L0077"], "{ty}");
        let [inner] = classifier(&parsed)
            .body
            .as_ref()
            .unwrap()
            .members
            .as_slice()
        else {
            panic!("one nested classifier");
        };
        let Item::Classifier(inner) = item(&parsed, *inner) else {
            panic!("nested classifier");
        };
        assert!(inner.body.is_some(), "{ty}");
        assert_eq!(sources.slice(inner.supertypes[0].span).unwrap(), ty);
    }

    let (sources, parsed) = file("class C: I<A async,\nvalue class Sentinel(val i: Int)");
    let [class, sentinel] = parsed.roots() else {
        panic!("unfinished parent and value class");
    };
    let items = parsed.ast().items();
    let Item::Classifier(class) = items.get(*class).unwrap().payload() else {
        panic!("unfinished classifier");
    };
    assert!(class.body.is_none());
    assert_eq!(
        sources.slice(items.get(*sentinel).unwrap().span()).unwrap(),
        "value class Sentinel(val i: Int)"
    );
}

#[test]
fn an_error_group_cannot_jump_past_a_proven_function_parameter_closer() {
    let text = "class C: (A async [X) -> I, J ], K {}\nclass Sentinel";
    let (sources, parsed) = file(text);
    let items = parsed.ast().items();
    let Item::Classifier(class) = items.get(parsed.roots()[0]).unwrap().payload() else {
        panic!("classifier");
    };
    let [function, sibling] = class.supertypes.as_slice() else {
        panic!("function parent and J");
    };
    assert_eq!(sources.slice(function.span).unwrap(), "(A async [X) -> I");
    assert_eq!(sources.slice(sibling.span).unwrap(), "J");
    let lang_frontend::parser::TypeRef::Function {
        arrow_span,
        parameters,
        ..
    } = parsed
        .ast()
        .type_refs()
        .get(function.type_ref)
        .unwrap()
        .payload()
    else {
        panic!("function TypeRef");
    };
    assert_eq!(sources.slice(*arrow_span).unwrap(), "->");
    let [parameter] = parameters.as_slice() else {
        panic!("one function parameter");
    };
    assert_eq!(
        sources.slice(parameter.trailing_error.unwrap()).unwrap(),
        "async [X"
    );
}

#[test]
fn a_proven_parenthesis_cannot_spend_an_explicit_file_separator() {
    let (sources, parsed) = file("class C: (A async;\nfun next(): Unit {} )\nclass Sentinel");
    let items = parsed.ast().items();
    let Item::Classifier(class) = items.get(parsed.roots()[0]).unwrap().payload() else {
        panic!("classifier");
    };
    assert!(class.body.is_none());
    assert_eq!(sources.slice(class.supertypes[0].span).unwrap(), "(A async");
    assert!(parsed.roots().iter().any(|id| {
        let item = items.get(*id).unwrap();
        sources.slice(item.span()).unwrap() == "fun next(): Unit {}"
    }));
    assert_eq!(
        sources
            .slice(items.get(*parsed.roots().last().unwrap()).unwrap().span())
            .unwrap(),
        "class Sentinel"
    );
}

#[test]
fn an_unproved_type_group_does_not_spend_the_standalone_separator() {
    let (sources, parsed) = declaration("class C: I<A async [X;");
    let class = classifier(&parsed);
    assert_eq!(
        sources.slice(class.supertypes[0].span).unwrap(),
        "I<A async [X"
    );
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| { sources.slice(diagnostic.primary_span()).unwrap() == ";" })
    );
}
