use super::boundary::{
    DeclarationStops, NameContext, TypeStops, class_field_start_kind, class_member_start_kind,
    classifier_declaration_start_kind, control_expression_start_kind, declaration_modifier_start,
    file_construct_start_kind, simple_declaration_start_kind,
};
use super::class::ClassifierHeaderOwner;
use super::{DeclarationModifiers, ParameterModeMarker, VisibilityModifier, parse_declaration};
use crate::diagnostic::codes;
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind, lex_test_source_twice};
use crate::source::{SourceMap, Span};

#[test]
fn starter_hierarchy_and_disjointness_match_spec() {
    for kw in [
        Keyword::Class,
        Keyword::Interface,
        Keyword::Enum,
        Keyword::Object,
    ] {
        let kind = LexemeKind::Token(TokenKind::Keyword(kw));
        assert!(classifier_declaration_start_kind(kind));
        assert!(simple_declaration_start_kind(kind));
        assert!(class_member_start_kind(kind));
        assert!(file_construct_start_kind(kind));
        assert!(!control_expression_start_kind(kind));
    }

    for kw in [
        Keyword::Val,
        Keyword::Var,
        Keyword::Public,
        Keyword::Internal,
        Keyword::Private,
        Keyword::Override,
    ] {
        let kind = LexemeKind::Token(TokenKind::Keyword(kw));
        assert!(class_field_start_kind(kind));
        assert!(simple_declaration_start_kind(kind));
        assert!(class_member_start_kind(kind));
    }

    for kw in [
        Keyword::If,
        Keyword::When,
        Keyword::Return,
        Keyword::Break,
        Keyword::Continue,
        Keyword::Super,
    ] {
        let kind = LexemeKind::Token(TokenKind::Keyword(kw));
        assert!(control_expression_start_kind(kind));
        assert!(!file_construct_start_kind(kind));
        assert!(!class_member_start_kind(kind));
    }
}

#[test]
fn name_context_stop_and_recovery_stops_agree() {
    let samples = [
        LexemeKind::Token(TokenKind::Symbol(Symbol::Colon)),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)),
        LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)),
        LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)),
        LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)),
        LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)),
        LexemeKind::Token(TokenKind::Keyword(Keyword::Val)),
        LexemeKind::Token(TokenKind::Keyword(Keyword::Var)),
        LexemeKind::Token(TokenKind::Keyword(Keyword::While)),
        LexemeKind::Token(TokenKind::Identifier),
        LexemeKind::Eof,
    ];
    for ctx in [
        NameContext::Declaration,
        NameContext::LocalDeclaration,
        NameContext::TypeParameter,
        NameContext::ValueParameter,
    ] {
        let stops = ctx.recovery_stops();
        for kind in samples {
            let sym = match kind {
                LexemeKind::Token(TokenKind::Symbol(s)) => Some(s),
                _ => None,
            };
            assert_eq!(
                ctx.is_stop_kind(kind),
                stops.contains_hard_kind(kind, sym) || stops.contains_soft_kind(kind, sym)
            );
        }
    }
}

#[test]
fn modifier_start_min_angle_comma_and_supertype_recovery_match_spec() {
    assert_eq!(
        declaration_modifier_start(DeclarationModifiers {
            visibility: None,
            override_span: None,
            receiver_mode: None,
        }),
        None
    );
    assert_eq!(
        declaration_modifier_start(DeclarationModifiers {
            visibility: Some(VisibilityModifier::Public(
                Span::synthetic_range_for_verification(10, 16)
            )),
            override_span: Some(Span::synthetic_range_for_verification(2, 10)),
            receiver_mode: Some(ParameterModeMarker::Borrow(
                Span::synthetic_range_for_verification(18, 24)
            )),
        }),
        Some(2)
    );

    let deleg = DeclarationStops::EMPTY.with(DeclarationStops::DELEGATION_ERROR_TARGET);
    assert!(deleg.keeps_unproved_angle_comma(false));
    assert!(deleg.keeps_unproved_angle_comma(true));
    let leading_only = DeclarationStops::EMPTY.with(DeclarationStops::LEADING_UNPROVED_ANGLE_COMMA);
    assert!(!leading_only.keeps_unproved_angle_comma(false));
    assert!(leading_only.keeps_unproved_angle_comma(true));

    let base = TypeStops::empty().with(TypeStops::CLASS_BODY);
    let file_rec = base.supertype_element_recovery(ClassifierHeaderOwner::File);
    assert!(file_rec.has_class_file_boundary() && !file_rec.stops_at_class_member());
    assert!(file_rec.0 & DeclarationStops::LEFT_BRACE != 0);
    let member_rec = base.supertype_element_recovery(ClassifierHeaderOwner::Member);
    assert!(!member_rec.has_class_file_boundary() && member_rec.stops_at_class_member());
    let decl_rec = base.supertype_element_recovery(ClassifierHeaderOwner::Declaration);
    assert!(!decl_rec.has_class_file_boundary() && !decl_rec.stops_at_class_member());
}

#[test]
fn boundary_production_entry_recovers_missing_parameter_name_at_colon() {
    let text = "fun f(: Int) {}";
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("missing-param-name.ko", text)
        .expect("unique source");
    let lexed = lex_test_source_twice(&sources, id, "missing param name");
    let parsed = parse_declaration(&sources, &lexed).expect("function must parse");
    assert_eq!(
        parsed.diagnostics().len(),
        1,
        "expected only missing parameter name diagnostic, got {:?}",
        parsed.diagnostics()
    );
    assert_eq!(
        parsed.diagnostics()[0].code().to_string(),
        codes::EXPECTED_PARAMETER_NAME
    );
    assert_eq!(
        parsed.diagnostics()[0].primary_span().start(),
        text.find(':').expect("colon offset")
    );
}
