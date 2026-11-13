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
    NullableLoanIsNull {
        source: Slot,
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
    /// Replaces one inline aggregate field through an exclusive owner loan, dropping the old value.
    InlineFieldReplace {
        receiver: Slot,
        field: usize,
        value: Slot,
    },
    /// The heap payload place of a class owner.
    HeapPayloadPlace {
        owner: Slot,
    },
    /// Exchanges one heap class field through an exclusive field loan, returning the old field value.
    HeapFieldExchange {
        owner: Slot,
        field: usize,
        loan: Slot,
        replacement: Slot,
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
    /// Consumes a `MutableMap<K, V>` owner into a readonly `Map<K, V>` owner.
    MapConsume {
        owner: Slot,
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
    /// Looks up `key` in `owner`, returning a Copyable `$map.result.tX` value.
    MapGet {
        owner: Slot,
        key: Slot,
    },
    /// Extracts the Copyable payload from a `$map.result.tX`, aborting when absent.
    MapResultUnwrap {
        result: Slot,
    },
    /// Consumes `owner`, `key`, and `value`, returning the updated `MutableMap<K, V>`.
    MapPut {
        owner: Slot,
        key: Slot,
        value: Slot,
    },
    /// Consumes `owner` and reads `key`, returning `[updated_owner, removed_result]`.
    MapRemove {
        owner: Slot,
        key: Slot,
    },
    /// Invokes `action` with a shared loan of the value mapped to `key`, returning a Boolean.
    MapWithValue {
        source: Slot,
        key: Slot,
        action: Slot,
    },
    /// Borrows the value slot mapped to `key` in `source`, aborting when absent.
    MapRequireValue {
        source: Slot,
        key: Slot,
    },
    /// Acquires the initial opaque cursor (`u64`) for iterating `source`.
    MapIterationFirst {
        source: Slot,
    },
    /// Tests whether `cursor` points to a valid entry in `source`.
    MapIterationHasNext {
        source: Slot,
        cursor: Slot,
    },
    /// Advances `cursor` to the next entry in `source`.
    MapIterationNext {
        source: Slot,
        cursor: Slot,
    },
    /// Borrows the key at `cursor` in `source`.
    MapIterationKeyLoan {
        source: Slot,
        cursor: Slot,
    },
    /// Borrows the value at `cursor` in `source`.
    MapIterationValueLoan {
        source: Slot,
        cursor: Slot,
    },
    /// Constructs a `RangeView` `[begin, end)` from `source`, returning `[view, protecting_loan]`.
    RangeConstruct {
        view: SsaTypeId,
        source: Slot,
        begin: Slot,
        end: Slot,
    },
    /// Calls a carrier-returning `RangeView` function, returning `[view, protecting_loan]`.
    RangeCall {
        callee: FunctionId,
        arguments: Vec<Slot>,
        source: Slot,
    },
    /// Reads the element count of a `RangeView` value or shared loan.
    RangeLength {
        view: Slot,
    },
    /// Projects the element place at `index` of a shared `RangeView` loan.
    RangeElementPlace {
        view: Slot,
        index: Slot,
    },
    /// Ends a `RangeView` descriptor `view` and its `source` protecting loan.
    RangeEnd {
        view: Slot,
        source: Slot,
    },
    /// Calls a borrow-returning function, returning its shared result loan.
    BorrowCall {
        callee: FunctionId,
        arguments: Vec<Slot>,
        source: Slot,
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
            | Self::NullableLoanIsNull { source: slot }
            | Self::HeapAllocate { payload: slot, .. }
            | Self::HeapPayloadPlace { owner: slot }
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
            | Self::MapConsume { owner: slot, .. }
            | Self::MapSize { owner: slot }
            | Self::MapResultUnwrap { result: slot }
            | Self::MapIterationFirst { source: slot }
            | Self::RangeLength { view: slot } => vec![*slot],
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
            | Self::InlineFieldReplace {
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
            }
            | Self::MapGet {
                owner: left,
                key: right,
            }
            | Self::MapRemove {
                owner: left,
                key: right,
            }
            | Self::MapRequireValue {
                source: left,
                key: right,
            }
            | Self::MapIterationHasNext {
                source: left,
                cursor: right,
            }
            | Self::MapIterationNext {
                source: left,
                cursor: right,
            }
            | Self::MapIterationKeyLoan {
                source: left,
                cursor: right,
            }
            | Self::MapIterationValueLoan {
                source: left,
                cursor: right,
            }
            | Self::RangeElementPlace {
                view: left,
                index: right,
            }
            | Self::RangeEnd {
                view: left,
                source: right,
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
            }
            | Self::MapWithValue {
                source,
                key: start,
                action: end,
            }
            | Self::RangeConstruct {
                source,
                begin: start,
                end,
                ..
            }
            | Self::HeapFieldExchange {
                owner: source,
                loan: start,
                replacement: end,
                ..
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
            Self::RangeCall {
                arguments, source, ..
            }
            | Self::BorrowCall {
                arguments, source, ..
            } => arguments
                .iter()
                .copied()
                .chain(std::iter::once(*source))
                .collect(),
            Self::AggregateConstruct {
                fields: arguments, ..
            }
            | Self::ContainerConstruct {
                elements: arguments,
                ..
            } => arguments.clone(),
        }
    }

    /// If this operation is a direct call, function value invocation, or borrow/range call,
    /// returns the optional receiver and argument slots whose value arguments are consumed.
    pub(super) fn call_operands(&self) -> Option<(Option<Slot>, &[Slot])> {
        match self {
            Self::DirectCall {
                receiver,
                arguments,
                ..
            } => Some((*receiver, arguments.as_slice())),
            Self::CallableInvoke { arguments, .. }
            | Self::CallableInvokeView { arguments, .. }
            | Self::BorrowCall { arguments, .. }
            | Self::RangeCall { arguments, .. } => Some((None, arguments.as_slice())),
            _ => None,
        }
    }

    /// Every slot consumed by value or whose loan ended at this operation.
    pub(super) fn retired_slots(&self, slots: &[EntityType]) -> Vec<Slot> {
        let is_value = |s: &Slot| matches!(slots.get(s.0), Some(EntityType::Value(_)));

        if let Some((receiver, arguments)) = self.call_operands() {
            let mut consumed = Vec::new();
            if let Some(r) = receiver
                && is_value(&r)
            {
                consumed.push(r);
            }
            consumed.extend(arguments.iter().copied().filter(is_value));
            return consumed;
        }

        match self {
            Self::Drop { owner }
            | Self::BorrowEnd { loan: owner }
            | Self::AggregateExplode { aggregate: owner }
            | Self::HeapAllocate { payload: owner, .. }
            | Self::TaggedConstruct { payload: owner, .. }
            | Self::SharedAllocate { payload: owner, .. }
            | Self::ContainerClear { owner }
            | Self::ContainerRemoveFirst { owner }
            | Self::ContainerRemoveLast { owner }
            | Self::MapConsume { owner, .. }
            | Self::MapResultUnwrap { result: owner }
            | Self::NullableWrap { owner, .. } => vec![*owner],

            Self::NullableTake { owner, proof } => vec![*owner, *proof],

            Self::RangeEnd { view, source } => vec![*view, *source],

            Self::AggregateConstruct { fields, .. } => fields.clone(),
            Self::ContainerConstruct { elements, .. } => elements.clone(),

            Self::ContainerAppend { owner, element }
            | Self::ContainerRemoveAt {
                owner,
                index: element,
            }
            | Self::MapRemove {
                owner,
                key: element,
            } => vec![*owner, *element],

            Self::ContainerInsertAt {
                owner,
                index,
                element,
            }
            | Self::ContainerReplace {
                owner,
                index,
                value: element,
            }
            | Self::MapPut {
                owner,
                key: index,
                value: element,
            } => vec![*owner, *index, *element],

            Self::HeapFieldReplace { value, .. }
            | Self::InlineFieldReplace { value, .. }
            | Self::LoanMutate { value, .. } => vec![*value],

            Self::HeapFieldExchange {
                owner,
                loan,
                replacement,
                ..
            } => vec![*owner, *loan, *replacement],

            Self::RootReplace {
                owner,
                loan,
                replacement,
            } => vec![*owner, *loan, *replacement],

            Self::RootSwap {
                left_owner,
                right_owner,
                left_loan,
                right_loan,
            } => vec![*left_owner, *right_owner, *left_loan, *right_loan],

            Self::ClosureConstruct { captures, .. } => {
                captures.iter().copied().filter(is_value).collect()
            }

            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum LoanTag {
    Consumer(super::loan_receipt::Tag),
    Receiver(super::receiver_receipt::Tag),
    Named(super::named_receipt::Tag),
    Temporary(super::temporary_receipt::Tag),
    Binding(super::binding_receipt::Tag),
    AuxBinding(super::aux_binding_receipt::Tag),
    DistinctAux(super::distinct_aux_receipt::Tag),
    LaterAux(super::later_aux_receipt::Tag),
    AuxiliarySequence(super::auxiliary_sequence_receipt::Tag),
    InterleavedAux(super::interleaved_aux_receipt::Tag),
    MultiBinding(super::multi_binding_receipt::Tag),
    Child(super::child_receipt::Tag),
    Forward(super::forward_receipt::Tag),
    Branch(super::branch_receipt::Tag),
    Nested(super::nested_receipt::Tag),
    Bounded(super::bounded_receipt::Tag),
    Single(super::single_receipt::Tag),
    Multi(super::multi_receipt::Tag),
}

#[derive(Clone, Debug)]
pub(super) struct RStmt {
    pub(super) loop_tag: Option<super::loop_receipt::Tag>,
    pub(super) fresh_loop_tag: Option<(usize, super::fresh_loop_receipt::Tag)>,
    pub(super) loan_tag: Option<LoanTag>,
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
    pub(super) fn operands(&self) -> Vec<Slot> {
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
            Self::Return(_)
            | Self::BorrowReturn { .. }
            | Self::RangeReturn { .. }
            | Self::Abort(_)
            | Self::Unreachable => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct RBlock {
    pub(super) has_predecessor: bool,
    pub(super) statements: Vec<RStmt>,
    pub(super) terminator: Option<(RTerm, Span)>,
    /// Set for a `NullBranch` non-null target: the proof view defined on entry.
    pub(super) view: Option<Slot>,
    /// Source of the construct this block joins; the function span when unset.
    pub(super) origin: Option<Span>,
    /// Carrier slots pinned as live-in on entry to this block.
    pub(super) pinned: Vec<Slot>,
    /// Keep a loop header block separate from its single entry predecessor.
    pub(super) preserve_header: bool,
}

/// Block 0 is the function entry; `parameters` are defined there.
#[derive(Clone, Debug, Default)]
pub(super) struct RefinedBody {
    pub(super) child_cycle_plans: super::child_cycle_receipt::Plans,
    pub(super) nested_body_plans: super::nested_body_receipt::Plans,
    pub(super) nested_resume_plans: super::nested_resume_receipt::Plans,
    pub(super) nested_exit_plans: super::nested_exit_receipt::Plans,
    pub(super) owner_bridge_plans: super::control_bridge_receipt::Plans,
    pub(super) loop_plans: super::loop_receipt::Plans,
    pub(super) fresh_loop_plans: super::fresh_loop_receipt::Plans,
    pub(super) arm_loop_plans: super::arm_loop_receipt::Plans,
    pub(super) arm_loop_required: Vec<lang_frontend::ownership_checking::UnitLoopBranchNoCarryFact>,
    pub(super) arm_counter_required:
        Vec<lang_frontend::ownership_checking::UnitLoopCounterBranchNoCarryFact>,
    pub(super) loan_plans: super::loan_receipt::Plans,
    pub(super) receiver_plans: super::receiver_receipt::Plans,
    pub(super) named_plans: super::named_receipt::Plans,
    pub(super) temporary_plans: super::temporary_receipt::Plans,
    pub(super) binding_plans: super::binding_receipt::Plans,
    pub(super) aux_binding_plans: super::aux_binding_receipt::Plans,
    pub(super) distinct_aux_plans: super::distinct_aux_receipt::Plans,
    pub(super) later_aux_plans: super::later_aux_receipt::Plans,
    pub(super) auxiliary_sequence_plans: super::auxiliary_sequence_receipt::Plans,
    pub(super) interleaved_aux_plans: super::interleaved_aux_receipt::Plans,
    pub(super) multi_binding_plans: super::multi_binding_receipt::Plans,
    pub(super) forward_plans: super::forward_receipt::Plans,
    pub(super) branch_plans: super::branch_receipt::Plans,
    pub(super) nested_plans: super::nested_receipt::Plans,
    pub(super) bounded_plans: super::bounded_receipt::Plans,
    pub(super) single_plans: super::single_receipt::Plans,
    pub(super) multi_plans: super::multi_receipt::Plans,
    pub(super) child_plans: super::child_receipt::Plans,
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
