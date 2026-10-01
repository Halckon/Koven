use std::collections::BTreeMap;

mod closure;
mod construction;
mod container;
mod control;
mod drop_planner;
mod elvis;
mod iteration;
mod loan;
mod nullable_when;
mod rc;

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    diagnostic::{Diagnostic, DiagnosticCode, Severity, codes, ordered_diagnostics},
    name_resolution::{NameResolution, ReferenceTarget, SymbolId, SymbolKind},
    parser::{
        AssignmentOperator, BinaryOperator, Expression, FunctionBody, FunctionForm, Item,
        NameMarker, ParsedFile, Statement, StringPart, VariableKind,
    },
    source::{SourceMap, Span},
    type_checking::{
        AggregateProjectionKind, CallReceiverDescriptor, CallReceiverOrigin, Copyability,
        DestructuringMode, ParameterMode, TypedFile,
    },
};

use super::{
    LoanFact, OwnershipBindingDescriptor, OwnershipBindingKind, OwnershipCheckedFile,
    OwnershipCheckingError, OwnershipDeferredFact, OwnershipDeferredReason, OwnershipPlace,
    capture, model::OwnershipCheckedParts,
};
use loan::ActiveLoan;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    replacements: Vec<SymbolId>,
    nullable_views: BTreeMap<SymbolId, nullable_when::Proof>,
    moved: BTreeMap<SymbolId, Span>,
    loans: Vec<ActiveLoan>,
    closures: BTreeMap<SymbolId, Vec<ExpressionId>>,
    closure_captures: BTreeMap<usize, Vec<ExpressionId>>,
    closure_results: BTreeMap<usize, Vec<ExpressionId>>,
    pending_closures: BTreeMap<usize, Vec<ExpressionId>>,
    non_owning: BTreeMap<SymbolId, Span>,
    immutable_captures: BTreeMap<SymbolId, Span>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpressionUse {
    Read,
    Consume,
    Place,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccessKind {
    Read,
    Move,
    Mutation,
    SharedLoan,
    ExclusiveLoan,
}

#[derive(Default)]
struct Flows {
    next: Option<State>,
    breaks: Option<State>,
    continues: Option<State>,
}

impl Flows {
    fn next(state: State) -> Self {
        Self {
            next: Some(state),
            ..Self::default()
        }
    }

    fn merge(&mut self, other: Self) {
        merge_optional_state(&mut self.next, other.next);
        merge_optional_state(&mut self.breaks, other.breaks);
        merge_optional_state(&mut self.continues, other.continues);
    }
}

pub(super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
) -> Result<OwnershipCheckedFile, OwnershipCheckingError> {
    sources.source_text(parsed.source_id())?;
    Checker::new(sources, parsed, names, typed)?.run()
}

struct Checker<'a> {
    iterations: BTreeMap<usize, super::IterationOwnershipPlan>,
    loop_has_exit: BTreeMap<usize, bool>,
    constant_materializations: BTreeMap<usize, super::ConstantMaterializationPlan>,
    non_null_assertions: BTreeMap<usize, super::NonNullAssertionOwnershipPlan>,
    nullable_whens: BTreeMap<usize, super::NullableWhenOwnershipPlan>,
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
    names: &'a NameResolution,
    typed: &'a TypedFile,
    symbols_by_span: BTreeMap<(usize, usize), SymbolId>,
    references_by_span: BTreeMap<(usize, usize), SymbolId>,
    calls_by_expression: BTreeMap<usize, Vec<ParameterMode>>,
    receivers_by_expression: BTreeMap<usize, CallReceiverDescriptor>,
    construction: construction::Analysis,
    rc_effects: Vec<super::RcOwnershipEffect>,
    cross_thread_by_expression: BTreeMap<usize, Vec<bool>>,
    variable_kinds: BTreeMap<SymbolId, VariableKind>,
    field_kinds: BTreeMap<SymbolId, VariableKind>,
    current_receiver_mode: Option<ParameterMode>,
    diagnostics: Vec<Diagnostic>,
    loans: Vec<LoanFact>,
    deferred: Vec<OwnershipDeferredFact>,
    captures: Vec<super::ClosureCaptureDescriptor>,
    closures: Vec<super::ClosureDescriptor>,
    transferabilities: Vec<super::Transferability>,
    expression_live_after: Vec<std::collections::BTreeSet<SymbolId>>,
    statement_live_after: Vec<std::collections::BTreeSet<SymbolId>>,
    use_after_move_code: DiagnosticCode,
    partial_move_code: DiagnosticCode,
    borrowed_move_code: DiagnosticCode,
    immutable_inout_code: DiagnosticCode,
    loan_conflict_code: DiagnosticCode,
    container_element_move_code: DiagnosticCode,
    borrowed_closure_escape_code: DiagnosticCode,
    illegal_owned_capture_code: DiagnosticCode,
    non_transferable_delivery_code: DiagnosticCode,
}

