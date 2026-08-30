//! Unit drop planner 的局部状态与 source-qualified fact 转换。

use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{SourceUnitId, UnitSymbolId},
    ownership_checking::{UnitClosureCaptureSource, UnitDropFact, UnitDropPoint, UnitDropTarget},
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
    AfterReplacement(ExpressionId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlannerDropTarget {
    Named(UnitSymbolId),
    Temporary(ExpressionId),
    ReplacedElement(ExpressionId),
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

impl PlannerDropFact {
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
        let statement = |id| UnitStatementId::new(source_unit, id);
        let point = match self.point {
            PlannerDropPoint::AfterExpression(id) => UnitDropPoint::AfterExpression(expression(id)),
            PlannerDropPoint::AfterBinaryOperands(id) => {
                UnitDropPoint::AfterBinaryOperands(expression(id))
            }
            PlannerDropPoint::AfterStatement(id) => UnitDropPoint::AfterStatement(statement(id)),
            PlannerDropPoint::CallReturn(id) => UnitDropPoint::CallReturn(expression(id)),
            PlannerDropPoint::ControlTransfer(id) => UnitDropPoint::ControlTransfer(expression(id)),
            PlannerDropPoint::BranchExit { control, branch } => UnitDropPoint::BranchExit {
                control: expression(control),
                branch,
            },
            PlannerDropPoint::LoopExit(id) => UnitDropPoint::LoopExit(statement(id)),
            PlannerDropPoint::FunctionEntry(id) => {
                UnitDropPoint::FunctionEntry(UnitItemId::new(source_unit, id))
            }
            PlannerDropPoint::LambdaEntry(id) => UnitDropPoint::LambdaEntry(expression(id)),
            PlannerDropPoint::AfterReplacement(id) => {
                UnitDropPoint::AfterReplacement(expression(id))
            }
        };
        let target = match self.target {
            PlannerDropTarget::Named(symbol) => UnitDropTarget::Named(symbol),
            PlannerDropTarget::Temporary(id) => UnitDropTarget::Temporary(expression(id)),
            PlannerDropTarget::ReplacedElement(id) => {
                UnitDropTarget::ReplacedElement(expression(id))
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

#[derive(Clone, Debug, Default)]
pub(super) struct ValueState {
    pub(super) values: Vec<OwnedValue>,
    pub(super) closures: BTreeMap<UnitSymbolId, ExpressionId>,
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
    merged
}
