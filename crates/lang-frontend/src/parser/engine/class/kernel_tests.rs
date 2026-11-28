use super::super::boundary::{ClassMemberContext, ReceiverModifierPolicy};
use super::super::parse_declaration;
use super::kernel::{
    ClassifierKindTag, classifier_forbids_type_parameters, classifier_requires_body,
    classifier_requires_constructor, classifier_supports_constructor,
    is_class_field_invalid_prefix_kind, is_classifier_name_boundary_kind, is_enum_body_stop_kind,
    is_enum_variant_param_invalid_prefix_kind, member_companion_is_unsupported,
    member_receiver_policy, member_visibility_invalid_for_fun,
};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind, lex_test_source_twice};
use crate::source::SourceMap;

#[test]
fn classifier_kind_and_member_context_policies_match_spec() {
    for tag in [
        ClassifierKindTag::ValueClass,
        ClassifierKindTag::Class,
        ClassifierKindTag::Interface,
        ClassifierKindTag::EnumClass,
        ClassifierKindTag::Object,
    ] {
        if classifier_requires_constructor(tag) {
            assert!(classifier_supports_constructor(tag));
        }
    }
    assert!(classifier_supports_constructor(
        ClassifierKindTag::ValueClass
    ));
    assert!(classifier_supports_constructor(ClassifierKindTag::Class));
    assert!(!classifier_supports_constructor(
        ClassifierKindTag::Interface
    ));
    assert!(!classifier_supports_constructor(
        ClassifierKindTag::EnumClass
    ));
    assert!(!classifier_supports_constructor(ClassifierKindTag::Object));
    assert!(classifier_requires_body(ClassifierKindTag::EnumClass));
    assert!(classifier_forbids_type_parameters(
        ClassifierKindTag::Object
    ));

    for ctx in [
        ClassMemberContext::ValueClass,
        ClassMemberContext::Class,
        ClassMemberContext::Interface,
        ClassMemberContext::Enum,
        ClassMemberContext::Object,
        ClassMemberContext::Companion,
    ] {
        assert_eq!(ctx.allows_companion(), !ctx.allows_constant());
        assert_eq!(
            member_receiver_policy(ctx),
            if matches!(ctx, ClassMemberContext::Companion) {
                ReceiverModifierPolicy::Rejected
            } else {
                ReceiverModifierPolicy::Allowed
            }
        );
        assert_eq!(
            member_companion_is_unsupported(ctx, false),
            !ctx.allows_companion()
        );
        assert!(member_companion_is_unsupported(ctx, true));
    }

    assert!(member_visibility_invalid_for_fun(
        ClassMemberContext::Interface,
        true,
        true
    ));
    assert!(!member_visibility_invalid_for_fun(
        ClassMemberContext::Interface,
        false,
        true
    ));
    assert!(!member_visibility_invalid_for_fun(
        ClassMemberContext::Class,
        true,
        true
    ));
}

#[test]
fn classifier_name_boundary_and_enum_body_stop_match_spec() {
    for sym in [
        Symbol::Less,
        Symbol::LeftParen,
        Symbol::Colon,
        Symbol::LeftBrace,
        Symbol::RightBrace,
    ] {
        assert!(is_classifier_name_boundary_kind(LexemeKind::Token(
            TokenKind::Symbol(sym)
        )));
    }
    assert!(is_classifier_name_boundary_kind(LexemeKind::Eof));
    assert!(!is_classifier_name_boundary_kind(LexemeKind::Token(
        TokenKind::Identifier
    )));

    assert!(is_enum_body_stop_kind(LexemeKind::Token(
        TokenKind::Symbol(Symbol::RightBrace)
    )));
    assert!(is_enum_body_stop_kind(LexemeKind::Token(
        TokenKind::Symbol(Symbol::Semicolon)
    )));
    assert!(is_enum_body_stop_kind(LexemeKind::Eof));
    assert!(!is_enum_body_stop_kind(LexemeKind::Token(
        TokenKind::Symbol(Symbol::Comma)
    )));
}

#[test]
fn field_and_enum_param_invalid_prefixes_match_spec() {
    let ident = LexemeKind::Token(TokenKind::Identifier);
    let val = LexemeKind::Token(TokenKind::Keyword(Keyword::Val));
    let var = LexemeKind::Token(TokenKind::Keyword(Keyword::Var));
    let vararg = LexemeKind::Token(TokenKind::Keyword(Keyword::Vararg));

    assert!(is_class_field_invalid_prefix_kind(ident, true, Some(val)));
    assert!(is_class_field_invalid_prefix_kind(ident, true, Some(var)));
    assert!(!is_class_field_invalid_prefix_kind(
        ident,
        true,
        Some(ident)
    ));
    assert!(!is_class_field_invalid_prefix_kind(ident, false, Some(val)));
    assert!(is_class_field_invalid_prefix_kind(vararg, false, None));

    assert!(is_enum_variant_param_invalid_prefix_kind(
        ident,
        true,
        Some(ident)
    ));
    assert!(!is_enum_variant_param_invalid_prefix_kind(
        ident,
        false,
        Some(ident)
    ));
    assert!(is_enum_variant_param_invalid_prefix_kind(val, false, None));
    assert!(is_enum_variant_param_invalid_prefix_kind(var, false, None));
    assert!(is_enum_variant_param_invalid_prefix_kind(
        vararg, false, None
    ));
}

#[test]
fn class_production_entry_enforces_classifier_and_member_policies() {
    for (text, expect_diagnostic) in [
        ("value class V(val x: Int)", false),
        ("value class V {}", true),
        ("object O<T> {}", true),
        ("enum class E", true),
        ("interface I { private fun f(): Int }", true),
        ("class C { companion {} companion {} }", true),
    ] {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("class-policy.ko", text)
            .expect("unique source");
        let lexed = lex_test_source_twice(&sources, id, "class policy");
        let parsed = parse_declaration(&sources, &lexed).expect("classifier must parse");
        assert_eq!(
            !parsed.diagnostics().is_empty(),
            expect_diagnostic,
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }
}
