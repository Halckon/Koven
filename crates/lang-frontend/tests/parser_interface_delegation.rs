//! SPEC-0064 / SPEC-0126 窄化接口委托的 Phase 1 AST、恢复与拒绝边界测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::{LexemeKind, TokenKind},
    parser::{ClassifierDeclaration, Item, NameMarker, ParsedDeclaration},
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
    assert_eq!(sources.slice(following.span).unwrap(), "J");
    assert!(classifier.body.as_ref().unwrap().right_brace_span.is_some());
}

#[test]
fn a_lexical_error_after_a_valid_target_keeps_the_target_name() {
    let (sources, parsed) = declaration("class C: I by delegate async, J {}");
    assert_eq!(codes(parsed.diagnostics()), ["L0002"]);
    let [first, following] = classifier(&parsed).supertypes.as_slice() else {
        panic!("delegation and following supertype");
    };
    let clause = first.delegation.unwrap();
    let NameMarker::Present(span) = clause.target else {
        panic!("delegate name stays available");
    };
    assert_eq!(sources.slice(span).unwrap(), "delegate");
    assert_eq!(sources.slice(clause.span).unwrap(), "by delegate");
    assert_eq!(sources.slice(following.span).unwrap(), "J");
}
