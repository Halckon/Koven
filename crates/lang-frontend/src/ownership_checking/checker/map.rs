use std::collections::BTreeMap;

use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity, codes},
    parser::{AssignmentOperator, Expression, ParsedFile, VariableKind},
    type_checking::{
        CallReceiverDescriptor, CallReceiverOrigin, ExpressionCategory, MapPutDescriptor,
        ParameterMode, TypedFile,
    },
};

use super::{AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State};

pub(super) fn populate_map_call_contracts(
    parsed: &ParsedFile,
    typed: &TypedFile,
    receivers_by_expression: &mut BTreeMap<usize, CallReceiverDescriptor>,
    calls_by_expression: &mut BTreeMap<usize, Vec<ParameterMode>>,
) {
    for construction in typed.map_constructions() {
        calls_by_expression
            .entry(construction.expression().index())
            .or_default();
    }

    for consume in typed.map_consumes() {
        receivers_by_expression.insert(
            consume.expression().index(),
            CallReceiverDescriptor {
                origin: CallReceiverOrigin::Expression(consume.receiver()),
                mode: ParameterMode::Value,
                category: typed
                    .expression_category(consume.receiver())
                    .unwrap_or(ExpressionCategory::Place),
                ty: consume.receiver_type(),
            },
        );
        calls_by_expression
            .entry(consume.expression().index())
            .or_default();
    }

    for contains in typed.map_contains_calls() {
        let receiver = contains.receiver();
        let receiver_ty = typed
            .expression_type(receiver)
            .expect("typed map contains receiver type");
        receivers_by_expression.insert(
            contains.expression().index(),
            CallReceiverDescriptor {
                origin: CallReceiverOrigin::Expression(receiver),
                mode: ParameterMode::Borrow,
                category: typed
                    .expression_category(receiver)
                    .unwrap_or(ExpressionCategory::Place),
                ty: receiver_ty,
            },
        );
        calls_by_expression
            .entry(contains.expression().index())
            .or_insert_with(|| vec![ParameterMode::Borrow]);
    }

    for require in typed.map_require_values() {
        let receiver = require.receiver();
        let receiver_ty = typed
            .expression_type(receiver)
            .expect("typed map require receiver type");
        receivers_by_expression.insert(
            require.expression().index(),
            CallReceiverDescriptor {
                origin: CallReceiverOrigin::Expression(receiver),
                mode: ParameterMode::Borrow,
                category: typed
                    .expression_category(receiver)
                    .unwrap_or(ExpressionCategory::Place),
                ty: receiver_ty,
            },
        );
        calls_by_expression
            .entry(require.expression().index())
            .or_insert_with(|| vec![ParameterMode::Borrow]);
    }
    for require in typed.map_with_values() {
        let receiver = require.receiver();
        let receiver_ty = typed
            .expression_type(receiver)
            .expect("typed map require receiver type");
        receivers_by_expression.insert(
            require.expression().index(),
            CallReceiverDescriptor {
                origin: CallReceiverOrigin::Expression(receiver),
                mode: ParameterMode::Borrow,
                category: typed
                    .expression_category(receiver)
                    .unwrap_or(ExpressionCategory::Place),
                ty: receiver_ty,
            },
        );
        calls_by_expression
            .entry(require.expression().index())
            .or_insert_with(|| vec![ParameterMode::Borrow, ParameterMode::Borrow]);
    }

    for get in typed.map_gets() {
        let Ok(node) = parsed.ast().expressions().get(get.expression()) else {
            continue;
        };
        if matches!(node.payload(), Expression::Call { .. }) {
            let receiver = get.receiver();
            let receiver_ty = typed
                .expression_type(receiver)
                .expect("typed map get receiver type");
            receivers_by_expression.insert(
                get.expression().index(),
                CallReceiverDescriptor {
                    origin: CallReceiverOrigin::Expression(receiver),
                    mode: ParameterMode::Borrow,
                    category: typed
                        .expression_category(receiver)
                        .unwrap_or(ExpressionCategory::Place),
                    ty: receiver_ty,
                },
            );
            calls_by_expression
                .entry(get.expression().index())
                .or_insert_with(|| vec![ParameterMode::Borrow]);
        }
    }

    for put in typed.map_puts() {
        let Ok(node) = parsed.ast().expressions().get(put.expression()) else {
            continue;
        };
        if matches!(node.payload(), Expression::Call { .. }) {
            let receiver = put.receiver();
            let receiver_ty = typed
                .expression_type(receiver)
                .expect("typed map put receiver type");
            receivers_by_expression.insert(
                put.expression().index(),
                CallReceiverDescriptor {
                    origin: CallReceiverOrigin::Expression(receiver),
                    mode: ParameterMode::Inout,
                    category: typed
                        .expression_category(receiver)
                        .unwrap_or(ExpressionCategory::Place),
                    ty: receiver_ty,
                },
            );
            calls_by_expression
                .entry(put.expression().index())
                .or_insert_with(|| vec![ParameterMode::Value, ParameterMode::Value]);
        }
    }

    for remove in typed.map_removes() {
        let receiver = remove.receiver();
        let receiver_ty = typed
            .expression_type(receiver)
            .expect("typed map remove receiver type");
        receivers_by_expression.insert(
            remove.expression().index(),
            CallReceiverDescriptor {
                origin: CallReceiverOrigin::Expression(receiver),
                mode: ParameterMode::Inout,
                category: typed
                    .expression_category(receiver)
                    .unwrap_or(ExpressionCategory::Place),
                ty: receiver_ty,
            },
        );
        calls_by_expression
            .entry(remove.expression().index())
            .or_insert_with(|| vec![ParameterMode::Borrow]);
    }
}

