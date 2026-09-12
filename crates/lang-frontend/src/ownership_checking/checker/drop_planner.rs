use std::collections::BTreeMap;

mod control;
mod liveness;
mod pending_call;

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
    DropTarget, LoanEndFact, LoanEndPoint, LoanTarget,
};

use super::{Checker, ExpressionUse, OwnershipCheckingError};

use self::liveness::Liveness;

pub(super) fn plan(
    checker: &Checker<'_>,
) -> Result<(Vec<DropFact>, Vec<LoanEndFact>), OwnershipCheckingError> {
    let liveness = Liveness::build(checker)?;
    DropPlanner::new(checker, liveness).run()
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnedValue {
    symbol: SymbolId,
    origin: Span,
    scope_depth: usize,
}

#[derive(Clone, Debug)]
struct NullableTemporary {
    transfers_at_call: bool,
    control: ExpressionId,
    subject: ExpressionId,
    origin: Span,
    loop_depth: usize,
    prior_symbols: Vec<SymbolId>,
}

#[derive(Clone, Debug, Default)]
struct ValueState {
    nullable_temporaries: Vec<NullableTemporary>,
    pending_calls: Vec<pending_call::PendingCall>,
    values: Vec<OwnedValue>,
    closures: BTreeMap<SymbolId, ExpressionId>,
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
        self.values.push(value);
    }
}

struct DropPlanner<'a, 'checker> {
    checker: &'a Checker<'checker>,
    liveness: Liveness<'a, 'checker>,
    facts: Vec<DropFact>,
    loan_ends: Vec<LoanEndFact>,
    loop_boundaries: Vec<usize>,
    scope_depth: usize,
    binding_depths: BTreeMap<SymbolId, usize>,
}

impl<'a, 'checker> DropPlanner<'a, 'checker> {
    fn new(checker: &'a Checker<'checker>, liveness: Liveness<'a, 'checker>) -> Self {
        Self {
            checker,
            liveness,
            facts: Vec::new(),
            loan_ends: Vec::new(),
            loop_boundaries: Vec::new(),
            scope_depth: 0,
            binding_depths: BTreeMap::new(),
        }
    }

