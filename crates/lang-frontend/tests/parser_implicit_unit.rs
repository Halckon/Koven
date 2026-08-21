//! SPEC-0011 / SPEC-0127 的具名函数隐式 `Unit` 与显式恢复契约测试。

use lang_frontend::{
    diagnostic::Diagnostic,
    parser::{
        Expression, FunctionBody, FunctionForm, Item, ParsedDeclaration, ParserInternalError,
        Statement, TypeRef, parse_declaration,
    },
    source::{SourceError, SourceId, SourceMap},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    assert_parser_error_twice, lex_and_parse_declaration_twice, parse_declaration_twice,
};

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources.add_source(name, text).expect("unique source")
}

fn parsed(text: &str) -> (SourceMap, ParsedDeclaration) {
    let mut sources = SourceMap::new();
    let id = add_source(&mut sources, "implicit-unit.ko", text);
    let parsed = parse_declaration_twice(&sources, id, text);
    (sources, parsed)
}

fn function(parsed: &ParsedDeclaration) -> &FunctionForm {
    let Item::Function { form, .. } = parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("root")
        .payload()
    else {
        panic!("function root")
    };
    form
}

fn codes(parsed: &ParsedDeclaration) -> Vec<String> {
    parsed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn fingerprint(diagnostic: &Diagnostic) -> (String, String, usize, usize) {
    let span = diagnostic.primary_span();
    (
        diagnostic.code().to_string(),
        diagnostic.message().to_owned(),
        span.start(),
        span.end(),
    )
}

#[test]
fn implicit_and_explicit_pass_matrix_preserves_closed_forms_and_real_spans() {
    for text in ["fun f()", "fun /*a*/ f( /*b*/ )"] {
        let (_, parsed) = parsed(text);
        assert!(parsed.diagnostics().is_empty(), "{text:?}");
        assert!(matches!(
            function(&parsed),
            FunctionForm::ImplicitUnitAbsent
        ));
        assert_eq!(
            parsed.ast().type_refs().len(),
            0,
            "no synthetic Unit TypeRef"
        );
        assert_eq!(
            parsed
                .ast()
                .items()
                .get(parsed.root())
                .unwrap()
                .span()
                .end(),
            text.len()
        );
    }

    for text in ["fun f() {}", "fun f() { x val y = 1 }"] {
        let (_, parsed) = parsed(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let FunctionForm::ImplicitUnitBlock(block) = function(&parsed) else {
            panic!("{text:?}: implicit block")
        };
        assert!(matches!(
            parsed.ast().statements().get(*block).unwrap().payload(),
            Statement::Block { .. }
        ));
        assert_eq!(
            parsed.ast().type_refs().len(),
            0,
            "no synthetic Unit TypeRef"
        );
        assert_eq!(
            parsed
                .ast()
                .items()
                .get(parsed.root())
                .unwrap()
                .span()
                .end(),
            text.len()
        );
    }

    for text in [
        "fun f(): Unit",
        "fun f(): Unit {}",
        "fun f(): Result<Int> { x }",
    ] {
        let (sources, parsed) = parsed(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let FunctionForm::Explicit {
            colon_span,
            type_ref,
            ..
        } = function(&parsed)
        else {
            panic!("{text:?}: explicit")
        };
        assert_eq!(sources.slice(*colon_span).unwrap(), ":");
        assert!(matches!(
            parsed.ast().type_refs().get(*type_ref).unwrap().payload(),
            TypeRef::Qualified { .. }
        ));
    }
}

#[test]
fn expression_body_missing_return_annotation_has_one_exact_recovery_root() {
    let text = "fun f() = 1";
    let (sources, parsed) = parsed(text);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(fingerprint)
            .collect::<Vec<_>>(),
        [(
            "L0021".to_owned(),
            "expected explicit return type".to_owned(),
            8,
            8
        )]
    );
    let FunctionForm::Explicit {
        colon_span,
        type_ref,
        body: FunctionBody::Expression {
            equals_span,
            expression,
        },
    } = function(&parsed)
    else {
        panic!("recovered explicit expression body")
    };
    assert_eq!((colon_span.start(), colon_span.end()), (8, 8));
    let type_node = parsed.ast().type_refs().get(*type_ref).unwrap();
    assert_eq!((type_node.span().start(), type_node.span().end()), (8, 8));
    assert!(matches!(type_node.payload(), TypeRef::Error));
    assert_eq!(sources.slice(*equals_span).unwrap(), "=");
    assert!(matches!(
        parsed
            .ast()
            .expressions()
            .get(*expression)
            .unwrap()
            .payload(),
        Expression::Literal(_)
    ));
    assert_eq!(
        parsed
            .ast()
            .items()
            .get(parsed.root())
            .unwrap()
            .span()
            .end(),
        text.len()
    );
}

#[test]
fn real_colon_commits_explicit_form_and_never_falls_back_to_implicit_unit() {
    for (text, expected_span, body_kind) in [
        ("fun f(): = 1", (9, 9), "expression"),
        ("fun f(): {}", (9, 9), "block"),
        ("fun f():", (8, 8), "absent"),
    ] {
        let (_, parsed) = parsed(text);
        assert_eq!(
            codes(&parsed),
            ["L0014"],
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let FunctionForm::Explicit { type_ref, body, .. } = function(&parsed) else {
            panic!("{text:?}: explicit")
        };
        let node = parsed.ast().type_refs().get(*type_ref).unwrap();
        assert!(matches!(node.payload(), TypeRef::Error));
        assert_eq!((node.span().start(), node.span().end()), expected_span);
        assert!(matches!(
            (body_kind, body),
            ("expression", FunctionBody::Expression { .. })
                | ("block", FunctionBody::Block(_))
                | ("absent", FunctionBody::Absent)
        ));
    }
}

#[test]
fn missing_colon_keeps_real_type_refs_while_unrelated_trailing_tokens_end_implicitly() {
    for text in [
        "fun f() Result<Int>",
        "fun f() T? {}",
        "fun f() () -> R = x",
    ] {
        let (_, parsed) = parsed(text);
        assert_eq!(
            codes(&parsed),
            ["L0021"],
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let FunctionForm::Explicit { type_ref, .. } = function(&parsed) else {
            panic!("{text:?}: explicit recovery")
        };
        assert!(!matches!(
            parsed.ast().type_refs().get(*type_ref).unwrap().payload(),
            TypeRef::Error
        ));
    }

    let (_, trailing) = parsed("fun f() +");
    assert!(matches!(
        function(&trailing),
        FunctionForm::ImplicitUnitAbsent
    ));
    assert_eq!(codes(&trailing), ["L0013"]);

    let (_, string) = parsed(r#"fun f() "complete""#);
    assert!(matches!(
        function(&string),
        FunctionForm::ImplicitUnitAbsent
    ));
    assert_eq!(codes(&string), ["L0013"]);
}

#[test]
fn trailing_poison_and_terminal_lexer_roots_do_not_gain_return_or_trailing_cascades() {
    for (text, expected) in [
        ("fun f() #", "L0001"),
        ("fun f() async", "L0002"),
        ("fun f() \"abc", "L0004"),
        (r#"fun f() "${answer"#, "L0005"),
        ("fun f() \"abc\\", "L0006"),
    ] {
        let (_, parsed) = parsed(text);
        assert!(matches!(
            function(&parsed),
            FunctionForm::ImplicitUnitAbsent
        ));
        assert_eq!(
            codes(&parsed),
            [expected],
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }
}

#[test]
fn expression_body_recovery_preserves_nested_owner_grammars_after_the_single_l0021() {
    for text in [
        "fun f() = (g(input))",
        "fun f() = g(input)",
        "fun f() = values[index]",
        r#"fun f() = "${({ x -> x })(input)}""#,
    ] {
        let (_, parsed) = parsed(text);
        assert_eq!(
            codes(&parsed),
            ["L0021"],
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let FunctionForm::Explicit {
            type_ref,
            body: FunctionBody::Expression { expression, .. },
            ..
        } = function(&parsed)
        else {
            panic!("{text:?}: recovered expression body")
        };
        assert!(matches!(
            parsed.ast().type_refs().get(*type_ref).unwrap().payload(),
            TypeRef::Error
        ));
        assert!(!matches!(
            parsed
                .ast()
                .expressions()
                .get(*expression)
                .unwrap()
                .payload(),
            Expression::Error
        ));
        assert_eq!(
            parsed
                .ast()
                .items()
                .get(parsed.root())
                .unwrap()
                .span()
                .end(),
            text.len()
        );
    }
}

#[test]
fn implicit_unit_is_deterministic_source_owned_and_utf8_byte_accurate() {
    fn shape(noise_first: bool) -> (usize, usize, Vec<String>) {
        let text = "fun f() { \"名\" }";
        let mut sources = SourceMap::new();
        if noise_first {
            add_source(&mut sources, "noise.ko", "val x = 1");
        }
        let id = add_source(&mut sources, "case.ko", text);
        let parsed = parse_declaration_twice(&sources, id, text);
        assert_eq!(parsed.source_id(), id);
        assert!(matches!(
            function(&parsed),
            FunctionForm::ImplicitUnitBlock(_)
        ));
        assert_eq!(
            parsed
                .ast()
                .items()
                .get(parsed.root())
                .unwrap()
                .span()
                .end(),
            text.len()
        );
        (
            parsed.ast().statements().len(),
            parsed.ast().type_refs().len(),
            codes(&parsed),
        )
    }
    assert_eq!(shape(false), shape(true));

    let mut owner = SourceMap::new();
    let id = add_source(&mut owner, "owner.ko", "fun f()");
    let (lexed, _) = lex_and_parse_declaration_twice(&owner, id, "owner source");
    let mut foreign = SourceMap::new();
    add_source(&mut foreign, "foreign.ko", "fun f()");
    assert_parser_error_twice(
        &foreign,
        &lexed,
        ParserInternalError::Source(SourceError::InvalidSourceId { source_id: id }),
        "foreign implicit Unit source",
        parse_declaration,
    );
}
