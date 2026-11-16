//! Pure SSA operation matching, not source admission or a producer-consumption certificate.
//! Callers must independently establish original facts and actual production identities.
use super::model::{EntityId, FunctionId, Instruction, LoanKind, Operation};

pub(super) enum Expected<'a> {
    Literal {
        bytes: &'a [u8],
        owner: EntityId,
    },
    Root {
        owner: EntityId,
        place: EntityId,
    },
    Begin {
        place: EntityId,
        loan: EntityId,
        caller: FunctionId,
    },
    Call {
        callee: FunctionId,
        arguments: &'a [EntityId],
        returns_value: bool,
    },
    End {
        loan: EntityId,
    },
    Drop {
        owner: EntityId,
    },
}

/// Outer `None` is a mismatch; inner value is the newly formed triple member.
pub(super) fn validate(expected: Expected<'_>, data: &Instruction) -> Option<Option<EntityId>> {
    match (expected, &data.operation) {
        (Expected::Literal { bytes, owner }, Operation::StringLiteral { bytes: actual, .. })
            if bytes == actual =>
        {
            let [result @ EntityId::Value(_)] = data.results.as_slice() else {
                return None;
            };
            (*result == owner).then_some(Some(*result))
        }
        (Expected::Root { owner, place }, Operation::RootPlace { owner: actual })
            if owner == EntityId::Value(*actual) =>
        {
            let [result @ EntityId::Place(_)] = data.results.as_slice() else {
                return None;
            };
            (*result == place).then_some(Some(*result))
        }
        (
            Expected::Begin {
                place,
                loan,
                caller,
            },
            Operation::BorrowBegin {
                place: actual,
                kind: LoanKind::Shared,
            },
        ) if place == EntityId::Place(*actual) => {
            let [result @ EntityId::Loan(id)] = data.results.as_slice() else {
                return None;
            };
            (*result == loan && id.function() == caller).then_some(Some(*result))
        }
        (
            Expected::Call {
                callee,
                arguments,
                returns_value,
            },
            Operation::DirectCall {
                callee: actual,
                receiver,
                arguments: actual_arguments,
            },
        ) if callee == *actual
            && receiver.is_none()
            && arguments == actual_arguments
            && data.results.len() == usize::from(returns_value)
            && data.results.iter().all(|e| matches!(e, EntityId::Value(_))) =>
        {
            Some(None)
        }
        (Expected::End { loan }, Operation::BorrowEnd { loan: actual })
            if loan == EntityId::Loan(*actual) && data.results.is_empty() =>
        {
            Some(None)
        }
        (Expected::Drop { owner }, Operation::Drop { owner: actual })
            if owner == EntityId::Value(*actual) && data.results.is_empty() =>
        {
            Some(None)
        }
        _ => None,
    }
}
