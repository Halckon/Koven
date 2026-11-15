//! Static delegation plans, resolution and deterministic forwarder ordering.
use super::*;

/// 一个已验证、可供后续 body 与 lowering 消费的 interface 委托计划。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDelegationPlan {
    owner: DeclarationId,
    interface: UnitTypeId,
    target: UnitSymbolId,
    delegation_span: Span,
    by_span: Span,
    forwarders: Vec<UnitDelegationForwarderDescriptor>,
}

/// 一个 Borrow-only interface requirement 的静态 delegate forwarder。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDelegationForwarderDescriptor {
    requirement: UnitCallableTarget,
    receiver_type: UnitTypeId,
    resolution: UnitDelegationForwarderResolution,
    type_parameters: Vec<UnitSymbolId>,
    parameters: Vec<UnitCallableParameter>,
    return_type: UnitTypeId,
    declaration_span: Span,
}

impl UnitDelegationForwarderDescriptor {
    pub(crate) fn new(
        requirement: UnitCallableTarget,
        receiver_type: UnitTypeId,
        implementation: Option<UnitDelegationImplementationDescriptor>,
        type_parameters: Vec<UnitSymbolId>,
        parameters: Vec<UnitCallableParameter>,
        return_type: UnitTypeId,
        declaration_span: Span,
    ) -> Self {
        Self {
            requirement,
            receiver_type,
            resolution: implementation.map_or(
                UnitDelegationForwarderResolution::Unresolved,
                UnitDelegationForwarderResolution::Implementation,
            ),
            type_parameters,
            parameters,
            return_type,
            declaration_span,
        }
    }

    /// 返回被转发的 interface member target。
    #[must_use]
    pub const fn requirement(&self) -> UnitCallableTarget {
        self.requirement
    }

    /// 返回完整 interface instance receiver 类型。
    #[must_use]
    pub const fn receiver_type(&self) -> UnitTypeId {
        self.receiver_type
    }

    /// 返回 delegate concrete type 上可直接调用的已验证有体实现。
    ///
    /// `None` 表示该 forwarder 是 exact next hop 或仍需具体单态化/实例化才能解析。
    #[must_use]
    pub const fn implementation(&self) -> Option<UnitDelegationImplementationDescriptor> {
        match self.resolution {
            UnitDelegationForwarderResolution::Implementation(implementation) => {
                Some(implementation)
            }
            UnitDelegationForwarderResolution::NextHop(_)
            | UnitDelegationForwarderResolution::Unresolved => None,
        }
    }

    /// 返回 delegate 自身 delegation plan 中已验证的精确下一跳。
    #[must_use]
    pub const fn next_hop(&self) -> Option<UnitDelegationNextHopDescriptor> {
        match self.resolution {
            UnitDelegationForwarderResolution::NextHop(next_hop) => Some(next_hop),
            UnitDelegationForwarderResolution::Implementation(_)
            | UnitDelegationForwarderResolution::Unresolved => None,
        }
    }

    pub(crate) const fn set_next_hop(&mut self, next_hop: UnitDelegationNextHopDescriptor) {
        self.resolution = UnitDelegationForwarderResolution::NextHop(next_hop);
    }

    pub(crate) const fn set_unresolved(&mut self) {
        self.resolution = UnitDelegationForwarderResolution::Unresolved;
    }

    /// delegate forwarder receiver 固定为 Borrow。
    #[must_use]
    pub const fn receiver_mode(&self) -> ParameterMode {
        ParameterMode::Borrow
    }

    /// 返回转发 callable 的源码顺序泛型参数。
    #[must_use]
    pub fn type_parameters(&self) -> &[UnitSymbolId] {
        &self.type_parameters
    }

    /// 返回转发 callable 的显式参数契约。
    #[must_use]
    pub fn parameters(&self) -> &[UnitCallableParameter] {
        &self.parameters
    }

    /// 返回转发 callable 的实例化返回类型。
    #[must_use]
    pub const fn return_type(&self) -> UnitTypeId {
        self.return_type
    }