    fn run(mut self) -> Result<(Vec<DropFact>, Vec<LoanEndFact>), OwnershipCheckingError> {
        for &root in self.checker.parsed.roots() {
            self.item(root)?;
        }
        Ok((self.facts, self.loan_ends))
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
                            symbol,
                            origin: marker_span(parameter.name),
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
                        self.statement(body, &mut state)?;
                        self.drop_all(DropPoint::AfterStatement(body), &mut state);
                    }
                    FunctionForm::Explicit { body, .. } => match body {
                        FunctionBody::Absent => {}
                        FunctionBody::Expression { expression, .. } => {
                            self.expression(expression, ExpressionUse::Consume, &mut state)?;
                            self.drop_all(DropPoint::ControlTransfer(expression), &mut state);
                        }
                        FunctionBody::Block(body) => {
                            self.statement(body, &mut state)?;
                            self.drop_all(DropPoint::AfterStatement(body), &mut state);
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
                let closure = self.closure_origin(initializer, state)?;
                if !self.expression(initializer, ExpressionUse::Consume, state)? {
                    return Ok(false);
                }
                if let Some(symbol) = self.checker.marker_symbol(name)
                    && self.checker.is_move_only_variable(symbol)
                {
                    self.binding_depths.insert(symbol, self.scope_depth);
                    state.insert(OwnedValue {
                        symbol,
                        origin: marker_span(name),
                        scope_depth: self.scope_depth,
                    });
                    if let Some(closure) = closure {
                        state.closures.insert(symbol, closure);
                    }
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
                                symbol,
                                origin,
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
                self.expression(condition, ExpressionUse::Read, state)?;
                let mut body_state = state.clone();
                self.loop_boundaries.push(self.scope_depth);
                self.statement(body, &mut body_state)?;
                self.loop_boundaries.pop();
                self.drop_loop_exit(id, state);
                Ok(true)
            }
            Statement::For { source, body, .. } => {
                self.expression(source, ExpressionUse::Read, state)?;
                let mut body_state = state.clone();
                self.loop_boundaries.push(self.scope_depth);
                self.statement(body, &mut body_state)?;
                self.loop_boundaries.pop();
                self.drop_loop_exit(id, state);
                Ok(true)
            }
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

    fn expression(
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
            }
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
                    if let StringPart::Interpolation { expression, .. } = part
                        && !self.expression(expression, ExpressionUse::Read, state)?
                    {
                        return Ok(false);
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
                self.expression(condition, ExpressionUse::Read, state)?;
                let base = state.clone();
                let mut branch_states = Vec::new();
                let mut then_state = base.clone();
                let usage = self.checker.control_result_usage(id);
                if self.control_body(then_branch, usage, &mut then_state)? {
                    self.drop_branch_exit(id, 0, &mut then_state);
                    branch_states.push(then_state);
                }
                if let Some(else_branch) = else_branch {
                    let mut else_state = base;
                    if self.control_body(else_branch, usage, &mut else_state)? {
                        self.drop_branch_exit(id, 1, &mut else_state);
                        branch_states.push(else_state);
                    }
                } else {
                    let mut implicit = base;
                    self.drop_branch_exit(id, 1, &mut implicit);
                    branch_states.push(implicit);
                }
                let continues = !branch_states.is_empty();
                *state = merge_value_states(branch_states);
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
                        if alternative
                            .is_none_or(|alternative| !alternative.match_domain().is_empty())
                        {
                            matched.push(input.clone());
                            matched_alternatives.push(alternative_index);
                        }
                        if alternative
                            .is_none_or(|alternative| !alternative.fallthrough_domain().is_empty())
                        {
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
                    let mut branch = merge_value_states(matched);
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
                *state = merge_value_states(branch_states);
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
                self.drop_deeper_than(boundary, DropPoint::ControlTransfer(id), state);
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
                for owned in state.values.iter().rev() {
                    self.push_fact(DropFact::new(
                        DropPoint::ControlTransfer(id),
                        DropTarget::Named(owned.symbol),
                        owned.origin,
                    ));
                }
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
                if !self.expression(value, ExpressionUse::Consume, state)? {
                    return Ok(false);
                }
                if let Some(place) = self.checker.place(target)?
                    && place.is_root()
                {
                    let symbol = place.root();
                    if operator != AssignmentOperator::Assign {
                        self.expression(target, ExpressionUse::Read, state)?;
                    }
                    let old = state.remove_value(symbol);
                    state.closures.remove(&symbol);
                    if let Some(old) = old {
                        self.push_fact(DropFact::new(
                            DropPoint::AfterExpression(value),
                            DropTarget::Named(symbol),
                            old.origin,
                        ));
                    }
                    if self.checker.is_move_only_variable(symbol) {
                        state.insert(OwnedValue {
                            symbol,
                            origin: node.span(),
                            scope_depth: old
                                .map(|value| value.scope_depth)
                                .or_else(|| self.binding_depths.get(&symbol).copied())
                                .unwrap_or(self.scope_depth),
                        });
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
                        if !self.expression(callee, ExpressionUse::Read, state)? {
                            return Ok(false);
                        }
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
                self.expression(receiver, ExpressionUse::Place, state)?;
                self.expression(index, ExpressionUse::Read, state)?;
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
        let (continues, right_drop) = self.string_view_operand(right, state)?;
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

    fn drop_named(&mut self, point: DropPoint, symbol: SymbolId, state: &mut ValueState) {
        if state.pending_calls.iter().any(|call| {
            call.loans.iter().any(
                |loan| matches!(loan.target(), LoanTarget::Place(place) if place.root() == symbol),
            )
        }) {
            return;
        }
        let closure = state.closures.remove(&symbol);
        if let Some(value) = state.remove_value(symbol) {
            let mut shared_sources = Vec::new();
            if let Some(closure) = closure {
                let captures = self.checker.captures_of(closure).collect::<Vec<_>>();
                for capture in captures.into_iter().rev() {
                    if capture.mode() == ClosureCaptureMode::Owned
                        && capture.effect() == ClosureCaptureEffect::Move
                    {
                        self.push_fact(DropFact::new(
                            point,
                            DropTarget::Captured {
                                closure,
                                source: capture.source(),
                            },
                            capture.reference_span(),
                        ));
                    } else if capture.mode() == ClosureCaptureMode::Shared
                        && let ClosureCaptureSource::Symbol(source) = capture.source()
                    {
                        shared_sources.push(source);
                    }
                }
            }
            self.push_fact(DropFact::new(
                point,
                DropTarget::Named(symbol),
                value.origin,
            ));
            for source in shared_sources {
                let still_captured = state.closures.values().any(|&closure| {
                    self.checker.captures_of(closure).any(|capture| {
                        capture.mode() == ClosureCaptureMode::Shared
                            && capture.source() == ClosureCaptureSource::Symbol(source)
                    })
                });
                // Match-edge cleanup must not shorten the lifetime of a shared
                // capture source that the entry body may still read. Its ordinary
                // body/branch liveness remains responsible for that source.
                if !still_captured
                    && !matches!(point, DropPoint::WhenAlternativeMatch { .. })
                    && !self.live_after(point).contains(&source)
                {
                    self.drop_named(point, source, state);
                }
            }
        }
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
            self.push_fact(DropFact::new(
                point,
                DropTarget::Temporary(temporary.subject),
                temporary.origin,
            ));
        }
    }

    fn drop_all(&mut self, point: DropPoint, state: &mut ValueState) {
        if let DropPoint::ControlTransfer(expression) = point {
            self.end_pending_calls(LoanEndPoint::ControlTransfer(expression), state, |_| true);
        }
        self.drop_nullable_temporaries(point, state, |_| true);
        while let Some(symbol) = state.values.last().map(|value| value.symbol) {
            self.drop_named(point, symbol, state);
        }
    }

    fn drop_scope(&mut self, depth: usize, point: DropPoint, state: &mut ValueState) {
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

    fn drop_deeper_than(&mut self, depth: usize, point: DropPoint, state: &mut ValueState) {
        let loop_depth = self.loop_boundaries.len();
        if let DropPoint::ControlTransfer(expression) = point {
            self.end_pending_calls(LoanEndPoint::ControlTransfer(expression), state, |call| {
                call.loop_depth >= loop_depth
            });
        }
        self.drop_nullable_temporaries(point, state, |temporary| {
            temporary.loop_depth >= loop_depth
        });
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
        let live_after = &self.liveness.statement_after[statement.index()];
        let symbols = state
            .values
            .iter()
            .filter(|value| !live_after.contains(&value.symbol))
            .map(|value| value.symbol)
            .collect::<Vec<_>>();
        for symbol in symbols.into_iter().rev() {
            self.drop_named(DropPoint::LoopExit(statement), symbol, state);
        }
    }

    fn push_fact(&mut self, mut fact: DropFact) {
        if let DropTarget::Temporary(expression) = fact.target()
            && let Some((owner, origin)) = self.checker.constant_temporary_origin(expression)
        {
            fact = DropFact::new(fact.point(), DropTarget::Temporary(owner), origin);
        }
        if !self.facts.contains(&fact) {
            self.facts.push(fact);
        }
    }

    fn live_after(&self, point: DropPoint) -> &std::collections::BTreeSet<SymbolId> {
        match point {
            DropPoint::AfterExpression(expression)
            | DropPoint::AfterBinaryOperands(expression)
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

fn merge_value_states(mut states: Vec<ValueState>) -> ValueState {
    let Some(mut merged) = states.pop() else {
        return ValueState::default();
    };
    merged.values.retain(|value| {
        states
            .iter()
            .all(|state| state.position(value.symbol).is_some())
    });
    for value in &mut merged.values {
        for state in &states {
            if let Some(index) = state.position(value.symbol) {
                let candidate = state.values[index].origin;
                if candidate.start() < value.origin.start() {
                    value.origin = candidate;
                }
            }
        }
    }
    merged
}
