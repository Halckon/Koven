use std::collections::{BTreeMap, BTreeSet};

mod binding;
mod borrow_result;
mod capture;
mod conditional;
mod control;
mod elvis;
mod expression;
mod iteration;
mod lambda;
mod liveness;
mod map;
mod nullable_comparison;
mod origins;
mod ownership_primitive;
mod pending_call;
mod resource;
mod short_circuit;
mod snapshot;
mod source_owner;
mod string;

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::SymbolId,
    parser::{
        AssignmentOperator, BinaryOperator, Expression, FunctionBody, FunctionForm, Item,
        NameMarker, Statement, StringPart, WhenCondition,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallReceiverOrigin, Copyability, DestructuringMode, ExpressionCategory,
        ParameterMode, TypeKind,
    },
};

use crate::ownership_checking::{
    ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource, DropFact, DropPoint,
    DropTarget, IterationCleanupAction, IterationExitKind, IterationExitPlan,
    IterationOwnershipPlan, LoanEndFact, LoanEndPoint, LoanTarget, OwnershipDeferredFact,
    OwnershipDeferredReason,
};

use super::{Checker, ExpressionUse, OwnershipCheckingError};

use self::conditional::ClosureOrigin;
use self::liveness::Liveness;
use self::source_owner::OwnerVersion;
use crate::ownership_checking::{CleanupConditionId, CleanupConditions};

#[derive(Default)]
pub(super) struct DropPlan {
    pub(super) cleanup_steps: Vec<(DropPoint, IterationCleanupAction)>,
    pub(super) cleanup_conditions: crate::ownership_checking::CleanupConditions,
    pub(super) drops: Vec<DropFact>,
    pub(super) borrow_ends:
        Vec<crate::ownership_checking::BorrowBindingEndFact<SymbolId, DropPoint>>,
    pub(super) loan_ends: Vec<LoanEndFact>,
    pub(super) iterations: Vec<IterationOwnershipPlan>,
    pub(super) map_iterations: Vec<crate::ownership_checking::MapIterationOwnershipPlan>,
    pub(super) map_iteration_required_exits:
        Vec<crate::ownership_checking::map_iteration::MapIterationRequiredExit>,
    pub(super) deferred: Vec<OwnershipDeferredFact>,
}

pub(super) fn plan(checker: &Checker<'_>) -> Result<DropPlan, OwnershipCheckingError> {
    let liveness = Liveness::build(checker)?;
    let (origins, captures) = origins::analyze(checker)?;
    DropPlanner::new(checker, liveness, origins, captures).run()
}

pub(super) struct CaptureLiveness {
    pub(super) expression_after: Vec<std::collections::BTreeSet<SymbolId>>,
    pub(super) statement_after: Vec<std::collections::BTreeSet<SymbolId>>,
}

