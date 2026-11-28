use super::super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::parser::engine) enum ClassifierKindTag {
    ValueClass,
    Class,
    Interface,
    EnumClass,
    Object,
}

#[inline]
pub(in crate::parser::engine) const fn classifier_kind_tag(
    kind: ClassifierKind,
) -> ClassifierKindTag {
    match kind {
        ClassifierKind::ValueClass { .. } => ClassifierKindTag::ValueClass,
        ClassifierKind::Class { .. } => ClassifierKindTag::Class,
        ClassifierKind::Interface { .. } => ClassifierKindTag::Interface,
        ClassifierKind::EnumClass { .. } => ClassifierKindTag::EnumClass,
        ClassifierKind::Object { .. } => ClassifierKindTag::Object,
    }
}

#[inline]
pub(in crate::parser::engine) const fn classifier_supports_constructor(
    tag: ClassifierKindTag,
) -> bool {
    matches!(
        tag,
        ClassifierKindTag::ValueClass | ClassifierKindTag::Class
    )
}

#[inline]
pub(in crate::parser::engine) const fn classifier_requires_constructor(
    tag: ClassifierKindTag,
) -> bool {
    matches!(tag, ClassifierKindTag::ValueClass)
}

#[inline]
pub(in crate::parser::engine) const fn classifier_requires_body(tag: ClassifierKindTag) -> bool {
    matches!(tag, ClassifierKindTag::EnumClass)
}

#[inline]
pub(in crate::parser::engine) const fn classifier_forbids_type_parameters(
    tag: ClassifierKindTag,
) -> bool {
    matches!(tag, ClassifierKindTag::Object)
}

#[inline]
pub(in crate::parser::engine) const fn is_classifier_name_boundary_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Eof
            | LexemeKind::Token(TokenKind::Symbol(
                Symbol::Less
                    | Symbol::LeftParen
                    | Symbol::Colon
                    | Symbol::LeftBrace
                    | Symbol::RightBrace
            ))
    )
}

#[inline]
pub(in crate::parser::engine) const fn is_class_field_invalid_prefix_kind(
    kind: LexemeKind,
    is_mode_ident: bool,
    next1: Option<LexemeKind>,
) -> bool {
    (is_mode_ident
        && matches!(
            next1,
            Some(LexemeKind::Token(TokenKind::Keyword(
                Keyword::Val | Keyword::Var
            )))
        ))
        || matches!(kind, LexemeKind::Token(TokenKind::Keyword(Keyword::Vararg)))
}

#[inline]
pub(in crate::parser::engine) const fn is_enum_variant_param_invalid_prefix_kind(
    kind: LexemeKind,
    is_mode_ident: bool,
    next1: Option<LexemeKind>,
) -> bool {
    (is_mode_ident && matches!(next1, Some(LexemeKind::Token(TokenKind::Identifier))))
        || matches!(
            kind,
            LexemeKind::Token(TokenKind::Keyword(
                Keyword::Val | Keyword::Var | Keyword::Vararg
            ))
        )
}

#[inline]
pub(in crate::parser::engine) const fn member_receiver_policy(
    context: ClassMemberContext,
) -> ReceiverModifierPolicy {
    if matches!(context, ClassMemberContext::Companion) {
        ReceiverModifierPolicy::Rejected
    } else {
        ReceiverModifierPolicy::Allowed
    }
}

#[inline]
pub(in crate::parser::engine) const fn member_visibility_invalid_for_fun(
    context: ClassMemberContext,
    is_fun: bool,
    is_internal_or_private: bool,
) -> bool {
    matches!(context, ClassMemberContext::Interface) && is_fun && is_internal_or_private
}

#[inline]
pub(in crate::parser::engine) const fn member_companion_is_unsupported(
    context: ClassMemberContext,
    companion_seen: bool,
) -> bool {
    !context.allows_companion() || companion_seen
}

#[inline]
pub(in crate::parser::engine) const fn is_enum_body_stop_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace | Symbol::Semicolon))
            | LexemeKind::Eof
    )
}
