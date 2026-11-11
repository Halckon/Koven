//! Preserve actual parameter-source tokens until a verified wrapper return executes.
use super::{DropPlanner, PlannerDropPoint, ValueState};
use crate::{
    ast::{ExpressionId, ItemId},
    name_resolution::{SymbolKind, UnitSymbolId},
    ownership_checking::{
        LoanKind, OwnershipBindingKind, UnitBorrowForwardTransportFact,
        UnitIterationCleanupAction as Action, UnitLoanFact, UnitLoanTarget,
    },
    parser::{Expression, FunctionBody, FunctionForm, Item},
    type_checking::{CallableResultSource, UnitCallTarget, UnitExpressionId, UnitItemId},
};

/// Created once at the actual narrow expression-body entry, never inferred at finish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ForwardScope {
    pub(super) region: Option<usize>,
    pub(super) callable: UnitItemId,
    pub(super) body: UnitExpressionId,
    pub(super) control: UnitExpressionId,
    pub(super) branch: Option<crate::ownership_checking::UnitLoanForwardBranch>,
    pub(super) branch_entry: Option<usize>,
    pub(super) argument: UnitExpressionId,
    pub(super) root: UnitSymbolId,
}

#[derive(Clone, Debug)]
pub(super) struct ForwardCause {
    pub(super) acquisition: super::action_source::ActionHandle,
    pub(super) scope: ForwardScope,
}

#[derive(Clone, Debug)]
pub(super) struct PendingForwardTransport {
    returned: UnitExpressionId,
    source: UnitLoanFact,
    cause: Option<ForwardCause>,
    nested_cause: Option<super::action_source::NestedForwardCause>,
}

// Metadata cannot change the old semantic retain qualification.
impl PartialEq for PendingForwardTransport {
    fn eq(&self, other: &Self) -> bool {
        self.returned == other.returned && self.source == other.source
    }
}
impl Eq for PendingForwardTransport {}
impl PendingForwardTransport {
    pub(super) fn reconcile(&mut self, alternatives: &[Self]) {
        let mut nested = alternatives.iter().filter(|other| *other == self);
        let same_nested = nested
            .next()
            .and_then(|other| self.nested_cause.as_ref().zip(other.nested_cause.as_ref()))
            .is_some_and(|(a, b)| a.state == b.state && a.acquisition.same_record(&b.acquisition))
            && nested.next().is_none();
        if !same_nested {
            self.nested_cause = None;
        }

        let mut matching = alternatives.iter().filter(|other| *other == self);
        let same = matching
            .next()
            .and_then(|other| self.cause.as_ref().zip(other.cause.as_ref()))
            .is_some_and(|(left, right)| {
                left.scope == right.scope && left.acquisition.same_record(&right.acquisition)
            })
            && matching.next().is_none();
        if !same {
            self.cause = None;
        }
    }

    pub(super) fn reconcile_nested_scope(
        &mut self,
        scope: Option<super::nested_forward_scope::NestedForwardState>,
    ) {
        if !self
            .nested_cause
            .as_ref()
            .is_some_and(|c| Some(c.state) == scope)
        {
            self.nested_cause = None;
        }
    }

    pub(super) fn matches_nested(
        &self,
        state: super::nested_forward_scope::NestedForwardState,
    ) -> bool {
        self.nested_cause
            .as_ref()
            .is_some_and(|cause| cause.state == state)
            && state
                .leaf
                .and_then(|i| state.plan.leaves.get(i))
                .is_some_and(|leaf| leaf.value == self.returned)
    }
    pub(super) fn matches_region(&self, scope: ForwardScope) -> bool {
        self.returned == scope.body
            && self
                .cause
                .as_ref()
                .is_some_and(|cause| cause.scope == scope)
    }

