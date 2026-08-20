//! SPEC-0064 窄化接口委托的 Phase 1 AST、恢复与拒绝边界测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::{LexemeKind, TokenKind, lex},
    parser::{ClassifierDeclaration, Item, NameMarker, ParsedDeclaration, parse_declaration},
    source::{SourceMap, Span},
};

fn declaration(text: &str) -> (SourceMap, ParsedDeclaration) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("delegation.ko", text)
        .expect("unique source");
    let lexed = lex(&sources, source_id).expect("lex");
    let parsed = parse_declaration(&sources, &lexed).expect("parse");
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
    let lexed = lex(&sources, source_id).expect("lex");
    assert!(lexed.lexemes().iter().any(|lexeme| {
        matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier))
            && sources
                .slice(lexeme.span())
                .is_ok_and(|slice| slice == "by")
    }));
    let parsed = parse_declaration(&sources, &lexed).expect("parse");
    assert!(parsed.diagnostics().is_empty());
}