impl<'a> Checker<'a> {
    fn new(
        sources: &'a SourceMap,
        parsed: &'a ParsedFile,
        names: &'a NameResolution,
        typed: &'a TypedFile,
    ) -> Result<Self, OwnershipCheckingError> {
        let symbols_by_span = names
            .symbols()
            .iter()
            .map(|symbol| (span_key(symbol.span()), symbol.id()))
            .collect();
        let references_by_span = names
            .references()
            .iter()
            .filter_map(|reference| match reference.target() {
                ReferenceTarget::Symbol(symbol) => Some((span_key(reference.span()), *symbol)),
                _ => None,
            })
            .collect();
        let mut calls_by_expression: BTreeMap<usize, Vec<ParameterMode>> = typed
            .calls()
            .iter()
            .map(|call| {
                let mut modes = vec![ParameterMode::Borrow; call.arguments().len()];
                for argument in call.arguments() {
                    modes[argument.argument_index()] = argument.mode();
                }
                (call.expression().index(), modes)
            })
            .collect();
        let cross_thread_by_expression = typed
            .calls()
            .iter()
            .map(|call| {
                let mut effects = vec![false; call.arguments().len()];
                for argument in call.arguments() {
                    effects[argument.argument_index()] = argument.crosses_thread();
                }
                (call.expression().index(), effects)
            })
            .collect();
        let receivers_by_expression = typed
            .calls()
            .iter()
            .filter_map(|call| {
                call.receiver()
                    .map(|receiver| (call.expression().index(), receiver))
            })
            .collect();
        for construction in typed.container_constructions() {
            calls_by_expression
                .entry(construction.expression().index())
                .or_insert_with(|| construction.parameter_modes().to_vec());
        }
        let capture::Analysis {
            captures,
            closures,
            transferabilities,
        } = capture::analyze(parsed, names, typed)?;
        let mut checker = Self {
            iterations: BTreeMap::new(),
            sources,
            parsed,
            names,
            typed,
            symbols_by_span,
            references_by_span,
            calls_by_expression,
            receivers_by_expression,
            construction: construction::Analysis::new(parsed, typed)?,
            rc_effects: Vec::new(),
            cross_thread_by_expression,
            variable_kinds: BTreeMap::new(),
            field_kinds: BTreeMap::new(),
            current_receiver_mode: None,
            loop_has_exit: BTreeMap::new(),
            constant_materializations: BTreeMap::new(),
            nullable_whens: BTreeMap::new(),
            non_null_assertions: BTreeMap::new(),
            diagnostics: Vec::new(),
            loans: Vec::new(),
            deferred: Vec::new(),
            captures,
            closures,
            transferabilities,
            expression_live_after: Vec::new(),
            statement_live_after: Vec::new(),
            use_after_move_code: codes::catalog()?.resolve(codes::USE_AFTER_MOVE)?,
            partial_move_code: codes::catalog()?.resolve(codes::PARTIAL_MOVE)?,
            borrowed_move_code: codes::catalog()?.resolve(codes::MOVE_FROM_BORROWED_BINDING)?,
            immutable_inout_code: codes::catalog()?.resolve(codes::IMMUTABLE_INOUT_PLACE)?,
            loan_conflict_code: codes::catalog()?.resolve(codes::LOAN_CONFLICT)?,
            container_element_move_code: codes::catalog()?
                .resolve(codes::MOVE_FROM_CONTAINER_ELEMENT)?,
            borrowed_closure_escape_code: codes::catalog()?
                .resolve(codes::BORROWED_CLOSURE_ESCAPE)?,
            illegal_owned_capture_code: codes::catalog()?.resolve(codes::ILLEGAL_OWNED_CAPTURE)?,
            non_transferable_delivery_code: codes::catalog()?
                .resolve(codes::NON_TRANSFERABLE_DELIVERY)?,
        };
        for (item, _) in parsed.ast().items().iter() {
            checker.collect_mutability(item)?;
        }
        Ok(checker)
    }

