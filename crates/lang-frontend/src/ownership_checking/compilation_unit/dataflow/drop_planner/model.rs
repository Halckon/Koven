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
    AfterStatement(StatementId),
    CallReturn(ExpressionId),
    ControlTransfer(ExpressionId),
    BranchExit {
        control: ExpressionId,
        branch: usize,
    },
    LoopExit(StatementId),
    FunctionEntry(ItemId),
    LambdaEntry(ExpressionId),
    BeforeReplacement(ExpressionId),
    AfterReplacement(ExpressionId),
}

impl PlannerDropPoint {
    fn into_unit(self, source_unit: SourceUnitId) -> UnitDropPoint {
        let expression = |id| UnitExpressionId::new(source_unit, id);
        let statement = |id| UnitStatementId::new(source_unit, id);
        match self {
            Self::AfterExpression(id) => UnitDropPoint::AfterExpression(expression(id)),
            Self::AfterBinaryOperands(id) => UnitDropPoint::AfterBinaryOperands(expression(id)),
            Self::AfterStatement(id) => UnitDropPoint::AfterStatement(statement(id)),
            Self::CallReturn(id) => UnitDropPoint::CallReturn(expression(id)),
            Self::ControlTransfer(id) => UnitDropPoint::ControlTransfer(expression(id)),
            Self::BranchExit { control, branch } => UnitDropPoint::BranchExit {
                control: expression(control),
                branch,
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
            PlannerDropTarget::Captured { closure, source } => UnitDropTarget::Captured {
                closure: expression(closure),
                source,
            },
        };
        UnitDropFact::new(point, target, self.value_origin)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OwnedValue {
    pub(super) symbol: UnitSymbolId,
    pub(super) origin: Span,
    pub(super) scope_depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OwnedThis {
    pub(super) owner: DeclarationId,
    pub(super) origin: Span,
    pub(super) conditional_type: Option<crate::type_checking::UnitTypeId>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ValueState {
    pub(super) pending_temporaries: Vec<super::pending_call::PendingTemporary>,
    pub(super) values: Vec<OwnedValue>,
    /// 已求值调用前缀的借用必须跨越后续实参中的分支与嵌套调用。
    pub(super) pending_borrows: Vec<(ExpressionId, UnitSymbolId)>,
    pub(super) closures: BTreeMap<UnitSymbolId, ExpressionId>,
    pub(super) this: Option<OwnedThis>,
}

impl ValueState {
    pub(super) fn position(&self, symbol: UnitSymbolId) -> Option<usize> {
        self.values.iter().position(|value| value.symbol == symbol)
    }

    pub(super) fn remove_value(&mut self, symbol: UnitSymbolId) -> Option<OwnedValue> {
        self.position(symbol).map(|index| self.values.remove(index))
    }

    pub(super) fn take(&mut self, symbol: UnitSymbolId) -> Option<OwnedValue> {
        self.closures.remove(&symbol);
        self.remove_value(symbol)
    }

    pub(super) fn insert(&mut self, value: OwnedValue) {
        self.remove_value(value.symbol);
        self.values.push(value);
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
    let Some(mut merged) = states.pop() else {
        return ValueState::default();
    };
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
