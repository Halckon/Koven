//! Slot-based refined CFG: every move, drop and loan is an explicit statement.
use lang_frontend::source::Span;

use crate::ssa::model::{
    CheckedArithmeticOperator, ComparisonOperator, EntityType, FunctionId, IntegerBitwiseOperator,
    LoanKind, ScalarConstant, SsaTypeId,
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
    Binary {
        operator: crate::ssa::model::BinaryOperator,
        left: Slot,
        right: Slot,
    },
    IntegerNot {
        operand: Slot,
    },
    IntegerBitwise {
        operator: IntegerBitwiseOperator,
        left: Slot,
        right: Slot,
    },
    CheckedArithmetic {
        operator: CheckedArithmeticOperator,
        left: Slot,
        right: Slot,
    },
    CheckedIntegerConvert {
        operand: Slot,
    },
    IntegerRangeConstruct {
        first: Slot,
        bound: Slot,
        inclusive: bool,
    },
    IntegerRangeContains {
        value: Slot,
        range: Slot,
    },
    IntegerRangeEqual {
        left: Slot,
        right: Slot,
    },
    StringClone {
        source: Slot,
    },
    StringCopyBytes {
        source: Slot,
    },
    StringCopySlice {
        source: Slot,
        start: Slot,
        end: Slot,
    },
    Utf8ErrorOffset {
        source: Slot,
    },
    StringByteLength {
        source: Slot,
    },
    StringByteAt {
        source: Slot,
        index: Slot,
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
    /// Projects an active shared loan of an inline aggregate to one field's shared loan.
    SharedFieldLoan {
        base: Slot,
        field: usize,
    },
    /// Projects an active shared loan of a heap class owner to one field's shared loan.
    SharedHeapFieldLoan {
        base: Slot,
        field: usize,
    },
    /// Dereferences an environment field's shared loan of a shared capture to the target's loan.
    SharedReferenceFollow {
        source: Slot,
    },
    /// Copies a Copyable field out of an inline aggregate value.
    AggregateProject {
        aggregate: Slot,
        field: usize,
    },
    /// Copies all fields out of a Copyable inline aggregate value.
    AggregateCopyExplode {
        aggregate: Slot,
    },
    /// Consumes a MoveOnly inline aggregate value into its component values.
    AggregateExplode {
        aggregate: Slot,
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
    /// ADR-0034: the address of a lambda thunk without captures (a function pointer).
    FunctionAddress {
        target: crate::ssa::model::FunctionId,
    },
    /// ADR-0034: a capturing lambda's closure value; captures are loans (shared) or values.
    ClosureConstruct {
        closure: crate::ssa::model::SsaTypeId,
        thunk: crate::ssa::model::FunctionId,
        captures: Vec<Slot>,
    },
    /// ADR-0034: re-borrows a callable loan as a callable view.
    CallableViewOf {
        source: Slot,
    },
    /// ADR-0034: calls through a callable view loan.
    CallableInvokeView {
        view: Slot,
        arguments: Vec<Slot>,
    },
    /// ADR-0034: calls a local callable binding through its shared loan.
    CallableInvoke {
        callable: Slot,
        arguments: Vec<Slot>,
    },
    /// Takes a MoveOnly owner back from its root place after its loans ended.
    RootPlaceTake {
        owner: Slot,
        place: Slot,
    },
    /// Copies a Copyable value into an independent SSA value identity.
    Copy {
        source: Slot,
    },
    /// Commits a new value into an exclusively borrowed root local, returning `[new_owner, old]`.
    RootReplace {
        owner: Slot,
        loan: Slot,
        replacement: Slot,
    },
    /// Swaps two exclusively borrowed root locals, returning `[new_left, new_right]`.
    RootSwap {
        left_owner: Slot,
        right_owner: Slot,
        left_loan: Slot,
        right_loan: Slot,
    },
    /// Replaces one payload field through an exclusive owner loan, dropping the old value.
    HeapFieldReplace {
        receiver: Slot,
        field: usize,
        value: Slot,
    },
    /// Consumes `elements` into a sequential container (`Array<T>`, `List<T>`, `MutableList<T>`).
    ContainerConstruct {
        container: SsaTypeId,
        elements: Vec<Slot>,
    },
    /// Reads the element count of a sequential container through a loan or owned value.
    ContainerLength {
        owner: Slot,
    },
    /// Projects the element place at `index` of a sequential container owner or loan.
    ContainerElementPlace {
        owner: Slot,
        index: Slot,
    },
    /// Consumes `owner` and `element`, returning the updated `MutableList<T>` owner.
    ContainerAppend {
        owner: Slot,
        element: Slot,
    },
    /// Consumes `owner` and clears all elements, returning the updated `MutableList<T>` owner.
    ContainerClear {
        owner: Slot,
    },
    /// Consumes `owner` and `index`, returning `[removed_element, updated_owner]`.
    ContainerRemoveAt {
        owner: Slot,
        index: Slot,
    },
    /// Consumes `owner`, returning `[first_element, updated_owner]`.
    ContainerRemoveFirst {
        owner: Slot,
    },
    /// Consumes `owner`, returning `[last_element, updated_owner]`.
    ContainerRemoveLast {
        owner: Slot,
    },
    /// Consumes `owner`, `index`, and `element`, returning the updated `MutableList<T>` owner.
    ContainerInsertAt {
        owner: Slot,
        index: Slot,
        element: Slot,
    },
    /// Replaces the element at `index` of a mutable container `owner` with `value`.
    ContainerReplace {
        owner: Slot,
        index: Slot,
        value: Slot,
    },
    /// Generates a runtime-length `Array<T>` or `List<T>` of `length` from `initializer`.
    ContainerGenerateBorrowed {
        container: SsaTypeId,
        length: Slot,
        initializer: Slot,
    },
    /// Constructs an empty `Map<K, V>` or `MutableMap<K, V>`.
    MapConstruct {
        map_type: SsaTypeId,
    },
    /// Reads the entry count of a `Map<K, V>` or `MutableMap<K, V>`.
    MapSize {
        owner: Slot,
    },
    /// Tests whether `owner` contains `key`.
    MapContains {
        owner: Slot,
        key: Slot,
    },
    /// Consumes `owner`, `key`, and `value`, returning the updated `MutableMap<K, V>`.
    MapPut {
        owner: Slot,
        key: Slot,
        value: Slot,
    },
}

impl ROp {
    pub(super) fn operands(&self) -> Vec<Slot> {
        match self {
            Self::Constant(_)
            | Self::StringLiteral { .. }
            | Self::NullableNull { .. }
            | Self::FunctionAddress { .. }
            | Self::MapConstruct { .. } => Vec::new(),
            Self::CallableViewOf { source } | Self::SharedReferenceFollow { source } => {
                vec![*source]
            }
            Self::ClosureConstruct { captures, .. } => captures.clone(),
            Self::CallableInvokeView { view, arguments } => std::iter::once(*view)
                .chain(arguments.iter().copied())
                .collect(),
            Self::CallableInvoke {
                callable,
                arguments,
            } => std::iter::once(*callable)
                .chain(arguments.iter().copied())
                .collect(),
            Self::Alias { source: slot }
            | Self::Copy { source: slot }
            | Self::PrintString { value: slot }
            | Self::ReadLoan { loan: slot }
            | Self::RootPlace { owner: slot }
            | Self::BorrowBegin { place: slot, .. }
            | Self::BorrowEnd { loan: slot }
            | Self::Drop { owner: slot }
            | Self::BooleanNot { operand: slot }
            | Self::IntegerNot { operand: slot }
            | Self::CheckedIntegerConvert { operand: slot }
            | Self::StringClone { source: slot }
            | Self::StringCopyBytes { source: slot }
            | Self::Utf8ErrorOffset { source: slot }
            | Self::StringByteLength { source: slot }
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
            | Self::SharedPayloadPlace { owner: slot }
            | Self::AggregateProject {
                aggregate: slot, ..
            }
            | Self::AggregateCopyExplode { aggregate: slot }
            | Self::AggregateExplode { aggregate: slot }
            | Self::SharedFieldLoan { base: slot, .. }
            | Self::SharedHeapFieldLoan { base: slot, .. }
            | Self::ContainerLength { owner: slot }
            | Self::ContainerClear { owner: slot }
            | Self::ContainerRemoveFirst { owner: slot }
            | Self::ContainerRemoveLast { owner: slot }
            | Self::MapSize { owner: slot } => vec![*slot],
            Self::Compare { left, right, .. }
            | Self::Binary { left, right, .. }
            | Self::IntegerBitwise { left, right, .. }
            | Self::CheckedArithmetic { left, right, .. }
            | Self::IntegerRangeConstruct {
                first: left,
                bound: right,
                ..
            }
            | Self::IntegerRangeContains {
                value: left,
                range: right,
            }
            | Self::IntegerRangeEqual { left, right }
            | Self::StringByteAt {
                source: left,
                index: right,
            }
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
            }
            | Self::ContainerElementPlace {
                owner: left,
                index: right,
            }
            | Self::ContainerAppend {
                owner: left,
                element: right,
            }
            | Self::ContainerRemoveAt {
                owner: left,
                index: right,
            }
            | Self::ContainerGenerateBorrowed {
                length: left,
                initializer: right,
                ..
            }
            | Self::MapContains {
                owner: left,
                key: right,
            } => {
                vec![*left, *right]
            }
            Self::StringCopySlice { source, start, end }
            | Self::RootReplace {
                owner: source,
                loan: start,
                replacement: end,
            }
            | Self::ContainerInsertAt {
                owner: source,
                index: start,
                element: end,
            }
            | Self::ContainerReplace {
                owner: source,
                index: start,
                value: end,
            }
            | Self::MapPut {
                owner: source,
                key: start,
                value: end,
            } => vec![*source, *start, *end],
            Self::RootSwap {
                left_owner,
                right_owner,
                left_loan,
                right_loan,
            } => vec![*left_owner, *right_owner, *left_loan, *right_loan],
            Self::NullableTake { owner, proof } => vec![*owner, *proof],
            Self::DirectCall {
                receiver,
                arguments,
                ..
            } => receiver.iter().chain(arguments).copied().collect(),
            Self::AggregateConstruct {
                fields: arguments, ..
            }
            | Self::ContainerConstruct {
                elements: arguments,
                ..
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
            Self::Abort(RAbort::Error { message }) => vec![*message],
            Self::Abort(_) => Vec::new(),
            Self::Branch { condition, .. } => vec![*condition],
            Self::NullBranch { owner, .. } => vec![*owner],
            Self::Return(Some(value)) => vec![*value],
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
