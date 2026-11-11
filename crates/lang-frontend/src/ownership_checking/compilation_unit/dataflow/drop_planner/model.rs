//! Unit drop planner 的局部状态与 source-qualified fact 转换。

use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{DeclarationId, SourceUnitId, UnitSymbolId},
    ownership_checking::{
        UnitClosureCaptureSource, UnitConditionalReceiverDropFact, UnitDropFact, UnitDropPoint,
        UnitDropTarget,
    },
    parser::NameMarker,
    source::Span,
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DropExpressionUse {
    Read,
    Consume,
    Place,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlannerDropPoint {
    AfterExpression(ExpressionId),
    AfterBinaryOperands(ExpressionId),
    WhenComparisonCompleted {
        control: ExpressionId,
        entry: usize,
        alternative: usize,
    },
    WhenAlternativeNoMatch {
        control: ExpressionId,
        entry: usize,
        alternative: usize,
    },
    AfterStatement(StatementId),
    CallReturn(ExpressionId),
    ControlTransfer(ExpressionId),
    BranchEntry {
        control: ExpressionId,
        branch: usize,
    },
    BranchExit {
        control: ExpressionId,
        branch: usize,
    },
    WhenAlternativeMatch {
        control: ExpressionId,
        entry: usize,
        alternative: usize,
    },
    LoopExit(StatementId),
    FunctionEntry(ItemId),
    LambdaEntry(ExpressionId),
    BeforeReplacement(ExpressionId),
    AfterReplacement(ExpressionId),
}

impl PlannerDropPoint {
    pub(super) fn into_unit(self, source_unit: SourceUnitId) -> UnitDropPoint {
        let expression = |id| UnitExpressionId::new(source_unit, id);
        let statement = |id| UnitStatementId::new(source_unit, id);
        match self {
            Self::AfterExpression(id) => UnitDropPoint::AfterExpression(expression(id)),
            Self::AfterBinaryOperands(id) => UnitDropPoint::AfterBinaryOperands(expression(id)),
            Self::WhenComparisonCompleted {
                control,
                entry,
                alternative,
            } => UnitDropPoint::WhenComparisonCompleted {
                control: expression(control),
                entry,
                alternative,
            },
            Self::WhenAlternativeNoMatch {
                control,
                entry,
                alternative,
            } => UnitDropPoint::WhenAlternativeNoMatch {
                control: expression(control),
                entry,
                alternative,
            },
            Self::AfterStatement(id) => UnitDropPoint::AfterStatement(statement(id)),
            Self::CallReturn(id) => UnitDropPoint::CallReturn(expression(id)),
            Self::ControlTransfer(id) => UnitDropPoint::ControlTransfer(expression(id)),
            Self::BranchEntry { control, branch } => UnitDropPoint::BranchEntry {
                control: expression(control),
                branch,
            },
            Self::BranchExit { control, branch } => UnitDropPoint::BranchExit {
                control: expression(control),
                branch,
            },
            Self::WhenAlternativeMatch {
                control,
                entry,
                alternative,
            } => UnitDropPoint::WhenAlternativeMatch {
                control: expression(control),
                entry,
                alternative,
            },
            Self::LoopExit(id) => UnitDropPoint::LoopExit(statement(id)),
            Self::FunctionEntry(id) => {
                UnitDropPoint::FunctionEntry(UnitItemId::new(source_unit, id))
            }
            Self::LambdaEntry(id) => UnitDropPoint::LambdaEntry(expression(id)),
            Self::BeforeReplacement(id) => UnitDropPoint::BeforeReplacement(expression(id)),
            Self::AfterReplacement(id) => UnitDropPoint::AfterReplacement(expression(id)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlannerDropTarget {
    This(DeclarationId),
    Named(UnitSymbolId),
    Temporary(ExpressionId),
    ReplacedElement(ExpressionId),
    ReplacedField {
        assignment: ExpressionId,
        field: UnitSymbolId,
    },
    ReplacedCapture {
        closure: ExpressionId,
        assignment: ExpressionId,
        symbol: UnitSymbolId,
    },
    Captured {
        closure: ExpressionId,
        source: UnitClosureCaptureSource,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PlannerDropFact {
    point: PlannerDropPoint,
    target: PlannerDropTarget,
    value_origin: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PlannerConditionalReceiverDropFact {
    pub(super) point: PlannerDropPoint,
    pub(super) owner: DeclarationId,
    pub(super) receiver_type: crate::type_checking::UnitTypeId,
    pub(super) value_origin: Span,
    pub(super) preceding_drops: usize,
}

impl PlannerConditionalReceiverDropFact {
    pub(super) fn into_unit(self, source_unit: SourceUnitId) -> UnitConditionalReceiverDropFact {
        UnitConditionalReceiverDropFact::new(
            self.point.into_unit(source_unit),
            self.owner,
            self.receiver_type,
            self.value_origin,
            self.preceding_drops,
        )
    }
}

impl PlannerDropFact {
    pub(super) const fn target(self) -> PlannerDropTarget {
        self.target
    }

    pub(super) const fn point(self) -> PlannerDropPoint {
        self.point
    }

    pub(super) const fn new(
        point: PlannerDropPoint,
        target: PlannerDropTarget,
        value_origin: Span,
    ) -> Self {
        Self {
            point,
            target,
            value_origin,
        }
    }

    pub(super) fn into_unit(self, source_unit: SourceUnitId) -> UnitDropFact {
        let expression = |id| UnitExpressionId::new(source_unit, id);
        let point = self.point.into_unit(source_unit);
        let target = match self.target {
            PlannerDropTarget::This(owner) => UnitDropTarget::This(owner),
            PlannerDropTarget::Named(symbol) => UnitDropTarget::Named(symbol),
            PlannerDropTarget::Temporary(id) => UnitDropTarget::Temporary(expression(id)),
            PlannerDropTarget::ReplacedElement(id) => {
                UnitDropTarget::ReplacedElement(expression(id))
            }
            PlannerDropTarget::ReplacedField { assignment, field } => {
                UnitDropTarget::ReplacedField {
                    assignment: expression(assignment),
                    field,
                }
            }
            PlannerDropTarget::ReplacedCapture {
                closure,
                assignment,
                symbol,
            } => UnitDropTarget::ReplacedCapture {
                closure: expression(closure),
                assignment: expression(assignment),
                symbol,
            },
            PlannerDropTarget::Captured { closure, source } => UnitDropTarget::Captured {
                closure: expression(closure),
                source,
            },
        };
        UnitDropFact::new(point, target, self.value_origin)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OwnedValue {
    pub(super) owner: Option<crate::ownership_checking::UnitOwnerId>,
    pub(super) symbol: UnitSymbolId,
    pub(super) origin: Span,
    pub(super) declaration: Span,
    pub(super) scope_depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OwnedThis {
    pub(super) owner: DeclarationId,
    pub(super) origin: Span,
    pub(super) conditional_type: Option<crate::type_checking::UnitTypeId>,
}

// Keep original pending action/depth semantics independent of observation identity.
#[derive(Clone, Debug)]
pub(super) struct PendingLoan(
    pub(super) crate::ownership_checking::UnitIterationCleanupAction,
    pub(super) usize,
    pub(super) Option<super::action_source::ActionHandle>,
);
impl PartialEq for PendingLoan {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && self.1 == other.1
    }
}
impl Eq for PendingLoan {}
impl PendingLoan {
    fn reconcile(&mut self, alternatives: &[Self]) {
        let mut matches = alternatives.iter().filter(|other| *other == self);
        let same = matches
            .next()
            .and_then(|other| self.2.as_ref().zip(other.2.as_ref()))
            .is_some_and(|(left, right)| left.same_record(right))
            && matches.next().is_none();
        if !same {
            self.2 = None;
        }
    }
}

/// Exact observation inputs captured from the ordinary merge, never a declaration-derived union.
#[derive(Clone, Debug)]
pub(super) struct MultiMerge {
    pub(super) inputs: Vec<super::action_source::MultiState>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ValueState {
    /// Pure observation scope; never part of value availability or owner eligibility.
    pub(super) forward_scope: Option<super::borrow_forward_transport::ForwardScope>,
    pub(super) multi_continuation: Option<super::action_source::MultiState>,
    pub(super) multi_merge: Option<MultiMerge>,
    pub(super) single_continuation: Option<super::action_source::SingleState>,
    pub(super) bounded_forward: Option<super::action_source::BoundedState>,
    pub(super) nested_forward: Option<super::nested_forward_scope::NestedForwardState>,
    pub(super) branch_forward_plan: Option<super::branch_forward_scope::BranchForwardPlan>,
    pub(super) replace_consumer:
        Option<super::owner_transport::replace_consumer::ReplaceConsumerCarrier>,
    pub(super) lambda: Option<ExpressionId>,
    pub(super) borrow_bindings: Vec<UnitSymbolId>,
    pub(super) pending_borrow_results: Vec<UnitSymbolId>,
    /// Observational direct initializer/source identities; not an established result.
    pub(super) borrow_initializer_sources: Vec<(
        crate::type_checking::UnitExpressionId,
        crate::type_checking::UnitExpressionId,
    )>,
    /// 实际已求值的实参loan与建立时loop深度；未求值后缀不得参与退出清理。
    pub(super) pending_loans: Vec<PendingLoan>,
    pub(super) borrow_call_transports: Vec<super::borrow_transport::PendingTransport>,
    pub(super) borrow_binding_transports:
        Vec<super::borrow_binding_transport::PendingBindingTransport>,
    pub(super) borrow_forward_transports:
        Vec<super::borrow_forward_transport::PendingForwardTransport>,
    pub(super) iterations: Vec<super::iteration::IterationFrame>,
    pub(super) pending_temporaries: Vec<super::pending_call::PendingTemporary>,
    pub(super) values: Vec<OwnedValue>,
    /// 普通赋值的旧 owner 必须保留到完整 RHS 求值结束。
    pub(super) replacements: Vec<UnitSymbolId>,
    /// 已求值调用前缀的借用必须跨越后续实参中的分支与嵌套调用。
    pub(super) pending_borrows: Vec<(ExpressionId, UnitSymbolId)>,
    /// Protects both the evaluated environment and its borrowed roots across inner loop jumps.
    pub(super) pending_capture_borrows: Vec<super::pending_call::PendingCaptureBorrow>,
    pub(super) closures: BTreeMap<UnitSymbolId, ExpressionId>,
    /// Concrete current environments only; a proof never grants owner availability.
    pub(super) closure_asap: BTreeMap<UnitSymbolId, bool>,
    pub(super) this: Option<OwnedThis>,
}

impl ValueState {
    pub(super) fn capture_borrow_protects(&self, symbol: UnitSymbolId) -> bool {
        self.pending_capture_borrows
            .iter()
            .any(|borrow| borrow.symbol == symbol)
    }

    pub(super) fn position(&self, symbol: UnitSymbolId) -> Option<usize> {
        self.values.iter().position(|value| value.symbol == symbol)
    }

    pub(super) fn remove_value(&mut self, symbol: UnitSymbolId) -> Option<OwnedValue> {
        self.position(symbol).map(|index| self.values.remove(index))
    }

    pub(super) fn take_closure(&mut self, symbol: UnitSymbolId) -> Option<ExpressionId> {
        self.closure_asap.remove(&symbol);
        self.closures.remove(&symbol)
    }

    pub(super) fn take(&mut self, symbol: UnitSymbolId) -> Option<OwnedValue> {
        self.take_closure(symbol);
        self.remove_value(symbol)
    }

    pub(super) fn insert(&mut self, value: OwnedValue) {
        self.remove_value(value.symbol);
        let index = self
            .values
            .partition_point(|prior| prior.declaration.start() < value.declaration.start());
        self.values.insert(index, value);
    }
}

#[derive(Clone, Copy)]
pub(super) enum StringOperandDrop {
    Named(UnitSymbolId),
    Temporary(ExpressionId, Span),
}

pub(super) const fn marker_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
    }
}

pub(super) fn merge_value_states(mut states: Vec<ValueState>) -> ValueState {
    let multi_merge = states
        .iter()
        .map(|state| {
            if state.multi_merge.is_none() {
                state.multi_continuation
            } else {
                None
            }
        })
        .collect::<Option<Vec<_>>>()
        .filter(|inputs| {
            inputs.first().is_some_and(|first| {
                inputs
                    .iter()
                    .all(|state| state.candidate == first.candidate)
            })
        })
        .map(|inputs| MultiMerge { inputs });
    let Some(mut merged) = states.pop() else {
        return ValueState::default();
    };
    // This bounded branch observation authorizes only the single surviving false edge.
    if !states.is_empty() {
        merged.branch_forward_plan = None;
        if merged
            .forward_scope
            .is_some_and(|scope| scope.branch.is_some())
        {
            merged.forward_scope = None;
        }
    }
    // Nested lineage never survives a live continuation merge, even a single survivor.
    merged.nested_forward = None;
    merged.bounded_forward = None;
    merged.multi_continuation = None;
    merged.multi_merge = multi_merge;
    // Only the original unique survivor retains observation identity; no reconstruction.
    if !states.is_empty() {
        merged.single_continuation = None;
    }
    // No general branch merge is authorized for the bounded suffix carrier.
    merged.replace_consumer = None;
    // Existing merge keeps the last state's pending actions. Only evidence degrades.
    for pending in &mut merged.pending_loans {
        for state in &states {
            pending.reconcile(&state.pending_loans);
        }
    }
    merged.borrow_forward_transports.retain(|transport| {
        states
            .iter()
            .all(|state| state.borrow_forward_transports.contains(transport))
    });
    if states
        .iter()
        .any(|state| state.forward_scope != merged.forward_scope)
    {
        merged.forward_scope = None;
    }
    for transport in &mut merged.borrow_forward_transports {
        for state in &states {
            transport.reconcile(&state.borrow_forward_transports);
        }
        transport.reconcile_scope(merged.forward_scope);
        transport.reconcile_nested_scope(merged.nested_forward);
        transport.reconcile_bounded_scope(merged.bounded_forward);
        transport.reconcile_single_scope(merged.single_continuation);
        transport.reconcile_multi_scope(merged.multi_continuation);
    }
    merged.borrow_binding_transports.retain(|transport| {
        states
            .iter()
            .all(|state| state.borrow_binding_transports.contains(transport))
    });
    for transport in &mut merged.borrow_binding_transports {
        for state in &states {
            transport.reconcile(&state.borrow_binding_transports);
        }
    }
    merged.borrow_call_transports.retain(|transport| {
        states
            .iter()
            .all(|state| state.borrow_call_transports.contains(transport))
    });
    // Preserve old transport eligibility, but never select an arbitrary observer edge.
    for transport in &mut merged.borrow_call_transports {
        for state in &states {
            transport.reconcile_transfer(&state.borrow_call_transports);
        }
    }
    merged.values.retain(|value| {
        states
            .iter()
            .all(|state| state.position(value.symbol).is_some())
    });
    for value in &mut merged.values {
        for state in &states {
            if let Some(index) = state.position(value.symbol) {
                let candidate = state.values[index].origin;
                if candidate.start() < value.origin.start() {
                    value.origin = candidate;
                }
            }
        }
    }
    // Purity needs the same current environment and proof on every incoming path.
    let closures = &merged.closures;
    merged.closure_asap.retain(|symbol, pure| {
        states.iter().all(|state| {
            state.closures.get(symbol) == closures.get(symbol)
                && state.closure_asap.get(symbol) == Some(pure)
        })
    });
    if !states.iter().all(|state| state.this.is_some()) {
        merged.this = None;
    } else if let Some(value) = merged.this.as_mut() {
        for state in &states {
            if let Some(candidate) = state.this
                && candidate.origin.start() < value.origin.start()
            {
                value.origin = candidate.origin;
            }
        }
    }
    merged
}
