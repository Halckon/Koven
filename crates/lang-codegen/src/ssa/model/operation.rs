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
    MapConstruct {
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
            Self::PrintString { value } => vec![EntityId::Loan(*value)],
            Self::StringClone { source } => vec![EntityId::Loan(*source)],
            Self::Binary { left, right, .. }
            | Self::CheckedArithmetic { left, right, .. }
            | Self::IntegerBitwise { left, right, .. }
            | Self::Compare { left, right, .. } => {
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
            Self::MapConstruct { .. } => Vec::new(),
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
            Self::BooleanNot { operand } | Self::IntegerNot { operand } => {
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
        }
    }
}
