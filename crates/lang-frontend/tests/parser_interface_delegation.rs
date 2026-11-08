//! SPEC-0064 / SPEC-0126 窄化接口委托的 Phase 1 AST、恢复与拒绝边界测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::{LexemeKind, TokenKind},
    parser::{ClassifierDeclaration, ClassifierKind, Item, NameMarker, ParsedDeclaration, TypeRef},
    source::{SourceMap, Span},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    lex_and_parse_declaration_twice, parse_declaration_twice, parse_file_twice,
};

fn declaration(text: &str) -> (SourceMap, ParsedDeclaration) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("delegation.ko", text)
        .expect("unique source");
    let parsed = parse_declaration_twice(&sources, source_id, text);
    (sources, parsed)
}

fn classifier(parsed: &ParsedDeclaration) -> &ClassifierDeclaration {
    let root = parsed.ast().items().get(parsed.root()).expect("root");
    let Item::Classifier(classifier) = root.payload() else {
        panic!("classifier")
    };
    classifier
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn marker_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
    }
}

#[test]
fn ordinary_class_preserves_mixed_delegated_supertypes_in_source_order() {
    let text = "class Service(private val logger: Logger, private val audit: Audit): Logger by logger, Closeable, Auditor by audit {}";
    let (sources, parsed) = declaration(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let entries = &classifier(&parsed).supertypes;
    assert_eq!(entries.len(), 3);
    assert!(
        entries
            .iter()
            .all(|entry| entry.trailing_lexical_error.is_none())
    );
    assert!(entries[1].delegation.is_none());
    for (entry, expected_clause, expected_target) in [
        (&entries[0], "by logger", "logger"),
        (&entries[2], "by audit", "audit"),
    ] {
        let clause = entry.delegation.expect("delegation");
        assert_eq!(sources.slice(clause.span).expect("clause"), expected_clause);
        assert_eq!(
            sources.slice(marker_span(clause.target)).expect("target"),
            expected_target
        );
        assert_eq!(clause.span.source_id(), parsed.source_id());
        assert_eq!(entry.span.end(), clause.span.end());
    }
}

#[test]
fn missing_target_uses_l0078_without_consuming_the_next_owner_boundary() {
    for (text, expected_primary) in [
        ("class C: I by, J", ""),
        ("class C: I by {}", ""),
        ("class C: I by", ""),
    ] {
        let (sources, parsed) = declaration(text);
        assert_eq!(codes(parsed.diagnostics()), ["L0078"], "{text:?}");
        let diagnostic = &parsed.diagnostics()[0];
        assert_eq!(
            sources.slice(diagnostic.primary_span()).expect("primary"),
            expected_primary
        );
        let clause = classifier(&parsed).supertypes[0]
            .delegation
            .expect("delegation");
        assert!(matches!(clause.target, NameMarker::Missing(_)));
        assert_eq!(clause.span.end(), clause.by_span.end());
        assert_eq!(
            classifier(&parsed).supertypes[0].span.end(),
            clause.by_span.end()
        );
        if text.contains(", J") {
            assert_eq!(classifier(&parsed).supertypes.len(), 2);
        }
        if text.ends_with("{}") {
            assert!(classifier(&parsed).body.is_some());
        }
    }

    let (sources, invalid) = declaration("class C: I by 42, J {}");
    assert_eq!(codes(invalid.diagnostics()), ["L0078"]);
    assert_eq!(
        sources
            .slice(invalid.diagnostics()[0].primary_span())
            .expect("primary"),
        "42"
    );
    let clause = classifier(&invalid).supertypes[0]
        .delegation
        .expect("delegation");
    assert!(matches!(clause.target, NameMarker::Error(_)));
    assert_eq!(classifier(&invalid).supertypes.len(), 2);
    assert!(classifier(&invalid).body.is_some());
}

#[test]
fn string_target_preserves_the_delegation_and_file_owners() {
    for target in [r#""""#, r#""${"nested"}""#] {
        let text = format!("class C: I by {target}, J {{ fun member(): Unit {{}} }}");
        let (sources, parsed) = declaration(&text);
        assert_eq!(codes(parsed.diagnostics()), ["L0078"]);
        assert_eq!(
            sources
                .slice(parsed.diagnostics()[0].primary_span())
                .unwrap(),
            "\""
        );
        let clause = classifier(&parsed).supertypes[0].delegation.unwrap();
        let NameMarker::Error(span) = clause.target else {
            panic!("invalid delegation target")
        };
        assert_eq!(sources.slice(span).unwrap(), target);
        assert_eq!(sources.slice(clause.span).unwrap(), format!("by {target}"));

        let mut sources = SourceMap::new();
        let source = sources
            .add_source("delegation-file.ko", format!("{text}\nclass Sentinel"))
            .unwrap();
        let parsed = parse_file_twice(&sources, source, target);
        assert_eq!(codes(parsed.diagnostics()), ["L0078"]);
        let [class, sentinel] = parsed.roots() else {
            panic!("expected both file declarations")
        };
        let first = parsed.ast().items().get(*class).unwrap();
        let Item::Classifier(classifier) = first.payload() else {
            panic!("class with recovered delegation")
        };
        assert_eq!(classifier.supertypes.len(), 2);
        let body = classifier.body.as_ref().unwrap();
        assert_eq!(body.members.len(), 1);
        assert_eq!(sources.slice(body.right_brace_span.unwrap()).unwrap(), "}");
        assert_eq!(sources.slice(first.span()).unwrap(), text);
        let sentinel = parsed.ast().items().get(*sentinel).unwrap();
        assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
    }
}

#[test]
fn poisoned_string_target_keeps_the_lexical_diagnostic_and_next_supertype() {
    let target = r#""bad\q""#;
    let text = format!("class C: I by {target}, J {{}}");
    let (sources, parsed) = declaration(&text);
    assert_eq!(codes(parsed.diagnostics()), ["L0006"]);
    let classifier = classifier(&parsed);
    assert_eq!(classifier.supertypes.len(), 2);
    let NameMarker::Error(span) = classifier.supertypes[0].delegation.unwrap().target else {
        panic!("invalid delegation target")
    };
    assert_eq!(sources.slice(span).unwrap(), target);
    assert!(classifier.body.as_ref().unwrap().right_brace_span.is_some());
}

#[test]
fn unterminated_string_target_preserves_the_next_file_declaration() {
    let text = "class C: I by \"bad\nclass Sentinel";
    let mut sources = SourceMap::new();
    let source = sources.add_source("delegation-file.ko", text).unwrap();
    let parsed = parse_file_twice(&sources, source, text);
    assert_eq!(codes(parsed.diagnostics()), ["L0004"]);
    let [class, sentinel] = parsed.roots() else {
        panic!("expected both file declarations");
    };
    let first = parsed.ast().items().get(*class).unwrap();
    let Item::Classifier(classifier) = first.payload() else {
        panic!("class with recovered delegation");
    };
    let NameMarker::Error(span) = classifier.supertypes[0].delegation.unwrap().target else {
        panic!("invalid delegation target");
    };
    assert_eq!(sources.slice(span).unwrap(), "\"bad");
    let sentinel = parsed.ast().items().get(*sentinel).unwrap();
    assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
}

#[test]
fn adjacent_unterminated_targets_do_not_consume_each_others_owner_events() {
    let (sources, parsed) = declaration("class C: I by \"bad\n, J by \"worse\n, K {}");
    assert_eq!(codes(parsed.diagnostics()), ["L0004", "L0004"]);
    let classifier = classifier(&parsed);
    let [first, second, third] = classifier.supertypes.as_slice() else {
        panic!("expected all three supertypes");
    };
    for (entry, target) in [(first, "\"bad"), (second, "\"worse")] {
        let NameMarker::Error(span) = entry.delegation.unwrap().target else {
            panic!("invalid delegation target")
        };
        assert_eq!(sources.slice(span).unwrap(), target);
    }
    assert!(third.delegation.is_none());
    assert!(classifier.body.as_ref().unwrap().right_brace_span.is_some());
}

#[test]
fn unsupported_contexts_and_expression_targets_remain_directionally_rejected() {
    for text in [
        "value class V(val d: D): I by d",
        "interface I: Parent by d",
        "enum class E: I by d { A }",
        "object O: I by d",
    ] {
        let (_, parsed) = declaration(text);
        assert_eq!(codes(parsed.diagnostics()), ["L0077"], "{text:?}");
    }

    let (_, property) = declaration("class C { val x: Int by d }");
    assert!(codes(property.diagnostics()).contains(&"L0077".to_owned()));

    for text in [
        "class C(val d: D): I by make(), J {}",
        "class C(val d: D): I by make({ -> 1 }), J {}",
        "class C: I({ -> 1 }), J {}",
        "class C(val d: D): I by d.member, J {}",
        "class C(val d: D): I by d by other, J {}",
    ] {
        let (_, parsed) = declaration(text);
        assert_eq!(codes(parsed.diagnostics()), ["L0077"], "{text:?}");
        assert_eq!(classifier(&parsed).supertypes.len(), 2, "{text:?}");
        assert!(classifier(&parsed).body.is_some(), "{text:?}");
    }
}

#[test]
fn grouped_invalid_delegation_does_not_promote_inner_tokens_to_supertypes() {
    for target in ["(x, y)", "[x, y]", "({ -> 1 })", r#"("${x}", y)"#] {
        let text = format!("class C: I by {target}, J {{ fun member(): Unit {{}} }}");
        let (sources, parsed) = declaration(&text);
        assert_eq!(codes(parsed.diagnostics()), ["L0078"], "{text}");
        let class = classifier(&parsed);
        let [first, second] = class.supertypes.as_slice() else {
            panic!("both real supertypes for {text}");
        };
        let NameMarker::Error(span) = first.delegation.unwrap().target else {
            panic!("grouped error target");
        };
        assert_eq!(sources.slice(span).unwrap(), target);
        assert_eq!(sources.slice(second.span).unwrap(), "J");
        let body = class.body.as_ref().unwrap();
        assert_eq!(body.members.len(), 1);
        assert!(body.right_brace_span.is_some());
    }
}

#[test]
fn phase_one_defers_constructor_field_and_interface_checks() {
    for text in [
        "class C(var mutable: Impl): I by mutable",
        "class C: I by missing",
    ] {
        let (_, parsed) = declaration(text);
        assert!(parsed.diagnostics().is_empty(), "{text:?}");
        assert!(classifier(&parsed).supertypes[0].delegation.is_some());
    }
}

#[test]
fn by_remains_an_identifier_outside_the_supertype_context() {
    let text = "val by = 1";
    let mut sources = SourceMap::new();
    let source_id = sources.add_source("identifier.ko", text).expect("source");
    let (lexed, parsed) = lex_and_parse_declaration_twice(&sources, source_id, text);
    assert!(lexed.lexemes().iter().any(|lexeme| {
        matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier))
            && sources
                .slice(lexeme.span())
                .is_ok_and(|slice| slice == "by")
    }));
    assert!(parsed.diagnostics().is_empty());
}

