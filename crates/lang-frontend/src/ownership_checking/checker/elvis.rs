//! Elvis 只求值一次 nullable source；只有非空边交付 inner，null 边求值 RHS。
use super::{AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::{
    ast::ExpressionId,
    parser::Expression,
    type_checking::{BuiltinType, Copyability, TypeKind},
};

impl Checker<'_> {
    pub(super) fn is_only_null(&self, expression: ExpressionId) -> bool {
        self.typed
            .expression_type(expression)
            .and_then(|ty| self.typed.types().get(ty))
            .is_some_and(|kind| {
                matches!(kind, TypeKind::Nullable(inner)
                if self.typed.types().get(*inner) == Some(&TypeKind::Builtin(BuiltinType::Nothing)))
            })
    }

    pub(super) fn check_elvis(
        &mut self,
        id: ExpressionId,
        left: ExpressionId,
        right: ExpressionId,
        state: State,
        escaping: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let errors = self.diagnostics.len();
        let mut prefix = self.check_expression(left, state, ExpressionUse::Place)?;
        let Some(mut base) = prefix.next.take() else {
            return Ok(prefix);
        };
        let span = self.parsed.ast().expressions().get(left)?.span();
        self.access_expression_place(left, AccessKind::Read, span, &mut base)?;
        let usage = self.control_result_usage(id);
        if !self.is_only_null(left) {
            let mut selected = base.clone();
            if self.diagnostics.len() == errors && usage == ExpressionUse::Consume {
                // 这是已求值 source 的交付，不能重跑 receiver、index 或 call。
                self.deliver_elvis_source(left, &mut selected)?;
            }
            if escaping {
                self.reject_borrowed_closure_escape(left, &selected)?;
            }
            let mut flow = Flows::next(selected);
            self.record_elvis_closures(id, left, &mut flow)?;
            prefix.merge(flow);
        }
        let mut fallback = if escaping {
            self.check_escaping_expression(right, base, usage)?
        } else {
            self.check_expression(right, base, usage)?
        };
        self.record_elvis_closures(id, right, &mut fallback)?;
        prefix.merge(fallback);
        self.release_dead_control_closures(id, &mut prefix);
        Ok(prefix)
    }

    fn deliver_elvis_source(
        &mut self,
        expression: ExpressionId,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload().clone() {
            Expression::Group { expression } => self.deliver_elvis_source(expression, state),
            Expression::Name => self.use_name(node.span(), ExpressionUse::Consume, state),
            Expression::Member { name_span, .. } => {
                if self.access_expression_place(expression, AccessKind::Move, name_span, state)? {
                    self.reject_partial_move(expression, name_span)?;
                }
                Ok(())
            }
            Expression::Index { .. } => {
                self.access_element_value(expression, ExpressionUse::Consume, state)
            }
            // A temporary already owns the evaluated wrapper and transfers it to its inner value.
            _ => Ok(()),
        }
    }

    fn record_elvis_closures(
        &self,
        control: ExpressionId,
        value: ExpressionId,
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(state) = flows.next.as_mut() else {
            return Ok(());
        };
        state.closure_results.remove(&control.index());
        if self.control_result_usage(control) == ExpressionUse::Consume {
            let origins = self.closure_origins(value, state)?;
            if !origins.is_empty() {
                state.closure_results.insert(control.index(), origins);
                if let Some(symbol) = self.expression_root_symbol(value)? {
                    state.closures.remove(&symbol);
                }
            }
        }
        Ok(())
    }

    pub(super) fn is_move_only_expression(&self, expression: ExpressionId) -> bool {
        self.typed
            .expression_type(expression)
            .and_then(|ty| self.typed.copyability(ty))
            == Some(Copyability::MoveOnly)
    }
}
