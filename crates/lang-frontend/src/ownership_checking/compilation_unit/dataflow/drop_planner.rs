mod control;
mod lambda;
mod model;
mod pending_call;

use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::UnitSymbolId,
    parser::{
        AssignmentOperator, BinaryOperator, Expression, FunctionBody, FunctionForm, Item,
        Statement, StringPart, WhenCondition,
    },
    type_checking::{
        BuiltinType, Copyability, DestructuringMode, ExpressionCategory, NominalKind,
        ParameterMode, UnitAggregateProjectionKind, UnitCallReceiverOrigin, UnitExpressionId,
        UnitStatementId, UnitTypeKind,
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

#[derive(Default)]
pub(super) struct Analysis {
    pub(super) drops: Vec<UnitDropFact>,
    pub(super) conditional_receiver_drops: Vec<UnitConditionalReceiverDropFact>,
    pub(super) deferred: Vec<UnitOwnershipDeferredFact>,
}

pub(super) fn plan(checker: &Checker<'_>) -> Result<Analysis, OwnershipCheckingError> {
    let liveness = liveness::build(checker)?;
    let deferred = liveness
        .deferred
        .iter()
        .copied()
        .map(|expression| {
            UnitOwnershipDeferredFact::new(expression, OwnershipDeferredReason::IndexPlace)
        })
        .collect();
    let planner = DropPlanner::new(checker, liveness).run()?;
    let drops = planner
        .facts
        .into_iter()
        .map(|fact| {
            let fact = fact.into_unit(checker.source_unit);
            if let crate::ownership_checking::UnitDropTarget::Temporary(expression) = fact.target()
                && let Some((owner, origin)) =
                    checker.constant_temporary_origin(expression.expression())
            {
                return UnitDropFact::new(
                    fact.point(),
                    crate::ownership_checking::UnitDropTarget::Temporary(owner),
                    origin,
                );
            }
            fact
        })
        .collect();
    let conditional_receiver_drops = planner
        .conditional_receiver_facts
        .into_iter()
        .map(|fact| fact.into_unit(checker.source_unit))
        .collect();
    Ok(Analysis {
        drops,
        conditional_receiver_drops,
        deferred,
    })
}

struct DropPlanner<'a, 'checker> {
    checker: &'a Checker<'checker>,
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
            checker,
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
                let mut state = ValueState::default();
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
                        self.drop_named(PlannerDropPoint::FunctionEntry(id), symbol, &mut state);
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
                self.drop_scope(frame, PlannerDropPoint::AfterStatement(id), state);
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
                if !self.expression(initializer, DropExpressionUse::Consume, state)? {
                    return Ok(false);
                }
                if let Some(symbol) = self.checker.marker_symbol(name).copied()
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
                        self.drop_named(PlannerDropPoint::AfterStatement(id), symbol, state);
                    }
                }
                Ok(true)
            }
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
                    for component in descriptor.components() {
                        let symbol = component.symbol();
                        if self.checker.is_move_only_variable(symbol) {
                            self.binding_depths.insert(symbol, self.scope_depth);
                            let origin = self.checker.symbol_span(symbol)?;
                            state.insert(OwnedValue {
                                symbol,
                                origin,
                                scope_depth: self.scope_depth,
                            });
                            if !self.liveness.statement_after[id.index()].contains(&symbol) {
                                self.drop_named(
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
            } => {
                self.expression(condition, DropExpressionUse::Read, state)?;
                let mut body_state = state.clone();
                self.loop_boundaries.push(self.scope_depth);
                self.statement(body, &mut body_state)?;
                self.loop_boundaries.pop();
                self.drop_loop_exit(id, state);
                Ok(true)
            }
            Statement::For { source, body, .. } => {
                self.expression(source, DropExpressionUse::Read, state)?;
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
                self.drop_loop_exit(id, state);
                Ok(true)
            }
        }
    }

    fn expression(
        &mut self,
        id: ExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        if self.checker.is_constant_use(id) {
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
                    self.drop_named(PlannerDropPoint::AfterExpression(id), root, state);
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
            let mut arguments = descriptor.arguments().to_vec();
            arguments.sort_by_key(|argument| argument.evaluation_index());
            for argument in arguments {
                if !self.expression(
                    argument.argument().expression(),
                    DropExpressionUse::Consume,
                    state,
                )? || self.checker.is_nothing_expression(argument.argument())
                {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
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
                for capture in self.checker.captures_of(id) {
                    if capture.mode() == ClosureCaptureMode::Owned
                        && capture.effect() == ClosureCaptureEffect::Move
                        && let UnitClosureCaptureSource::Symbol(symbol) = capture.source()
                    {
                        state.take(symbol);
                    }
                }
                Ok(true)
            }
            Expression::Name => {
                if let Some(symbol) = self.checker.reference_symbol(node.span()) {
                    match usage {
                        DropExpressionUse::Consume => {
                            state.take(symbol);
                        }
                        DropExpressionUse::Read => {
                            if !self.liveness.expression_after[id.index()].contains(&symbol) {
                                self.drop_named(
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
            Expression::Group { expression } => self.expression(expression, usage, state),
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part
                        && !self.expression(expression, DropExpressionUse::Read, state)?
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
                let branch_usage = self.control_result_usage(id);
                if !self.expression(condition, DropExpressionUse::Read, state)? {
                    return Ok(false);
                }
                let base = state.clone();
                let mut branch_states = Vec::new();
                let mut then_state = base.clone();
                if self.control_body(then_branch, branch_usage, &mut then_state)? {
                    self.drop_branch_exit(id, 0, &mut then_state);
                    branch_states.push(then_state);
                }
                if let Some(else_branch) = else_branch {
                    let mut else_state = base;
                    if self.control_body(else_branch, branch_usage, &mut else_state)? {
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
                let branch_usage = self.control_result_usage(id);
                if let Some(subject) = subject
                    && !self.expression(subject, DropExpressionUse::Read, state)?
                {
                    return Ok(false);
                }
                for entry in &entries {
                    for condition in &entry.conditions {
                        if let WhenCondition::Expression(expression)
                        | WhenCondition::Contains { expression, .. } = condition
                            && !self.expression(*expression, DropExpressionUse::Read, state)?
                        {
                            return Ok(false);
                        }
                    }
                }
                let base = state.clone();
                let mut branch_states = Vec::new();
                for (index, entry) in entries.iter().enumerate() {
                    let mut branch = base.clone();
                    if self.control_body(entry.body, branch_usage, &mut branch)? {
                        self.drop_branch_exit(id, index, &mut branch);
                        branch_states.push(branch);
                    }
                }
                if entries.iter().all(|entry| entry.else_span.is_none()) {
                    let mut implicit = base;
                    self.drop_branch_exit(id, entries.len(), &mut implicit);
                    branch_states.push(implicit);
                }
                let continues = !branch_states.is_empty();
                *state = merge_value_states(branch_states);
                Ok(continues)
            }
            Expression::Return { value, .. } => {
                if let Some(value) = value
                    && !self.expression(value, DropExpressionUse::Consume, state)?
                {
                    return Ok(false);
                }
                self.drop_all(PlannerDropPoint::ControlTransfer(id), state);
                Ok(false)
            }
            Expression::Break { .. } | Expression::Continue { .. } => {
                let boundary = self.loop_boundaries.last().copied().unwrap_or(0);
                self.drop_deeper_than(boundary, PlannerDropPoint::ControlTransfer(id), state);
                Ok(false)
            }
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
            } if matches!(
                operator,
                BinaryOperator::Add | BinaryOperator::Equal | BinaryOperator::NotEqual
            ) && self.is_string_expression(left)
                && self.is_string_expression(right) =>
            {
                self.string_binary(left, right, id, state)
            }
            Expression::Binary { left, right, .. } => {
                self.expression(left, DropExpressionUse::Read, state)?;
                self.expression(right, DropExpressionUse::Read, state)
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => {
                if let Some(descriptor) = self.checker.element_place_descriptor(target)? {
                    self.expression(
                        descriptor.receiver().expression(),
                        DropExpressionUse::Place,
                        state,
                    )?;
                    self.expression(
                        descriptor.index().expression(),
                        DropExpressionUse::Read,
                        state,
                    )?;
                    self.expression(value, DropExpressionUse::Consume, state)?;
                    if self.checker.typed.copyability(descriptor.element_type())
                        == Copyability::MoveOnly
                    {
                        self.push_fact(PlannerDropFact::new(
                            PlannerDropPoint::AfterReplacement(id),
                            PlannerDropTarget::ReplacedElement(id),
                            self.checker.parsed.ast().expressions().get(target)?.span(),
                        ));
                    }
                    if let Some(temporary) = self
                        .checker
                        .temporary_element_owner(target)?
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
                    } else if let Some(place) = self.checker.place(target)?
                        && !self.liveness.expression_after[id.index()].contains(&place.root())
                    {
                        self.drop_named(PlannerDropPoint::AfterExpression(id), place.root(), state);
                    }
                    return Ok(true);
                }
                if !self.expression(value, DropExpressionUse::Consume, state)? {
                    return Ok(false);
                }
                let assignment = UnitExpressionId::new(self.checker.source_unit, id);
                let assignment_target = UnitExpressionId::new(self.checker.source_unit, target);
                let assignment_value = UnitExpressionId::new(self.checker.source_unit, value);
                if operator == AssignmentOperator::Assign
                    && self
                        .checker
                        .typed
                        .assignment(assignment)
                        .is_some_and(|descriptor| {
                            descriptor.expression() == assignment
                                && descriptor.target() == assignment_target
                                && descriptor.value() == assignment_value
                                && descriptor.operator() == AssignmentOperator::Assign
                                && descriptor.falls_through()
                        })
                    && let Some(projection) =
                        self.checker.typed.aggregate_projection(assignment_target)
                    && projection.kind() == UnitAggregateProjectionKind::Field
                    && self.checker.typed.copyability(projection.ty()) == Copyability::MoveOnly
                {
                    self.push_fact(PlannerDropFact::new(
                        PlannerDropPoint::BeforeReplacement(id),
                        PlannerDropTarget::ReplacedField {
                            assignment: id,
                            field: projection.field(),
                        },
                        self.checker.parsed.ast().expressions().get(target)?.span(),
                    ));
                }
                if let Some(place) = self.checker.place(target)?
                    && place.is_root()
                {
                    let symbol = place.root();
                    if operator != AssignmentOperator::Assign {
                        self.expression(target, DropExpressionUse::Read, state)?;
                    }
                    let old = state.remove_value(symbol);
                    state.closures.remove(&symbol);
                    if let Some(old) = old {
                        self.push_fact(PlannerDropFact::new(
                            PlannerDropPoint::AfterExpression(value),
                            PlannerDropTarget::Named(symbol),
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
                            self.drop_named(PlannerDropPoint::AfterExpression(id), symbol, state);
                        }
                    }
                }
                Ok(true)
            }
            Expression::Member { receiver, .. } => {
                if let Some(place) = self.checker.place(id)? {
                    let root = place.root();
                    if usage == DropExpressionUse::Consume && place.is_root() {
                        state.take(root);
                    } else if !self.liveness.expression_after[id.index()].contains(&root) {
                        self.drop_named(PlannerDropPoint::AfterExpression(id), root, state);
                    }
                    Ok(true)
                } else {
                    self.expression(receiver, DropExpressionUse::Read, state)
                }
            }
            Expression::Call {
                callee, arguments, ..
            } => {
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
                                    if !conditional_static_self {
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
                                            self.register_pending_temporary(
                                                id, temporary, origin, false, state,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        UnitCallReceiverOrigin::ImplicitThis(_) => {
                            if contract.kind() == UnitCallArgumentOwnershipKind::Value
                                && self.checker.typed.copyability(contract.receiver_type())
                                    == Copyability::MoveOnly
                            {
                                state.this = None;
                            }
                        }
                    }
                } else if !self.expression(callee, DropExpressionUse::Read, state)? {
                    return Ok(false);
                }
                for (index, argument) in arguments.into_iter().enumerate() {
                    let kind = contracts
                        .and_then(|contracts| contracts.get(index))
                        .map(|contract| contract.kind())
                        .unwrap_or(UnitCallArgumentOwnershipKind::SharedLoan);
                    match kind {
                        UnitCallArgumentOwnershipKind::Value => {
                            if !self.expression(
                                argument.value,
                                DropExpressionUse::Consume,
                                state,
                            )? {
                                return Ok(false);
                            }
                            self.register_value_argument(id, argument.value, state)?;
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
                                }
                            } else {
                                if !self.expression(
                                    argument.value,
                                    DropExpressionUse::Place,
                                    state,
                                )? {
                                    return Ok(false);
                                }
                                if let Some(temporary) = self
                                    .checker
                                    .temporary_element_owner(argument.value)?
                                    .map(UnitExpressionId::expression)
                                    .or_else(|| {
                                        (kind == UnitCallArgumentOwnershipKind::SharedLoan
                                            && self.is_move_only_temporary(argument.value))
                                        .then_some(argument.value)
                                    })
                                {
                                    let origin = self
                                        .checker
                                        .parsed
                                        .ast()
                                        .expressions()
                                        .get(temporary)?
                                        .span();
                                    self.register_pending_temporary(
                                        id, temporary, origin, false, state,
                                    );
                                }
                            }
                        }
                    }
                }
                if !returns {
                    return Ok(false);
                }
                state.pending_borrows.retain(|(call, _)| *call != id);
                self.finish_pending_temporaries(id, state);
                for root in borrowed_roots {
                    if !self.liveness.expression_after[id.index()].contains(&root) {
                        self.drop_named(PlannerDropPoint::CallReturn(id), root, state);
                    }
                }
                Ok(true)
            }
            Expression::Index { receiver, index } => {
                self.expression(receiver, DropExpressionUse::Place, state)?;
                self.expression(index, DropExpressionUse::Read, state)?;
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
                    self.drop_named(PlannerDropPoint::AfterExpression(id), place.root(), state);
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
        let unit = self.checker.unit_expression(expression);
        let Some(ty) = self.checker.typed.expression_type(unit) else {
            return false;
        };
        self.checker.typed.expression_category(unit) == Some(ExpressionCategory::Temporary)
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

    fn is_string_expression(&self, expression: ExpressionId) -> bool {
        self.checker
            .typed
            .expression_type(self.checker.unit_expression(expression))
            .and_then(|ty| self.checker.typed.types().get(ty))
            == Some(&UnitTypeKind::Builtin(BuiltinType::String))
    }

    fn string_binary(
        &mut self,
        left: ExpressionId,
        right: ExpressionId,
        binary: ExpressionId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let left_drop = self.string_view_operand(left, state)?;
        let right_drop = self.string_view_operand(right, state)?;
        let point = PlannerDropPoint::AfterBinaryOperands(binary);
        for pending in [right_drop, left_drop].into_iter().flatten() {
            match pending {
                StringOperandDrop::Named(symbol) => self.drop_named(point, symbol, state),
                StringOperandDrop::Temporary(expression, origin) => self.push_fact(
                    PlannerDropFact::new(point, PlannerDropTarget::Temporary(expression), origin),
                ),
            }
        }
        Ok(true)
    }

    fn string_view_operand(
        &mut self,
        expression: ExpressionId,
        state: &mut ValueState,
    ) -> Result<Option<StringOperandDrop>, OwnershipCheckingError> {
        let node = self.checker.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Group { expression } => self.string_view_operand(*expression, state),
            Expression::Name if !self.checker.is_constant_use(expression) => {
                let Some(symbol) = self.checker.reference_symbol(node.span()) else {
                    return Ok(None);
                };
                Ok(
                    (!self.liveness.expression_after[expression.index()].contains(&symbol))
                        .then_some(StringOperandDrop::Named(symbol)),
                )
            }
            _ => {
                self.expression(expression, DropExpressionUse::Read, state)?;
                if !self.is_move_only_temporary(expression) {
                    return Ok(None);
                }
                Ok(Some(StringOperandDrop::Temporary(expression, node.span())))
            }
        }
    }

    fn drop_named(
        &mut self,
        point: PlannerDropPoint,
        symbol: UnitSymbolId,
        state: &mut ValueState,
    ) {
        if !matches!(point, PlannerDropPoint::ControlTransfer(_))
            && state
                .pending_borrows
                .iter()
                .any(|(_, root)| *root == symbol)
        {
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
            self.push_fact(PlannerDropFact::new(
                point,
                PlannerDropTarget::Named(symbol),
                value.origin,
            ));
            for source in shared_sources {
                let still_captured = state.closures.values().any(|&closure| {
                    self.checker.captures_of(closure).any(|capture| {
                        capture.mode() == ClosureCaptureMode::Shared
                            && capture.source() == UnitClosureCaptureSource::Symbol(source)
                    })
                });
                if !still_captured && !self.live_after(point).contains(&source) {
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

    fn drop_all(&mut self, point: PlannerDropPoint, state: &mut ValueState) {
        // callable 已退出，不能再由尚未提交的调用前缀阻止 owner 清理。
        state.pending_borrows.clear();
        self.drop_pending_temporaries(point, state, |_| true);
        while let Some(symbol) = state.values.last().map(|value| value.symbol) {
            self.drop_named(point, symbol, state);
        }
        if let Some(receiver) = state.this.take() {
            self.push_this_fact(point, receiver);
        }
    }

    fn push_this_fact(&mut self, point: PlannerDropPoint, receiver: OwnedThis) {
        if let Some(receiver_type) = receiver.conditional_type {
            let fact = PlannerConditionalReceiverDropFact {
                point,
                owner: receiver.owner,
                receiver_type,
                value_origin: receiver.origin,
            };
            if !self.conditional_receiver_facts.contains(&fact) {
                self.conditional_receiver_facts.push(fact);
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

    fn drop_deeper_than(&mut self, depth: usize, point: PlannerDropPoint, state: &mut ValueState) {
        let loop_depth = self.loop_boundaries.len();
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
            self.drop_named(
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
            self.drop_named(PlannerDropPoint::LoopExit(statement), symbol, state);
        }
    }

    fn push_fact(&mut self, fact: PlannerDropFact) {
        if !self.facts.contains(&fact) {
            self.facts.push(fact);
        }
    }

    fn live_after(&self, point: PlannerDropPoint) -> &std::collections::BTreeSet<UnitSymbolId> {
        match point {
            PlannerDropPoint::AfterExpression(expression)
            | PlannerDropPoint::AfterBinaryOperands(expression)
            | PlannerDropPoint::CallReturn(expression)
            | PlannerDropPoint::ControlTransfer(expression)
            | PlannerDropPoint::BeforeReplacement(expression)
            | PlannerDropPoint::AfterReplacement(expression)
            | PlannerDropPoint::BranchExit {
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
