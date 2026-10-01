use std::collections::{BTreeMap, BTreeSet};

mod capture;
mod conditional;
mod control;
mod elvis;
mod iteration;
mod lambda;
mod liveness;
mod origins;
mod pending_call;
mod snapshot;
mod source_owner;

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
    pub(super) loan_ends: Vec<LoanEndFact>,
    pub(super) iterations: Vec<IterationOwnershipPlan>,
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
    iteration_exits: Vec<(StatementId, IterationExitKind, DropPoint)>,
    loan_ends: Vec<LoanEndFact>,
    loop_boundaries: Vec<usize>,
    scope_depth: usize,
    binding_depths: BTreeMap<SymbolId, usize>,
}

impl<'a, 'checker> DropPlanner<'a, 'checker> {
    fn owner_protected_by_context(&self, symbol: SymbolId, state: &ValueState) -> bool {
        state
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
            binding_depths: BTreeMap::new(),
        }
    }

    fn run(mut self) -> Result<DropPlan, OwnershipCheckingError> {
        for &root in self.checker.parsed.roots() {
            self.item(root)?;
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
        DropPlan {
            cleanup_steps: self.cleanup,
            cleanup_conditions: self.conditions,
            drops: self.facts,
            loan_ends: self.loan_ends,
            iterations,
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
                        self.drop_named(DropPoint::FunctionEntry(id), symbol, &mut state);
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

    fn statement(
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
            Statement::LocalVariable { declaration } => {
                let Item::Variable {
                    name, initializer, ..
                } = self
                    .checker
                    .parsed
                    .ast()
                    .items()
                    .get(declaration)?
                    .payload()
                    .clone()
                else {
                    return Ok(true);
                };
                if !self.expression(initializer, ExpressionUse::Consume, state)? {
                    return Ok(false);
                }
                let snapshot = self.save_result_snapshot(initializer, state)?;
                let closures = std::mem::take(&mut state.result_closures);
                if let Some(symbol) = self.checker.marker_symbol(name)
                    && self.checker.is_move_only_variable(symbol)
                {
                    self.binding_depths.insert(symbol, self.scope_depth);
                    let versions = self.bind_result_owners(marker_span(name), state);
                    state.insert(OwnedValue {
                        versions,
                        condition: state.path,
                        symbol,
                        origin: marker_span(name),
                        declaration: marker_span(name),
                        scope_depth: self.scope_depth,
                    });
                    if !closures.is_empty() {
                        state.closures.insert(symbol, closures);
                    }
                    self.commit_snapshot(snapshot, initializer, symbol);
                    if !self.liveness.statement_after[id.index()].contains(&symbol) {
                        self.drop_named(DropPoint::AfterStatement(id), symbol, state);
                    }
                }
                Ok(true)
            }
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
                    for component in descriptor.components() {
                        let symbol = component.symbol();
                        if self.checker.is_move_only_variable(symbol) {
                            self.binding_depths.insert(symbol, self.scope_depth);
                            let origin = self.checker.names.symbols()[symbol.index()].span();
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
                                self.drop_named(DropPoint::AfterStatement(id), symbol, state);
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
                let mut body_state = state.clone();
                self.loop_boundaries.push(self.scope_depth);
                self.statement(body, &mut body_state)?;
                self.loop_boundaries.pop();
                self.drop_loop_exit(id, state);
                Ok(true)
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

    fn expression_inner(
        &mut self,
        id: ExpressionId,
        usage: ExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        if self.checker.is_constant_use(id) {
            return Ok(true);
        }
        if let Some(operation) = self.checker.typed.rc_operation(id) {
            if let Some(place) = self.checker.place(operation.receiver())? {
                let root = place.root();
                if !self.liveness.expression_after[id.index()].contains(&root) {
                    self.drop_named(DropPoint::AfterExpression(id), root, state);
                }
                return Ok(true);
            }
            return self.expression(operation.receiver(), ExpressionUse::Read, state);
        }
        if let Some(descriptor) = self
            .checker
            .construction
            .descriptors
            .get(&id.index())
            .cloned()
        {
            let mut arguments = descriptor.arguments().to_vec();
            arguments.sort_by_key(|argument| argument.evaluation_index());
            for argument in arguments {
                if !self.expression(argument.argument(), ExpressionUse::Consume, state)?
                    || self.checker.is_nothing_expression(argument.argument())
                {
                    return Ok(false);
                }
                self.register_pending_argument(
                    id,
                    argument.argument(),
                    ParameterMode::Value,
                    state,
                )?;
            }
            // A completed construction takes each pending Value input; an earlier control
            // transfer instead consumes these obligations through drop_all.
            state
                .nullable_temporaries
                .retain(|temporary| temporary.control != id || !temporary.transfers_at_call);
            return Ok(true);
        }
        let node = self.checker.parsed.ast().expressions().get(id)?;
        match node.payload().clone() {
            Expression::Error
            | Expression::This
            | Expression::Literal(_)
            | Expression::SuperMember { .. } => Ok(true),
            Expression::Lambda { .. } => {
                for capture in self.checker.captures_of(id) {
                    if capture.mode() == ClosureCaptureMode::Owned
                        && capture.effect() == ClosureCaptureEffect::Move
                        && let ClosureCaptureSource::Symbol(symbol) = capture.source()
                    {
                        state.take(symbol);
                    }
                }
                Ok(true)
            }
            Expression::Name => {
                if let Some(symbol) = self.checker.reference_symbol(node.span()) {
                    match usage {
                        ExpressionUse::Consume => {
                            state.take(symbol);
                        }
                        ExpressionUse::Read => {
                            if !self.liveness.expression_after[id.index()].contains(&symbol) {
                                self.drop_named(DropPoint::AfterExpression(id), symbol, state);
                            }
                        }
                        ExpressionUse::Place => {}
                    }
                }
                Ok(true)
            }
            Expression::Group { expression } => self.expression(expression, usage, state),
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        if !self.expression(expression, ExpressionUse::Read, state)? {
                            state
                                .nullable_temporaries
                                .retain(|temporary| temporary.control != id);
                            return Ok(false);
                        }
                        if self.is_move_only_temporary(expression) {
                            state.nullable_temporaries.push(NullableTemporary {
                                versions: Vec::new(),
                                closures: Vec::new(),
                                transfers_at_call: false,
                                control: id,
                                subject: expression,
                                origin: self
                                    .checker
                                    .parsed
                                    .ast()
                                    .expressions()
                                    .get(expression)?
                                    .span(),
                                loop_depth: self.loop_boundaries.len(),
                                prior_symbols: state
                                    .values
                                    .iter()
                                    .map(|value| value.symbol)
                                    .collect(),
                            });
                        }
                    }
                }
                // Interpolation has consumed its inputs; the outer String result remains owned.
                // This completion must not release named owners that are live after the String.
                for index in (0..state.nullable_temporaries.len()).rev() {
                    if state.nullable_temporaries[index].control == id {
                        let temporary = state.nullable_temporaries.remove(index);
                        self.push_fact(DropFact::new(
                            DropPoint::AfterExpression(id),
                            DropTarget::Temporary(temporary.subject),
                            temporary.origin,
                        ));
                    }
                }
                Ok(true)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                if !self.expression(condition, ExpressionUse::Read, state)? {
                    return Ok(false);
                }
                let base = state.clone();
                let mut branch_states = Vec::new();
                let mut then_state = base.clone();
                self.enter_branch(id, 2, 0, &mut then_state)?;
                let usage = self.checker.control_result_usage(id);
                if self.control_body(then_branch, usage, &mut then_state)? {
                    self.drop_branch_exit(id, 0, &mut then_state);
                    branch_states.push(then_state);
                }
                if let Some(else_branch) = else_branch {
                    let mut else_state = base;
                    self.enter_branch(id, 2, 1, &mut else_state)?;
                    if self.control_body(else_branch, usage, &mut else_state)? {
                        self.drop_branch_exit(id, 1, &mut else_state);
                        branch_states.push(else_state);
                    }
                } else {
                    let mut implicit = base;
                    self.enter_branch(id, 2, 1, &mut implicit)?;
                    self.drop_branch_exit(id, 1, &mut implicit);
                    branch_states.push(implicit);
                }
                let continues = !branch_states.is_empty();
                *state = self.merge_value_states(branch_states);
                Ok(continues)
            }
            Expression::When {
                subject, entries, ..
            } => {
                if let Some(subject) = subject
                    && !self.expression(subject, ExpressionUse::Read, state)?
                {
                    return Ok(false);
                }
                let plan = self.checker.typed.nullable_when(id).cloned();
                if let Some(plan) = &plan
                    && plan.category()
                        == crate::type_checking::NullableWhenSubjectCategory::Temporary
                    && self.is_move_only_temporary(plan.subject())
                {
                    state.nullable_temporaries.push(NullableTemporary {
                        versions: Vec::new(),
                        closures: Vec::new(),
                        transfers_at_call: false,
                        control: id,
                        subject: plan.subject(),
                        origin: self
                            .checker
                            .parsed
                            .ast()
                            .expressions()
                            .get(plan.subject())?
                            .span(),
                        loop_depth: self.loop_boundaries.len(),
                        prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
                    });
                }
                let mut remaining = Some(state.clone());
                let mut branch_states = Vec::new();
                for (index, entry) in entries.iter().enumerate() {
                    let descriptor = plan.as_ref().and_then(|plan| plan.entries().get(index));
                    let mut matched = Vec::new();
                    let mut matched_alternatives = Vec::new();
                    if entry.else_span.is_some() {
                        matched.extend(remaining.take());
                    }
                    for (alternative_index, condition) in entry.conditions.iter().enumerate() {
                        let Some(mut input) = remaining.take() else {
                            break;
                        };
                        if let WhenCondition::Expression(expression)
                        | WhenCondition::Contains { expression, .. } = condition
                            && !self.expression(*expression, ExpressionUse::Read, &mut input)?
                        {
                            break;
                        }
                        let alternative = descriptor
                            .and_then(|entry| entry.alternatives().get(alternative_index));
                        let can_match = alternative
                            .is_none_or(|alternative| !alternative.match_domain().is_empty());
                        let can_continue = alternative
                            .is_none_or(|alternative| !alternative.fallthrough_domain().is_empty());
                        if can_match {
                            let mut success = input.clone();
                            if can_continue {
                                self.enter_when_alternative(
                                    id,
                                    index,
                                    alternative_index,
                                    true,
                                    &mut success,
                                )?;
                            }
                            matched.push(success);
                            matched_alternatives.push(alternative_index);
                        }
                        if can_continue {
                            if can_match {
                                self.enter_when_alternative(
                                    id,
                                    index,
                                    alternative_index,
                                    false,
                                    &mut input,
                                )?;
                            }
                            remaining = Some(input);
                        }
                    }
                    if matched.is_empty() {
                        continue;
                    }
                    // A successful ownership check guarantees that an owner missing on
                    // another incoming edge cannot be read by the shared body. Release
                    // each remaining obligation on its own edge before intersecting states.
                    let common = matched[0]
                        .values
                        .iter()
                        .filter(|value| {
                            matched
                                .iter()
                                .all(|incoming| incoming.position(value.symbol).is_some())
                        })
                        .map(|value| value.symbol)
                        .collect::<std::collections::BTreeSet<_>>();
                    for (alternative, incoming) in
                        matched_alternatives.into_iter().zip(&mut matched)
                    {
                        let exclusive = incoming
                            .values
                            .iter()
                            .rev()
                            .filter(|value| !common.contains(&value.symbol))
                            .map(|value| value.symbol)
                            .collect::<Vec<_>>();
                        for symbol in exclusive {
                            self.drop_named(
                                DropPoint::WhenAlternativeMatch {
                                    control: id,
                                    entry: index,
                                    alternative,
                                },
                                symbol,
                                incoming,
                            );
                        }
                    }
                    let mut branch = self.merge_value_states(matched);
                    if self.control_body(
                        entry.body,
                        self.checker.control_result_usage(id),
                        &mut branch,
                    )? {
                        self.drop_branch_exit(id, index, &mut branch);
                        branch_states.push(branch);
                    }
                }
                if let Some(mut implicit) = remaining {
                    self.drop_branch_exit(id, entries.len(), &mut implicit);
                    branch_states.push(implicit);
                }
                let continues = !branch_states.is_empty();
                *state = self.merge_value_states(branch_states);
                Ok(continues)
            }
            Expression::Return { value, .. } => {
                if let Some(value) = value
                    && !self.expression(value, ExpressionUse::Consume, state)?
                {
                    return Ok(false);
                }
                self.drop_all(DropPoint::ControlTransfer(id), state);
                Ok(false)
            }
            Expression::Break { .. } | Expression::Continue { .. } => {
                let boundary = self.loop_boundaries.last().copied().unwrap_or(0);
                self.drop_deeper_than(
                    boundary,
                    self.loop_boundaries.len(),
                    DropPoint::ControlTransfer(id),
                    state,
                );
                if state
                    .iterations
                    .last()
                    .is_some_and(|frame| frame.loop_depth == self.loop_boundaries.len())
                {
                    if matches!(node.payload(), Expression::Break { .. }) {
                        self.finish_iteration(
                            DropPoint::ControlTransfer(id),
                            IterationExitKind::Break(id),
                            state,
                        );
                    } else {
                        self.end_iteration_element(
                            DropPoint::ControlTransfer(id),
                            IterationExitKind::Continue(id),
                            state,
                        );
                    }
                }
                Ok(false)
            }
            Expression::NonNullAssert { operand, .. } => {
                self.expression(operand, ExpressionUse::Consume, state)
            }
            Expression::Prefix { operand, .. }
            | Expression::Cast {
                expression: operand,
                ..
            }
            | Expression::TypeTest {
                expression: operand,
                ..
            } => self.expression(operand, ExpressionUse::Read, state),
            Expression::Propagate { value, .. } => {
                self.expression(value, ExpressionUse::Read, state)?;
                // Err 是独立退出边；正常路径继续持有原状态。
                self.drop_all(DropPoint::ControlTransfer(id), &mut state.clone());
                Ok(true)
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } if matches!(
                operator,
                BinaryOperator::Add | BinaryOperator::Equal | BinaryOperator::NotEqual
            ) && self.is_string_expression(left)
                && self.is_string_expression(right) =>
            {
                self.string_binary(left, right, id, state)
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::Elvis,
                right,
                ..
            } => self.elvis(id, left, right, state),
            Expression::Binary { left, right, .. } => {
                if !self.expression(left, ExpressionUse::Read, state)? {
                    return Ok(false);
                }
                self.expression(right, ExpressionUse::Read, state)
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => {
                if let Some(descriptor) = self.checker.element_place_descriptor(target)? {
                    self.expression(descriptor.receiver(), ExpressionUse::Place, state)?;
                    self.expression(descriptor.index(), ExpressionUse::Read, state)?;
                    if !self.expression(value, ExpressionUse::Consume, state)? {
                        return Ok(false);
                    }
                    if self.checker.typed.copyability(descriptor.element_type())
                        == Some(Copyability::MoveOnly)
                    {
                        self.push_fact(DropFact::new(
                            DropPoint::AfterReplacement(id),
                            DropTarget::ReplacedElement(id),
                            self.checker.parsed.ast().expressions().get(target)?.span(),
                        ));
                    }
                    if let Some(temporary) = self.checker.temporary_element_owner(target)? {
                        self.push_fact(DropFact::new(
                            DropPoint::AfterExpression(id),
                            DropTarget::Temporary(temporary),
                            self.checker
                                .parsed
                                .ast()
                                .expressions()
                                .get(temporary)?
                                .span(),
                        ));
                    } else if let Some(place) = self.checker.place(target)?
                        && !self.liveness.expression_after[id.index()].contains(&place.root())
                    {
                        self.drop_named(DropPoint::AfterExpression(id), place.root(), state);
                    }
                    return Ok(true);
                }
                let root = self
                    .checker
                    .place(target)?
                    .filter(|place| place.is_root())
                    .map(|place| place.root());
                if let Some(root) = root {
                    state.replacements.push(root);
                }
                let continues = self.expression(value, ExpressionUse::Consume, state)?;
                if root.is_some() {
                    state.replacements.pop();
                }
                if !continues {
                    return Ok(false);
                }
                if let Some(place) = self.checker.place(target)?
                    && place.is_root()
                {
                    let symbol = place.root();
                    let snapshot = if self.checker.is_move_only_variable(symbol) {
                        self.save_result_snapshot(value, state)?
                    } else {
                        None
                    };
                    if operator != AssignmentOperator::Assign {
                        self.expression(target, ExpressionUse::Read, state)?;
                    }
                    let old = state.remove_value(symbol);
                    let previous = state.closures.remove(&symbol).unwrap_or_default();
                    if let Some(old) = &old {
                        // The RHS result still holds its captures while the old environment drops.
                        self.drop_closure_owner(
                            DropFact::new(
                                DropPoint::AfterExpression(value),
                                DropTarget::Named(symbol),
                                old.origin,
                            ),
                            old.condition,
                            &old.versions,
                            previous,
                            state,
                        );
                    }
                    let closures = std::mem::take(&mut state.result_closures);
                    if self.checker.is_move_only_variable(symbol) {
                        let versions = self.bind_result_owners(node.span(), state);
                        state.insert(OwnedValue {
                            versions,
                            condition: state.path,
                            symbol,
                            origin: node.span(),
                            declaration: self.checker.names.symbols()[symbol.index()].span(),
                            scope_depth: old
                                .map(|value| value.scope_depth)
                                .or_else(|| self.binding_depths.get(&symbol).copied())
                                .unwrap_or(self.scope_depth),
                        });
                        if !closures.is_empty() {
                            state.closures.insert(symbol, closures);
                        }
                        self.commit_snapshot(snapshot, value, symbol);
                        if !self.liveness.expression_after[id.index()].contains(&symbol) {
                            self.drop_named(DropPoint::AfterExpression(id), symbol, state);
                        }
                    }
                }
                Ok(true)
            }
            Expression::Member { receiver, .. } => {
                if let Some(place) = self.checker.place(id)? {
                    let root = place.root();
                    if usage == ExpressionUse::Consume && place.is_root() {
                        state.take(root);
                    } else if !self.liveness.expression_after[id.index()].contains(&root) {
                        self.drop_named(DropPoint::AfterExpression(id), root, state);
                    }
                    Ok(true)
                } else {
                    self.expression(receiver, ExpressionUse::Read, state)
                }
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                state.pending_calls.push(pending_call::PendingCall::new(
                    id,
                    self.loop_boundaries.len(),
                ));
                let modes = self.checker.calls_by_expression.get(&id.index()).cloned();
                match self
                    .checker
                    .receivers_by_expression
                    .get(&id.index())
                    .copied()
                {
                    Some(receiver) => {
                        if let CallReceiverOrigin::Expression(expression) = receiver.origin() {
                            let usage = if receiver.mode() == ParameterMode::Value {
                                ExpressionUse::Consume
                            } else {
                                ExpressionUse::Place
                            };
                            if !self.expression(expression, usage, state)? {
                                return Ok(false);
                            }
                            self.register_pending_argument(id, expression, receiver.mode(), state)?;
                        }
                    }
                    None => {
                        if !self.expression(callee, ExpressionUse::Place, state)? {
                            return Ok(false);
                        }
                        self.register_pending_callee(id, callee, state)?;
                    }
                }
                for (index, argument) in arguments.into_iter().enumerate() {
                    let mode = modes
                        .as_ref()
                        .and_then(|modes| modes.get(index))
                        .copied()
                        .unwrap_or(ParameterMode::Borrow);
                    let usage = if mode == ParameterMode::Value {
                        ExpressionUse::Consume
                    } else {
                        ExpressionUse::Place
                    };
                    if !self.expression(argument.value, usage, state)? {
                        return Ok(false);
                    }
                    self.register_pending_argument(id, argument.value, mode, state)?;
                }
                if let Some((callee, closure)) = state
                    .pending_calls
                    .iter()
                    .rev()
                    .find(|frame| frame.call == id)
                    .and_then(|frame| {
                        let environment = frame.closure_environment?;
                        let root = *frame.callees.first()?;
                        state
                            .values
                            .iter()
                            .find(|value| value.symbol == root)
                            .filter(|value| {
                                self.conditions.and(state.path, value.condition) == state.path
                                    && value.versions.iter().any(|version| {
                                        version.owner == environment.0
                                            && self.conditions.and(state.path, version.condition)
                                                == state.path
                                    })
                            })
                            .map(|_| environment)
                    })
                {
                    self.cleanup.push((
                        DropPoint::CallEntry(id),
                        IterationCleanupAction::PassClosureEnvironment { callee, closure },
                    ));
                }
                if self.checker.is_nothing_expression(id) {
                    return Ok(false);
                }
                let roots = self
                    .end_pending_calls(LoanEndPoint::CallReturn(id), state, |call| call.call == id);
                // Value parameters now own their arguments; only borrowed temporaries expire here.
                state
                    .nullable_temporaries
                    .retain(|temporary| temporary.control != id || !temporary.transfers_at_call);
                self.drop_nullable_temporaries(DropPoint::CallReturn(id), state, |temporary| {
                    temporary.control == id
                });
                for root in roots {
                    if !self.liveness.expression_after[id.index()].contains(&root) {
                        self.drop_named(DropPoint::CallReturn(id), root, state);
                    }
                }
                Ok(true)
            }
            Expression::Index { receiver, index } => {
                if !self.expression(receiver, ExpressionUse::Place, state)? {
                    return Ok(false);
                }
                // Index 求值可能 return/abort；此前已求值的 backing owner 仍有清理义务。
                if let Some(owner) = self.checker.temporary_element_owner(id)? {
                    let origin = self.checker.parsed.ast().expressions().get(owner)?.span();
                    let version = self.temporary_owner_version(owner, origin, state);
                    state.nullable_temporaries.push(NullableTemporary {
                        versions: vec![version],
                        closures: Vec::new(),
                        transfers_at_call: false,
                        control: id,
                        subject: owner,
                        origin,
                        loop_depth: self.loop_boundaries.len(),
                        prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
                    });
                }
                if !self.expression(index, ExpressionUse::Read, state)? {
                    return Ok(false);
                }
                state
                    .nullable_temporaries
                    .retain(|temporary| temporary.control != id);
                if usage != ExpressionUse::Place
                    && let Some(temporary) = self.checker.temporary_element_owner(id)?
                {
                    self.push_fact(DropFact::new(
                        DropPoint::AfterExpression(id),
                        DropTarget::Temporary(temporary),
                        self.checker
                            .parsed
                            .ast()
                            .expressions()
                            .get(temporary)?
                            .span(),
                    ));
                } else if usage != ExpressionUse::Place
                    && let Some(place) = self.checker.place(id)?
                    && !self.liveness.expression_after[id.index()].contains(&place.root())
                {
                    self.drop_named(DropPoint::AfterExpression(id), place.root(), state);
                }
                Ok(true)
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.expression(receiver, ExpressionUse::Read, state)
                } else {
                    Ok(true)
                }
            }
        }
    }

    fn is_move_only_temporary(&self, expression: ExpressionId) -> bool {
        self.checker.typed.expression_category(expression) == Some(ExpressionCategory::Temporary)
            && self
                .checker
                .typed
                .expression_type(expression)
                .and_then(|ty| self.checker.typed.copyability(ty))
                == Some(Copyability::MoveOnly)
    }

    fn is_string_expression(&self, expression: ExpressionId) -> bool {
        self.checker
            .typed
            .expression_type(expression)
            .and_then(|ty| self.checker.typed.types().get(ty))
            == Some(&TypeKind::Builtin(BuiltinType::String))
    }

    fn string_binary(
        &mut self,
        left: ExpressionId,
        right: ExpressionId,
        binary: ExpressionId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let (continues, left_drop) = self.string_view_operand(left, state)?;
        if !continues {
            return Ok(false);
        }
        // The right operand can leave this expression before the operation executes.
        // Keep the completed left temporary in the existing control-transfer cleanup stack.
        if let Some(StringOperandDrop::Temporary(subject, origin)) = left_drop {
            state.nullable_temporaries.push(NullableTemporary {
                versions: Vec::new(),
                closures: Vec::new(),
                transfers_at_call: false,
                control: binary,
                subject,
                origin,
                loop_depth: self.loop_boundaries.len(),
                prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
            });
        }
        let (continues, right_drop) = self.string_view_operand(right, state)?;
        // Normal completion uses AfterBinaryOperands below. A terminated path has either
        // already cleaned the obligation at its transfer, or aborted without unwinding.
        state
            .nullable_temporaries
            .retain(|temporary| temporary.control != binary);
        if !continues {
            return Ok(false);
        }
        let point = DropPoint::AfterBinaryOperands(binary);
        for pending in [right_drop, left_drop].into_iter().flatten() {
            match pending {
                StringOperandDrop::Named(symbol) => self.drop_named(point, symbol, state),
                StringOperandDrop::Temporary(expression, origin) => self.push_fact(DropFact::new(
                    point,
                    DropTarget::Temporary(expression),
                    origin,
                )),
            }
        }
        Ok(true)
    }

    fn string_view_operand(
        &mut self,
        expression: ExpressionId,
        state: &mut ValueState,
    ) -> Result<(bool, Option<StringOperandDrop>), OwnershipCheckingError> {
        let node = self.checker.parsed.ast().expressions().get(expression)?;
        if self.checker.is_constant_use(expression) {
            return Ok((
                true,
                Some(StringOperandDrop::Temporary(expression, node.span())),
            ));
        }
        match node.payload() {
            Expression::Group { expression } => self.string_view_operand(*expression, state),
            Expression::Name => {
                let Some(symbol) = self.checker.reference_symbol(node.span()) else {
                    return Ok((true, None));
                };
                Ok((
                    true,
                    (!self.liveness.expression_after[expression.index()].contains(&symbol))
                        .then_some(StringOperandDrop::Named(symbol)),
                ))
            }
            _ => {
                if !self.expression(expression, ExpressionUse::Read, state)? {
                    return Ok((false, None));
                }
                if !self.is_move_only_temporary(expression) {
                    return Ok((true, None));
                }
                Ok((
                    true,
                    Some(StringOperandDrop::Temporary(expression, node.span())),
                ))
            }
        }
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
                    newer && leaving
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
            self.drop_named(DropPoint::BranchExit { control, branch }, symbol, state);
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
            self.drop_named(point, symbol, state);
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