#[test]
fn consecutive_lexical_errors_remain_in_the_invalid_delegation_target() {
    let text = "class C: I by β async, J {}";
    let (sources, parsed) = declaration(text);
    assert_eq!(codes(parsed.diagnostics()), ["L0001", "L0002"]);
    let classifier = classifier(&parsed);
    let [first, following] = classifier.supertypes.as_slice() else {
        panic!("recovered delegation and following supertype");
    };
    let clause = first.delegation.unwrap();
    let NameMarker::Error(span) = clause.target else {
        panic!("reachable target error");
    };
    assert_eq!(sources.slice(span).unwrap(), "β async");
    assert_eq!(sources.slice(clause.span).unwrap(), "by β async");
    assert_eq!(sources.slice(first.span).unwrap(), "I by β async");
    assert!(first.trailing_lexical_error.is_none());
    assert_eq!(sources.slice(following.span).unwrap(), "J");
    assert!(classifier.body.as_ref().unwrap().right_brace_span.is_some());
}

#[test]
fn a_lexical_error_after_a_valid_target_keeps_the_target_name() {
    let text = "class C: I by delegate async β  , J {}\nclass Sentinel";
    let mut sources = SourceMap::new();
    let source = sources.add_source("delegation-file.ko", text).unwrap();
    let parsed = parse_file_twice(&sources, source, text);
    assert_eq!(codes(parsed.diagnostics()), ["L0002", "L0001"]);
    let [class, sentinel] = parsed.roots() else {
        panic!("expected both file declarations");
    };
    let Item::Classifier(classifier) = parsed.ast().items().get(*class).unwrap().payload() else {
        panic!("class with recovered delegation");
    };
    let [first, following] = classifier.supertypes.as_slice() else {
        panic!("delegation and following supertype");
    };
    let clause = first.delegation.unwrap();
    let NameMarker::Present(span) = clause.target else {
        panic!("delegate name stays available");
    };
    assert_eq!(sources.slice(span).unwrap(), "delegate");
    assert_eq!(sources.slice(clause.span).unwrap(), "by delegate");
    assert_eq!(sources.slice(first.span).unwrap(), "I by delegate async β");
    assert_eq!(
        sources
            .slice(first.trailing_lexical_error.unwrap())
            .unwrap(),
        "async β"
    );
    assert!(following.trailing_lexical_error.is_none());
    assert_eq!(sources.slice(following.span).unwrap(), "J");
    assert!(classifier.body.as_ref().unwrap().right_brace_span.is_some());
    let sentinel = parsed.ast().items().get(*sentinel).unwrap();
    assert_eq!(sources.slice(sentinel.span()).unwrap(), "class Sentinel");
}

