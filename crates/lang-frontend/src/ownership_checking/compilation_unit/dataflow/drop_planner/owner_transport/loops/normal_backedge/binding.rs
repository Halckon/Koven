//! Actual tail Take/Binding/Drop of H; never a fixed two-alternative carrier.
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::{
    HeaderEquation, ReturnConsumerTarget,
};
use crate::ownership_checking::{
    UnitDropPoint, UnitOwnerNormalBindDropV1, UnitOwnerReplaceBreakStateV1,
};

pub(super) enum NormalSuffix {
    Return(ReturnConsumerTarget),
    BindDrop(Box<BindInput>),
}
pub(super) struct BindInput {
    initializer: UnitExpressionId,
    statement: UnitStatementId,
    retired: UnitSymbolId,
    equation: Option<HeaderEquation>,
    taken: Option<UnitOwnerReplaceBreakStateV1>,
    bound: Option<UnitOwnerReplaceBreakStateV1>,
    valid: bool,
    result_completed: bool,
}
impl BindInput {
    pub(super) fn admit(
        checker: &Checker<'_>,
        statement: StatementId,
        root: UnitSymbolId,
    ) -> Option<Self> {
        let (retired, initializer) =
            super::super::replace_break::variable(checker, statement, VariableKind::Val)?;
        let node = checker.parsed.ast().expressions().get(initializer).ok()?;
        if retired == root
            || !matches!(node.payload(), Expression::Name)
            || checker.reference_symbol(node.span()) != Some(root)
            || !exact_type(checker, initializer, BuiltinType::String)
        {
            return None;
        }
        Some(Self {
            initializer: checker.unit_expression(initializer),
            statement: UnitStatementId::new(checker.source_unit, statement),
            retired,
            equation: None,
            taken: None,
            bound: None,
            valid: true,
            result_completed: false,
        })
    }
    pub(super) fn install(&mut self, equation: HeaderEquation) {
        if self.equation.is_some() {
            self.valid = false;
        } else {
            self.equation = Some(equation);
        }
    }
    pub(super) fn take(&mut self, id: UnitExpressionId, value: Option<&OwnedValue>) {
        if id != self.initializer {
            return;
        }
        let actual = value.and_then(super::super::super::replace_consumer::snapshot);
        if !self.valid
            || self.taken.is_some()
            || self.bound.is_some()
            || actual.is_none()
            || self
                .equation
                .as_ref()
                .is_none_or(|e| actual.as_ref() != Some(&e.false_sample))
        {
            self.valid = false;
            return;
        }
        self.taken = actual;
    }
    fn bind(&mut self, id: UnitExpressionId, symbol: UnitSymbolId, state: &ValueState) {
        if id != self.initializer || symbol != self.retired {
            return;
        }
        let Some([value]) = boundary_values(state) else {
            self.valid = false;
            return;
        };
        let actual = super::super::super::replace_consumer::snapshot(value);
        if !self.valid
            || !self.result_completed
            || self.bound.is_some()
            || value.symbol != self.retired
            || value.scope_depth != 1
            || value.origin != value.declaration
            || self
                .taken
                .as_ref()
                .is_none_or(|t| value.owner.as_ref() != Some(&t.owner))
        {
            self.valid = false;
            return;
        }
        self.bound = actual;
    }
    fn dropped(
        &mut self,
        point: UnitDropPoint,
        value: &OwnedValue,
        state: &ValueState,
    ) -> Option<UnitOwnerNormalBindDropV1> {
        if value.symbol != self.retired {
            return None;
        }
        let actual = super::super::super::replace_consumer::snapshot(value);
        if !self.valid
            || point != UnitDropPoint::AfterStatement(self.statement)
            || actual.is_none()
            || actual != self.bound
            || !empty_boundary(state)
        {
            self.valid = false;
            return None;
        }
        self.valid = false;
        Some(UnitOwnerNormalBindDropV1 {
            core: self.equation.take()?,
            initializer: self.initializer,
            statement: self.statement,
            retired: self.retired,
            taken: self.taken.take()?,
            bound: self.bound.take()?,
            point,
            empty: true,
            result_completed: self.result_completed,
        })
    }
}
impl DropPlanner<'_, '_> {
    // Preserve only the actual owner returned by the exact terminal Name evaluation.
    // The legacy whole-function observer is inactive after the loop; no owner is
    // reconstructed from the equation or the captured Take snapshot here.
    pub(in super::super::super::super) fn complete_normal_binding_result(
        &mut self,
        id: ExpressionId,
        usage: DropExpressionUse,
        continues: bool,
        owner: Option<&UnitOwnerId>,
        state: &ValueState,
    ) -> bool {
        if !self.normal_tail() {
            return false;
        }
        let Some(n) = self.owner_loop.as_mut().and_then(|t| t.normal.as_mut()) else {
            return false;
        };
        let NormalSuffix::BindDrop(input) = &mut n.suffix else {
            return false;
        };
        if self.checker.unit_expression(id) != input.initializer {
            return false;
        }
        if !n.valid
            || !input.valid
            || input.result_completed
            || input.bound.is_some()
            || !continues
            || !matches!(usage, DropExpressionUse::Consume)
            || !empty_boundary(state)
            || input
                .taken
                .as_ref()
                .is_none_or(|taken| owner != Some(&taken.owner))
            || input.equation.as_ref().is_none_or(|e| !e.is_consistent())
        {
            input.valid = false;
            return false;
        }
        input.result_completed = true;
        true
    }

    pub(in super::super::super::super) fn bind_normal_header(
        &mut self,
        id: ExpressionId,
        symbol: UnitSymbolId,
        state: &ValueState,
    ) {
        if !self.normal_tail() {
            return;
        }
        let Some(n) = self.owner_loop.as_mut().and_then(|t| t.normal.as_mut()) else {
            return;
        };
        if !n.valid {
            return;
        }
        if let NormalSuffix::BindDrop(input) = &mut n.suffix {
            input.bind(self.checker.unit_expression(id), symbol, state);
        }
    }
    pub(in super::super::super::super) fn drop_normal_header(
        &mut self,
        point: PlannerDropPoint,
        value: &OwnedValue,
        state: &ValueState,
    ) {
        if !self.normal_tail() {
            return;
        }
        let Some(t) = self.owner_loop.as_mut() else {
            return;
        };
        let Some(n) = &mut t.normal else {
            return;
        };
        if !n.valid || t.replace_result.is_some() {
            return;
        }
        if let NormalSuffix::BindDrop(input) = &mut n.suffix
            && let Some(product) =
                input.dropped(point.into_unit(self.checker.source_unit), value, state)
            && product.is_consistent()
        {
            t.replace_result = Some(UnitOwnerOpenLoopObservationV1::NormalBindDrop(Box::new(
                product,
            )));
        }
    }
}