impl Checker<'_> {
    pub(super) fn check_map_index(
        &mut self,
        receiver: ExpressionId,
        index: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let flows = self.check_expression(receiver, state, ExpressionUse::Read)?;
        self.chain_expression(flows, index, ExpressionUse::Read)
    }

    pub(super) fn check_map_assignment(
        &mut self,
        put: &MapPutDescriptor,
        target: ExpressionId,
        _operator: AssignmentOperator,
        value: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let diagnostic_count = self.diagnostics.len();
        let flows = self.check_expression(put.receiver(), state, ExpressionUse::Place)?;
        let mut flows = self.chain_escaping_expression(flows, put.key(), ExpressionUse::Consume)?;
        flows = self.chain_escaping_expression(flows, value, ExpressionUse::Consume)?;
        let Some(state) = flows.next.as_mut() else {
            return Ok(flows);
        };
        if self.diagnostics.len() != diagnostic_count {
            return Ok(flows);
        }
        let Some(place) = self.place(put.receiver())? else {
            return Ok(flows);
        };
        let target_span = self.parsed.ast().expressions().get(target)?.span();
        if !self.is_mutable_place(put.receiver())? {
            self.diagnostics.push(Diagnostic::new(
                self.sources,
                Severity::Error,
                self.immutable_inout_code,
                "map update requires a mutable receiver place",
                target_span,
            )?);
            return Ok(flows);
        }
        if let Some(origin) = state.moved.get(&place.root()).copied() {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.use_after_move_code,
                "cannot update map after its owner was moved",
                target_span,
            )?;
            diagnostic.add_label(self.sources, origin, "map owner was moved here")?;
            self.diagnostics.push(diagnostic);
            return Ok(flows);
        }
        self.access_place(&place, AccessKind::Mutation, false, target_span, state)?;
        Ok(flows)
    }
}

impl Checker<'_> {
    /// 只改变 consume 外层 receiver 的交付选择；真实子树仍检查一次。
    pub(super) fn check_map_consume_receiver(
        &mut self,
        receiver: ExpressionId,
        state: State,
    ) -> Result<(Flows, bool), OwnershipCheckingError> {
        let errors = self.diagnostics.len();
        let deferred = self.deferred.len();
        let place = self.place(receiver)?;
        let borrowed_root = place.as_ref().is_some_and(|place| {
            place.is_root()
                && (matches!(
                    self.typed.parameter_mode(place.root()),
                    Some(ParameterMode::Borrow | ParameterMode::Inout)
                ) || state.non_owning.contains_key(&place.root()))
        });
        let local_root = place.as_ref().is_some_and(|place| {
            place.is_root()
                && crate::ownership_checking::ownership_primitive::is_local_variable(
                    self.names,
                    place.root(),
                )
                && matches!(
                    self.variable_kinds.get(&place.root()),
                    Some(VariableKind::Val | VariableKind::Var)
                )
                && self.typed.parameter_mode(place.root()).is_none()
                && !state.non_owning.contains_key(&place.root())
        });
        let mut core = receiver;
        while let Expression::Group { expression } =
            self.parsed.ast().expressions().get(core)?.payload()
        {
            core = *expression;
        }
        let rejected_projection = if let Some(place) = &place {
            (place.elements().is_empty()
                && !place.fields().is_empty()
                && self
                    .typed
                    .aggregate_projection(core)
                    .is_some_and(|projection| {
                        projection.kind() == crate::type_checking::AggregateProjectionKind::Field
                    }))
                || self.element_place_descriptor(core)?.is_some()
        } else {
            false
        };
        if local_root || borrowed_root || rejected_projection {
            // Borrow/field/element paths already reject without committing the outer root move.
            let flows = self.check_expression(receiver, state, ExpressionUse::Consume)?;
            let admitted = local_root
                && self.diagnostics.len() == errors
                && self.deferred.len() == deferred
                && flows.next.is_some();
            return Ok((flows, admitted));
        }

        // Extra eligibility failures must not first execute Value/Consume and then undo it.
        let mut flows = self.check_expression(receiver, state, ExpressionUse::Place)?;
        if self.diagnostics.len() != errors || self.deferred.len() != deferred {
            return Ok((flows, false));
        }
        let Some(next) = flows.next.as_mut() else {
            return Ok((flows, false));
        };
        let span = self.parsed.ast().expressions().get(receiver)?.span();
        if let Some(place) = &place {
            // access_place(Move) retires this view. Restore only that field, after the
            // real subtree has run; its other state changes and exits remain untouched.
            let view = next.nullable_views.get(&place.root()).cloned();
            let accessed = self.access_place(place, AccessKind::Move, true, span, next);
            match view {
                Some(view) => {
                    next.nullable_views.insert(place.root(), view);
                }
                None => {
                    next.nullable_views.remove(&place.root());
                }
            }
            if !accessed? || self.diagnostics.len() != errors {
                return Ok((flows, false));
            }
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
