//! Instance receiver 与 Borrow-only delegation 的 Phase 3 稳定产物。

use crate::{
    name_resolution::{DeclarationId, UnitSymbolId},
    source::Span,
    type_checking::{
        ExpressionCategory, UnitCallReceiverOrigin, UnitCallTarget, UnitCallableTarget,
        UnitExpressionId, UnitTypeId,
    },
};

use super::{UnitCallArgumentOwnershipKind, UnitOwnershipPlace};

/// member call receiver 在 ownership 阶段采用的规范契约。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitCallReceiverOwnershipContract {
    call: UnitExpressionId,
    target: UnitCallTarget,
    source: UnitCallReceiverOrigin,
    receiver_type: UnitTypeId,
    category: ExpressionCategory,
    kind: UnitCallArgumentOwnershipKind,
    receiver_span: Span,
    call_span: Span,
    declaration_span: Option<Span>,
}

impl UnitCallReceiverOwnershipContract {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        call: UnitExpressionId,
        target: UnitCallTarget,
        source: UnitCallReceiverOrigin,
        receiver_type: UnitTypeId,
        category: ExpressionCategory,
        kind: UnitCallArgumentOwnershipKind,
        receiver_span: Span,
        call_span: Span,
        declaration_span: Option<Span>,
    ) -> Self {
        Self {
            call,
            target,
            source,
            receiver_type,
            category,
            kind,
            receiver_span,
            call_span,
            declaration_span,
        }
    }

    #[must_use]
    pub const fn call(self) -> UnitExpressionId {
        self.call
    }

    #[must_use]
    pub const fn target(self) -> UnitCallTarget {
        self.target
    }

    #[must_use]
    pub const fn source(self) -> UnitCallReceiverOrigin {
        self.source
    }

    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.receiver_type
    }

    #[must_use]
    pub const fn category(self) -> ExpressionCategory {
        self.category
    }

    #[must_use]
    pub const fn kind(self) -> UnitCallArgumentOwnershipKind {
        self.kind
    }

    #[must_use]
    pub const fn receiver_span(self) -> Span {
        self.receiver_span
    }

    #[must_use]
    pub const fn call_span(self) -> Span {
        self.call_span
    }

    #[must_use]
    pub const fn declaration_span(self) -> Option<Span> {
        self.declaration_span
    }
}

/// interface default 中尚待 concrete specialization 决定 Copy/Move 的 Value receiver 交付。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitConditionalReceiverDeliveryFact {
    call: UnitExpressionId,
    source: UnitCallReceiverOrigin,
    owner: DeclarationId,
    target: UnitCallTarget,
    receiver_type: UnitTypeId,
    receiver_origin: Span,
    delivery_span: Span,
}

impl UnitConditionalReceiverDeliveryFact {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        call: UnitExpressionId,
        source: UnitCallReceiverOrigin,
        owner: DeclarationId,
        target: UnitCallTarget,
        receiver_type: UnitTypeId,
        receiver_origin: Span,
        delivery_span: Span,
    ) -> Self {
        Self {
            call,
            source,
            owner,
            target,
            receiver_type,
            receiver_origin,
            delivery_span,
        }
    }

    /// 返回发生 conditional delivery 的 member call。
    #[must_use]
    pub const fn call(self) -> UnitExpressionId {
        self.call
    }

    /// 返回显式 `this` expression 或裸 member 的 implicit-this identity。
    #[must_use]
    pub const fn source(self) -> UnitCallReceiverOrigin {
        self.source
    }

    /// 返回当前 `StaticSelf` receiver 的 interface owner。
    #[must_use]
    pub const fn owner(self) -> DeclarationId {
        self.owner
    }

    /// 返回类型阶段唯一选择的静态 callable target。
    #[must_use]
    pub const fn target(self) -> UnitCallTarget {
        self.target
    }

    /// 返回必须由下游实例化的 `StaticSelf` receiver template。
    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.receiver_type
    }

    /// 返回当前 Value receiver binding 的声明位置。
    #[must_use]
    pub const fn receiver_origin(self) -> Span {
        self.receiver_origin
    }

    /// 返回本次 receiver delivery operand 的源码位置。
    #[must_use]
    pub const fn delivery_span(self) -> Span {
        self.delivery_span
    }
}

