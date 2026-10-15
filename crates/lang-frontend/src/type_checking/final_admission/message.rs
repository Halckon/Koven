//! Private diagnostic presentation; no analysis facts or qualification decisions.
use crate::type_checking::DeferredReason;

pub(super) fn deferred_message(reason: DeferredReason) -> String {
    format!("final type analysis is incomplete: {reason:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_messages_cover_all_deferred_reasons() {
        let cases = [
            (
                DeferredReason::AnyValueRepresentation,
                "type checking could not complete: the value representation of Any is unresolved",
            ),
            (
                DeferredReason::NominalOrTypeParameter,
                "type checking could not complete: the nominal type or type parameter is unresolved",
            ),
            (
                DeferredReason::QualifiedType,
                "type checking could not complete: the qualified type is unresolved",
            ),
            (
                DeferredReason::FunctionContainsDeferred,
                "type checking could not complete: a component of the function type is unresolved",
            ),
            (
                DeferredReason::UnboundExternalType,
                "type checking could not complete: the external symbol's type is unresolved",
            ),
            (
                DeferredReason::ForwardValueType,
                "type checking could not complete: the value's type is not yet determined",
            ),
            (
                DeferredReason::OverloadSelection,
                "type checking could not complete: overload selection is unresolved",
            ),
            (
                DeferredReason::ThisType,
                "type checking could not complete: the type of this is unresolved",
            ),
            (
                DeferredReason::MemberAccess,
                "type checking could not complete: the member's type is unresolved",
            ),
            (
                DeferredReason::Call,
                "type checking could not complete: the call's type is unresolved",
            ),
            (
                DeferredReason::Index,
                "type checking could not complete: the indexed expression's type is unresolved",
            ),
            (
                DeferredReason::Assignment,
                "type checking could not complete: assignment type checking is unresolved",
            ),
            (
                DeferredReason::CastOrTypeTest,
                "type checking could not complete: the cast or type test is unresolved",
            ),
            (
                DeferredReason::ErrorPropagation,
                "type checking could not complete: error propagation type checking is unresolved",
            ),
            (
                DeferredReason::WhenTyping,
                "type checking could not complete: when type checking is unresolved",
            ),
            (
                DeferredReason::ControlJoin,
                "type checking could not complete: the control-flow result type is unresolved",
            ),
            (
                DeferredReason::Destructuring,
                "type checking could not complete: destructuring component types are unresolved",
            ),
            (
                DeferredReason::LoopSource,
                "type checking could not complete: the iteration source type is unresolved",
            ),
        ];
        assert_eq!(cases.len(), 18);
        let actual = cases.map(|(reason, _)| deferred_message(reason));
        let expected = cases.map(|(_, message)| message);
        assert_eq!(actual, expected);
    }
}
