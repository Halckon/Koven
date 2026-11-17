//! Terminators and abort reasons for the refined CFG.
use super::{BlockRef, Slot};
use crate::ssa::model::{CheckedArithmeticOperator, SsaTypeId};

#[derive(Clone, Debug)]
pub(crate) enum RAbort {
    CheckedArithmetic {
        operator: CheckedArithmeticOperator,
        divisor: Slot,
    },
    CheckedConversion {
        target: SsaTypeId,
    },
    /// `!!` on a null value.
    NullAssertion,
    /// Negative runtime container length.
    NegativeSize,
    /// The fall-through of a value `when` over a closed domain the frontend proved
    /// covered; unreachable at run time.
    UnmatchedWhen,
    /// `error(message)` call.
    Error {
        message: Slot,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum RTerm {
    Goto(BlockRef),
    Branch {
        condition: Slot,
        when_true: BlockRef,
        when_false: BlockRef,
    },
    /// `non_null` is a dedicated target whose last parameter is the `view` slot.
    NullBranch {
        owner: Slot,
        when_null: BlockRef,
        when_non_null: BlockRef,
    },
    Return(Option<Slot>),
    BorrowReturn {
        loan: Slot,
    },
    RangeReturn {
        view: Slot,
        source: Slot,
    },
    Abort(RAbort),
    Unreachable,
}

impl RTerm {
    pub(crate) fn operands(&self) -> Vec<Slot> {
        match self {
            Self::Goto(_) | Self::Return(None) | Self::Unreachable => Vec::new(),
            Self::Abort(RAbort::Error { message }) => vec![*message],
            Self::Abort(RAbort::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Divide | CheckedArithmeticOperator::Remainder,
                divisor,
            }) => vec![*divisor],
            Self::Abort(_) => Vec::new(),
            Self::Branch { condition, .. } => vec![*condition],
            Self::NullBranch { owner, .. } => vec![*owner],
            Self::Return(Some(value)) => vec![*value],
            Self::BorrowReturn { loan } => vec![*loan],
            Self::RangeReturn { view, source } => vec![*view, *source],
        }
    }

    pub(crate) fn successors(&self) -> Vec<BlockRef> {
        match self {
            Self::Goto(target) => vec![*target],
            Self::Branch {
                when_true,
                when_false,
                ..
            } => vec![*when_true, *when_false],
            Self::NullBranch {
                when_null,
                when_non_null,
                ..
            } => vec![*when_null, *when_non_null],
            Self::Return(_)
            | Self::BorrowReturn { .. }
            | Self::RangeReturn { .. }
            | Self::Abort(_)
            | Self::Unreachable => Vec::new(),
        }
    }
}
