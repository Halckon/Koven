//! SSA operation 与显式 operand 枚举合同。
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Constant(ScalarConstant),
    /// 编译器封闭的 UTF-8 stdout 行输出；bytes 必须以 `\n` 结尾。
    PrintLiteral {
        bytes: Vec<u8>,
    },
    /// 以已解码 UTF-8 bytes 创建普通 String owner。
    StringLiteral {
        string: SsaTypeId,
        bytes: Vec<u8>,
    },
    /// 读取两个 String view 并创建新的唯一 owner，不消费具名 operand。
    StringConcat {
        left: EntityId,
        right: EntityId,
    },
    /// 深拷贝 active shared String loan，产生独立的唯一 owner。
    StringClone {
        source: LoanId,
    },
    /// Read the byte count from an active shared String loan as a Koven Int.
    StringByteLength {
        source: LoanId,
    },
    /// Read one byte after checking the signed Int index; invalid indices Abort.
    StringByteAt {
        source: LoanId,
        index: ValueId,
    },
    /// Strictly scan an active shared Array<UByte> loan; -1 means valid UTF-8.
    Utf8ErrorOffset {
        source: LoanId,
    },
    /// Validate and copy a shared Array<UByte> into an independent String owner.
    StringCopyBytes {
        source: LoanId,
    },
    /// Copy a checked half-open scalar-aligned String byte range into a fresh owner.
    StringCopySlice {
        source: LoanId,
        start: ValueId,
        end: ValueId,
    },
    /// 按 UTF-8 bytes 比较两个 String view，不进行 Unicode normalization。
    StringEqual {
        left: EntityId,
        right: EntityId,
    },
    /// 读取 active shared loan，输出全部 String bytes 后追加换行。
    PrintString {
        value: LoanId,
    },
    /// SPEC-0033 的低层 primitive；算术变体只允许在范围证明后使用。
    /// 源语言 `+`/`-`/`*` 必须先 lower 为 `CheckedArithmetic`。
    Binary {
        operator: BinaryOperator,
        left: ValueId,
        right: ValueId,
    },
    /// 返回算术结果与失败标志；lowering 必须把失败标志导向 `Abort`。
    CheckedArithmetic {
        operator: CheckedArithmeticOperator,
        left: ValueId,
        right: ValueId,
    },
    /// Returns target-typed value and Boolean failure. Failed values have no semantic value.
    /// Bounds use the original mathematical operand; lowering routes failure to Abort.
    CheckedIntegerConvert {
        operand: ValueId,
    },
    /// Pure registered numeric Range value; inclusive or half-open upper endpoint.
    IntegerRangeConstruct {
        first: ValueId,
        bound: ValueId,
        inclusive: bool,
    },
    IntegerRangeContains {
        value: ValueId,
        range: ValueId,
    },
    IntegerRangeEqual {
        left: ValueId,
        right: ValueId,
    },
    /// Exact-width integer operations; shift counts are masked by width - 1.
    IntegerBitwise {
        operator: IntegerBitwiseOperator,
        left: ValueId,
        right: ValueId,
    },
    /// Bitwise inversion within the integer operand/result width.
    IntegerNot {
        operand: ValueId,
    },
    Compare {
        operator: ComparisonOperator,
        left: ValueId,
        right: ValueId,
    },
    BooleanNot {
        operand: ValueId,
    },
    DirectCall {
        callee: FunctionId,
        /// instance call 的隐藏第零操作数；普通函数调用为 `None`。
        receiver: Option<EntityId>,
        arguments: Vec<EntityId>,
    },
    FunctionAddress {
        target: FunctionId,
    },
    /// 普通只读借用调用，结果依赖唯一的 shared 来源。
    BorrowCall {
        callee: FunctionId,
        arguments: Vec<EntityId>,
        source: LoanId,
    },
    RangeConstruct {
        view: SsaTypeId,
        source: LoanId,
        begin: ValueId,
        end: ValueId,
    },
    RangeCall {
        callee: FunctionId,
        arguments: Vec<EntityId>,
        source: LoanId,
    },
    RangeLength {
        view: EntityId,
    },
    /// A read-only element place depends on the actual descriptor metadata loan.
    RangeElementPlace {
        view: LoanId,
        index: ValueId,
    },
    RangeEnd {
        view: ValueId,
        source: LoanId,
    },
    ClosureConstruct {
        closure: SsaTypeId,
        thunk: FunctionId,
        captures: Vec<ClosureCaptureOperand>,
    },
    CallableInvoke {
        callable: ValueId,
        arguments: Vec<EntityId>,
    },
    /// Re-borrows a shared loan of a function pointer or concrete closure as a callable view
    /// loan (ADR-0034); the view depends on `source` like a shared reborrow.
    CallableViewOf {
        source: LoanId,
    },
    /// Calls through a callable view loan; reads, never consumes, the borrowed callable.
    CallableInvokeView {
        view: LoanId,
        arguments: Vec<EntityId>,
    },
    AggregateConstruct {
        aggregate: SsaTypeId,
        fields: Vec<ValueId>,
    },
    AggregateProject {
        aggregate: ValueId,
        field: usize,
    },
    AggregateExplode {
        aggregate: ValueId,
    },
    AggregateCopyExplode {
        aggregate: ValueId,
    },
    TaggedConstruct {
        tagged: SsaTypeId,
        variant: usize,
        payload: ValueId,
    },
    TaggedPayloadPlace {
        owner: ValueId,
        variant: usize,
    },
    TaggedDiscriminant {
        owner: ValueId,
    },
    HeapAllocate {
        owner: SsaTypeId,
        payload: ValueId,
    },
    HeapPayloadPlace {
        owner: ValueId,
    },
    /// Read one Copyable payload field through an active heap-owner receiver loan.
    HeapFieldRead {
        receiver: LoanId,
        field: usize,
    },
    /// Replace one payload field through an active exclusive heap-owner receiver loan.
    /// MoveOnly fields implicitly drop the old value before committing the replacement.
    HeapFieldReplace {
        receiver: LoanId,
        field: usize,
        value: ValueId,
    },
    /// Exchange one direct payload field under its exact exclusive field loan.
    /// Consumes the loan and replacement, returns the old field value, and preserves owner.
    /// No drop or observable uninitialized state occurs during this ownership commit.
    HeapFieldExchange {
        owner: ValueId,
        field: usize,
        loan: LoanId,
        replacement: ValueId,
    },
    /// Replace one field of an inline aggregate through an active exclusive receiver loan.
    /// MoveOnly fields implicitly drop the old value before committing the replacement.
    InlineFieldReplace {
        receiver: LoanId,
        field: usize,
        value: ValueId,
    },
    SharedAllocate {
        owner: SsaTypeId,
        payload: ValueId,
    },
    SharedRetain {
        owner: EntityId,
    },
    SharedPayloadPlace {
        owner: EntityId,
    },
    NullableWrap {
        nullable: SsaTypeId,
        owner: ValueId,
    },
    NullableNull {
        nullable: SsaTypeId,
    },
    NullableIsNull {
        owner: ValueId,
    },
    NullableLoanIsNull {
        source: LoanId,
    },
    NullableTake {
        owner: ValueId,
        proof: LoanId,
    },
    ContainerConstruct {
        container: SsaTypeId,
        elements: Vec<ValueId>,
    },
    ContainerGenerate {
        container: SsaTypeId,
        length: ValueId,
        initializer: FunctionId,
    },
    /// Synchronously read a shared callable loan without transferring its owner.
    ContainerGenerateBorrowed {
        container: SsaTypeId,
        length: ValueId,
        initializer: LoanId,
    },
    ContainerLength {
        owner: EntityId,
    },
    ContainerElementPlace {
        owner: EntityId,
        index: ValueId,
    },
    ContainerAppend {
        owner: ValueId,
        element: ValueId,
    },
    ContainerClear {
        owner: ValueId,
    },
    ContainerRemoveAt {
        owner: ValueId,
        index: ValueId,
    },
    ContainerRemoveFirst {
        owner: ValueId,
    },
    ContainerRemoveLast {
        owner: ValueId,
    },
    ContainerInsertAt {
        owner: ValueId,
        index: ValueId,
        element: ValueId,
    },
    ContainerReplace {
        owner: ValueId,
        index: ValueId,
        value: ValueId,
    },
    /// Snapshot the first insertion-chain slot as a target-independent unsigned 64-bit cursor.
    MapIterationFirst {
        source: LoanId,
    },
    MapIterationHasNext {
        source: LoanId,
        cursor: ValueId,
    },
    /// Advance only after both component loans and their descendants have ended.
    MapIterationNext {
        source: LoanId,
        cursor: ValueId,
    },
    MapIterationKeyLoan {
        source: LoanId,
        cursor: ValueId,
    },
    MapIterationValueLoan {
        source: LoanId,
        cursor: ValueId,
    },
    MapConstruct {
        map_type: SsaTypeId,
    },
    /// Transfer a MutableMap owner into the same-K/V readonly Map owner.
    MapConsume {
        owner: ValueId,
        map_type: SsaTypeId,
    },
    MapSize {
        owner: EntityId,
    },
    MapContains {
        owner: EntityId,
        key: EntityId,
    },
    /// 返回真实 V 槽位的 shared loan；缺失 Abort，不复制或移动 V。
    MapRequireValue {
        source: LoanId,
        key: EntityId,
    },
    /// 同步 scoped callback；真实 receiver/key/action loan 仅活过该调用。
    MapWithValue {
        source: LoanId,
        key: EntityId,
        action: LoanId,
    },
    MapGet {
        owner: EntityId,
        key: EntityId,
    },
    /// Copyable 查询结果的受检提取；缺失时 Abort，不发布第二个 owner。
    MapResultUnwrap {
        result: ValueId,
    },
    MapPut {
        owner: ValueId,
        key: ValueId,
        value: ValueId,
    },
    MapRemove {
        owner: ValueId,
        key: EntityId,
    },
    FieldPlace {
        base: PlaceId,
        field: usize,
    },
    /// Project an active shared aggregate loan to one shared field loan.
    SharedFieldLoan {
        base: LoanId,
        field: usize,
    },
    /// Project an active shared heap-owner receiver loan to one shared payload-field loan.
    SharedHeapFieldLoan {
        base: LoanId,
        field: usize,
    },
    /// Narrow an active shared/exclusive loan to a call-scoped shared loan.
    SharedReborrow {
        source: LoanId,
    },
    /// Follow a shared reference slot to a shared target view within its parent loan's extent.
    SharedReferenceFollow {
        source: LoanId,
    },
    Copy {
        source: ValueId,
    },
    Consume {
        owner: ValueId,
    },
    RootPlace {
        owner: ValueId,
    },
    /// Take one MoveOnly owner back from its direct root place after all loans end.
    RootPlaceTake {
        owner: ValueId,
        place: PlaceId,
    },
    /// Atomically replace an owned root under its active exclusive loan.
    /// Results are the new root identity followed by the detached old value; the loan ends.
    RootReplace {
        owner: ValueId,
        loan: LoanId,
        replacement: ValueId,
    },
    /// Atomically exchange two disjoint owned roots and end both exclusive loans.
    /// Results are the new identities in the same order as `owners`.
    RootSwap {
        owners: [ValueId; 2],
        loans: [LoanId; 2],
    },
    BorrowBegin {
        place: PlaceId,
        kind: LoanKind,
    },
    BorrowEnd {
        loan: LoanId,
    },
    Read {
        source: PlaceAccess,
    },
    Mutate {
        place: PlaceId,
        value: ValueId,
    },
    /// Replace the whole Copyable target of an active exclusive loan (an `Inout` binding);
    /// ADR-0016 lets the loan holder modify the target through `PlaceAccess::Loan`.
    LoanMutate {
        loan: LoanId,
        value: ValueId,
    },
    Drop {
        owner: ValueId,
    },
}

