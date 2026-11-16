mod analysis;
use analysis::plan_with_observations;
mod action_source;
mod assignment;
mod binding;
mod borrow_binding_transport;
mod borrow_call_handoff;
mod borrow_forward_transport;
mod borrow_result;
mod borrow_transport;
mod bounded_forward_plan;
mod branch_forward_scope;
mod call_loan_end;
mod control;
mod expression;
mod item_traversal;
mod iteration;
mod lambda;
mod loan_auxiliary_binding_region;
mod loan_binding_region;
mod loan_bounded_forward_region;
mod loan_branch_forward_region;
mod loan_child_region;
mod loan_consumer_region;
mod loan_forward_region;
mod loan_loop_invariant;
mod loan_named_region;
mod loan_nested_forward_region;
mod loan_provenance;
mod loan_receiver_region;
mod loan_region;
mod loop_invariant_capture;
mod map;
mod model;
mod nested_forward_scope;
pub(super) mod no_carry_exit;
mod nullable_comparison;
mod owner_cleanup;
mod owner_transport;
mod pending_call;
mod receiver_loan_end;
mod resource;
mod root_traversal;
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
    #[cfg(feature = "d10-catalog-experiment")]
    pub(super) drop_traversals: super::super::d10_catalog_experiment::DropCollections,
    #[cfg(feature = "d10-catalog-experiment")]
    pub(super) split_source_catalog: Option<super::super::d10_catalog_experiment::SplitSourcePending>,
    #[cfg(feature = "d10-catalog-experiment")]
    pub(super) catalog:
        Option<crate::ownership_checking::compilation_unit::d10_catalog_experiment::Pending>,
    #[cfg(feature = "d10-catalog-experiment")]
    pub(super) loop_catalog:
        Option<crate::ownership_checking::compilation_unit::d10_catalog_experiment::LoopPending>,
    #[cfg(feature = "d10-catalog-experiment")]
    pub(super) nested_catalog:
        Option<crate::ownership_checking::compilation_unit::d10_catalog_experiment::NestedPending>,
    #[cfg(feature = "d10-catalog-experiment")]
    pub(super) dual_nested_catalog: Option<
        crate::ownership_checking::compilation_unit::d10_catalog_experiment::DualNestedPending,
    >,
    #[cfg(feature = "d10-catalog-experiment")]
    pub(super) after_child_catalog: Option<
        crate::ownership_checking::compilation_unit::d10_catalog_experiment::AfterChildPending,
    >,
    #[cfg(feature = "d10-catalog-experiment")]
    pub(super) wrapped_dual_catalog: Option<
        crate::ownership_checking::compilation_unit::d10_catalog_experiment::WrappedDualPending,
    >,
    #[cfg(feature = "d10-catalog-experiment")]
    pub(super) wrapped_after_child_catalog: Option<
        crate::ownership_checking::compilation_unit::d10_catalog_experiment::WrappedAfterChildPending,
    >,

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
    pub(super) no_carry: no_carry_exit::Receipts,
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
    #[cfg(feature = "loan-observation-test-support")]
    let loan_enabled = super::super::loan_observation_test_support::enabled();

    #[cfg(not(feature = "loan-observation-test-support"))]
    let loan_enabled = true;
    plan_with_observations(checker, enabled, loan_enabled)
}

struct DropPlanner<'a, 'checker> {
    #[cfg(test)]
    carry_probe: Option<Vec<owner_transport::carry_probe::Visit>>,
    // Opt-in contiguous source-root boundaries; not a callable capability or filtered trace.
    #[cfg(any(test, feature = "d10-catalog-experiment"))]
    root_trace_ranges: Option<Vec<(crate::type_checking::UnitItemId, std::ops::Range<usize>)>>,
    #[cfg(any(test, feature = "d10-catalog-experiment"))]
    origin_probe: Option<Vec<owner_transport::origin_probe::Event>>,
    #[cfg(any(test, feature = "d10-catalog-experiment"))]
    producer_catalog: Option<owner_transport::producer_catalog::Catalog>,
    scalar_entry: Option<owner_transport::loops::scalar_entry::Tracker>,
    no_carry_exit: no_carry_exit::Tracker,
    #[cfg(test)]
    owner_hook_probe: Option<Vec<crate::ownership_checking::UnitOwnerTransportEvent>>,
    #[cfg(test)]
    owner_control_probe: Option<Vec<owner_transport::loops::ControlVisit>>,
    owner_controls: owner_transport::loops::ControlStack,
    owner_observation_enabled: bool,
    owner_choice: Option<owner_transport::choice::ChoiceTransaction>,
    owner_loop: owner_transport::loops::ControlTransactions,
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
    pending_borrow_handoffs: Vec<borrow_call_handoff::PendingHandoff>,
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
    #[cfg(feature = "d10-catalog-experiment")]
    drop_traversals: super::super::d10_catalog_experiment::DropCollections,
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
            carry_probe: None,
            #[cfg(any(test, feature = "d10-catalog-experiment"))]
            root_trace_ranges: None,
            #[cfg(any(test, feature = "d10-catalog-experiment"))]
            origin_probe: None,
            #[cfg(any(test, feature = "d10-catalog-experiment"))]
            producer_catalog: None,
            scalar_entry: None,
            no_carry_exit: no_carry_exit::Tracker::default(),
            #[cfg(test)]
            owner_hook_probe: None,
            #[cfg(test)]
            owner_control_probe: None,
            owner_controls: owner_transport::loops::ControlStack::default(),
            owner_observation_enabled: true,
            owner_choice: None,
            owner_loop: owner_transport::loops::ControlTransactions::default(),
            owner_branch_depth: 0,
            owner_observation: None,
            owner_observations: Vec::new(),
            string_clone_loan_ends: Vec::new(),
            iteration_conditional_scopes: Vec::new(),
            iteration_temporary_scopes: Vec::new(),
            iteration_scope_depths: BTreeMap::new(),
            iteration_actions: action_source::ActionSource::default(),
            borrow_binding_transports: Vec::new(),
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
            #[cfg(feature = "d10-catalog-experiment")]
            drop_traversals: Default::default(),
            facts: Vec::new(),
            conditional_receiver_facts: Vec::new(),
            loop_boundaries: Vec::new(),
            scope_depth: 0,
            binding_depths: BTreeMap::new(),
        }
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
