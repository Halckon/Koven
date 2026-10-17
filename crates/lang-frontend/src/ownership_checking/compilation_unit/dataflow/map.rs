//! Map 下标赋值按源码顺序求值 receiver/key/value，并取得独占变异权限。
use super::{AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::{
    diagnostic::{Diagnostic, Severity},
    type_checking::UnitMapPutDescriptor,
};

impl Checker<'_> {
    pub(super) fn check_map_assignment(
        &mut self,
        put: &UnitMapPutDescriptor,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let receiver = put.receiver().expression();
        let diagnostic_count = self.diagnostics.len();
        let flows = self.check_expression(
            receiver,
            state,
            ExpressionUse::Place {
                parameter_span: None,
            },
        )?;
        let flows = self.chain_expression(
            flows,
            put.key().expression(),
            ExpressionUse::Consume {
                parameter_span: None,
            },
        )?;
        let mut flows = self.chain_expression(
            flows,
            put.value().expression(),
            ExpressionUse::Consume {
                parameter_span: None,
            },
        )?;
        if self.diagnostics.len() != diagnostic_count {
            return Ok(flows);
        }
        let Some(state) = flows.next.as_mut() else {
            return Ok(flows);
        };
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(put.expression().expression())?
            .span();
        if !self.is_mutable_place(receiver)? {
            self.diagnostics.push(Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.immutable_inout,
                "map update requires a mutable receiver place",
                span,
            )?);
            return Ok(flows);
        }
        if let Some(place) = self.place(receiver)? {
            self.access_place(&place, AccessKind::Mutation, span, None, state)?;
        }
        Ok(flows)
    }
}

impl Checker<'_> {
    /// 不提交非法 outer receiver；子树自身的 move、诊断和控制流不回滚。
    pub(super) fn check_map_consume_receiver(
        &mut self,
        receiver: crate::ast::ExpressionId,
        state: State,
    ) -> Result<(Flows, bool), OwnershipCheckingError> {
        use crate::{
            diagnostic::codes,
            ownership_checking::OwnershipBindingKind,
            parser::{Expression, VariableKind},
            type_checking::UnitAggregateProjectionKind,
        };
        let errors = self.diagnostics.len();
        let deferred = self.primitive_deferred.len();
        let place = self.place(receiver)?;
        let borrowed_root = place.as_ref().is_some_and(|place| {
            place.is_root()
                && (self
                    .bindings
                    .get(&place.root())
                    .is_some_and(|binding| binding.kind() != OwnershipBindingKind::Owned)
                    || state.non_owning.contains_key(&place.root()))
        });
        let local_root = place.as_ref().is_some_and(|place| {
            place.is_root()
                && self
                    .names
                    .names()
                    .source_units()
                    .get(place.root().source_unit().index())
                    .is_some_and(|source| {
                        crate::ownership_checking::ownership_primitive::is_local_variable(
                            source.resolution(),
                            place.root().symbol(),
                        )
                    })
                && matches!(
                    self.variable_kinds.get(&place.root()),
                    Some(VariableKind::Val | VariableKind::Var)
                )
                && !self.bindings.contains_key(&place.root())
                && !state.non_owning.contains_key(&place.root())
        });
        let mut core = receiver;
        while let Expression::Group { expression } =
            self.parsed.ast().expressions().get(core)?.payload()
        {
            core = *expression;
        }
        // unit consume_place 在 access 校验后也拒绝裸 Field 根，不提交该根的 move。
        let rejected_projection = if let Some(place) = &place {
            (place.element().is_none()
                && (self.symbol_kind(place.root())
                    == Some(crate::name_resolution::SymbolKind::Field)
                    || (matches!(
                        self.parsed.ast().expressions().get(core)?.payload(),
                        Expression::Member { .. }
                    ) && self
                        .typed
                        .aggregate_projection(self.unit_expression(core))
                        .is_some_and(|projection| {
                            projection.kind() == UnitAggregateProjectionKind::Field
                        }))))
                || self.element_place_descriptor(core)?.is_some()
        } else {
            false
        };
        if local_root || borrowed_root || rejected_projection {
            let flows = self.check_expression(
                receiver,
                state,
                ExpressionUse::Consume {
                    parameter_span: None,
                },
            )?;
            let admitted = local_root
                && self.diagnostics.len() == errors
                && self.primitive_deferred.len() == deferred
                && flows.next.is_some();
            return Ok((flows, admitted));
        }
        let mut flows = self.check_expression(
            receiver,
            state,
            ExpressionUse::Place {
                parameter_span: None,
            },
        )?;
        if self.diagnostics.len() != errors || self.primitive_deferred.len() != deferred {
            return Ok((flows, false));
        }
        let Some(next) = flows.next.as_mut() else {
            return Ok((flows, false));
        };
        let span = self.parsed.ast().expressions().get(receiver)?.span();
        if let Some(place) = &place
            && (!self.access_place(place, AccessKind::Move, span, None, next)?
                || self.diagnostics.len() != errors)
        {
            return Ok((flows, false));
        }
        self.diagnostics.push(Diagnostic::new(
            self.sources,
            Severity::Error,
            codes::catalog()?.resolve(codes::INVALID_CONTAINER_MEMBER)?,
            "MutableMap.consume requires an owned local val/var root",
            span,
        )?);
        Ok((flows, false))
    }
}