impl Operation {
    pub(crate) fn entities(&self) -> Vec<EntityId> {
        match self {
            Self::Constant(_)
            | Self::PrintLiteral { .. }
            | Self::StringLiteral { .. }
            | Self::NullableNull { .. } => Vec::new(),
            Self::StringConcat { left, right } | Self::StringEqual { left, right } => {
                vec![*left, *right]
            }
            Self::StringCopySlice { source, start, end } => vec![
                EntityId::Loan(*source),
                EntityId::Value(*start),
                EntityId::Value(*end),
            ],
            Self::PrintString { value } => vec![EntityId::Loan(*value)],
            Self::StringClone { source }
            | Self::StringByteLength { source }
            | Self::Utf8ErrorOffset { source }
            | Self::StringCopyBytes { source } => {
                vec![EntityId::Loan(*source)]
            }
            Self::StringByteAt { source, index } => {
                vec![EntityId::Loan(*source), EntityId::Value(*index)]
            }
            Self::Binary { left, right, .. }
            | Self::CheckedArithmetic { left, right, .. }
            | Self::IntegerBitwise { left, right, .. }
            | Self::Compare { left, right, .. } => {
                vec![EntityId::Value(*left), EntityId::Value(*right)]
            }
            Self::IntegerRangeConstruct { first, bound, .. } => {
                vec![EntityId::Value(*first), EntityId::Value(*bound)]
            }
            Self::IntegerRangeContains { value, range } => {
                vec![EntityId::Value(*value), EntityId::Value(*range)]
            }
            Self::IntegerRangeEqual { left, right } => {
                vec![EntityId::Value(*left), EntityId::Value(*right)]
            }
            Self::DirectCall {
                receiver,
                arguments,
                ..
            } => receiver
                .iter()
                .copied()
                .chain(arguments.iter().copied())
                .collect(),
            Self::FunctionAddress { .. } => Vec::new(),
            Self::BorrowCall {
                arguments, source, ..
            }
            | Self::RangeCall {
                arguments, source, ..
            } => {
                let mut entities = arguments.clone();
                entities.push(EntityId::Loan(*source));
                entities
            }
            Self::RangeConstruct {
                source, begin, end, ..
            } => vec![
                EntityId::Loan(*source),
                EntityId::Value(*begin),
                EntityId::Value(*end),
            ],
            Self::RangeLength { view } => vec![*view],
            Self::RangeElementPlace { view, index } => {
                vec![EntityId::Loan(*view), EntityId::Value(*index)]
            }
            Self::RangeEnd { view, source } => {
                vec![EntityId::Value(*view), EntityId::Loan(*source)]
            }
            Self::ClosureConstruct { captures, .. } => captures
                .iter()
                .map(|capture| match capture {
                    ClosureCaptureOperand::Shared(loan) => EntityId::Loan(*loan),
                    ClosureCaptureOperand::Owned(value) => EntityId::Value(*value),
                })
                .collect(),
            Self::CallableInvoke {
                callable,
                arguments,
            } => {
                let mut entities = vec![EntityId::Value(*callable)];
                entities.extend(arguments.iter().copied());
                entities
            }
            Self::CallableViewOf { source } => vec![EntityId::Loan(*source)],
            Self::CallableInvokeView { view, arguments } => {
                let mut entities = vec![EntityId::Loan(*view)];
                entities.extend(arguments.iter().copied());
                entities
            }
            Self::AggregateConstruct { fields, .. } => {
                fields.iter().copied().map(EntityId::Value).collect()
            }
            Self::AggregateProject { aggregate, .. }
            | Self::AggregateExplode { aggregate }
            | Self::AggregateCopyExplode { aggregate }
            | Self::HeapPayloadPlace { owner: aggregate } => {
                vec![EntityId::Value(*aggregate)]
            }
            Self::HeapFieldRead { receiver, .. } => vec![EntityId::Loan(*receiver)],
            Self::HeapFieldReplace {
                receiver, value, ..
            }
            | Self::InlineFieldReplace {
                receiver, value, ..
            } => vec![EntityId::Loan(*receiver), EntityId::Value(*value)],
            Self::HeapFieldExchange {
                owner,
                loan,
                replacement,
                ..
            } => vec![
                EntityId::Value(*owner),
                EntityId::Loan(*loan),
                EntityId::Value(*replacement),
            ],
            Self::SharedRetain { owner } | Self::SharedPayloadPlace { owner } => vec![*owner],
            Self::NullableWrap { owner, .. } | Self::NullableIsNull { owner } => {
                vec![EntityId::Value(*owner)]
            }
            Self::NullableLoanIsNull { source } => vec![EntityId::Loan(*source)],
            Self::NullableTake { owner, proof } => {
                vec![EntityId::Value(*owner), EntityId::Loan(*proof)]
            }
            Self::TaggedConstruct { payload, .. } => vec![EntityId::Value(*payload)],
            Self::TaggedPayloadPlace { owner, .. } => vec![EntityId::Value(*owner)],
            Self::TaggedDiscriminant { owner } => vec![EntityId::Value(*owner)],
            Self::HeapAllocate { payload, .. } => vec![EntityId::Value(*payload)],
            Self::SharedAllocate { payload, .. } => vec![EntityId::Value(*payload)],
            Self::ContainerConstruct { elements, .. } => {
                elements.iter().copied().map(EntityId::Value).collect()
            }
            Self::ContainerGenerate { length, .. } => vec![EntityId::Value(*length)],
            Self::ContainerGenerateBorrowed {
                length,
                initializer,
                ..
            } => {
                vec![EntityId::Value(*length), EntityId::Loan(*initializer)]
            }
            Self::ContainerLength { owner } => vec![*owner],
            Self::ContainerElementPlace { owner, index } => vec![*owner, EntityId::Value(*index)],
            Self::ContainerAppend { owner, element } => {
                vec![EntityId::Value(*owner), EntityId::Value(*element)]
            }
            Self::ContainerClear { owner } => vec![EntityId::Value(*owner)],
            Self::ContainerRemoveAt { owner, index } => {
                vec![EntityId::Value(*owner), EntityId::Value(*index)]
            }
            Self::ContainerRemoveFirst { owner } => vec![EntityId::Value(*owner)],
            Self::ContainerRemoveLast { owner } => vec![EntityId::Value(*owner)],
            Self::ContainerInsertAt {
                owner,
                index,
                element,
            } => vec![
                EntityId::Value(*owner),
                EntityId::Value(*index),
                EntityId::Value(*element),
            ],
            Self::ContainerReplace {
                owner,
                index,
                value,
            } => vec![
                EntityId::Value(*owner),
                EntityId::Value(*index),
                EntityId::Value(*value),
            ],
            Self::MapIterationFirst { source } => vec![EntityId::Loan(*source)],
            Self::MapIterationHasNext { source, cursor }
            | Self::MapIterationNext { source, cursor }
            | Self::MapIterationKeyLoan { source, cursor }
            | Self::MapIterationValueLoan { source, cursor } => {
                vec![EntityId::Loan(*source), EntityId::Value(*cursor)]
            }
            Self::MapConstruct { .. } => Vec::new(),
            Self::MapConsume { owner, .. } => vec![EntityId::Value(*owner)],
            Self::MapSize { owner } => vec![*owner],
            Self::MapContains { owner, key } => vec![*owner, *key],
            Self::MapWithValue {
                source,
                key,
                action,
            } => vec![EntityId::Loan(*source), *key, EntityId::Loan(*action)],
            Self::MapGet { owner, key } => vec![*owner, *key],
            Self::MapRequireValue { source, key } => vec![EntityId::Loan(*source), *key],
            Self::MapResultUnwrap { result } => vec![EntityId::Value(*result)],
            Self::MapPut { owner, key, value } => vec![
                EntityId::Value(*owner),
                EntityId::Value(*key),
                EntityId::Value(*value),
            ],
            Self::MapRemove { owner, key } => {
                vec![EntityId::Value(*owner), *key]
            }
            Self::FieldPlace { base, .. } => vec![EntityId::Place(*base)],
            Self::SharedFieldLoan { base, .. } | Self::SharedHeapFieldLoan { base, .. } => {
                vec![EntityId::Loan(*base)]
            }
            Self::SharedReborrow { source } | Self::SharedReferenceFollow { source } => {
                vec![EntityId::Loan(*source)]
            }
            Self::BooleanNot { operand }
            | Self::IntegerNot { operand }
            | Self::CheckedIntegerConvert { operand } => {
                vec![EntityId::Value(*operand)]
            }
            Self::Copy { source } => vec![EntityId::Value(*source)],
            Self::Consume { owner } | Self::RootPlace { owner } | Self::Drop { owner } => {
                vec![EntityId::Value(*owner)]
            }
            Self::RootPlaceTake { owner, place } => {
                vec![EntityId::Value(*owner), EntityId::Place(*place)]
            }
            Self::RootReplace {
                owner,
                loan,
                replacement,
            } => vec![
                EntityId::Value(*owner),
                EntityId::Loan(*loan),
                EntityId::Value(*replacement),
            ],
            Self::RootSwap { owners, loans } => vec![
                EntityId::Value(owners[0]),
                EntityId::Loan(loans[0]),
                EntityId::Value(owners[1]),
                EntityId::Loan(loans[1]),
            ],
            Self::BorrowBegin { place, .. } => vec![EntityId::Place(*place)],
            Self::BorrowEnd { loan } => vec![EntityId::Loan(*loan)],
            Self::Read { source } => vec![match source {
                PlaceAccess::Place(place) => EntityId::Place(*place),
                PlaceAccess::Loan(loan) => EntityId::Loan(*loan),
            }],
            Self::Mutate { place, value } => {
                vec![EntityId::Place(*place), EntityId::Value(*value)]
            }
            Self::LoanMutate { loan, value } => {
                vec![EntityId::Loan(*loan), EntityId::Value(*value)]
            }
        }
    }
}

