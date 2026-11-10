mod action_source;
mod assignment;
mod binding;
mod borrow_binding_transport;
mod borrow_call_handoff;
mod borrow_forward_transport;
mod borrow_result;
mod borrow_transport;
mod call_loan_end;
mod control;
mod iteration;
mod lambda;
mod loan_named_region;
mod loan_provenance;
mod loan_receiver_region;
mod loan_region;
mod map;
mod model;
mod nullable_comparison;
mod owner_transport;
mod pending_call;
mod receiver_loan_end;
mod resource;
mod short_circuit;
mod string;
mod string_when;

use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::UnitSymbolId,
    parser::{BinaryOperator, Expression, FunctionBody, FunctionForm, Item, Statement, StringPart},
    type_checking::{
        BuiltinType, Copyability, DestructuringMode, ExpressionCategory, NominalKind,
        ParameterMode, UnitCallReceiverOrigin, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

use crate::ownership_checking::{
    ClosureCaptureEffect, ClosureCaptureMode, OwnershipDeferredReason, UnitClosureCaptureSource,
    UnitConditionalReceiverDropFact, UnitDropFact, UnitOwnershipDeferredFact,
};

use super::{Checker, OwnershipCheckingError, UnitCallArgumentOwnershipKind, liveness, span_key};
use model::{
    DropExpressionUse, OwnedThis, OwnedValue, PlannerConditionalReceiverDropFact, PlannerDropFact,
    PlannerDropPoint, PlannerDropTarget, StringOperandDrop, ValueState, marker_span,
    merge_value_states,
};

#[derive(Default, Debug, PartialEq, Eq)]
pub(super) struct Analysis {
    pub(super) iteration_conditional_scopes:
        Vec<(UnitConditionalReceiverDropFact, Vec<UnitStatementId>)>,
    pub(super) iteration_owner_scopes: Vec<(
        UnitStatementId,
        crate::ownership_checking::UnitDropTarget,
        bool,
    )>,
    pub(super) iteration_loan_ends: Vec<(
        crate::ownership_checking::UnitDropPoint,
        crate::ownership_checking::UnitIterationCleanupAction,
    )>,
    pub(super) iterations: Vec<crate::ownership_checking::UnitIterationOwnershipPlan>,
    pub(super) map_iterations: Vec<crate::ownership_checking::UnitMapIterationOwnershipPlan>,
    pub(super) iteration_required_exits: Vec<(
        UnitStatementId,
        crate::ownership_checking::UnitIterationExitKind,
        crate::ownership_checking::UnitDropPoint,
    )>,
    pub(super) borrow_ends: Vec<
        crate::ownership_checking::BorrowBindingEndFact<
            UnitSymbolId,
            crate::ownership_checking::UnitDropPoint,
        >,
    >,
    pub(super) owner_observations: Vec<crate::ownership_checking::UnitOwnerObservation>,
    pub(super) receiver_call_prefix_ends:
        Vec<crate::ownership_checking::UnitReceiverCallPrefixEndFact>,
    pub(super) receiver_call_return_ends:
        Vec<crate::ownership_checking::UnitReceiverCallReturnEndFact>,
    pub(super) exclusive_call_prefix_ends:
        Vec<crate::ownership_checking::UnitExclusiveCallPrefixEndFact>,
    pub(super) borrow_call_prefix_ends: Vec<crate::ownership_checking::UnitBorrowCallPrefixEndFact>,
    pub(super) borrow_call_handoffs: Vec<crate::ownership_checking::UnitBorrowCallHandoffFact>,
    pub(super) borrow_call_transports: Vec<crate::ownership_checking::UnitBorrowCallTransportFact>,
    pub(super) borrow_binding_transports:
        Vec<crate::ownership_checking::UnitBorrowBindingTransportFact>,
    pub(super) borrow_forward_transports:
        Vec<crate::ownership_checking::UnitBorrowForwardTransportFact>,
    pub(super) loan_action_observations: Vec<crate::ownership_checking::UnitLoanActionObservation>,
    pub(super) direct_call_loan_ends: Vec<crate::ownership_checking::UnitDirectCallLoanEndFact>,
    pub(super) string_clone_loan_ends: Vec<crate::ownership_checking::UnitStringCloneLoanEndFact>,
    pub(super) drops: Vec<UnitDropFact>,
    pub(super) conditional_receiver_drops: Vec<UnitConditionalReceiverDropFact>,
    pub(super) deferred: Vec<UnitOwnershipDeferredFact>,
}

pub(super) fn plan(checker: &Checker<'_>) -> Result<Analysis, OwnershipCheckingError> {
    plan_with_owner_observation(checker, true)
}

fn plan_with_owner_observation(
    checker: &Checker<'_>,
    enabled: bool,
) -> Result<Analysis, OwnershipCheckingError> {
    plan_with_observations(checker, enabled, true)
}

fn plan_with_observations(
    checker: &Checker<'_>,
    enabled: bool,
    loan_actions_enabled: bool,
) -> Result<Analysis, OwnershipCheckingError> {
    let liveness = liveness::build(checker, true)?;
    let mut deferred: Vec<_> = liveness
        .deferred
        .iter()
        .copied()
        .map(|expression| {
            UnitOwnershipDeferredFact::new(expression, OwnershipDeferredReason::IndexPlace)
        })
        .collect();
    let mut planner = DropPlanner::new(checker, liveness);
    planner.owner_observation_enabled = enabled;
    planner.iteration_actions.enabled = loan_actions_enabled;
    let planner = planner.run()?;
    let iterations = planner.iteration_plans()?;
    let map_iterations = planner.map_iteration_plans()?;
    let mut iteration_owner_scopes: Vec<_> = planner
        .iteration_scope_depths
        .iter()
        .flat_map(|(statement, depth)| {
            planner
                .binding_depths
                .iter()
                .map(move |(symbol, binding_depth)| {
                    (
                        *statement,
                        crate::ownership_checking::UnitDropTarget::Named(*symbol),
                        binding_depth > depth,
                    )
                })
        })
        .collect();
    iteration_owner_scopes.extend(planner.iteration_temporary_scopes.iter().copied());
    let iteration_required_exits = planner
        .iteration_exits
        .iter()
        .map(|(owner, kind, point)| (*owner, *kind, point.into_unit(checker.source_unit)))
        .collect();
    let iteration_loan_ends = planner
        .iteration_actions
        .iter()
        .filter(|(_, action)| {
            matches!(
                action,
                crate::ownership_checking::UnitIterationCleanupAction::EndCallLoan(_)
                    | crate::ownership_checking::UnitIterationCleanupAction::EndReceiverLoan(_)
                    | crate::ownership_checking::UnitIterationCleanupAction::EndCaptureLoan { .. }
            )
        })
        .map(|(point, action)| (point.into_unit(checker.source_unit), action.clone()))
        .collect();
    deferred.extend(planner.resource_deferred.iter().copied());
    if !planner.resource_deferred.is_empty() {
        return Ok(Analysis {
            owner_observations: planner.owner_observations,
            deferred,
            ..Analysis::default()
        });
    }
    let direct_call_loan_ends = planner.direct_call_loan_ends();
    let borrow_call_prefix_ends = planner.borrow_call_prefix_ends();
    let exclusive_call_prefix_ends = planner.exclusive_call_prefix_ends();
    let receiver_call_prefix_ends = planner.receiver_call_prefix_ends();
    let receiver_call_return_ends = planner.iteration_actions.receiver_return_ends();
    let borrow_call_handoffs = planner.iteration_actions.borrow_handoffs();
    let borrow_ends = planner.iteration_actions.borrow_binding_ends();
    let borrow_call_transports = planner.iteration_actions.consumer_transports();
    let drops = planner
        .facts
        .into_iter()
        .map(|fact| fact.into_unit(checker.source_unit))
        .collect();
    let conditional_receiver_drops = planner
        .conditional_receiver_facts
        .into_iter()
        .map(|fact| fact.into_unit(checker.source_unit))
        .collect();
    Ok(Analysis {
        owner_observations: planner.owner_observations,
        loan_action_observations: planner
            .iteration_actions
            .finish(checker.source_unit, checker.parsed.source_id()),
        direct_call_loan_ends,
        string_clone_loan_ends: planner.string_clone_loan_ends,
        borrow_call_prefix_ends,
        borrow_call_handoffs,
        borrow_call_transports,
        borrow_binding_transports: planner.borrow_binding_transports,
        borrow_forward_transports: planner.borrow_forward_transports,
        exclusive_call_prefix_ends,
        receiver_call_prefix_ends,
        receiver_call_return_ends,
        borrow_ends,
        iteration_conditional_scopes: planner.iteration_conditional_scopes,
        iteration_owner_scopes,
        iteration_loan_ends,
        iterations,
        map_iterations,
        iteration_required_exits,
        drops,
        conditional_receiver_drops,
        deferred,
    })
}

struct DropPlanner<'a, 'checker> {
    #[cfg(test)]
    owner_hook_probe: Option<Vec<crate::ownership_checking::UnitOwnerTransportEvent>>,
    owner_observation_enabled: bool,
    owner_choice: Option<owner_transport::choice::ChoiceTransaction>,
    owner_loop: Option<owner_transport::loops::LoopTransaction>,
    owner_branch_depth: usize,
    owner_observation: Option<crate::ownership_checking::UnitOwnerObservation>,
    owner_observations: Vec<crate::ownership_checking::UnitOwnerObservation>,
    string_clone_loan_ends: Vec<crate::ownership_checking::UnitStringCloneLoanEndFact>,
    iteration_conditional_scopes: Vec<(UnitConditionalReceiverDropFact, Vec<UnitStatementId>)>,
    iteration_temporary_scopes: Vec<(
        UnitStatementId,
        crate::ownership_checking::UnitDropTarget,
        bool,
    )>,
    iteration_scope_depths: BTreeMap<UnitStatementId, usize>,
    borrow_binding_transports: Vec<crate::ownership_checking::UnitBorrowBindingTransportFact>,
    borrow_forward_transports: Vec<crate::ownership_checking::UnitBorrowForwardTransportFact>,
    pending_borrow_handoffs: Vec<crate::ownership_checking::UnitBorrowCallHandoffFact>,
    borrow_source_unwind_actions: std::collections::BTreeSet<usize>,
    iteration_actions: action_source::ActionSource,
    iteration_exits: Vec<(
        UnitStatementId,
        crate::ownership_checking::UnitIterationExitKind,
        PlannerDropPoint,
    )>,
    planned_iterations: std::collections::BTreeSet<UnitStatementId>,
    resource_deferred: Vec<UnitOwnershipDeferredFact>,
    call_return_copy_source: Option<UnitSymbolId>,
    checker: &'a Checker<'checker>,
    loan_provenance: std::cell::OnceCell<loan_provenance::LoanProvenance>,
    loan_region_index: std::cell::OnceCell<loan_region::AdmissionIndex>,
    borrow_forward_index: std::cell::OnceCell<loan_provenance::BorrowForwardIndex>,
    liveness: liveness::Liveness,
    facts: Vec<PlannerDropFact>,
    conditional_receiver_facts: Vec<PlannerConditionalReceiverDropFact>,
    loop_boundaries: Vec<usize>,
    scope_depth: usize,
    binding_depths: BTreeMap<UnitSymbolId, usize>,
}

impl<'a, 'checker> DropPlanner<'a, 'checker> {
    fn new(checker: &'a Checker<'checker>, liveness: liveness::Liveness) -> Self {
        Self {
            #[cfg(test)]
            owner_hook_probe: None,
            owner_observation_enabled: true,
            owner_choice: None,
            owner_loop: None,
            owner_branch_depth: 0,
            owner_observation: None,
            owner_observations: Vec::new(),
            string_clone_loan_ends: Vec::new(),
            iteration_conditional_scopes: Vec::new(),
            iteration_temporary_scopes: Vec::new(),
            iteration_scope_depths: BTreeMap::new(),
            iteration_actions: action_source::ActionSource::default(),
            borrow_binding_transports: Vec::new(),
            borrow_forward_transports: Vec::new(),
            pending_borrow_handoffs: Vec::new(),
            borrow_source_unwind_actions: std::collections::BTreeSet::new(),
            iteration_exits: Vec::new(),
            planned_iterations: std::collections::BTreeSet::new(),
            resource_deferred: Vec::new(),
            call_return_copy_source: None,
            checker,
            loan_provenance: std::cell::OnceCell::new(),
            loan_region_index: std::cell::OnceCell::new(),
            borrow_forward_index: std::cell::OnceCell::new(),
            liveness,
            facts: Vec::new(),
            conditional_receiver_facts: Vec::new(),
            loop_boundaries: Vec::new(),
            scope_depth: 0,
            binding_depths: BTreeMap::new(),
        }
    }

    fn run(mut self) -> Result<Self, OwnershipCheckingError> {
        for &root in self.checker.parsed.roots() {
            self.item(root)?;
        }
        for (lambda, node) in self.checker.parsed.ast().expressions().iter() {
            let Expression::Lambda {
                opener_span,
                parameters,
                arrow_span,
                body,
                ..
            } = node.payload()
            else {
                continue;
            };
            if !self
                .checker
                .visited_lambdas
                .contains(&self.checker.unit_expression(lambda))
            {
                continue;
            }
            let mut effective_parameters = parameters.clone();
            if arrow_span.is_none()
                && let Some(symbol) = self.checker.symbols_by_span.get(&span_key(*opener_span))
                && self.checker.typed.body_parameter_mode(*symbol).is_some()
            {
                effective_parameters.push(*opener_span);
            }
            self.plan_lambda_body(lambda, &effective_parameters, *body)?;
        }
        Ok(self)
    }

    fn item(&mut self, id: ItemId) -> Result<(), OwnershipCheckingError> {
        match self.checker.parsed.ast().items().get(id)?.payload().clone() {
            Item::Modified { declaration, .. } => self.item(declaration)?,
            Item::Function {
                name,
                parameters,
                form,
                ..
            } => {
                if self.liveness.skipped_functions.contains(&id.index()) {
                    return Ok(());
                }
                self.begin_owner_function(id);
                let mut state = ValueState::default();
                if self.checker.receiver_context(name).is_some() {
                    self.reject_owner_callable(id);
                }
                if let Some(receiver) = self.checker.receiver_context(name)
                    && receiver.mode == ParameterMode::Value
                {
                    let conditional_type = match self.checker.typed.types().get(receiver.ty) {
                        Some(UnitTypeKind::StaticSelf(_)) => Some(receiver.ty),
                        _ => None,
                    };
                    if conditional_type.is_some()
                        || self.checker.typed.copyability(receiver.ty) == Copyability::MoveOnly
                    {
                        state.this = Some(OwnedThis {
                            owner: receiver.owner,
                            origin: receiver.declaration_span,
                            conditional_type,
                        });
                    }
                }
                for parameter in parameters {
                    if let Some(symbol) = self.checker.marker_symbol(parameter.name).copied()
                        && self.checker.is_move_only_variable(symbol)
                    {
                        self.binding_depths.insert(symbol, 0);
                        state.insert(OwnedValue {
                            owner: self.parameter_owner(id, symbol),
                            symbol,
                            origin: marker_span(parameter.name),
                            declaration: marker_span(parameter.name),
                            scope_depth: 0,
                        });
                    }
                }
                let live_in = self
                    .liveness
                    .function_live_in
                    .get(&id.index())
                    .cloned()
                    .unwrap_or_default();
                let has_body = !matches!(
                    form,
                    FunctionForm::ImplicitUnitAbsent
                        | FunctionForm::Explicit {
                            body: FunctionBody::Absent,
                            ..
                        }
                );
                if !has_body {
                    self.reject_owner_callable(id);
                }
                if has_body {
                    let unused_parameters = state
                        .values
                        .iter()
                        .filter(|value| !live_in.contains(&value.symbol))
                        .map(|value| value.symbol)
                        .collect::<Vec<_>>();
                    for symbol in unused_parameters.into_iter().rev() {
                        self.drop_named_asap(
                            PlannerDropPoint::FunctionEntry(id),
                            symbol,
                            &mut state,
                        );
                    }
                }
                match form {
                    FunctionForm::ImplicitUnitAbsent => {}
                    FunctionForm::ImplicitUnitBlock(body) => {
                        if self.statement(body, &mut state)? {
                            self.drop_all(PlannerDropPoint::AfterStatement(body), &mut state);
                        }
                    }
                    FunctionForm::Explicit { body, .. } => match body {
                        FunctionBody::Absent => {}
                        FunctionBody::Expression { expression, .. } => {
                            if self.expression(
                                expression,
                                DropExpressionUse::Consume,
                                &mut state,
                            )? {
                                self.drop_all(
                                    PlannerDropPoint::ControlTransfer(expression),
                                    &mut state,
                                );
                            }
                        }
                        FunctionBody::Block(body) => {
                            if self.statement(body, &mut state)? {
                                self.drop_all(PlannerDropPoint::AfterStatement(body), &mut state);
                            }
                        }
                    },
                }
                self.finish_owner_observation();
            }
            Item::Classifier(classifier) => {
                if let Some(body) = classifier.body {
                    for member in body.members {
                        self.item(member)?;
                    }
                }
            }
            Item::Companion(companion) => {
                for member in companion.body.members {
                    self.item(member)?;
                }
            }
            Item::Deinit { body, .. } => {
                self.begin_unsupported_owner(
                    crate::ownership_checking::UnitOwnerCallable::Deinit(
                        crate::type_checking::UnitItemId::new(self.checker.source_unit, id),
                    ),
                    PlannerDropPoint::FunctionEntry(id).into_unit(self.checker.source_unit),
                );
                let mut state = ValueState::default();
                if self.statement(body, &mut state)? {
                    self.drop_all(PlannerDropPoint::AfterStatement(body), &mut state);
                }
                self.finish_owner_observation();
            }
            Item::Error | Item::Variable { .. } | Item::Constant { .. } => {}
        }
        Ok(())
    }

    fn statement_inner(
        &mut self,
        id: StatementId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        self.check_owner_statement(id, state)?;
        match self
            .checker
            .parsed
            .ast()
            .statements()
            .get(id)?
            .payload()
            .clone()
        {
            Statement::Error => Ok(true),
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => {
                self.scope_depth += 1;
                let frame = self.scope_depth;
                for element in elements {
                    if !self.statement(element, state)? {
                        self.scope_depth -= 1;
                        return Ok(false);
                    }
                }
                self.drop_scope(frame, PlannerDropPoint::AfterStatement(id), state);
                self.scope_depth -= 1;
                Ok(true)
            }
            Statement::LocalVariable { declaration } => self.local_variable(id, declaration, state),
            Statement::LocalDestructuring { initializer, .. } => {
                let usage = self
                    .checker
                    .typed
                    .destructuring(UnitStatementId::new(self.checker.source_unit, id))
                    .map(|descriptor| match descriptor.mode() {
                        DestructuringMode::Copy => DropExpressionUse::Read,
                        DestructuringMode::Consume => DropExpressionUse::Consume,
                    })
                    .unwrap_or(DropExpressionUse::Read);
                self.expression(initializer, usage, state)?;
                if let Some(descriptor) = self
                    .checker
                    .typed
                    .destructuring(UnitStatementId::new(self.checker.source_unit, id))
                {
                    // The typed descriptor follows layout order. Locals are declared in pattern order.
                    let mut bindings = descriptor
                        .components()
                        .iter()
                        .map(|component| {
                            let symbol = component.symbol();
                            Ok((self.checker.symbol_span(symbol)?, symbol))
                        })
                        .collect::<Result<Vec<_>, OwnershipCheckingError>>()?;
                    bindings.sort_unstable_by_key(|(span, _)| span.start());
                    for (origin, symbol) in bindings {
                        if self.checker.is_move_only_variable(symbol) {
                            self.binding_depths.insert(symbol, self.scope_depth);
                            state.insert(OwnedValue {
                                owner: None,
                                symbol,
                                origin,
                                declaration: origin,
                                scope_depth: self.scope_depth,
                            });
                            if !self.liveness.statement_after[id.index()].contains(&symbol) {
                                self.drop_named_asap(
                                    PlannerDropPoint::AfterStatement(id),
                                    symbol,
                                    state,
                                );
                            }
                        }
                    }
                }
                Ok(true)
            }
            Statement::Expression { expression } => {
                let continues = self.expression(expression, DropExpressionUse::Read, state)?;
                if continues && self.is_move_only_temporary(expression) {
                    let span = self
                        .checker
                        .parsed
                        .ast()
                        .expressions()
                        .get(expression)?
                        .span();
                    self.push_fact(PlannerDropFact::new(
                        PlannerDropPoint::AfterExpression(expression),
                        PlannerDropTarget::Temporary(expression),
                        span,
                    ));
                }
                Ok(continues)
            }
            Statement::While {
                condition, body, ..
            } => self.owner_while_statement(id, condition, body, state),
            Statement::For { source, body, .. } => self.iteration(id, source, body, state),
            Statement::Loop { body, .. } => {
                let mut body_state = state.clone();
                self.loop_boundaries.push(self.scope_depth);
                self.statement(body, &mut body_state)?;
                self.loop_boundaries.pop();
                self.drop_loop_exit(id, state);
                Ok(true)
            }
        }
    }

    fn expression_inner(
        &mut self,
        id: ExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
        owner_result: &mut Option<crate::ownership_checking::UnitOwnerId>,
        replace_consumer: &mut Option<owner_transport::replace_consumer::ReplaceConsumerCarrier>,
    ) -> Result<bool, OwnershipCheckingError> {
        if self.checker.is_constant_use(id) {
            return Ok(true);
        }
        if let Some(operation) = self
            .checker
            .typed
            .integer_operation(self.checker.unit_expression(id))
        {
            return self.expression(
                operation.receiver().expression(),
                DropExpressionUse::Read,
                state,
            );
        }
        if let Some(receiver_id) = self
            .checker
            .typed
            .string_operation(self.checker.unit_expression(id))
            .map(|operation| operation.receiver())
            .or_else(|| {
                self.checker
                    .typed
                    .container_size(self.checker.unit_expression(id))
                    .map(|size| size.receiver())
            })
            .or_else(|| {
                self.checker
                    .typed
                    .map_size(self.checker.unit_expression(id))
                    .map(|size| size.receiver())
            })
        {
            let receiver = receiver_id.expression();
            if !self.expression(receiver, DropExpressionUse::Place, state)? {
                return Ok(false);
            }
            self.observe_string_clone_return(id);
            for loan in self.checker.loans.iter().filter(|loan| {
                loan.call() == self.checker.unit_expression(id) && loan.argument() == receiver_id
            }) {
                match loan.target() {
                    crate::ownership_checking::UnitLoanTarget::Place(place) => {
                        if !self.liveness.expression_after[id.index()].contains(&place.root()) {
                            self.drop_named_asap(
                                PlannerDropPoint::CallReturn(id),
                                place.root(),
                                state,
                            );
                        }
                    }
                    // A borrowed receiver binding does not introduce another owner.
                    crate::ownership_checking::UnitLoanTarget::This(_) => {}
                    crate::ownership_checking::UnitLoanTarget::Temporary(owner) => {
                        let owner = owner.expression();
                        self.push_fact(PlannerDropFact::new(
                            PlannerDropPoint::CallReturn(id),
                            PlannerDropTarget::Temporary(owner),
                            self.checker.parsed.ast().expressions().get(owner)?.span(),
                        ));
                    }
                }
            }
            return Ok(true);
        }
        if let Some(operation) = self
            .checker
            .typed
            .rc_operation(self.checker.unit_expression(id))
        {
            if operation.receiver().source_unit() != self.checker.source_unit {
                return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                    source_unit: self.checker.source_unit.index(),
                    expression: id.index(),
                });
            }
            let receiver = operation.receiver().expression();
            if let Some(place) = self.checker.place(receiver)? {
                let root = place.root();
                if !self.liveness.expression_after[id.index()].contains(&root) {
                    self.drop_named_asap(PlannerDropPoint::AfterExpression(id), root, state);
                }
                return Ok(true);
            }
            return self.expression(receiver, DropExpressionUse::Read, state);
        }
        if let Some(descriptor) = self
            .checker
            .construction_descriptors
            .get(&self.checker.unit_expression(id))
            .cloned()
        {
            return self.construction_operands(id, &descriptor, state);
        }
        if let Some(plan) = self.checker.short_circuit_plan(id)? {
            use super::super::constant::UnitShortCircuitRhs;
            if !self.checker.constant_control
                && self
                    .checker
                    .typed
                    .expression_falls_through(self.checker.unit_expression(id))
                    != Some(false)
            {
                return self.short_circuit(id, plan, state);
            }
            if !self.expression(plan.left.expression(), DropExpressionUse::Read, state)? {
                return Ok(false);
            }
            return match plan.rhs {
                UnitShortCircuitRhs::Never => Ok(true),
                UnitShortCircuitRhs::Always => {
                    self.expression(plan.right.expression(), DropExpressionUse::Read, state)
                }
                UnitShortCircuitRhs::Conditional => {
                    let mut skipped = state.clone();
                    self.drop_branch_exit(id, 1 - plan.rhs_branch, &mut skipped);
                    let mut branches = vec![skipped];
                    if self.expression(plan.right.expression(), DropExpressionUse::Read, state)? {
                        self.drop_branch_exit(id, plan.rhs_branch, state);
                        branches.push(state.clone());
                    }
                    *state = self.merge_resource_states(id, branches);
                    Ok(true)
                }
            };
        }
        let usage = if self
            .checker
            .borrow_return_origins
            .iter()
            .any(|fact| fact.expression() == self.checker.unit_expression(id))
            || self.checker.is_range_return_expression(id)
        {
            DropExpressionUse::Read
        } else {
            usage
        };
        let node = self.checker.parsed.ast().expressions().get(id)?;
        match node.payload().clone() {
            Expression::Error | Expression::Literal(_) | Expression::SuperMember { .. } => Ok(true),
            Expression::This => {
                if usage == DropExpressionUse::Consume {
                    state.this = None;
                }
                Ok(true)
            }
            Expression::Lambda { .. } => {
                *owner_result = self.form_capture_owner(id, usage, state);
                Ok(true)
            }
            Expression::Name => {
                if let Some(symbol) = self.checker.reference_symbol(node.span()) {
                    match usage {
                        DropExpressionUse::Consume => {
                            let value = self.take_named(id, symbol, state);
                            *replace_consumer = state.replace_consumer.take().and_then(|carrier| {
                                carrier.take(self.checker.unit_expression(id), value.as_ref())
                            });
                            *owner_result = self.moved_owner(id, value);
                        }
                        DropExpressionUse::Read => {
                            if !self.liveness.expression_after[id.index()].contains(&symbol) {
                                self.drop_named_asap(
                                    PlannerDropPoint::AfterExpression(id),
                                    symbol,
                                    state,
                                );
                            }
                        }
                        DropExpressionUse::Place => {}
                    }
                }
                Ok(true)
            }
            Expression::Group { expression } => {
                let result = self.evaluate_owner_expression(expression, usage, state)?;
                *owner_result = result.owner;
                Ok(result.continues)
            }
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        if !self.expression(expression, DropExpressionUse::Read, state)? {
                            state
                                .pending_temporaries
                                .retain(|pending| pending.control != id);
                            return Ok(false);
                        }
                        if self.is_move_only_temporary(expression) {
                            let origin = self
                                .checker
                                .parsed
                                .ast()
                                .expressions()
                                .get(expression)?
                                .span();
                            self.register_pending_temporary(id, expression, origin, false, state)?;
                        }
                    }
                }
                *owner_result = self.define_string_owner(id);
                // Release only interpolation inputs; the outer String and live named owners remain.
                self.finish_pending_temporaries(id, PlannerDropPoint::AfterExpression(id), state);
                Ok(true)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.owner_if_expression(id, condition, then_branch, else_branch, state),
            Expression::When {
                subject, entries, ..
            } => self.when_expression(id, subject, &entries, state),
            Expression::Return { value, .. } => self.owner_return_expression(id, value, state),
            jump @ (Expression::Break { .. } | Expression::Continue { .. }) => self
                .owner_loop_jump_expression(id, matches!(jump, Expression::Continue { .. }), state),
            Expression::NonNullAssert { operand, .. } => {
                self.expression(operand, DropExpressionUse::Consume, state)
            }
            Expression::Prefix { operand, .. }
            | Expression::Cast {
                expression: operand,
                ..
            }
            | Expression::TypeTest {
                expression: operand,
                ..
            } => self.expression(operand, DropExpressionUse::Read, state),
            Expression::Propagate { value, .. } => {
                self.expression(value, DropExpressionUse::Read, state)?;
                for owned in state.values.iter().rev() {
                    self.push_fact(PlannerDropFact::new(
                        PlannerDropPoint::ControlTransfer(id),
                        PlannerDropTarget::Named(owned.symbol),
                        owned.origin,
                    ));
                }
                if let Some(receiver) = state.this {
                    self.push_this_fact(PlannerDropPoint::ControlTransfer(id), receiver);
                }
                Ok(true)
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } if self.uses_string_binary_operands(left, operator, right) => {
                self.string_binary(left, right, id, state)
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                if !self.expression(left, DropExpressionUse::Read, state)?
                    || !self.expression(right, DropExpressionUse::Read, state)?
                {
                    return Ok(false);
                }
                self.drop_nullable_comparison_temporary(id, left, operator, right)?;
                Ok(true)
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => self.assignment(id, target, operator, value, state),
            Expression::Member { receiver, .. } => {
                if let Some(place) = self.checker.place(id)? {
                    let root = place.root();
                    if usage == DropExpressionUse::Consume && place.is_root() {
                        let _ = self.take_named(id, root, state);
                    } else if usage != DropExpressionUse::Place
                        && !self.liveness.expression_after[id.index()].contains(&root)
                    {
                        self.drop_named_asap(PlannerDropPoint::AfterExpression(id), root, state);
                    }
                    Ok(true)
                } else {
                    let receiver_use = if usage == DropExpressionUse::Place {
                        DropExpressionUse::Place
                    } else {
                        DropExpressionUse::Read
                    };
                    self.expression(receiver, receiver_use, state)
                }
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                let loan_region = self.enter_loan_region(id, state);
                let returns = !self
                    .checker
                    .is_nothing_expression(self.checker.unit_expression(id));
                let receiver_contract = self
                    .checker
                    .receiver_contracts_by_call
                    .get(&self.checker.unit_expression(id))
                    .copied();
                let contracts = self
                    .checker
                    .contracts_by_call
                    .get(&self.checker.unit_expression(id));
                let mut borrowed_roots = Vec::new();
                if let Some(contract) = receiver_contract {
                    match contract.source() {
                        UnitCallReceiverOrigin::Expression(receiver) => {
                            let receiver = receiver.expression();
                            match contract.kind() {
                                UnitCallArgumentOwnershipKind::Value => {
                                    let conditional_static_self =
                                        self.checker.expression_is_this(receiver)?
                                            && matches!(
                                                self.checker
                                                    .typed
                                                    .types()
                                                    .get(contract.receiver_type()),
                                                Some(UnitTypeKind::StaticSelf(_))
                                            );
                                    let usage = if conditional_static_self {
                                        DropExpressionUse::Place
                                    } else {
                                        DropExpressionUse::Consume
                                    };
                                    if !self.expression(receiver, usage, state)? {
                                        return Ok(false);
                                    }
                                    if conditional_static_self {
                                        self.register_pending_this(id, state);
                                    } else {
                                        self.register_value_argument(id, receiver, state)?;
                                    }
                                }
                                UnitCallArgumentOwnershipKind::SharedLoan
                                | UnitCallArgumentOwnershipKind::ExclusiveLoan => {
                                    if let Some(place) = self.checker.place(receiver)? {
                                        if !self.expression(
                                            receiver,
                                            DropExpressionUse::Place,
                                            state,
                                        )? {
                                            return Ok(false);
                                        }
                                        if state.position(place.root()).is_some() {
                                            borrowed_roots.push(place.root());
                                            state.pending_borrows.push((id, place.root()));
                                            self.hold_pending_capture_borrows(
                                                id,
                                                receiver,
                                                Some(place.root()),
                                                state,
                                            );
                                        }
                                    } else {
                                        if !self.expression(
                                            receiver,
                                            DropExpressionUse::Place,
                                            state,
                                        )? {
                                            return Ok(false);
                                        }
                                        if contract.kind()
                                            == UnitCallArgumentOwnershipKind::SharedLoan
                                            && let Some(temporary) = self
                                                .checker
                                                .temporary_expression_origin(receiver)?
                                                .map(UnitExpressionId::expression)
                                                .filter(|temporary| {
                                                    self.is_move_only_temporary(*temporary)
                                                })
                                        {
                                            let origin = self
                                                .checker
                                                .parsed
                                                .ast()
                                                .expressions()
                                                .get(temporary)?
                                                .span();
                                            self.hold_pending_capture_borrows(
                                                id, receiver, None, state,
                                            );
                                            self.register_pending_temporary(
                                                id, temporary, origin, false, state,
                                            )?;
                                        }
                                    }
                                }
                            }
                        }
                        UnitCallReceiverOrigin::ImplicitThis(_) => {
                            if contract.kind() == UnitCallArgumentOwnershipKind::Value {
                                self.register_pending_this(id, state);
                            }
                        }
                    }
                    self.register_iteration_receiver_loan(id, state);
                } else if self
                    .checker
                    .typed
                    .call(self.checker.unit_expression(id))
                    .is_some_and(|call| {
                        call.target() == crate::type_checking::UnitCallTarget::FunctionValue
                    })
                    && let Some(place) = self.checker.place(callee)?
                {
                    if !self.expression(callee, DropExpressionUse::Place, state)? {
                        return Ok(false);
                    }
                    // callee 的最后一次源码读取先于实参，但调用完成前仍需保留其 owner/capture。
                    if state.position(place.root()).is_some() {
                        borrowed_roots.push(place.root());
                        state.pending_borrows.push((id, place.root()));
                        self.hold_pending_capture_borrows(id, callee, Some(place.root()), state);
                    }
                } else {
                    if !self.expression(callee, DropExpressionUse::Read, state)? {
                        return Ok(false);
                    }
                    if self.capture_sources_at_call(id, callee).is_some() {
                        self.register_borrow_temporary_argument(
                            id,
                            callee,
                            UnitCallArgumentOwnershipKind::SharedLoan,
                            state,
                        )?;
                    }
                }
                for (index, argument) in arguments.into_iter().enumerate() {
                    let kind = contracts
                        .and_then(|contracts| contracts.get(index))
                        .map(|contract| contract.kind())
                        .unwrap_or(UnitCallArgumentOwnershipKind::SharedLoan);
                    match kind {
                        UnitCallArgumentOwnershipKind::Value => {
                            let result = self.evaluate_owner_expression(
                                argument.value,
                                DropExpressionUse::Consume,
                                state,
                            )?;
                            if !result.continues {
                                return Ok(false);
                            }
                            self.register_value_argument(id, argument.value, state)?;
                            self.register_pending_identity(id, argument.value, result.owner, state);
                        }
                        UnitCallArgumentOwnershipKind::SharedLoan
                        | UnitCallArgumentOwnershipKind::ExclusiveLoan => {
                            if let Some(place) = self.checker.place(argument.value)? {
                                let root = place.root();
                                if !self.expression(
                                    argument.value,
                                    DropExpressionUse::Place,
                                    state,
                                )? {
                                    return Ok(false);
                                }
                                if state.position(root).is_some() && !borrowed_roots.contains(&root)
                                {
                                    borrowed_roots.push(root);
                                    state.pending_borrows.push((id, root));
                                    self.hold_pending_capture_borrows(
                                        id,
                                        argument.value,
                                        Some(root),
                                        state,
                                    );
                                }
                            } else {
                                if !self.expression(
                                    argument.value,
                                    DropExpressionUse::Place,
                                    state,
                                )? {
                                    return Ok(false);
                                }
                                self.register_borrow_temporary_argument(
                                    id,
                                    argument.value,
                                    kind,
                                    state,
                                )?;
                            }
                        }
                    }
                    if kind != UnitCallArgumentOwnershipKind::Value {
                        self.register_iteration_pending_loans(id, argument.value, state);
                    }
                }
                if !returns {
                    return Ok(false);
                }
                if let Some(plan) = self
                    .checker
                    .ownership_primitives
                    .iter()
                    .find(|plan| plan.descriptor().expression() == self.checker.unit_expression(id))
                    .cloned()
                {
                    *owner_result = self.commit_owner_primitive(id, state);
                    // Both roots remain initialized. Their old owners are returned/swapped,
                    // never destroyed by assignment replacement glue.
                    for place in plan.places() {
                        if let Some(index) = state.position(place.root()) {
                            state.values[index].origin = node.span();
                        }
                    }
                    self.commit_choice_primitive(id, state);
                }
                let copied_source = self.continue_borrow_call_source(id, state);
                self.finish_call_borrows(id, &mut borrowed_roots, state);
                self.finish_direct_call_loans(id, state);
                self.observe_borrow_call_handoff(id, state);
                self.transport_borrow_call_source(id, state);
                self.transport_borrow_binding_source(id, state);
                self.transport_borrow_forward_source(id, state);
                self.finish_borrow_call_transports(id, state);
                self.finish_call_pending_loans(id, state);
                self.continue_range_source(id, state);
                // Closure cleanup can recursively try to drop the borrowed source.
                self.call_return_copy_source = copied_source.map(|(root, _)| root);
                self.finish_pending_temporaries(id, PlannerDropPoint::CallReturn(id), state);
                let mut copied_drop = None;
                for root in borrowed_roots {
                    if !self.liveness.expression_after[id.index()].contains(&root) {
                        if let Some(copy) = copied_source.filter(|(source, _)| *source == root) {
                            copied_drop = Some(copy);
                        } else {
                            self.drop_named_asap(PlannerDropPoint::CallReturn(id), root, state);
                        }
                    }
                }
                self.call_return_copy_source = None;
                if let Some((root, point)) = copied_drop {
                    self.drop_named_asap(point, root, state);
                }
                self.iteration_actions.complete_region(loan_region);
                Ok(true)
            }
            Expression::Index { receiver, index } => {
                if self
                    .checker
                    .typed
                    .map_get(self.checker.unit_expression(id))
                    .is_some()
                {
                    return self.map_subscript(id, state);
                }
                if !self.expression(receiver, DropExpressionUse::Place, state)? {
                    return Ok(false);
                }
                // The backing owner already exists if evaluating the index exits early.
                if let Some(owner) = self.checker.temporary_element_owner(id)? {
                    let origin = self
                        .checker
                        .parsed
                        .ast()
                        .expressions()
                        .get(owner.expression())?
                        .span();
                    self.register_pending_temporary(id, owner.expression(), origin, false, state)?;
                }
                if !self.expression(index, DropExpressionUse::Read, state)? {
                    return Ok(false);
                }
                state
                    .pending_temporaries
                    .retain(|pending| pending.control != id);
                if usage != DropExpressionUse::Place
                    && let Some(temporary) = self
                        .checker
                        .temporary_element_owner(id)?
                        .map(UnitExpressionId::expression)
                {
                    self.push_fact(PlannerDropFact::new(
                        PlannerDropPoint::AfterExpression(id),
                        PlannerDropTarget::Temporary(temporary),
                        self.checker
                            .parsed
                            .ast()
                            .expressions()
                            .get(temporary)?
                            .span(),
                    ));
                } else if usage != DropExpressionUse::Place
                    && let Some(place) = self.checker.place(id)?
                    && !self.liveness.expression_after[id.index()].contains(&place.root())
                {
                    self.drop_named_asap(
                        PlannerDropPoint::AfterExpression(id),
                        place.root(),
                        state,
                    );
                }
                Ok(true)
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.expression(receiver, DropExpressionUse::Read, state)
                } else {
                    Ok(true)
                }
            }
        }
    }

    fn is_move_only_temporary(&self, expression: ExpressionId) -> bool {
        if self
            .checker
            .typed
            .expression_type(self.checker.unit_expression(expression))
            .and_then(|ty| self.checker.typed.types().get(ty))
            .is_some_and(|ty| {
                matches!(
                    ty,
                    UnitTypeKind::Intrinsic {
                        constructor: crate::type_checking::IntrinsicTypeConstructor::View,
                        ..
                    }
                )
            })
        {
            return false;
        }
        let unit = self.checker.unit_expression(expression);
        let Some(ty) = self.checker.typed.expression_type(unit) else {
            return false;
        };
        self.checker.borrow_result_call(expression).is_none()
            && self.checker.typed.expression_category(unit) == Some(ExpressionCategory::Temporary)
            && self.checker.typed.copyability(ty) == Copyability::MoveOnly
            && !self.is_stateless_object_type(ty)
    }

    fn is_stateless_object_type(&self, ty: crate::type_checking::UnitTypeId) -> bool {
        let Some(UnitTypeKind::Nominal { declaration, .. }) = self.checker.typed.types().get(ty)
        else {
            return false;
        };
        self.checker
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .is_some_and(|nominal| nominal.kind() == NominalKind::Object)
    }

    fn drop_named(
        &mut self,
        point: PlannerDropPoint,
        symbol: UnitSymbolId,
        state: &mut ValueState,
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
            self.drop_owned_value(point, value, closure, state);
        }
    }

    fn drop_owned_value(
        &mut self,
        point: PlannerDropPoint,
        value: OwnedValue,
        closure: Option<ExpressionId>,
        state: &mut ValueState,
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
        self.push_fact(PlannerDropFact::new(
            point,
            PlannerDropTarget::Named(value.symbol),
            value.origin,
        ));
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
    fn has_live_shared_capture(&self, state: &ValueState, symbol: UnitSymbolId) -> bool {
        state.closures.values().any(|&closure| {
            self.checker.captures_of(closure).any(|capture| {
                capture.mode() == ClosureCaptureMode::Shared
                    && capture.source() == UnitClosureCaptureSource::Symbol(symbol)
            })
        })
    }

    fn closure_origin(
        &self,
        expression: ExpressionId,
        state: &ValueState,
    ) -> Result<Option<ExpressionId>, OwnershipCheckingError> {
        let node = self.checker.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Lambda { .. } => Ok(Some(expression)),
            Expression::Group { expression } => self.closure_origin(*expression, state),
            Expression::Name => Ok(self
                .checker
                .reference_symbol(node.span())
                .and_then(|symbol| state.closures.get(&symbol).copied())),
            _ => Ok(None),
        }
    }

    fn drop_all(&mut self, point: PlannerDropPoint, state: &mut ValueState) {
        self.finish_borrow_forward_transport(point, state);
        state.pending_borrow_results.clear();
        self.end_borrow_bindings(point, state, |_| true);
        // callable 已退出，不能再由尚未提交的调用前缀阻止 owner 清理。
        state.pending_borrows.clear();
        state.pending_capture_borrows.clear();
        self.drop_pending_temporaries(point, state, |_| true);
        while let Some(symbol) = state.values.last().map(|value| value.symbol) {
            self.drop_named(point, symbol, state);
        }
        if let Some(receiver) = state.this.take() {
            self.push_this_fact(point, receiver);
        }
    }

    fn push_this_fact(&mut self, point: PlannerDropPoint, receiver: OwnedThis) {
        self.push_this_fact_in_iteration_scopes(point, receiver, &[]);
    }

    /// Pending receiver保留形成时所在provider集合，独立于退出动作排列。
    fn push_this_fact_in_iteration_scopes(
        &mut self,
        point: PlannerDropPoint,
        receiver: OwnedThis,
        scopes: &[UnitStatementId],
    ) {
        if let Some(receiver_type) = receiver.conditional_type {
            let fact = PlannerConditionalReceiverDropFact {
                point,
                owner: receiver.owner,
                receiver_type,
                value_origin: receiver.origin,
                preceding_drops: self
                    .facts
                    .iter()
                    .filter(|fact| fact.point() == point)
                    .count(),
            };
            if !self.conditional_receiver_facts.contains(&fact) {
                self.conditional_receiver_facts.push(fact);
                let public = fact.into_unit(self.checker.source_unit);
                self.iteration_conditional_scopes
                    .push((public, scopes.to_vec()));
                self.iteration_actions.push((
                    point,
                    crate::ownership_checking::UnitIterationCleanupAction::DropConditionalReceiver(
                        public,
                    ),
                ));
            }
        } else {
            self.push_fact(PlannerDropFact::new(
                point,
                PlannerDropTarget::This(receiver.owner),
                receiver.origin,
            ));
        }
    }

    fn drop_scope(&mut self, depth: usize, point: PlannerDropPoint, state: &mut ValueState) {
        self.end_borrow_bindings(point, state, |binding_depth| binding_depth == depth);
        let mut index = state.values.len();
        while index > 0 {
            index -= 1;
            if state.values[index].scope_depth != depth {
                continue;
            }
            let symbol = state.values[index].symbol;
            self.drop_named(point, symbol, state);
        }
    }

    fn drop_deeper_than(
        &mut self,
        depth: usize,
        loop_depth: usize,
        point: PlannerDropPoint,
        state: &mut ValueState,
    ) {
        self.end_borrow_bindings(point, state, |binding_depth| binding_depth > depth);
        state
            .pending_capture_borrows
            .retain(|borrow| borrow.loop_depth < loop_depth);
        self.drop_pending_temporaries(point, state, |pending| pending.loop_depth >= loop_depth);
        let mut index = state.values.len();
        while index > 0 {
            index -= 1;
            if state.values[index].scope_depth <= depth {
                continue;
            }
            let symbol = state.values[index].symbol;
            self.drop_named(point, symbol, state);
        }
    }

    fn drop_branch_exit(&mut self, control: ExpressionId, branch: usize, state: &mut ValueState) {
        let live_after = &self.liveness.expression_after[control.index()];
        let symbols = state
            .values
            .iter()
            .filter(|value| !live_after.contains(&value.symbol))
            .map(|value| value.symbol)
            .collect::<Vec<_>>();
        for symbol in symbols.into_iter().rev() {
            self.drop_named_asap(
                PlannerDropPoint::BranchExit { control, branch },
                symbol,
                state,
            );
        }
    }

    fn drop_loop_exit(&mut self, statement: StatementId, state: &mut ValueState) {
        let live_after = &self.liveness.statement_after[statement.index()];
        let symbols = state
            .values
            .iter()
            .filter(|value| !live_after.contains(&value.symbol))
            .map(|value| value.symbol)
            .collect::<Vec<_>>();
        for symbol in symbols.into_iter().rev() {
            self.drop_named_asap(PlannerDropPoint::LoopExit(statement), symbol, state);
        }
    }

    fn push_fact(&mut self, mut fact: PlannerDropFact) {
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
            self.iteration_actions.push((
                fact.point(),
                crate::ownership_checking::UnitIterationCleanupAction::Drop(
                    fact.into_unit(self.checker.source_unit),
                ),
            ));
        }
    }

    fn live_after(&self, point: PlannerDropPoint) -> &std::collections::BTreeSet<UnitSymbolId> {
        match point {
            PlannerDropPoint::BranchEntry { control, branch } => self
                .liveness
                .if_branch_live_in
                .get(&control.index())
                .and_then(|arms| arms.get(branch))
                .or_else(|| {
                    self.liveness
                        .when_entry_live_in
                        .get(&(control.index(), branch))
                })
                .unwrap_or(&self.liveness.expression_after[control.index()]),
            PlannerDropPoint::AfterExpression(expression)
            | PlannerDropPoint::AfterBinaryOperands(expression)
            | PlannerDropPoint::CallReturn(expression)
            | PlannerDropPoint::ControlTransfer(expression)
            | PlannerDropPoint::BeforeReplacement(expression)
            | PlannerDropPoint::AfterReplacement(expression)
            | PlannerDropPoint::BranchExit {
                control: expression,
                ..
            }
            | PlannerDropPoint::WhenComparisonCompleted {
                control: expression,
                ..
            }
            | PlannerDropPoint::WhenAlternativeNoMatch {
                control: expression,
                ..
            }
            | PlannerDropPoint::WhenAlternativeMatch {
                control: expression,
                ..
            } => &self.liveness.expression_after[expression.index()],
            PlannerDropPoint::AfterStatement(statement) | PlannerDropPoint::LoopExit(statement) => {
                &self.liveness.statement_after[statement.index()]
            }
            PlannerDropPoint::FunctionEntry(item) => &self.liveness.function_live_in[&item.index()],
            PlannerDropPoint::LambdaEntry(expression) => {
                &self.liveness.lambda_live_in[&expression.index()]
            }
        }
    }
}