    fn run(mut self) -> Result<OwnershipCheckedFile, OwnershipCheckingError> {
        let liveness = drop_planner::capture_liveness(&self)?;
        self.expression_live_after = liveness.expression_after;
        self.statement_live_after = liveness.statement_after;
        let mut state = State::default();
        for &root in self.parsed.roots() {
            self.check_item(root, &mut state)?;
        }
        let diagnostics = ordered_diagnostics(self.sources, &self.diagnostics)?
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        if !diagnostics.is_empty() {
            self.loans.clear();
            self.rc_effects.clear();
        }
        let bindings = self
            .typed
            .parameter_bindings()
            .iter()
            .map(|binding| {
                let kind = match binding.mode() {
                    ParameterMode::Value => OwnershipBindingKind::Owned,
                    ParameterMode::Borrow => OwnershipBindingKind::Shared,
                    ParameterMode::Inout => OwnershipBindingKind::Exclusive,
                };
                OwnershipBindingDescriptor::new(binding.symbol(), kind)
            })
            .collect();
        let drop_plan = if diagnostics.is_empty() {
            drop_planner::plan(&self)?
        } else {
            drop_planner::DropPlan::default()
        };
        let drop_planner::DropPlan {
            cleanup_steps,
            cleanup_conditions,
            drops,
            loan_ends,
            iterations,
            deferred,
        } = drop_plan;
        self.deferred.extend(deferred);
        self.finish_nullable_drops(&drops)?;
        let constant_materializations = if diagnostics.is_empty()
            && self.deferred.is_empty()
            && self.typed.constants().is_some()
        {
            Some(super::ValidatedConstantMaterializations {
                typed_analysis_owner: self.typed.analysis_owner().clone(),
                plans: self.constant_materializations.into_values().collect(),
            })
        } else {
            None
        };
        let construction_plans = self.construction.finish(diagnostics.is_empty());
        let non_null_assertions = if diagnostics.is_empty() {
            self.non_null_assertions.into_values().collect()
        } else {
            Vec::new()
        };
        let nullable_whens = if diagnostics.is_empty() {
            self.nullable_whens.into_values().collect()
        } else {
            Vec::new()
        };
        let captures = if diagnostics.is_empty() {
            self.captures
        } else {
            Vec::new()
        };
        Ok(OwnershipCheckedFile::new(
            self.parsed.source_id(),
            self.typed.environment_owner().clone(),
            self.typed.analysis_owner().clone(),
            diagnostics,
            OwnershipCheckedParts {
                cleanup_steps,
                cleanup_conditions,
                iterations,
                constant_materializations,
                non_null_assertions,
                nullable_whens,
                loan_ends,
                bindings,
                loans: self.loans,
                drops,
                construction_plans,
                rc_effects: self.rc_effects,
                captures,
                closures: self.closures,
                transferabilities: self.transferabilities,
                deferred: self.deferred,
            },
        ))
    }

    fn collect_mutability(&mut self, id: ItemId) -> Result<(), OwnershipCheckingError> {
        match self.parsed.ast().items().get(id)?.payload().clone() {
            Item::Error | Item::Constant { .. } | Item::Function { .. } | Item::Deinit { .. } => {}
            Item::Modified { declaration, .. } => self.collect_mutability(declaration)?,
            Item::Variable { kind, name, .. } => {
                if let Some(symbol) = self.marker_symbol(name) {
                    self.variable_kinds.insert(symbol, kind);
                }
            }
            Item::Classifier(classifier) => {
                if let Some(constructor) = classifier.primary_constructor {
                    for field in constructor.fields {
                        if let Some(symbol) = self.marker_symbol(field.name) {
                            self.field_kinds.insert(symbol, field.kind);
                        }
                    }
                }
                if let Some(body) = classifier.body {
                    for member in body.members {
                        self.collect_mutability(member)?;
                    }
                }
            }
            Item::Companion(companion) => {
                for member in companion.body.members {
                    self.collect_mutability(member)?;
                }
            }
        }
        Ok(())
    }

    fn check_item(&mut self, id: ItemId, state: &mut State) -> Result<(), OwnershipCheckingError> {
        match self.parsed.ast().items().get(id)?.payload().clone() {
            Item::Error | Item::Constant { .. } => {}
            Item::Modified { declaration, .. } => self.check_item(declaration, state)?,
            Item::Variable {
                name, initializer, ..
            } => {
                if let Some(next) = self.check_variable(name, initializer, state.clone())?.next {
                    *state = next;
                }
            }
            Item::Function {
                name,
                parameters,
                form,
                ..
            } => {
                let mut function_state = State::default();
                for parameter in parameters {
                    self.mark_available(parameter.name, &mut function_state);
                }
                let receiver_mode = self.marker_symbol(name).and_then(|symbol| {
                    self.typed
                        .callables()
                        .iter()
                        .find(|callable| callable.symbol() == symbol)
                        .and_then(|callable| callable.receiver())
                        .map(|receiver| receiver.mode())
                });
                let previous = std::mem::replace(&mut self.current_receiver_mode, receiver_mode);
                let result = self.check_function(form, function_state);
                self.current_receiver_mode = previous;
                result?;
            }
            Item::Classifier(classifier) => {
                if let Some(body) = classifier.body {
                    for member in body.members {
                        self.check_item(member, &mut State::default())?;
                    }
                }
            }
            Item::Companion(companion) => {
                for member in companion.body.members {
                    self.check_item(member, &mut State::default())?;
                }
            }
            Item::Deinit { body, .. } => {
                let previous = std::mem::replace(&mut self.current_receiver_mode, Some(ParameterMode::Borrow));
                let result = self.check_statement(body, State::default());
                self.current_receiver_mode = previous;
                result?;
            }
        }
        Ok(())
    }

    /// Only a normally completed initializer establishes a binding; preserve all other edges.
    fn check_variable(
        &mut self,
        name: NameMarker,
        initializer: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let moved_closure = self.expression_root_symbol(initializer)?;
        let mut flows = self.check_expression(initializer, state, ExpressionUse::Consume)?;
        if let Some(next) = flows.next.as_mut() {
            let closures = self.closure_origins(initializer, next)?;
            self.mark_available(name, next);
            if let Some(source) = moved_closure {
                next.closures.remove(&source);
            }
            if let Some(symbol) = self.marker_symbol(name)
                && !closures.is_empty()
            {
                next.closures.insert(symbol, closures);
            }
        }
        Ok(flows)
    }