pub(super) fn capture_liveness(
    checker: &Checker<'_>,
) -> Result<CaptureLiveness, OwnershipCheckingError> {
    let liveness = Liveness::build(checker)?;
    Ok(CaptureLiveness {
        expression_after: liveness.expression_after,
        statement_after: liveness.statement_after,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OwnedValue {
    versions: Vec<OwnerVersion>,
    condition: CleanupConditionId,
    symbol: SymbolId,
    origin: Span,
    /// 值来源可随赋值变化；scope/return 清理仍按原 binding 声明排序。
    declaration: Span,
    scope_depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RetainedSource {
    statement: StatementId,
    owner: crate::ownership_checking::CleanupOwnerValueId,
    symbol: SymbolId,
    condition: CleanupConditionId,
    origin: Span,
}

#[derive(Clone, Debug)]
struct NullableTemporary {
    versions: Vec<OwnerVersion>,
    closures: Vec<ClosureOrigin>,
    transfers_at_call: bool,
    control: ExpressionId,
    subject: ExpressionId,
    origin: Span,
    loop_depth: usize,
    prior_symbols: Vec<SymbolId>,
}

#[derive(Clone, Debug, Default)]
struct ValueState {
    borrow_bindings: Vec<SymbolId>,
    pending_borrow_results: Vec<SymbolId>,
    replacements: Vec<SymbolId>,
    iterations: Vec<iteration::IterationFrame>,
    nullable_temporaries: Vec<NullableTemporary>,
    pending_calls: Vec<pending_call::PendingCall>,
    values: Vec<OwnedValue>,
    retained_sources: Vec<RetainedSource>,
    closures: BTreeMap<SymbolId, Vec<ClosureOrigin>>,
    result_closures: Vec<ClosureOrigin>,
    result_owners: Vec<OwnerVersion>,
    path: CleanupConditionId,
}

impl ValueState {
    fn position(&self, symbol: SymbolId) -> Option<usize> {
        self.values.iter().position(|value| value.symbol == symbol)
    }

    fn remove_value(&mut self, symbol: SymbolId) -> Option<OwnedValue> {
        self.position(symbol).map(|index| self.values.remove(index))
    }

    fn take(&mut self, symbol: SymbolId) -> Option<OwnedValue> {
        self.closures.remove(&symbol);
        self.remove_value(symbol)
    }

    fn insert(&mut self, value: OwnedValue) {
        self.remove_value(value.symbol);
        let index = self
            .values
            .partition_point(|prior| prior.declaration.start() < value.declaration.start());
        self.values.insert(index, value);
    }
}

struct DropPlanner<'a, 'checker> {
    resource_lifetime: Option<ExpressionId>,
    call_return_copy_source: Option<SymbolId>,
    loop_origins: BTreeMap<usize, crate::ownership_checking::IterationClosureFlow>,
    captured_origins: origins::CapturedOrigins,
    recursive_capture_phi: Option<ExpressionId>,
    recursive_phi_bindings: BTreeSet<crate::ownership_checking::CleanupOwnerValueId>,
    recursive_release_phi_bindings: BTreeSet<crate::ownership_checking::CleanupOwnerValueId>,
    recursive_release_closure_owners: BTreeMap<
        crate::ownership_checking::CleanupOwnerValueId,
        crate::ownership_checking::ClosureReleaseLayout,
    >,
    recursive_snapshot_sources:
        BTreeMap<crate::ownership_checking::CleanupOwnerValueId, Vec<OwnerVersion>>,
    snapshot_phi_roots: BTreeMap<
        crate::ownership_checking::CleanupOwnerValueId,
        Vec<(
            ExpressionId,
            crate::ownership_checking::CleanupOwnerValueId,
            CleanupConditionId,
        )>,
    >,
    enclosing_capture_phi: Option<ExpressionId>,
    coexisting_capture_phi: Option<ExpressionId>,
    conditional_nested_phi: Option<ExpressionId>,
    temporary_backing_owners: BTreeMap<usize, crate::ownership_checking::CleanupOwnerValueId>,
    loop_capture_graphs: BTreeMap<usize, crate::ownership_checking::IterationCaptureGraph>,
    loop_phis: BTreeMap<usize, Vec<crate::ownership_checking::IterationClosurePhiBinding>>,
    loop_phi_incomings: BTreeMap<usize, Vec<crate::ownership_checking::IterationPhiIncoming>>,
    current_environment: Option<(crate::ownership_checking::CleanupOwnerValueId, ExpressionId)>,
    checker: &'a Checker<'checker>,
    liveness: Liveness<'a, 'checker>,
    conditions: CleanupConditions,
    facts: Vec<DropFact>,
    cleanup: Vec<(DropPoint, IterationCleanupAction)>,
    iteration_exits: Vec<iteration::IterationExitRecord>,
    loan_ends: Vec<LoanEndFact>,
    loop_boundaries: Vec<usize>,
    scope_depth: usize,
    borrow_ends: Vec<crate::ownership_checking::BorrowBindingEndFact<SymbolId, DropPoint>>,
    binding_depths: BTreeMap<SymbolId, usize>,
}

impl<'a, 'checker> DropPlanner<'a, 'checker> {
    fn owner_protected_by_context(&self, symbol: SymbolId, state: &ValueState) -> bool {
        self.call_return_copy_source == Some(symbol)
            || self.borrow_source_protected(symbol, state)
            || state
                .iterations
                .iter()
                .any(|frame| frame.source_root == Some(symbol))
            || state.pending_calls.iter().any(|call| {
                call.callees.contains(&symbol)
                    || call.loans.iter().any(|loan| {
                        matches!(loan.target(), LoanTarget::Place(place) if place.root() == symbol)
                    })
            })
    }

    fn new(
        checker: &'a Checker<'checker>,
        liveness: Liveness<'a, 'checker>,
        loop_origins: BTreeMap<usize, crate::ownership_checking::IterationClosureFlow>,
        captured_origins: origins::CapturedOrigins,
    ) -> Self {
        Self {
            resource_lifetime: None,
            call_return_copy_source: None,
            loop_origins,
            captured_origins,
            recursive_capture_phi: None,
            recursive_phi_bindings: BTreeSet::new(),
            recursive_release_phi_bindings: BTreeSet::new(),
            recursive_release_closure_owners: BTreeMap::new(),
            recursive_snapshot_sources: BTreeMap::new(),
            snapshot_phi_roots: BTreeMap::new(),
            enclosing_capture_phi: None,
            coexisting_capture_phi: None,
            conditional_nested_phi: None,
            temporary_backing_owners: BTreeMap::new(),
            loop_capture_graphs: BTreeMap::new(),
            loop_phis: BTreeMap::new(),
            loop_phi_incomings: BTreeMap::new(),
            current_environment: None,
            checker,
            liveness,
            conditions: CleanupConditions::default(),
            facts: Vec::new(),
            cleanup: Vec::new(),
            iteration_exits: Vec::new(),
            loan_ends: Vec::new(),
            loop_boundaries: Vec::new(),
            scope_depth: 0,
            borrow_ends: Vec::new(),
            binding_depths: BTreeMap::new(),
        }
    }

    fn run(mut self) -> Result<DropPlan, OwnershipCheckingError> {
        for &root in self.checker.parsed.roots() {
            self.item(root)?;
        }
        if let Some(expression) = self.resource_lifetime {
            return Ok(DropPlan {
                deferred: vec![OwnershipDeferredFact::new(
                    expression,
                    OwnershipDeferredReason::ResourceLifetime,
                )],
                ..DropPlan::default()
            });
        }
        if let Some((expression, reason)) = self
            .recursive_capture_phi
            .map(|expression| (expression, OwnershipDeferredReason::RecursiveClosureCapture))
            .or_else(|| {
                self.enclosing_capture_phi.map(|expression| {
                    (
                        expression,
                        OwnershipDeferredReason::EnclosingEnvironmentCapture,
                    )
                })
            })
            .or_else(|| {
                self.coexisting_capture_phi.map(|expression| {
                    (
                        expression,
                        OwnershipDeferredReason::AmbiguousClosureInstanceTransport,
                    )
                })
            })
            .or_else(|| {
                self.conditional_nested_phi.map(|expression| {
                    (
                        expression,
                        OwnershipDeferredReason::AmbiguousClosureInstanceTransport,
                    )
                })
            })
        {
            return Ok(DropPlan {
                deferred: vec![OwnershipDeferredFact::new(expression, reason)],
                ..DropPlan::default()
            });
        }
        Ok(self.into_candidate_facts())
    }

    /// 组装已检查的候选事实；调用方必须先处理 deferred，不能直接发布。
    fn into_candidate_facts(self) -> DropPlan {
        let iterations = self.iteration_plans();
        let map_iterations = self.map_iteration_plans();
        let map_iteration_required_exits = self.map_iteration_required_exits();
        DropPlan {
            cleanup_steps: self.cleanup,
            cleanup_conditions: self.conditions,
            drops: self.facts,
            borrow_ends: self.borrow_ends,
            loan_ends: self.loan_ends,
            iterations,
            map_iterations,
            map_iteration_required_exits,
            deferred: Vec::new(),
        }
    }

    fn item(&mut self, id: ItemId) -> Result<(), OwnershipCheckingError> {
        match self.checker.parsed.ast().items().get(id)?.payload().clone() {
            Item::Modified { declaration, .. } => self.item(declaration)?,
            Item::Function {
                parameters, form, ..
            } => {
                if self.liveness.skipped_functions.contains(&id.index()) {
                    return Ok(());
                }
                let mut state = ValueState::default();
                for parameter in parameters {
                    if let Some(symbol) = self.checker.marker_symbol(parameter.name)
                        && self.checker.is_move_only_variable(symbol)
                    {
                        self.binding_depths.insert(symbol, 0);
                        state.insert(OwnedValue {
                            versions: vec![
                                self.parameter_owner(symbol, marker_span(parameter.name)),
                            ],
                            condition: state.path,
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
                if has_body {
                    let unused_parameters = state
                        .values
                        .iter()
                        .filter(|value| !live_in.contains(&value.symbol))
                        .map(|value| value.symbol)
                        .collect::<Vec<_>>();
                    for symbol in unused_parameters.into_iter().rev() {
                        self.drop_named_asap(DropPoint::FunctionEntry(id), symbol, &mut state);
                    }
                }
                match form {
                    FunctionForm::ImplicitUnitAbsent => {}
                    FunctionForm::ImplicitUnitBlock(body) => {
                        if self.statement(body, &mut state)? {
                            self.drop_all(DropPoint::AfterStatement(body), &mut state);
                        }
                    }
                    FunctionForm::Explicit { body, .. } => match body {
                        FunctionBody::Absent => {}
                        FunctionBody::Expression { expression, .. } => {
                            if self.expression(expression, ExpressionUse::Consume, &mut state)? {
                                self.drop_all(DropPoint::ControlTransfer(expression), &mut state);
                            }
                        }
                        FunctionBody::Block(body) => {
                            if self.statement(body, &mut state)? {
                                self.drop_all(DropPoint::AfterStatement(body), &mut state);
                            }
                        }
                    },
                }
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
                let mut state = ValueState::default();
                if self.statement(body, &mut state)? {
                    self.drop_all(DropPoint::AfterStatement(body), &mut state);
                }
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
                self.drop_scope(frame, DropPoint::AfterStatement(id), state);
                self.scope_depth -= 1;
                Ok(true)
            }
            Statement::LocalVariable { declaration } => self.local_variable(id, declaration, state),
            Statement::LocalDestructuring { initializer, .. } => {
                let usage = self
                    .checker
                    .typed
                    .destructuring(id)
                    .map(|descriptor| match descriptor.mode() {
                        DestructuringMode::Copy => ExpressionUse::Read,
                        DestructuringMode::Consume => ExpressionUse::Consume,
                    })
                    .unwrap_or(ExpressionUse::Read);
                if !self.expression(initializer, usage, state)? {
                    return Ok(false);
                }
                if let Some(descriptor) = self.checker.typed.destructuring(id) {
                    // The typed descriptor follows layout order. Locals are declared in pattern order.
                    let mut bindings = descriptor
                        .components()
                        .iter()
                        .map(|component| {
                            let symbol = component.symbol();
                            (self.checker.names.symbols()[symbol.index()].span(), symbol)
                        })
                        .collect::<Vec<_>>();
                    bindings.sort_unstable_by_key(|(span, _)| span.start());
                    for (origin, symbol) in bindings {
                        if self.checker.is_move_only_variable(symbol) {
                            self.binding_depths.insert(symbol, self.scope_depth);
                            state.insert(OwnedValue {
                                versions: vec![
                                    self.component_owner(id, symbol, origin, state.path),
                                ],
                                condition: state.path,
                                symbol,
                                origin,
                                declaration: origin,
                                scope_depth: self.scope_depth,
                            });
                            if !self.liveness.statement_after[id.index()].contains(&symbol) {
                                self.drop_named_asap(DropPoint::AfterStatement(id), symbol, state);
                            }
                        }
                    }
                }
                Ok(true)
            }
            Statement::Expression { expression } => {
                let continues = self.expression(expression, ExpressionUse::Read, state)?;
                if continues && self.is_move_only_temporary(expression) {
                    let span = self
                        .checker
                        .parsed
                        .ast()
                        .expressions()
                        .get(expression)?
                        .span();
                    self.push_fact(DropFact::new(
                        DropPoint::AfterExpression(expression),
                        DropTarget::Temporary(expression),
                        span,
                    ));
                }
                Ok(continues)
            }
            Statement::While {
                condition, body, ..
            } => {
                if !self.expression(condition, ExpressionUse::Read, state)? {
                    return Ok(false);
                }
                self.plan_optional_loop_body(id, body, state)
            }
            Statement::For { source, body, .. } => self.iteration(id, source, body, state),
            Statement::Loop { body, .. } => {
                let mut body_state = state.clone();
                self.loop_boundaries.push(self.scope_depth);
                self.statement(body, &mut body_state)?;
                self.loop_boundaries.pop();
                let has_exit = self
                    .checker
                    .loop_has_exit
                    .get(&id.index())
                    .copied()
                    .unwrap_or(false);
                if has_exit {
                    self.drop_loop_exit(id, state);
                }
                Ok(has_exit)
            }
        }
    }

    fn is_move_only_temporary(&self, expression: ExpressionId) -> bool {
        if self
            .checker
            .typed
            .expression_type(expression)
            .and_then(|ty| self.checker.typed.types().get(ty))
            .is_some_and(|ty| {
                matches!(
                    ty,
                    TypeKind::Intrinsic {
                        constructor: crate::type_checking::IntrinsicTypeConstructor::View,
                        ..
                    }
                )
            })
        {
            return false;
        }
        self.checker.borrow_result_call(expression).is_none()
            && self.checker.typed.expression_category(expression)
                == Some(ExpressionCategory::Temporary)
            && self
                .checker
                .typed
                .expression_type(expression)
                .and_then(|ty| self.checker.typed.copyability(ty))
                == Some(Copyability::MoveOnly)
    }

    /// Release inner scopes first, then the subject temporary, then older named owners.
    fn drop_nullable_temporaries(
        &mut self,
        point: DropPoint,
        state: &mut ValueState,
        selected: impl Fn(&NullableTemporary) -> bool,
    ) {
        for index in (0..state.nullable_temporaries.len()).rev() {
            if !selected(&state.nullable_temporaries[index]) {
                continue;
            }
            let temporary = state.nullable_temporaries.remove(index);
            let newer = state
                .values
                .iter()
                .rev()
                .filter(|value| {
                    let newer = !temporary.prior_symbols.contains(&value.symbol)
                        || value.origin.start() > temporary.origin.end();
                    let leaving = match point {
                        DropPoint::BranchExit { control, .. } | DropPoint::CallReturn(control) => {
                            !self.liveness.expression_after[control.index()].contains(&value.symbol)
                        }
                        DropPoint::ControlTransfer(expression) => {
                            self.checker
                                .parsed
                                .ast()
                                .expressions()
                                .get(expression)
                                .is_ok_and(|node| {
                                    matches!(node.payload(), Expression::Return { .. })
                                })
                                || value.scope_depth
                                    > self.loop_boundaries.last().copied().unwrap_or(0)
                        }
                        _ => true,
                    };
                    newer
                        && leaving
                        && (matches!(point, DropPoint::ControlTransfer(_))
                            || self.is_asap_owner(value.symbol, state))
                })
                .map(|value| value.symbol)
                .collect::<Vec<_>>();
            for symbol in newer {
                self.drop_named(point, symbol, state);
            }
            self.drop_closure_owner(
                DropFact::new(
                    point,
                    DropTarget::Temporary(temporary.subject),
                    temporary.origin,
                ),
                state.path,
                &temporary.versions,
                temporary.closures,
                state,
            );
        }
    }

    fn drop_all(&mut self, point: DropPoint, state: &mut ValueState) {
        state.pending_borrow_results.clear();
        self.end_borrow_bindings(point, state, |_| true);
        if let DropPoint::ControlTransfer(expression) = point {
            while let Some(frame) = state.iterations.last() {
                self.drop_deeper_than(frame.scope_depth, frame.loop_depth, point, state);
                self.finish_iteration(point, IterationExitKind::Return(expression), state);
            }
            self.end_pending_calls(LoanEndPoint::ControlTransfer(expression), state, |_| true);
        }
        self.drop_nullable_temporaries(point, state, |_| true);
        let symbols = state
            .values
            .iter()
            .rev()
            .map(|value| value.symbol)
            .collect::<Vec<_>>();
        for symbol in symbols {
            self.drop_named(point, symbol, state);
        }
    }

    fn drop_scope(&mut self, depth: usize, point: DropPoint, state: &mut ValueState) {
        self.end_borrow_bindings(point, state, |binding_depth| binding_depth == depth);
        // Dropping a closure may recursively remove an earlier captured source.
        let symbols = state
            .values
            .iter()
            .rev()
            .filter(|value| value.scope_depth == depth)
            .map(|value| value.symbol)
            .collect::<Vec<_>>();
        for symbol in symbols {
            self.drop_named(point, symbol, state);
        }
    }

    fn drop_deeper_than(
        &mut self,
        depth: usize,
        loop_depth: usize,
        point: DropPoint,
        state: &mut ValueState,
    ) {
        self.end_borrow_bindings(point, state, |binding_depth| binding_depth > depth);
        if let DropPoint::ControlTransfer(expression) = point {
            self.end_pending_calls(LoanEndPoint::ControlTransfer(expression), state, |call| {
                call.loop_depth >= loop_depth
            });
        }
        self.drop_nullable_temporaries(point, state, |temporary| {
            temporary.loop_depth >= loop_depth
        });
        let symbols = state
            .values
            .iter()
            .rev()
            .filter(|value| value.scope_depth > depth)
            .map(|value| value.symbol)
            .collect::<Vec<_>>();
        for symbol in symbols {
            self.drop_named(point, symbol, state);
        }
    }

    fn drop_branch_exit(&mut self, control: ExpressionId, branch: usize, state: &mut ValueState) {
        self.drop_nullable_temporaries(
            DropPoint::BranchExit { control, branch },
            state,
            |temporary| temporary.control == control,
        );
        let live_after = &self.liveness.expression_after[control.index()];
        let symbols = state
            .values
            .iter()
            .filter(|value| !live_after.contains(&value.symbol))
            .map(|value| value.symbol)
            .collect::<Vec<_>>();
        for symbol in symbols.into_iter().rev() {
            self.drop_named_asap(DropPoint::BranchExit { control, branch }, symbol, state);
        }
    }

    fn drop_loop_exit(&mut self, statement: StatementId, state: &mut ValueState) {
        self.drop_loop_exit_at(statement, DropPoint::LoopExit(statement), state);
    }

    fn drop_loop_exit_at(
        &mut self,
        statement: StatementId,
        point: DropPoint,
        state: &mut ValueState,
    ) {
        let live_after = &self.liveness.statement_after[statement.index()];
        let symbols = state
            .values
            .iter()
            .filter(|value| !live_after.contains(&value.symbol))
            .map(|value| value.symbol)
            .collect::<Vec<_>>();
        for symbol in symbols.into_iter().rev() {
            self.drop_named_asap(point, symbol, state);
        }
    }

    fn push_fact(&mut self, mut fact: DropFact) {
        if let DropTarget::Temporary(expression) = fact.target()
            && let Some((owner, origin)) = self.checker.constant_temporary_origin(expression)
        {
            let condition = fact.condition();
            let value = fact.owner();
            fact = DropFact::new(fact.point(), DropTarget::Temporary(owner), origin);
            if let Some(value) = value {
                fact = fact.with_owner(value);
            }
            if let Some(condition) = condition {
                fact = fact.with_condition(condition);
            }
        }
        if !self.facts.contains(&fact) {
            let action = fact
                .owner()
                .and_then(|root_owner| self.recursive_release_layout(root_owner))
                .map(|layout| IterationCleanupAction::ReleaseClosureInstances {
                    layout,
                    root: fact,
                })
                .unwrap_or(IterationCleanupAction::Drop(fact));
            self.cleanup.push((fact.point(), action));
            self.facts.push(fact);
        }
    }

    fn recursive_release_layout(
        &self,
        root_owner: crate::ownership_checking::CleanupOwnerValueId,
    ) -> Option<crate::ownership_checking::ClosureReleaseLayout> {
        if let Some(&layout) = self.recursive_release_closure_owners.get(&root_owner) {
            return Some(layout);
        }
        if self.recursive_snapshot_sources.contains_key(&root_owner) {
            // A snapshot may choose roots from different loops or an ordinary branch.
            return Some(crate::ownership_checking::ClosureReleaseLayout::File);
        }
        let Some(crate::ownership_checking::CleanupOwnerValue::IterationPhi { statement, .. }) =
            self.conditions.owner_value(root_owner)
        else {
            return None;
        };
        (self.recursive_release_phi_bindings.contains(&root_owner)
            && self.loop_phis.get(&statement.index()).is_some_and(|phis| {
                phis.iter()
                    .any(|phi| phi.owner() == root_owner && !phi.root_nodes().is_empty())
            }))
        .then_some(crate::ownership_checking::ClosureReleaseLayout::Iteration(
            *statement,
        ))
    }

    fn recursive_capture_layout(
        &self,
        value: crate::ownership_checking::CleanupCaptureValue,
    ) -> Option<crate::ownership_checking::ClosureReleaseLayout> {
        use crate::ownership_checking::{
            CleanupCaptureValue, ClosureCaptureEffect, ClosureCaptureMode, ClosureReleaseLayout,
        };

        let (owner, source, slot) = match value {
            CleanupCaptureValue::Owner(owner) => return self.recursive_release_layout(owner),
            CleanupCaptureValue::Place(_) => return None,
            CleanupCaptureValue::Environment {
                owner,
                source,
                slot,
            } => (owner, source, slot),
        };
        let mut pending = vec![(owner, source, slot)];
        let mut seen = BTreeSet::new();
        while let Some((owner, source, slot)) = pending.pop() {
            if !seen.insert((owner, slot)) {
                continue;
            }
            for edge in self
                .conditions
                .closure_capture_edges(owner)
                .into_iter()
                .flatten()
                .filter(|edge| edge.target() == slot && edge.input().source() == source)
            {
                let input = edge.input();
                if input.mode() != ClosureCaptureMode::Owned
                    || input.effect() != ClosureCaptureEffect::Move
                {
                    continue;
                }
                match input.value() {
                    CleanupCaptureValue::Owner(source)
                        if self.recursive_release_layout(source).is_some() =>
                    {
                        return Some(ClosureReleaseLayout::File);
                    }
                    CleanupCaptureValue::Environment {
                        owner,
                        source,
                        slot,
                    } => pending.push((owner, source, slot)),
                    _ => {}
                }
            }
        }
        None
    }

    fn live_after(&self, point: DropPoint) -> &std::collections::BTreeSet<SymbolId> {
        match point {
            DropPoint::AfterExpression(expression)
            | DropPoint::AfterBinaryOperands(expression)
            | DropPoint::CallEntry(expression)
            | DropPoint::CallReturn(expression)
            | DropPoint::ControlTransfer(expression)
            | DropPoint::AfterReplacement(expression)
            | DropPoint::BranchExit {
                control: expression,
                ..
            }
            | DropPoint::WhenAlternativeMatch {
                control: expression,
                ..
            } => &self.liveness.expression_after[expression.index()],
            DropPoint::AfterStatement(statement) | DropPoint::LoopExit(statement) => {
                &self.liveness.statement_after[statement.index()]
            }
            DropPoint::FunctionEntry(item) => &self.liveness.function_live_in[&item.index()],
            DropPoint::LambdaEntry(lambda) => &self.liveness.lambda_live_in[&lambda.index()],
        }
    }
}

#[derive(Clone, Copy)]
enum StringOperandDrop {
    Named(SymbolId),
    Temporary(ExpressionId, Span),
}

fn marker_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
    }
}