fn assert_delegation_suffix(suffix: &str, diagnostics: &[&str]) {
    let text = format!("class C: I by delegate {suffix}  , J {{}}");
    let (sources, parsed) = declaration(&text);
    assert_eq!(codes(parsed.diagnostics()), diagnostics, "{text}");
    let classifier = classifier(&parsed);
    let [first, following] = classifier.supertypes.as_slice() else {
        panic!("delegation and following supertype");
    };
    let parent = parsed.ast().type_refs().get(first.type_ref).unwrap();
    assert!(matches!(parent.payload(), TypeRef::Qualified { .. }));
    assert_eq!(sources.slice(parent.span()).unwrap(), "I");
    assert_eq!(
        sources.slice(first.span).unwrap(),
        format!("I by delegate {suffix}")
    );
    assert_eq!(
        sources
            .slice(first.trailing_lexical_error.unwrap())
            .unwrap(),
        suffix
    );
    assert_eq!(
        sources.slice(first.delegation.unwrap().span).unwrap(),
        "by delegate"
    );
    assert_eq!(sources.slice(following.span).unwrap(), "J");
    assert!(classifier.body.as_ref().unwrap().right_brace_span.is_some());
}

#[test]
fn closed_type_arguments_after_delegation_poison_keep_the_outer_comma() {
    assert_delegation_suffix("async<A, B>", &["L0002"]);
}