    fn check_function(
        &mut self,
        form: FunctionForm,
        state: State,
    ) -> Result<(), OwnershipCheckingError> {
        match form {
            FunctionForm::ImplicitUnitAbsent => {}
            FunctionForm::ImplicitUnitBlock(body) => {
                self.check_statement(body, state)?;
            }
            FunctionForm::Explicit { body, .. } => match body {
                FunctionBody::Absent => {}
                FunctionBody::Expression { expression, .. } => {
                    self.check_return_expression(expression, state, ExpressionUse::Consume)?;
                }
                FunctionBody::Block(body) => {
                    self.check_statement(body, state)?;
                }
            },
        }
        Ok(())
    }

    fn check_statement(
        &mut self,
        id: StatementId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        match self.parsed.ast().statements().get(id)?.payload().clone() {
            Statement::Error => Ok(Flows::next(state)),
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => self.check_elements(&elements, state),
            Statement::LocalVariable { declaration } => {
                let Item::Variable {
                    name, initializer, ..
                } = self
                    .parsed
                    .ast()
                    .items()
                    .get(declaration)?
                    .payload()
                    .clone()
                else {
                    return Ok(Flows::next(state));
                };
                let mut flows = self.check_variable(name, initializer, state)?;
                if let Some(next) = flows.next.as_mut()
                    && let Some(symbol) = self.marker_symbol(name)
                    && !self.statement_live_after[id.index()].contains(&symbol)
                {
                    self.release_closure(symbol, next);
                }
                Ok(flows)
            }
            Statement::LocalDestructuring { initializer, .. } => {
                self.check_destructuring(id, initializer, state)
            }
            Statement::While {
                condition, body, ..
            } => {
                let errors = self.diagnostics.len();
                let condition = self.check_expression(condition, state, ExpressionUse::Read)?;
                let mut flows = self.check_maybe_loop(condition, body, errors)?;
                self.release_dead_loop_closures(id, &mut flows);
                Ok(flows)
            }
            Statement::For { source, body, .. } => {
                let mut flows = self.check_iteration(id, source, body, state)?;
                self.release_dead_loop_closures(id, &mut flows);
                Ok(flows)
            }
            Statement::Loop { body, .. } => {
                let errors = self.diagnostics.len();
                let body_id = body;
                let body = self.check_statement(body, state)?;
                if self.diagnostics.len() == errors {
                    self.check_loop_backedge(body_id, &body)?;
                }
                self.loop_has_exit.insert(id.index(), body.breaks.is_some());
                let mut flows = Flows {
                    next: body.breaks,
                    breaks: None,
                    continues: None,
                };
                self.release_dead_loop_closures(id, &mut flows);
                Ok(flows)
            }
            Statement::Expression { expression } => {
                self.check_expression(expression, state, ExpressionUse::Read)
            }
        }
    }

    fn check_elements(
        &mut self,
        elements: &[StatementId],
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let mut flows = Flows::next(state);
        for &element in elements {
            let Some(next) = flows.next.take() else {
                break;
            };
            flows.merge(self.check_statement(element, next)?);
        }
        Ok(flows)
    }

    fn check_maybe_loop(
        &mut self,
        mut prefix: Flows,
        body: StatementId,
        errors: usize,
    ) -> Result<Flows, OwnershipCheckingError> {
        let Some(base) = prefix.next.take() else {
            return Ok(prefix);
        };
        let body_id = body;
        let body = self.check_statement(body, base.clone())?;
        if self.diagnostics.len() == errors {
            self.check_loop_backedge(body_id, &body)?;
        }
        let mut next = base;
        for state in [body.next, body.breaks, body.continues]
            .into_iter()
            .flatten()
        {
            merge_state(&mut next, state);
        }
        prefix.next = Some(next);
        Ok(prefix)
    }