    /// 返回 interface requirement 声明范围。
    #[must_use]
    pub const fn declaration_span(&self) -> Span {
        self.declaration_span
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnitDelegationForwarderResolution {
    Implementation(UnitDelegationImplementationDescriptor),
    NextHop(UnitDelegationNextHopDescriptor),
    Unresolved,
}

/// 一个 delegate forwarder 已解析出的直接有体实现及其 receiver template。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitDelegationImplementationDescriptor {
    target: UnitCallableTarget,
    receiver_type: UnitTypeId,
}

impl UnitDelegationImplementationDescriptor {
    pub(crate) const fn new(target: UnitCallableTarget, receiver_type: UnitTypeId) -> Self {
        Self {
            target,
            receiver_type,
        }
    }

    /// 返回可直接调用的有体实现 identity。
    #[must_use]
    pub const fn target(self) -> UnitCallableTarget {
        self.target
    }

    /// 返回实现所属的完整 receiver type template。
    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.receiver_type
    }
}

/// 一个 delegate forwarder 已解析出的 delegation plan 下一跳。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitDelegationNextHopDescriptor {
    requirement: UnitCallableTarget,
    receiver_type: UnitTypeId,
}

impl UnitDelegationNextHopDescriptor {
    pub(crate) const fn new(requirement: UnitCallableTarget, receiver_type: UnitTypeId) -> Self {
        Self {
            requirement,
            receiver_type,
        }
    }

    /// 返回下一跳 forwarder 的精确 requirement identity。
    #[must_use]
    pub const fn requirement(self) -> UnitCallableTarget {
        self.requirement
    }

    /// 返回下一跳 requirement 所属的完整 receiver type template。
    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.receiver_type
    }
}

impl UnitDelegationPlan {
    pub(crate) const fn new(
        owner: DeclarationId,
        interface: UnitTypeId,
        target: UnitSymbolId,
        delegation_span: Span,
        by_span: Span,
    ) -> Self {
        Self {
            owner,
            interface,
            target,
            delegation_span,
            by_span,
            forwarders: Vec::new(),
        }
    }

    /// 返回声明委托的 concrete classifier。
    #[must_use]
    pub const fn owner(&self) -> DeclarationId {
        self.owner
    }

    /// 返回包含完整类型实参的目标 interface instance。
    #[must_use]
    pub const fn interface(&self) -> UnitTypeId {
        self.interface
    }

    /// 返回同一主构造器中的 immutable delegate field。
    #[must_use]
    pub const fn target(&self) -> UnitSymbolId {
        self.target
    }

    /// 返回从 `by` 到 target 的完整 delegation clause 范围。
    #[must_use]
    pub const fn delegation_span(&self) -> Span {
        self.delegation_span
    }

    /// 返回真实 `by` token 范围。
    #[must_use]
    pub const fn by_span(&self) -> Span {
        self.by_span
    }

    /// 返回源码顺序的 Borrow-only delegate forwarder。
    #[must_use]
    pub fn forwarders(&self) -> &[UnitDelegationForwarderDescriptor] {
        &self.forwarders
    }

    pub(crate) fn forwarders_mut(&mut self) -> &mut [UnitDelegationForwarderDescriptor] {
        &mut self.forwarders
    }

    pub(crate) fn push_forwarder(&mut self, forwarder: UnitDelegationForwarderDescriptor) {
        self.forwarders.push(forwarder);
    }

    pub(crate) fn clear_forwarders(&mut self) {
        self.forwarders.clear();
    }

    pub(crate) fn sort_forwarders(&mut self) {
        self.forwarders.sort_by_key(|forwarder| {
            let symbol = match forwarder.requirement {
                UnitCallableTarget::Symbol(symbol) => symbol,
                UnitCallableTarget::Declaration(_) => {
                    unreachable!("delegate requirements are instance member symbols")
                }
            };
            (
                symbol.source_unit().index(),
                forwarder.declaration_span.start(),
            )
        });
    }
}
