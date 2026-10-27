//! Slot-based refined CFG: every move, drop and loan is an explicit statement.
use lang_frontend::source::Span;

use crate::ssa::model::{
    CheckedArithmeticOperator, ComparisonOperator, EntityType, FunctionId, LoanKind,
    ScalarConstant, SsaTypeId,
};

/// A function-local variable; SSA construction gives it one entity per definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Slot(pub(super) usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct BlockRef(pub(super) usize);

/// Operations with slot operands; each maps to exactly one SSA `Operation`.
#[derive(Clone, Debug)]
pub(super) enum ROp {
    /// SSA renaming: the result slot now names the source entity; no instruction.
    Alias {
        source: Slot,
    },
    Constant(ScalarConstant),
    StringLiteral {
        string: SsaTypeId,
        bytes: Vec<u8>,
    },
    PrintString {
        value: Slot,
    },
    /// Operands are owned values or shared loans; neither is consumed.
    StringConcat {
        left: Slot,
        right: Slot,
    },
    StringEqual {
        left: Slot,
        right: Slot,
    },
    /// Copy a Copyable value out of a shared loan.
    ReadLoan {
        loan: Slot,
    },
    RootPlace {
        owner: Slot,
    },
    BorrowBegin {
        place: Slot,
        kind: LoanKind,
    },
    BorrowEnd {
        loan: Slot,
    },
    Drop {
        owner: Slot,
    },
    Compare {
        operator: ComparisonOperator,
        left: Slot,
        right: Slot,
    },
    BooleanNot {
        operand: Slot,
    },
    CheckedArithmetic {
        operator: CheckedArithmeticOperator,
        left: Slot,
        right: Slot,
    },
    DirectCall {
        callee: FunctionId,
        /// The hidden zeroth operand of a member call.
        receiver: Option<Slot>,
        arguments: Vec<Slot>,
    },
    NullableWrap {
        nullable: SsaTypeId,
        owner: Slot,
    },
    NullableNull {
        nullable: SsaTypeId,
    },
    NullableIsNull {
        owner: Slot,
    },
    NullableTake {
        owner: Slot,
        proof: Slot,
    },
    /// Consumes `fields` (payload field order) into a class payload aggregate.
    AggregateConstruct {
        aggregate: SsaTypeId,
        fields: Vec<Slot>,
    },
    HeapAllocate {
        owner: SsaTypeId,
        payload: Slot,
    },
    /// Copies a Copyable field out through a shared loan of the owner.
    HeapFieldRead {
        receiver: Slot,
        field: usize,
    },
    /// Consumes `payload` into the `variant` of a tagged union.
    TaggedConstruct {
        tagged: SsaTypeId,
        variant: usize,
        payload: Slot,
    },
    /// The `Int` discriminant of a tagged value.
    TaggedDiscriminant {
        owner: Slot,
    },
    /// The payload place of `variant`; the flow facts proved the owner holds it.
    TaggedPayloadPlace {
        owner: Slot,
        variant: usize,
    },
    FieldPlace {
        base: Slot,
        field: usize,
    },
    /// Copies a Copyable value out of a place.
    ReadPlace {
        place: Slot,
    },
    /// Replaces the Copyable target of an exclusive loan.
    LoanMutate {
        loan: Slot,
        value: Slot,
    },
    /// Moves `payload` into a fresh `Rc` shared owner.
    SharedAllocate {
        owner: SsaTypeId,
        payload: Slot,
    },
    /// Retains a new strong owner of a shared owner (a value or a loan of one).
    SharedRetain {
        owner: Slot,
    },
    /// The payload place of a shared owner, valid while the owner lives.
    SharedPayloadPlace {
        owner: Slot,
    },
    /// Narrows an active exclusive loan to a call-scoped shared loan.
    SharedReborrow {
        source: Slot,
    },
    /// Takes a MoveOnly owner back from its root place after its loans ended.
    RootPlaceTake {
        owner: Slot,
        place: Slot,
    },
    /// Replaces one payload field through an exclusive owner loan, dropping the old value.
    HeapFieldReplace {
        receiver: Slot,
        field: usize,
        value: Slot,
    },
}