    pub(super) fn reconcile_scope(&mut self, scope: Option<ForwardScope>) {
        if !self
            .cause
            .as_ref()
            .is_some_and(|cause| Some(cause.scope) == scope)
        {
            self.cause = None;
        }
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn forward_expression_scope(
        &self,
        item: ItemId,
        expression: ExpressionId,
    ) -> Option<ForwardScope> {
        if !self.iteration_actions.enabled || !self.checker.parsed.roots().contains(&item) {
            return None;
        }
        let ast = self.checker.parsed.ast();
        let Item::Function {
            name,
            parameters,
            form,
            ..
        } = ast.items().get(item).ok()?.payload()
        else {
            return None;
        };
        let FunctionForm::Explicit {
            body: FunctionBody::Expression {
                expression: body, ..
            },
            ..
        } = form
        else {
            return None;
        };
        let [parameter] = parameters.as_slice() else {
            return None;
        };
        if *body != expression || self.checker.receiver_context(*name).is_some() {
            return None;
        }
        let Expression::Call {
            callee,
            type_arguments,
            arguments,
            ..
        } = ast.expressions().get(expression).ok()?.payload()
        else {
            return None;
        };
        let [argument] = arguments.as_slice() else {
            return None;
        };
        if !type_arguments.is_empty()
            || argument.named_prefix.is_some()
            || argument.mode_marker.is_some()
            || !matches!(
                ast.expressions().get(*callee).ok()?.payload(),
                Expression::Name
            )
            || !matches!(
                ast.expressions().get(argument.value).ok()?.payload(),
                Expression::Name
            )
        {
            return None;
        }
        let body = self.checker.unit_expression(expression);
        let argument = self.checker.unit_expression(argument.value);
        let call = self.checker.typed.call(body)?;
        if call.receiver().is_some()
            || call.aborts()
            || !matches!(call.target(), UnitCallTarget::Declaration(_))
            || !matches!(call.result_source(), CallableResultSource::Borrow(_))
            || !call.instance().type_arguments().is_empty()
        {
            return None;
        }
        let (forwarded, origin) = self.forwarded_borrow_source(body)?;
        let UnitLoanTarget::Place(place) = origin.origin() else {
            return None;
        };
        let root = place.root();
        if origin.expression() != body
            || forwarded.call() != body
            || forwarded.argument() != argument
            || !place.is_root()
            || root.source_unit() != self.checker.source_unit
            || self.checker.marker_symbol(parameter.name) != Some(&root)
            || self.checker.symbol_kind(root) != Some(SymbolKind::ValueParameter)
            || !self
                .checker
                .bindings
                .get(&root)
                .is_some_and(|binding| binding.kind() == OwnershipBindingKind::Shared)
            || !self
                .checker
                .place(argument.expression())
                .is_ok_and(|actual| actual.as_ref() == Some(place))
        {
            return None;
        }
        Some(ForwardScope {
            region: None,
            callable: UnitItemId::new(self.checker.source_unit, item),
            body,
            control: body,
            branch: None,
            branch_entry: None,
            argument,
            root,
        })
    }

    pub(super) fn transport_borrow_forward_source(&self, id: ExpressionId, state: &mut ValueState) {
        let source_call = self.checker.unit_expression(id);
        let Some(call) = self.checker.typed.call(source_call) else {
            return;
        };
        if call.receiver().is_some()
            || !matches!(call.target(), UnitCallTarget::Declaration(_))
            || !matches!(call.result_source(), CallableResultSource::Borrow(_))
        {
            return;
        }
        let Some((forwarded, origin)) = self.forwarded_borrow_source(source_call) else {
            return;
        };
        let UnitLoanTarget::Place(place) = origin.origin() else {
            return;
        };
        // A borrowed parameter has no local owner to release. Do not generalize this
        // handoff to local aliases, fields, receiver roots or exclusive sources.
        // An unrelated active provider does not own this parameter token; an
        // overlapping active provider still remains outside this observation.
        if !place.is_root()
            || state.iterations.iter().any(|frame| {
                frame.source_root.is_some_and(|provider| {
                    provider == place.root()
                        || self
                            .checker
                            .borrow_results
                            .bindings()
                            .iter()
                            .any(|binding| {
                                // The checker already canonicalized this stable alias;
                                // field aliases protect the complete source root too.
                                binding.binding() == provider
                                    && matches!(binding.origin(), UnitLoanTarget::Place(origin)
                                        if origin.root() == place.root())
                            })
                })
            })
            || self.checker.symbol_kind(place.root()) != Some(SymbolKind::ValueParameter)
            || !self
                .checker
                .bindings
                .get(&place.root())
                .is_some_and(|binding| binding.kind() == OwnershipBindingKind::Shared)
            || state.borrow_bindings.contains(&place.root())
        {
            return;
        }
        let Some(index) = state.pending_loans.iter().position(|super::model::PendingLoan(action, _, _)| {
            matches!(action, Action::EndCallLoan(loan)
                if loan.call() == forwarded.call()
                && loan.argument() == forwarded.argument()
                && loan.target() == origin.origin()
                && loan.kind() == LoanKind::Shared
                && self.checker.place(loan.argument().expression()).is_ok_and(|actual| actual.as_ref() == Some(place)))
        }) else { return; };
        // Remove the real token before the call-local retain can discard it. The
        // verified descriptor selects the token but never manufactures a transfer.
        let super::model::PendingLoan(Action::EndCallLoan(source), _, acquisition) =
            state.pending_loans.remove(index)
        else {
            unreachable!("matched actual source token")
        };
        state
            .borrow_forward_transports
            .push(PendingForwardTransport {
                returned: origin.expression(),
                source,
                nested_cause: acquisition
                    .clone()
                    .zip(state.nested_forward.filter(|nested| {
                        nested
                            .leaf
                            .and_then(|i| nested.plan.leaves.get(i))
                            .is_some_and(|leaf| {
                                leaf.value == source_call
                                    && leaf.argument == forwarded.argument()
                                    && nested.plan.root == place.root()
                            })
                    }))
                    .map(
                        |(acquisition, state)| super::action_source::NestedForwardCause {
                            acquisition,
                            state,
                        },
                    ),
                cause: acquisition
                    .zip(state.forward_scope.filter(|scope| {
                        scope.body == source_call
                            && scope.argument == forwarded.argument()
                            && scope.root == place.root()
                    }))
                    .map(|(acquisition, scope)| ForwardCause { acquisition, scope }),
            });
    }

    pub(super) fn finish_borrow_forward_transport(
        &mut self,
        point: PlannerDropPoint,
        state: &mut ValueState,
    ) {
        let PlannerDropPoint::ControlTransfer(expression) = point else {
            return;
        };
        let Ok(node) = self.checker.parsed.ast().expressions().get(expression) else {
            return;
        };
        let returned = match node.payload() {
            Expression::Return {
                value: Some(value), ..
            } => self.checker.unit_expression(*value),
            Expression::Return { value: None, .. } => return,
            // Expression-bodied functions use their result expression as the transfer point.
            _ => self.checker.unit_expression(expression),
        };
        let Some(index) = state
            .borrow_forward_transports
            .iter()
            .position(|pending| pending.returned == returned)
        else {
            return;
        };
        let PendingForwardTransport {
            returned,
            source,
            cause,
            nested_cause,
        } = state.borrow_forward_transports.remove(index);
        // Consume the transport state, not the live source loan. Ownership of the
        // token passes to the wrapper result; no loan-end or cleanup action is added.
        // Every old family still publishes, including observer-off and unsupported scopes.
        self.iteration_actions.forward_handoff(
            UnitBorrowForwardTransportFact {
                source,
                returned,
                point: point.into_unit(self.checker.source_unit),
            },
            cause,
            state.forward_scope,
        );
        self.iteration_actions
            .record_nested_forward_handoff(nested_cause, state.nested_forward);
    }
}

#[cfg(test)]
mod tests;
