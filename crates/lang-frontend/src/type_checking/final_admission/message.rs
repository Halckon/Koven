//! Private diagnostic presentation; no analysis facts or qualification decisions.
use crate::type_checking::DeferredReason;

pub(super) fn deferred_message(reason: DeferredReason) -> String {
    format!("final type analysis is incomplete: {reason:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_messages_preserve_all_deferred_reasons() {
        let cases = [
            (
                DeferredReason::AnyValueRepresentation,
                "final type analysis is incomplete: AnyValueRepresentation",
            ),
            (
                DeferredReason::NominalOrTypeParameter,
                "final type analysis is incomplete: NominalOrTypeParameter",
            ),
            (
                DeferredReason::QualifiedType,
                "final type analysis is incomplete: QualifiedType",
            ),
            (
                DeferredReason::FunctionContainsDeferred,
                "final type analysis is incomplete: FunctionContainsDeferred",
            ),
            (
                DeferredReason::UnboundExternalType,
                "final type analysis is incomplete: UnboundExternalType",
            ),
            (
                DeferredReason::ForwardValueType,
                "final type analysis is incomplete: ForwardValueType",
            ),
            (
                DeferredReason::OverloadSelection,
                "final type analysis is incomplete: OverloadSelection",
            ),
            (
                DeferredReason::ThisType,
                "final type analysis is incomplete: ThisType",
            ),
            (
                DeferredReason::MemberAccess,
                "final type analysis is incomplete: MemberAccess",
            ),
            (
                DeferredReason::Call,
                "final type analysis is incomplete: Call",
            ),
            (
                DeferredReason::Index,
                "final type analysis is incomplete: Index",
            ),
            (
                DeferredReason::Assignment,
                "final type analysis is incomplete: Assignment",
            ),
            (
                DeferredReason::CastOrTypeTest,
                "final type analysis is incomplete: CastOrTypeTest",
            ),
            (
                DeferredReason::ErrorPropagation,
                "final type analysis is incomplete: ErrorPropagation",
            ),
            (
                DeferredReason::WhenTyping,
                "final type analysis is incomplete: WhenTyping",
            ),
            (
                DeferredReason::ControlJoin,
                "final type analysis is incomplete: ControlJoin",
            ),
            (
                DeferredReason::Destructuring,
                "final type analysis is incomplete: Destructuring",
            ),
            (
                DeferredReason::LoopSource,
                "final type analysis is incomplete: LoopSource",
            ),
        ];
        assert_eq!(cases.len(), 18);
        for (reason, expected) in cases {
            assert_eq!(deferred_message(reason), expected);
        }
    }
}