impl ROp {
    pub(super) fn operands(&self) -> Vec<Slot> {
        match self {
            Self::Constant(_) | Self::StringLiteral { .. } | Self::NullableNull { .. } => {
                Vec::new()
            }
            Self::Alias { source: slot }
            | Self::PrintString { value: slot }
            | Self::ReadLoan { loan: slot }
            | Self::RootPlace { owner: slot }
            | Self::BorrowBegin { place: slot, .. }
            | Self::BorrowEnd { loan: slot }
            | Self::Drop { owner: slot }
            | Self::BooleanNot { operand: slot }
            | Self::NullableWrap { owner: slot, .. }
            | Self::NullableIsNull { owner: slot }
            | Self::HeapAllocate { payload: slot, .. }
            | Self::HeapFieldRead { receiver: slot, .. }
            | Self::TaggedConstruct { payload: slot, .. }
            | Self::TaggedDiscriminant { owner: slot }
            | Self::TaggedPayloadPlace { owner: slot, .. }
            | Self::FieldPlace { base: slot, .. }
            | Self::ReadPlace { place: slot }
            | Self::SharedReborrow { source: slot }
            | Self::SharedAllocate { payload: slot, .. }
            | Self::SharedRetain { owner: slot }
            | Self::SharedPayloadPlace { owner: slot } => vec![*slot],
            Self::Compare { left, right, .. }
            | Self::CheckedArithmetic { left, right, .. }
            | Self::StringConcat { left, right }
            | Self::StringEqual { left, right }
            | Self::LoanMutate {
                loan: left,
                value: right,
            }
            | Self::HeapFieldReplace {
                receiver: left,
                value: right,
                ..
            }
            | Self::RootPlaceTake {
                owner: left,
                place: right,
            } => {
                vec![*left, *right]
            }
            Self::NullableTake { owner, proof } => vec![*owner, *proof],
            Self::DirectCall {
                receiver,
                arguments,
                ..
            } => receiver.iter().chain(arguments).copied().collect(),
            Self::AggregateConstruct {
                fields: arguments, ..
            } => arguments.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct RStmt {
    pub(super) op: ROp,
    pub(super) results: Vec<Slot>,
    pub(super) span: Span,
}

#[derive(Clone, Debug)]
pub(super) enum RAbort {
    CheckedArithmetic {
        operator: CheckedArithmeticOperator,
        divisor: Slot,
    },
    /// `!!` on a null value.
    NullAssertion,
    /// The fall-through of a value `when` over a closed domain the frontend proved
    /// covered; unreachable at run time.
    UnmatchedWhen,
}

#[derive(Clone, Debug)]
pub(super) enum RTerm {
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
    Abort(RAbort),
}

impl RTerm {
    pub(super) fn operands(&self) -> Vec<Slot> {
        match self {
            Self::Goto(_) | Self::Return(None) => Vec::new(),
            Self::Branch { condition, .. } => vec![*condition],
            Self::NullBranch { owner, .. } => vec![*owner],
            Self::Return(Some(value)) => vec![*value],
            Self::Abort(RAbort::CheckedArithmetic { divisor, .. }) => vec![*divisor],
            Self::Abort(RAbort::NullAssertion | RAbort::UnmatchedWhen) => Vec::new(),
        }
    }

    pub(super) fn successors(&self) -> Vec<BlockRef> {
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
            Self::Return(_) | Self::Abort(_) => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct RBlock {
    pub(super) statements: Vec<RStmt>,
    pub(super) terminator: Option<(RTerm, Span)>,
    /// Set for a `NullBranch` non-null target: the proof view defined on entry.
    pub(super) view: Option<Slot>,
    /// Source of the construct this block joins; the function span when unset.
    pub(super) origin: Option<Span>,
}

/// Block 0 is the function entry; `parameters` are defined there.
#[derive(Clone, Debug, Default)]
pub(super) struct RefinedBody {
    pub(super) slots: Vec<EntityType>,
    pub(super) blocks: Vec<RBlock>,
    pub(super) parameters: Vec<Slot>,
}

impl RefinedBody {
    pub(super) fn slot(&mut self, ty: EntityType) -> Slot {
        self.slots.push(ty);
        Slot(self.slots.len() - 1)
    }

    pub(super) fn block(&mut self) -> BlockRef {
        self.blocks.push(RBlock::default());
        BlockRef(self.blocks.len() - 1)
    }
}