/// Scalar constants; synthetic zero exists solely in internal test SSA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScalarConstant {
    #[cfg(test)]
    SyntheticZero,
    Unit,
    Boolean(bool),
    /// Verifier rejects surrogate and out-of-range codepoints.
    Char(u32),
    Integer(i128),
}

#[cfg(test)]
mod map_with_tests {
    //! 正常源码产物的真实 slot、共享 callback 与清理合同；不改写 IR。
    use crate::ssa::{
        borrow_memory_test_support::single as memory_unit, borrow_storage_contract_tests::unit,
        model::*, verify::verify_program,
    };
    #[test]
    fn map_with_memory_and_rooted_verify_real_slot_callback_and_temporary_cleanup() {
        for (name, text, _) in crate::native_map_with_cases::cases() {
            for program in [memory_unit(&text).unwrap(), unit(&text).unwrap()] {
                verify_program(&program).unwrap();
                let module = &program.modules[0];
                let llvm = crate::llvm::render_verified_program(&program).unwrap();
                let mut queries = 0;
                for function in &module.functions {
                    for (index, i) in function.instructions.iter().enumerate() {
                        let Operation::MapWithValue {
                            source,
                            key,
                            action,
                        } = i.operation
                        else {
                            continue;
                        };
                        queries += 1;
                        let result = function.entity(i.results[0]).unwrap().ty;
                        assert!(super::verify_map_operation(
                            module,
                            function,
                            &i.operation,
                            &[result]
                        ));
                        assert!(!super::verify_map_operation(
                            module,
                            function,
                            &Operation::MapWithValue {
                                source: action,
                                key,
                                action
                            },
                            &[result]
                        ));
                        assert!(!super::verify_map_operation(
                            module,
                            function,
                            &Operation::MapWithValue {
                                source,
                                key,
                                action: source
                            },
                            &[result]
                        ));
                        assert!(!super::verify_map_operation(
                            module,
                            function,
                            &i.operation,
                            &[function.entity(EntityId::Loan(source)).unwrap().ty]
                        ));
                        let source_type = function
                            .entity(EntityId::Loan(source))
                            .unwrap()
                            .ty
                            .semantic_type();
                        let value_type = module.map_container(source_type).unwrap().2;
                        let action_type = function
                            .entity(EntityId::Loan(action))
                            .unwrap()
                            .ty
                            .semantic_type();
                        let signature = module.callable_signature(action_type).unwrap();
                        assert_eq!(
                            signature.parameters,
                            [EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: value_type
                            }]
                        );
                        assert!(signature.returns.is_empty());
                        let end = |target| {
                            function.instructions.iter().position(
                                |j| matches!(j.operation,Operation::BorrowEnd {loan} if loan==target),
                            )
                        };
                        let action_end = end(action).unwrap();
                        assert!(index < action_end);
                        let EntityId::Loan(key_loan) = key else {
                            unreachable!()
                        };
                        let key_end = end(key_loan).unwrap();
                        assert!(index < key_end);
                        if let Some(source_end) = end(source) {
                            assert!(index < source_end);
                        }
                        // A real temporary key/callback must be dropped after its loan has ended.
                        for (loan, ended) in [(key_loan, key_end), (action, action_end)] {
                            let place = function
                                .instructions
                                .iter()
                                .find_map(|j| match j.operation {
                                    Operation::BorrowBegin { place, .. }
                                        if j.results == [EntityId::Loan(loan)] =>
                                    {
                                        Some(place)
                                    }
                                    _ => None,
                                });
                            let owner = place.and_then(|place| {
                                function
                                    .instructions
                                    .iter()
                                    .find_map(|j| match j.operation {
                                        Operation::RootPlace { owner }
                                            if j.results == [EntityId::Place(place)] =>
                                        {
                                            Some(owner)
                                        }
                                        _ => None,
                                    })
                            });
                            if let Some(owner)=owner.and_then(|owner|function.instructions.iter().position(|j|matches!(j.operation,Operation::Drop {owner:current} if current==owner))) {assert!(ended<owner,"{name}");}
                        }
                    }
                }
                assert!(queries > 0);
                assert_eq!(
                    llvm.lines()
                        .filter(|l| l.contains("with.value.slot")
                            && l.contains(" = getelementptr")
                            && l.ends_with(", i32 0, i32 2"))
                        .count(),
                    queries
                );
                for body in llvm
                    .split("with.found")
                    .skip(1)
                    .filter(|s| s.starts_with(':'))
                {
                    let found = body.split("with.missing").next().unwrap();
                    assert_eq!(found.lines().filter(|l| l.contains("call void")).count(), 1);
                }
            }
        }
    }

    #[test]
    fn map_with_value_rejects_a_non_returning_callback_signature() {
        use crate::ssa::verify::{VerifyErrorKind, VerifyLocation};

        let (_, text, _) = crate::native_map_with_cases::cases()
            .into_iter()
            .next()
            .unwrap();
        let mut program = unit(&text).unwrap();
        let module = &mut program.modules[0];
        let (instruction, ty) = module
            .functions
            .iter()
            .find_map(|function| {
                function.instructions.iter().find_map(|instruction| {
                    let Operation::MapWithValue { action, .. } = instruction.operation else {
                        return None;
                    };
                    let ty = function.entity(EntityId::Loan(action))?.ty.semantic_type();
                    Some((instruction.id, ty))
                })
            })
            .unwrap();
        let (SsaTypeKind::FunctionPointer { signature }
        | SsaTypeKind::ConcreteClosure { signature, .. }) = &mut module.types[ty.index()]
        else {
            panic!("expected a callback");
        };
        signature.never_returns = true;
        assert!(
            verify_program(&program)
                .unwrap_err()
                .errors
                .iter()
                .any(|error| {
                    error.location == VerifyLocation::Instruction(instruction)
                        && matches!(error.kind, VerifyErrorKind::OperationContract { .. })
                })
        );
    }
}