/// member receiver ownership effect 的稳定目标。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitReceiverOwnershipTarget {
    /// 显式 receiver 的稳定 place。
    Place(UnitOwnershipPlace),
    /// 显式 receiver 求值产生的 temporary。
    Temporary(UnitExpressionId),
    /// 当前 callable 的隐式 receiver。
    This(DeclarationId),
}

/// member receiver 在调用前实际发生的 ownership effect。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitReceiverOwnershipKind {
    /// Borrow receiver 建立 shared loan。
    SharedLoan,
    /// Inout receiver 建立 exclusive loan。
    ExclusiveLoan,
    /// Copyable Value receiver 复制交付。
    Copy,
    /// MoveOnly Value receiver 从 owned root 移动交付。
    Move,
    /// Value temporary 直接交付。
    Temporary,
}

/// codegen 可直接消费的 source-qualified receiver ownership fact。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitReceiverOwnershipFact {
    call: UnitExpressionId,
    source: UnitCallReceiverOrigin,
    target: UnitReceiverOwnershipTarget,
    kind: UnitReceiverOwnershipKind,
    receiver_type: UnitTypeId,
    begin_span: Span,
    end_span: Span,
    declaration_span: Option<Span>,
}

impl UnitReceiverOwnershipFact {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        call: UnitExpressionId,
        source: UnitCallReceiverOrigin,
        target: UnitReceiverOwnershipTarget,
        kind: UnitReceiverOwnershipKind,
        receiver_type: UnitTypeId,
        begin_span: Span,
        end_span: Span,
        declaration_span: Option<Span>,
    ) -> Self {
        Self {
            call,
            source,
            target,
            kind,
            receiver_type,
            begin_span,
            end_span,
            declaration_span,
        }
    }

    #[must_use]
    /// 返回所属 member call。
    pub const fn call(&self) -> UnitExpressionId {
        self.call
    }

    #[must_use]
    /// 返回显式 expression 或 implicit-this identity。
    pub const fn source(&self) -> UnitCallReceiverOrigin {
        self.source
    }

    #[must_use]
    /// 返回 place、temporary 或 `this` target。
    pub const fn target(&self) -> &UnitReceiverOwnershipTarget {
        &self.target
    }

    #[must_use]
    /// 返回 shared/exclusive loan 或 Value delivery kind。
    pub const fn kind(&self) -> UnitReceiverOwnershipKind {
        self.kind
    }

    #[must_use]
    /// 返回实例化 receiver 类型。
    pub const fn receiver_type(&self) -> UnitTypeId {
        self.receiver_type
    }

    #[must_use]
    /// 返回 effect 起点。
    pub const fn begin_span(&self) -> Span {
        self.begin_span
    }

    #[must_use]
    /// 返回同步 call 结束位置。
    pub const fn end_span(&self) -> Span {
        self.end_span
    }

    #[must_use]
    /// 返回 selected receiver 声明位置。
    pub const fn declaration_span(&self) -> Option<Span> {
        self.declaration_span
    }
}

/// Borrow-only interface delegate 的静态 ownership 转发计划。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDelegationOwnershipPlan {
    owner: DeclarationId,
    target: UnitSymbolId,
    delegation_span: Span,
    forwarders: Vec<UnitCallableTarget>,
}

impl UnitDelegationOwnershipPlan {
    pub(super) fn new(
        owner: DeclarationId,
        target: UnitSymbolId,
        delegation_span: Span,
        forwarders: Vec<UnitCallableTarget>,
    ) -> Self {
        Self {
            owner,
            target,
            delegation_span,
            forwarders,
        }
    }

    #[must_use]
    /// 返回需要 shared-borrow 的 outer receiver owner。
    pub const fn owner(&self) -> DeclarationId {
        self.owner
    }

    #[must_use]
    /// 返回 outer receiver 上需要 shared-borrow 的 delegate field。
    pub const fn target(&self) -> UnitSymbolId {
        self.target
    }

    #[must_use]
    /// 返回委托 clause 的源码范围。
    pub const fn delegation_span(&self) -> Span {
        self.delegation_span
    }

    #[must_use]
    /// 返回复用同一 outer/field shared-loan plan 的 Borrow forwarders。
    pub fn forwarders(&self) -> &[UnitCallableTarget] {
        &self.forwarders
    }
}
