use std::collections::BTreeMap;

use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity},
    parser::{AssignmentOperator, Expression, ParsedFile},
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