#[test]
fn a_closed_group_after_delegation_poison_keeps_its_own_brace() {
    assert_delegation_suffix("async({ -> 1 })", &["L0002"]);
}

#[test]
fn an_unclosed_type_list_after_delegation_poison_keeps_the_outer_comma() {
    assert_delegation_suffix("async<A", &["L0002"]);
}

#[test]
fn lexical_delegation_suffix_keeps_nested_type_and_group_commas_local() {
    for (suffix, diagnostics) in [
        ("async Junk<A, β>", &["L0002", "L0001"][..]),
        ("async Junk(A, { -> 1 })", &["L0002"][..]),
        ("async(A)<B, C>", &["L0002"][..]),
        (r#""bad\q""#, &["L0006"][..]),
    ] {
        assert_delegation_suffix(suffix, diagnostics);
    }
}

#[test]
fn recovered_delegation_suffix_leaves_the_enclosing_member_to_its_owner() {
    for (suffix, separator, expected) in [
        ("async Junk<A, β>", ";", &["L0077", "L0002", "L0001"][..]),
        ("async<A", "", &["L0077", "L0002"][..]),
    ] {
        let text = format!(
            "class Outer {{\n class Inner: I by delegate {suffix}{separator}\n own fun keep(): Unit {{}}\n}}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("delegation-member.ko", &text).unwrap();
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
            assert_eq!(codes(diagnostics), expected, "{text}");
            let Item::Classifier(outer) = ast.items().get(root).unwrap().payload() else {
                panic!("outer classifier");
            };
            let [inner, member] = outer.body.as_ref().unwrap().members.as_slice() else {
                panic!("nested classifier and its outer sibling");
            };
            let Item::Classifier(inner) = ast.items().get(*inner).unwrap().payload() else {
                panic!("nested classifier");
            };
            assert!(inner.body.is_none());
            let [parent] = inner.supertypes.as_slice() else {
                panic!("one recovered delegation");
            };
            assert_eq!(
                sources.slice(parent.trailing_lexical_error.unwrap()),
                Ok(suffix)
            );
            assert_eq!(
                sources.slice(parent.delegation.unwrap().span),
                Ok("by delegate")
            );
            assert_eq!(
                sources.slice(ast.items().get(*member).unwrap().span()),
                Ok("own fun keep(): Unit {}")
            );
        }
    }
}

#[test]
fn lexical_suffix_extends_an_existing_delegation_error_target() {
    let (sources, parsed) = declaration("class C: I by 42 async Junk<A, β>, J {}");
    assert_eq!(codes(parsed.diagnostics()), ["L0078", "L0002", "L0001"]);
    let [parent, following] = classifier(&parsed).supertypes.as_slice() else {
        panic!("error delegation and original following parent");
    };
    let NameMarker::Error(target) = parent.delegation.unwrap().target else {
        panic!("existing invalid target");
    };
    assert_eq!(sources.slice(target), Ok("42 async Junk<A, β>"));
    assert!(parent.trailing_lexical_error.is_none());
    assert_eq!(sources.slice(following.span), Ok("J"));
}

#[test]
fn poisoned_string_before_unproved_delegation_angle_keeps_the_outer_comma() {
    assert_delegation_suffix(r#""bad\q"<A"#, &["L0006"]);
    assert_delegation_suffix(r#"async "bad\q"<A"#, &["L0002", "L0006"]);
}

#[test]
fn invalid_delegation_target_keeps_proven_inner_commas_out_of_the_parent_list() {
    for (target, expected) in [
        ("β<A, B>", &["L0001"][..]),
        ("async<A, β>", &["L0002", "L0001"][..]),
        ("42<A, B>", &["L0078"][..]),
        (r#""bad\q"<A, B>"#, &["L0006"][..]),
        ("async<A", &["L0002"][..]),
        ("β Junk<A", &["L0001"][..]),
        ("42 Junk<A", &["L0078"][..]),
        ("β async Junk<A", &["L0001", "L0002"][..]),
        ("<A, B>", &["L0078"][..]),
        ("<A", &["L0078"][..]),
        ("β Junk([A, B], { -> 1 })", &["L0001"][..]),
    ] {
        let text = format!("class C: I by {target}, J {{ fun keep(): Unit {{}} }}");
        let (sources, parsed) = declaration(&text);
        let file = parse_file_twice(&sources, parsed.source_id(), &text);
        let Item::Classifier(file_class) =
            file.ast().items().get(file.roots()[0]).unwrap().payload()
        else {
            panic!("file classifier");
        };
        for (class, diagnostics) in [
            (classifier(&parsed), parsed.diagnostics()),
            (file_class, file.diagnostics()),
        ] {
            assert_eq!(codes(diagnostics), expected, "{text}");
            let [invalid, following] = class.supertypes.as_slice() else {
                panic!("original error entry and following parent for {text}");
            };
            let clause = invalid.delegation.unwrap();
            assert!(matches!(clause.target, NameMarker::Error(_)));
            assert_eq!(sources.slice(marker_span(clause.target)), Ok(target));
            assert_eq!(
                sources.slice(clause.span),
                Ok(format!("by {target}").as_str())
            );
            assert!(invalid.trailing_lexical_error.is_none());
            assert_eq!(sources.slice(following.span), Ok("J"));
            let body = class.body.as_ref().unwrap();
            assert_eq!(body.members.len(), 1);
            assert!(body.right_brace_span.is_some());
        }
    }
}

#[test]
fn invalid_delegation_target_only_stops_at_proven_member_prefixes() {
    for target in ["β value Junk<A, B>", "β deinit", "β deinit()"] {
        let text = format!(
            "class Outer {{\n class Inner: I by {target}, J;\n own fun keep(): Unit {{}}\n}}"
        );
        let (sources, parsed) = declaration(&text);
        let file = parse_file_twice(&sources, parsed.source_id(), &text);
        let Item::Classifier(file_class) =
            file.ast().items().get(file.roots()[0]).unwrap().payload()
        else {
            panic!("file classifier");
        };
        for (ast, outer, diagnostics) in [
            (parsed.ast(), classifier(&parsed), parsed.diagnostics()),
            (file.ast(), file_class, file.diagnostics()),
        ] {
            assert_eq!(codes(diagnostics), ["L0077", "L0001"], "{text}");
            let [inner, member] = outer.body.as_ref().unwrap().members.as_slice() else {
                panic!("nested classifier and actual enclosing member");
            };
            let Item::Classifier(inner) = ast.items().get(*inner).unwrap().payload() else {
                panic!("nested classifier");
            };
            assert!(inner.body.is_none());
            let [invalid, following] = inner.supertypes.as_slice() else {
                panic!("the invalid target and original following parent");
            };
            let NameMarker::Error(error) = invalid.delegation.unwrap().target else {
                panic!("invalid target");
            };
            assert_eq!(sources.slice(error), Ok(target));
            assert_eq!(sources.slice(following.span), Ok("J"));
            assert_eq!(
                sources.slice(ast.items().get(*member).unwrap().span()),
                Ok("own fun keep(): Unit {}")
            );
        }
    }
}

#[test]
fn invalid_delegation_target_keeps_a_file_value_class_prefix() {
    let text = "class C: I by β Junk value class Sentinel(val x: Int)";
    let mut sources = SourceMap::new();
    let source = sources.add_source("delegation-file.ko", text).unwrap();
    let parsed = parse_file_twice(&sources, source, text);
    let [first, sentinel] = parsed.roots() else {
        panic!("the recovered class and the following value class");
    };
    let Item::Classifier(class) = parsed.ast().items().get(*first).unwrap().payload() else {
        panic!("original class");
    };
    let [parent] = class.supertypes.as_slice() else {
        panic!("one original parent");
    };
    let NameMarker::Error(error) = parent.delegation.unwrap().target else {
        panic!("invalid delegation target");
    };
    assert_eq!(sources.slice(error), Ok("β Junk"));
    let sentinel = parsed.ast().items().get(*sentinel).unwrap();
    let Item::Classifier(class) = sentinel.payload() else {
        panic!("following value class");
    };
    assert!(matches!(class.kind, ClassifierKind::ValueClass { .. }));
    assert_eq!(
        sources.slice(sentinel.span()),
        Ok("value class Sentinel(val x: Int)")
    );
}

#[test]
fn invalid_delegation_target_leaves_the_actual_following_deinit() {
    let text = "class Outer {\n class Inner: I by β\n deinit() {}\n}";
    let (sources, parsed) = declaration(text);
    let file = parse_file_twice(&sources, parsed.source_id(), text);
    let Item::Classifier(file_class) = file.ast().items().get(file.roots()[0]).unwrap().payload()
    else {
        panic!("file classifier");
    };
    for (ast, outer, diagnostics) in [
        (parsed.ast(), classifier(&parsed), parsed.diagnostics()),
        (file.ast(), file_class, file.diagnostics()),
    ] {
        assert_eq!(codes(diagnostics), ["L0077", "L0001"]);
        let [inner, deinit] = outer.body.as_ref().unwrap().members.as_slice() else {
            panic!("nested classifier and actual destructor");
        };
        let Item::Classifier(inner) = ast.items().get(*inner).unwrap().payload() else {
            panic!("nested classifier");
        };
        let [parent] = inner.supertypes.as_slice() else {
            panic!("one original parent");
        };
        assert_eq!(
            sources.slice(marker_span(parent.delegation.unwrap().target)),
            Ok("β")
        );
        assert_eq!(
            sources.slice(ast.items().get(*deinit).unwrap().span()),
            Ok("deinit() {}")
        );
    }
}