    fn check_expression(
        &mut self,
        id: ExpressionId,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        if let Some(descriptor) = self
            .typed
            .constants()
            .and_then(|constants| constants.use_at(id))
        {
            let kind = if matches!(
                descriptor.value(),
                crate::type_checking::ConstValue::String(_)
            ) {
                super::ConstantMaterializationKind::StringTemporary
            } else {
                super::ConstantMaterializationKind::InlineCopy
            };
            self.constant_materializations.insert(
                id.index(),
                super::ConstantMaterializationPlan {
                    descriptor: descriptor.clone(),
                    kind,
                },
            );
            return Ok(Flows::next(state));
        }
        if let Some(descriptor) = self.construction.descriptor(id) {
            return self.check_construction(descriptor, state, usage);
        }
        if let Some(descriptor) = self.typed.rc_operation(id) {
            return self.check_rc_operation(descriptor, state, usage);
        }
        let node = self.parsed.ast().expressions().get(id)?;
        let span = node.span();
        match node.payload().clone() {
            Expression::Error
            | Expression::This
            | Expression::Literal(_)
            | Expression::SuperMember { .. } => Ok(Flows::next(state)),
            Expression::Name => {
                let mut state = state;
                let proof = self
                    .reference_symbol(span)
                    .and_then(|symbol| state.nullable_views.get(&symbol).cloned());
                let errors = self.diagnostics.len();
                self.use_name(span, usage, &mut state)?;
                if errors == self.diagnostics.len() {
                    self.record_nullable_extraction(id, usage, proof);
                }
                Ok(Flows::next(state))
            }
            Expression::Group { expression } => self.check_expression(expression, state, usage),
            Expression::String { parts } => {
                let mut flows = Flows::next(state);
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        flows = self.chain_expression(flows, expression, ExpressionUse::Read)?;
                    }
                }
                Ok(flows)
            }
            Expression::Lambda {
                parameters, body, ..
            } => self.check_lambda(id, &parameters, body, state),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.check_if(id, condition, then_branch, else_branch, state, false),
            Expression::When {
                subject, entries, ..
            } => self.check_when(id, subject, &entries, state, false),
            Expression::Return { value, .. } => {
                let mut flows = Flows::next(state);
                if let Some(value) = value
                    && let Some(next) = flows.next.take()
                {
                    flows.merge(self.check_return_expression(
                        value,
                        next,
                        ExpressionUse::Consume,
                    )?);
                }
                flows.next = None;
                Ok(flows)
            }
            Expression::Break { .. } => Ok(Flows {
                breaks: Some(state),
                ..Flows::default()
            }),
            Expression::Continue { .. } => Ok(Flows {
                continues: Some(state),
                ..Flows::default()
            }),
            Expression::NonNullAssert { operand, .. } => {
                // Only the successful continuation consumes; null directly aborts without cleanup.
                let descriptor = self.typed.non_null_assertion(id).copied();
                let usage = if descriptor.is_some_and(|d| d.copyability() == Copyability::MoveOnly)
                {
                    ExpressionUse::Consume
                } else {
                    ExpressionUse::Read
                };
                let diagnostic_count = self.diagnostics.len();
                let flows = self.check_expression(operand, state, usage)?;
                if flows.next.is_some()
                    && self.diagnostics.len() == diagnostic_count
                    && let Some(descriptor) = descriptor
                {
                    self.non_null_assertions.insert(
                        id.index(),
                        super::NonNullAssertionOwnershipPlan {
                            descriptor,
                            source_place: self.place(operand)?,
                            non_null_transfer: if usage == ExpressionUse::Consume {
                                super::NonNullAssertionTransferKind::Consume
                            } else {
                                super::NonNullAssertionTransferKind::Copy
                            },
                        },
                    );
                }
                Ok(flows)
            }
            Expression::Prefix { operand, .. }
            | Expression::Cast {
                expression: operand,
                ..
            }
            | Expression::TypeTest {
                expression: operand,
                ..
            }
            | Expression::Propagate { value: operand, .. } => {
                self.check_expression(operand, state, ExpressionUse::Read)
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::Elvis,
                right,
                ..
            } => self.check_elvis(id, left, right, state, false),
            Expression::Binary { left, right, .. } => {
                let flows = self.check_expression(left, state, ExpressionUse::Read)?;
                self.chain_expression(flows, right, ExpressionUse::Read)
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => {
                if self.element_place_descriptor(target)?.is_some() {
                    return self.check_element_assignment(target, operator, value, state);
                }
                let diagnostic_count = self.diagnostics.len();
                let root = self
                    .place(target)?
                    .filter(|place| place.is_root())
                    .map(|place| place.root());
                let mut state = state;
                if let Some(root) = root {
                    state.replacements.push(root);
                }
                // Inout storage belongs to the caller, so RHS capture loans cannot escape into it.
                let mut flows = if self.place(target)?.is_some_and(|place| {
                    !place.is_root()
                        || self.typed.parameter_mode(place.root()) == Some(ParameterMode::Inout)
                }) {
                    self.check_return_expression(value, state, ExpressionUse::Consume)?
                } else {
                    self.check_expression(value, state, ExpressionUse::Consume)?
                };
                if root.is_some() {
                    for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
                        .into_iter()
                        .flatten()
                    {
                        state.replacements.pop();
                    }
                }
                self.finish_assignment(
                    flows,
                    id,
                    target,
                    operator,
                    value,
                    self.diagnostics.len() == diagnostic_count,
                )
            }
            Expression::Member {
                receiver,
                name_span,
                ..
            } => {
                let diagnostic_count = self.diagnostics.len();
                let mut state = state;
                let flows = if self.place(id)?.is_some() {
                    let access = match usage {
                        ExpressionUse::Read => Some(AccessKind::Read),
                        ExpressionUse::Consume => Some(AccessKind::Move),
                        ExpressionUse::Place => None,
                    };
                    if let Some(access) = access {
                        self.access_expression_place(id, access, name_span, &mut state)?;
                    } else if let Some(place) = self.place(id)? {
                        self.ensure_place_available(&place, name_span, &state)?;
                    }
                    Flows::next(state)
                } else if self.typed.aggregate_projection(id).is_some()
                    && self.element_place_descriptor(receiver)?.is_some()
                {
                    self.defer(id, OwnershipDeferredReason::IndexPlace);
                    self.check_expression(receiver, state, ExpressionUse::Place)?
                } else {
                    self.defer(id, OwnershipDeferredReason::MemberReceiver);
                    self.check_expression(receiver, state, ExpressionUse::Read)?
                };
                if matches!(usage, ExpressionUse::Consume)
                    && flows.next.is_some()
                    && self.diagnostics.len() == diagnostic_count
                    && self.place(id)?.is_some()
                {
                    self.reject_partial_move(id, name_span)?;
                }
                Ok(flows)
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                let diagnostic_count = self.diagnostics.len();
                let receiver = self.receivers_by_expression.get(&id.index()).copied();
                let mut receiver_expression = None;
                let mut flows = match receiver.map(CallReceiverDescriptor::origin) {
                    Some(CallReceiverOrigin::Expression(expression)) => {
                        receiver_expression = Some(expression);
                        let usage = if receiver
                            .is_some_and(|receiver| receiver.mode() == ParameterMode::Value)
                        {
                            ExpressionUse::Consume
                        } else {
                            ExpressionUse::Place
                        };
                        self.check_expression(expression, state, usage)?
                    }
                    Some(CallReceiverOrigin::ImplicitThis(_)) => {
                        if receiver.is_some_and(|receiver| receiver.mode() == ParameterMode::Inout)
                            && self.current_receiver_mode != Some(ParameterMode::Inout)
                        {
                            self.diagnostics.push(Diagnostic::new(
                                self.sources,
                                Severity::Error,
                                self.immutable_inout_code,
                                "current this cannot supply an inout receiver",
                                self.parsed.ast().expressions().get(id)?.span(),
                            )?);
                        } else if receiver.is_some_and(|receiver| {
                            receiver.mode() == ParameterMode::Value
                                && self.typed.copyability(receiver.ty())
                                    == Some(Copyability::MoveOnly)
                        }) && self.current_receiver_mode != Some(ParameterMode::Value)
                        {
                            self.diagnostics.push(Diagnostic::new(
                                self.sources,
                                Severity::Error,
                                self.borrowed_move_code,
                                "cannot move this from a non-owning receiver",
                                self.parsed.ast().expressions().get(id)?.span(),
                            )?);
                        }
                        Flows::next(state)
                    }
                    None => self.check_expression(callee, state, ExpressionUse::Read)?,
                };
                if let (Some(receiver), Some(expression)) = (receiver, receiver_expression) {
                    let span = self.parsed.ast().expressions().get(expression)?.span();
                    self.apply_argument_contract(
                        id,
                        crate::parser::CallArgument {
                            span,
                            named_prefix: None,
                            mode_marker: None,
                            value: expression,
                        },
                        receiver.mode(),
                        true,
                        &mut flows,
                    )?;
                }
                self.hold_call_closures(id, receiver_expression.unwrap_or(callee), &mut flows)?;
                let modes = self.calls_by_expression.get(&id.index()).cloned();
                let cross_thread = self.cross_thread_by_expression.get(&id.index()).cloned();
                let argument_expressions = arguments
                    .iter()
                    .map(|argument| argument.value)
                    .collect::<Vec<_>>();
                for (index, argument) in arguments.into_iter().enumerate() {
                    let mode = modes.as_ref().and_then(|modes| modes.get(index)).copied();
                    let crosses_thread = cross_thread
                        .as_ref()
                        .and_then(|effects| effects.get(index))
                        .copied()
                        .unwrap_or(false);
                    let usage = match mode {
                        Some(ParameterMode::Value) => ExpressionUse::Consume,
                        Some(ParameterMode::Borrow | ParameterMode::Inout) => ExpressionUse::Place,
                        None => ExpressionUse::Read,
                    };
                    let argument_diagnostics = self.diagnostics.len();
                    flows = if mode == Some(ParameterMode::Value) && !crosses_thread {
                        self.chain_escaping_expression(flows, argument.value, usage)?
                    } else {
                        self.chain_expression(flows, argument.value, usage)?
                    };
                    self.hold_call_closures(id, argument.value, &mut flows)?;
                    if crosses_thread
                        && self.diagnostics.len() == argument_diagnostics
                        && let Some(next) = flows.next.as_ref()
                    {
                        self.check_cross_thread_delivery(argument.value, next)?;
                    }
                    if self.diagnostics.len() == argument_diagnostics
                        && let Some(mode) = mode
                    {
                        self.apply_argument_contract(id, argument, mode, false, &mut flows)?;
                    }
                }
                self.end_call_loans(id, &mut flows);
                self.finish_call_closures(id, &mut flows);
                for expression in receiver_expression
                    .into_iter()
                    .chain(receiver.is_none().then_some(callee))
                    .chain(argument_expressions)
                {
                    self.release_last_closure_use(expression, &mut flows)?;
                }
                if flows.next.is_some()
                    && self.diagnostics.len() == diagnostic_count
                    && self
                        .typed
                        .aggregate_projection(id)
                        .is_some_and(|projection| {
                            projection.kind() == AggregateProjectionKind::StructuralComponent
                        })
                {
                    let callee = self.parsed.ast().expressions().get(callee)?;
                    if let Expression::Member { name_span, .. } = callee.payload() {
                        self.reject_partial_move(id, *name_span)?;
                    }
                }
                if self.is_nothing_expression(id) {
                    flows.next = None;
                }
                Ok(flows)
            }
            Expression::Index { receiver, index } => {
                if self.element_place_descriptor(id)?.is_some() {
                    self.check_element_expression(id, state, usage)
                } else {
                    self.defer(id, OwnershipDeferredReason::IndexPlace);
                    let flows = self.check_expression(receiver, state, ExpressionUse::Read)?;
                    self.chain_expression(flows, index, ExpressionUse::Read)
                }
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.check_expression(receiver, state, ExpressionUse::Read)
                } else {
                    Ok(Flows::next(state))
                }
            }
        }
    }

    fn chain_expression(
        &mut self,
        mut flows: Flows,
        id: ExpressionId,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        if let Some(next) = flows.next.take() {
            flows.merge(self.check_expression(id, next, usage)?);
        }
        Ok(flows)
    }

    fn check_destructuring(
        &mut self,
        statement: StatementId,
        initializer: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let Some(descriptor) = self.typed.destructuring(statement) else {
            return self.check_expression(initializer, state, ExpressionUse::Read);
        };
        let usage = match descriptor.mode() {
            DestructuringMode::Copy => ExpressionUse::Read,
            DestructuringMode::Consume => ExpressionUse::Consume,
        };
        let bindings = descriptor
            .components()
            .iter()
            .map(|component| component.symbol())
            .collect::<Vec<_>>();
        let mut flows = self.check_expression(initializer, state, usage)?;
        if let Some(state) = flows.next.as_mut() {
            for binding in bindings {
                state.moved.remove(&binding);
            }
        }
        Ok(flows)
    }

    fn finish_assignment(
        &mut self,
        flows: Flows,
        assignment: ExpressionId,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        value_is_valid: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let target_node = self.parsed.ast().expressions().get(target)?;
        let mut flows = flows;
        if let Some(state) = flows.next.as_mut()
            && let Some(place) = self.place(target)?
        {
            let span = target_node.span();
            let allowed = if operator == AssignmentOperator::Assign {
                self.access_place(&place, AccessKind::Mutation, false, span, state)?
            } else {
                self.access_place(&place, AccessKind::Read, false, span, state)?
                    && self.access_place(&place, AccessKind::Mutation, false, span, state)?
            };
            if allowed
                && value_is_valid
                && operator == AssignmentOperator::Assign
                && place.is_root()
            {
                let origins = self.closure_origins(value, state)?;
                // RHS has completed: replace the environment without ending a transferred loan.
                self.release_closure_except(place.root(), state, &origins);
                if let Some(source) = self.expression_root_symbol(value)? {
                    state.closures.remove(&source);
                }
                if !origins.is_empty() {
                    state.closures.insert(place.root(), origins);
                }
                state.moved.remove(&place.root());
                if !self.expression_live_after[assignment.index()].contains(&place.root()) {
                    self.release_closure(place.root(), state);
                }
            }
            return Ok(flows);
        }
        self.chain_expression(flows, target, ExpressionUse::Read)
    }

    /// Group forwards the checked constant value, without introducing a second temporary owner.
    fn constant_temporary_origin(
        &self,
        mut expression: ExpressionId,
    ) -> Option<(ExpressionId, Span)> {
        loop {
            let node = self.parsed.ast().expressions().get(expression).ok()?;
            if self.is_constant_use(expression) {
                return Some((expression, node.span()));
            }
            let Expression::Group { expression: inner } = node.payload() else {
                return None;
            };
            expression = *inner;
        }
    }

    /// Typed constant reads produce values, never runtime declaration places or receivers.
    fn is_constant_use(&self, expression: ExpressionId) -> bool {
        self.typed
            .constants()
            .is_some_and(|constants| constants.use_at(expression).is_some())
    }

    fn use_name(
        &mut self,
        span: Span,
        usage: ExpressionUse,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(symbol) = self.reference_symbol(span) else {
            return Ok(());
        };
        if usage == ExpressionUse::Place {
            let place = OwnershipPlace::new(symbol, Vec::new());
            return self.ensure_place_available(&place, span, state);
        }
        let place = OwnershipPlace::new(symbol, Vec::new());
        let move_only = self
            .typed
            .symbol_type(symbol)
            .and_then(|ty| self.typed.copyability(ty))
            == Some(Copyability::MoveOnly);
        let access = if usage == ExpressionUse::Consume && move_only {
            AccessKind::Move
        } else {
            AccessKind::Read
        };
        if !self.access_place(&place, access, move_only, span, state)? {
            return Ok(());
        }
        if let Some(origin) = state.moved.get(&symbol).copied() {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.use_after_move_code,
                "use of moved value",
                span,
            )?;
            diagnostic.add_label(self.sources, origin, "value was moved here")?;
            self.diagnostics.push(diagnostic);
            return Ok(());
        }
        if matches!(usage, ExpressionUse::Consume) && self.is_move_only_variable(symbol) {
            state.moved.insert(symbol, span);
            state.nullable_views.remove(&symbol);
        }
        Ok(())
    }

    fn ensure_place_available(
        &mut self,
        place: &OwnershipPlace,
        primary: Span,
        state: &State,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(origin) = state.moved.get(&place.root()).copied() else {
            return Ok(());
        };
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.use_after_move_code,
            "use of moved value",
            primary,
        )?;
        diagnostic.add_label(self.sources, origin, "value was moved here")?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn reject_partial_move(
        &mut self,
        expression: ExpressionId,
        primary: Span,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(projection) = self.typed.aggregate_projection(expression) else {
            return Ok(());
        };
        if self.typed.copyability(projection.ty()) != Some(Copyability::MoveOnly) {
            return Ok(());
        }
        let Some(field) = self.names.symbols().get(projection.field().index()) else {
            return Ok(());
        };
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.partial_move_code,
            "cannot move a non-Copyable component out of its owner",
            primary,
        )?;
        diagnostic.add_label(
            self.sources,
            field.span(),
            "non-Copyable component declared here",
        )?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn is_move_only_variable(&self, symbol: SymbolId) -> bool {
        let Some(symbol_data) = self.names.symbols().get(symbol.index()) else {
            return false;
        };
        if !matches!(
            symbol_data.kind(),
            SymbolKind::Variable
                | SymbolKind::ValueParameter
                | SymbolKind::LambdaParameter
                | SymbolKind::ForBinding
                | SymbolKind::DestructuringBinding
        ) {
            return false;
        }
        if matches!(
            symbol_data.kind(),
            SymbolKind::ValueParameter | SymbolKind::LambdaParameter
        ) && self.typed.parameter_mode(symbol) != Some(ParameterMode::Value)
        {
            return false;
        }
        self.typed
            .symbol_type(symbol)
            .and_then(|ty| self.typed.copyability(ty))
            == Some(Copyability::MoveOnly)
    }

    fn mark_available(&self, marker: NameMarker, state: &mut State) {
        if let Some(symbol) = self.marker_symbol(marker) {
            state.moved.remove(&symbol);
        }
    }

    fn marker_symbol(&self, marker: NameMarker) -> Option<SymbolId> {
        let NameMarker::Present(span) = marker else {
            return None;
        };
        self.symbols_by_span.get(&span_key(span)).copied()
    }

    fn reference_symbol(&self, span: Span) -> Option<SymbolId> {
        self.references_by_span.get(&span_key(span)).copied()
    }

    pub(super) fn captures_of(
        &self,
        lambda: ExpressionId,
    ) -> impl Iterator<Item = super::ClosureCaptureDescriptor> + '_ {
        self.captures
            .iter()
            .copied()
            .filter(move |capture| capture.lambda() == lambda)
    }
}

fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}

fn merge_optional_state(target: &mut Option<State>, source: Option<State>) {
    let Some(source) = source else {
        return;
    };
    if let Some(target) = target {
        merge_state(target, source);
    } else {
        *target = Some(source);
    }
}

fn merge_state(target: &mut State, source: State) {
    target
        .nullable_views
        .retain(|symbol, proof| source.nullable_views.get(symbol) == Some(proof));
    // A loan active on either incoming edge can conflict after the join.
    for loan in source.loans {
        if !target.loans.contains(&loan) {
            target.loans.push(loan);
        }
    }
    for (symbol, origins) in source.closures {
        let merged = target.closures.entry(symbol).or_default();
        merged.extend(origins);
        merged.sort_by_key(|origin| origin.index());
        merged.dedup();
    }
    for (closure, origins) in source.closure_captures {
        let merged = target.closure_captures.entry(closure).or_default();
        merged.extend(origins);
        merged.sort_by_key(|origin| origin.index());
        merged.dedup();
    }
    for (call, origins) in source.pending_closures {
        let merged = target.pending_closures.entry(call).or_default();
        merged.extend(origins);
        merged.sort_by_key(|origin| origin.index());
        merged.dedup();
    }
    for (expression, origins) in source.closure_results {
        let merged = target.closure_results.entry(expression).or_default();
        merged.extend(origins);
        merged.sort_by_key(|origin| origin.index());
        merged.dedup();
    }
    target
        .non_owning
        .retain(|symbol, origin| source.non_owning.get(symbol) == Some(origin));
    target
        .immutable_captures
        .retain(|symbol, origin| source.immutable_captures.get(symbol) == Some(origin));
    for (symbol, origin) in source.moved {
        target
            .moved
            .entry(symbol)
            .and_modify(|current| {
                if origin.start() < current.start() {
                    *current = origin;
                }
            })
            .or_insert(origin);
    }
}
