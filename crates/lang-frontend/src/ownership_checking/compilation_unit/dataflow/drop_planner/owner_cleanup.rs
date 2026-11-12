//! Named-owner destruction: protections, actual removal, capture cleanup and ordered fact publication.
//! Exit selection and liveness remain with the enclosing planner; no observation changes execution.
use super::{
    DropPlanner, ExpressionId, OwnedValue, PlannerDropFact, PlannerDropPoint, PlannerDropTarget,
    UnitSymbolId, ValueState, action_source,
};
use crate::{
    ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode, UnitClosureCaptureSource},
    parser::Expression,
};

impl DropPlanner<'_, '_> {
    pub(super) fn drop_named(
        &mut self,
        point: PlannerDropPoint,
        symbol: UnitSymbolId,
        state: &mut ValueState,
    ) {
        self.drop_named_with_binding_drop(point, symbol, state, None);
    }

    pub(super) fn drop_named_with_binding_drop(
        &mut self,
        point: PlannerDropPoint,
        symbol: UnitSymbolId,
        state: &mut ValueState,
        cause: Option<action_source::BindingDropCause>,
    ) {
        if self.call_return_copy_source == Some(symbol)
            || self.borrow_source_protected(symbol, state)
            || state.capture_borrow_protects(symbol)
        {
            return;
        }
        // A provider keeps its source even without a later explicit source read.
        // Control-transfer cleanup must finish that provider before releasing the root.
        if state
            .iterations
            .iter()
            .any(|frame| frame.source_root == Some(symbol))
        {
            return;
        }
        if !matches!(point, PlannerDropPoint::ControlTransfer(_))
            && (state
                .pending_borrows
                .iter()
                .any(|(_, root)| *root == symbol)
                || self.has_live_shared_capture(state, symbol))
        {
            return;
        }
        if state.replacements.contains(&symbol) {
            // 与单文件 planner 一致：RHS 放弃时清理，内层 loop 跳转保留外层旧值。
            let leaving = match point {
                PlannerDropPoint::ControlTransfer(expression) => self
                    .checker
                    .parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .is_ok_and(|node| {
                        matches!(node.payload(), Expression::Return { .. })
                            || (matches!(
                                node.payload(),
                                Expression::Break { .. } | Expression::Continue { .. }
                            ) && self.loop_boundaries.last().is_some_and(|depth| {
                                state
                                    .position(symbol)
                                    .is_some_and(|index| state.values[index].scope_depth > *depth)
                            }))
                    }),
                _ => false,
            };
            if !leaving {
                return;
            }
        }
        let closure = state.take_closure(symbol);
        if let Some(value) = state.remove_value(symbol) {
            self.drop_owned_value_with_binding_drop(point, value, closure, state, cause);
        }
    }

    pub(super) fn drop_owned_value(
        &mut self,
        point: PlannerDropPoint,
        value: OwnedValue,
        closure: Option<ExpressionId>,
        state: &mut ValueState,
    ) {
        self.drop_owned_value_with_binding_drop(point, value, closure, state, None);
    }

    fn drop_owned_value_with_binding_drop(
        &mut self,
        point: PlannerDropPoint,
        value: OwnedValue,
        closure: Option<ExpressionId>,
        state: &mut ValueState,
        cause: Option<action_source::BindingDropCause>,
    ) {
        let mut shared_sources = Vec::new();
        if let Some(closure) = closure {
            let captures = self.checker.captures_of(closure).collect::<Vec<_>>();
            for capture in captures.into_iter().rev() {
                if capture.mode() == ClosureCaptureMode::Shared {
                    self.end_shared_capture(point, &value, capture, state);
                }
                if capture.mode() == ClosureCaptureMode::Owned
                    && capture.effect() == ClosureCaptureEffect::Move
                {
                    self.drop_capture_owner(point, value.owner.as_ref(), capture.source());
                    self.push_fact(PlannerDropFact::new(
                        point,
                        PlannerDropTarget::Captured {
                            closure,
                            source: capture.source(),
                        },
                        capture.reference_span(),
                    ));
                } else if capture.mode() == ClosureCaptureMode::Shared
                    && let UnitClosureCaptureSource::Symbol(source) = capture.source()
                {
                    shared_sources.push(source);
                }
            }
        }
        if let Some(choice) = &mut self.owner_choice {
            choice.dropped(point.into_unit(self.checker.source_unit), value.symbol);
        }
        if let Some(owner) = value.owner.as_ref() {
            self.owner_event(crate::ownership_checking::UnitOwnerTransportEvent::Drop {
                point: point.into_unit(self.checker.source_unit),
                symbol: value.symbol,
                owner: owner.clone(),
            });
        }
        let cause = cause.and_then(|cause| {
            self.iteration_actions
                .verify_removed_binding_owner(cause, &value)
        });
        self.push_fact_with_binding_drop(
            PlannerDropFact::new(point, PlannerDropTarget::Named(value.symbol), value.origin),
            cause,
        );
        self.drop_normal_header(point, &value, state);
        self.drop_replace_consumer(point, &value, state);
        for source in shared_sources {
            // Match-edge liveness excludes reads in the selected entry body.
            if !matches!(point, PlannerDropPoint::WhenAlternativeMatch { .. })
                && !self.has_live_shared_capture(state, source)
                && !self.live_after(point).contains(&source)
            {
                self.drop_named_asap(point, source, state);
            }
        }
    }

    // A closure can keep its source alive after the last explicit source read.
    pub(super) fn has_live_shared_capture(&self, state: &ValueState, symbol: UnitSymbolId) -> bool {
        state.closures.values().any(|&closure| {
            self.checker.captures_of(closure).any(|capture| {
                capture.mode() == ClosureCaptureMode::Shared
                    && capture.source() == UnitClosureCaptureSource::Symbol(symbol)
            })
        })
    }

    pub(super) fn push_fact(&mut self, fact: PlannerDropFact) {
        self.push_fact_with_binding_drop(fact, None);
    }

    fn push_fact_with_binding_drop(
        &mut self,
        mut fact: PlannerDropFact,
        cause: Option<action_source::BindingDropCause>,
    ) {
        // Normalize once before both the flat drop product and ordered cleanup consume it.
        if let crate::ownership_checking::UnitDropTarget::Temporary(expression) =
            fact.into_unit(self.checker.source_unit).target()
            && let Some((owner, origin)) = self
                .checker
                .constant_temporary_origin(expression.expression())
        {
            fact = PlannerDropFact::new(
                fact.point(),
                PlannerDropTarget::Temporary(owner.expression()),
                origin,
            );
        }
        if !self.facts.contains(&fact) {
            self.facts.push(fact);
            self.iteration_actions.push_with_binding_drop(
                (
                    fact.point(),
                    crate::ownership_checking::UnitIterationCleanupAction::Drop(
                        fact.into_unit(self.checker.source_unit),
                    ),
                ),
                cause,
            );
        }
    }
}
