//! Closure loan 与后续 drop planner 共享的 source-qualified 活跃性。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::UnitSymbolId,
    parser::{
        AssignmentOperator, Expression, FunctionBody, FunctionForm, Item, Statement, StringPart,
        WhenCondition,
    },
    type_checking::{DestructuringMode, UnitExpressionId, UnitStatementId},
};

use super::{Checker, ExpressionUse, OwnershipCheckingError, UnitCallArgumentOwnershipKind};

type LiveSet = BTreeSet<UnitSymbolId>;

pub(super) struct Liveness {
    pub(super) expression_after: Vec<LiveSet>,
    pub(super) statement_after: Vec<LiveSet>,
    pub(super) skipped_functions: BTreeSet<usize>,
    pub(super) function_live_in: BTreeMap<usize, LiveSet>,
    pub(super) lambda_live_in: BTreeMap<usize, LiveSet>,
    pub(super) deferred: Vec<UnitExpressionId>,
}

struct Builder<'a, 'checker> {
    checker: &'a Checker<'checker>,
    expression_after: Vec<LiveSet>,
    statement_after: Vec<LiveSet>,
    loop_stack: Vec<(LiveSet, LiveSet)>,
    skipped_functions: BTreeSet<usize>,
    function_live_in: BTreeMap<usize, LiveSet>,
    lambda_live_in: BTreeMap<usize, LiveSet>,
    deferred: Vec<UnitExpressionId>,
}

pub(super) fn build(
    checker: &Checker<'_>,
    reachable_only: bool,
) -> Result<Liveness, OwnershipCheckingError> {
    let mut builder = Builder {
        checker,
        expression_after: vec![LiveSet::new(); checker.parsed.ast().expressions().len()],
        statement_after: vec![LiveSet::new(); checker.parsed.ast().statements().len()],
        loop_stack: Vec::new(),
        skipped_functions: BTreeSet::new(),
        function_live_in: BTreeMap::new(),
        lambda_live_in: BTreeMap::new(),
        deferred: Vec::new(),
    };
    for &root in checker.parsed.roots() {
        builder.item(root)?;
    }
    for (expression, node) in checker.parsed.ast().expressions().iter() {
        if let Expression::Lambda { body, .. } = node.payload()
            && (!reachable_only
                || checker
                    .visited_lambdas
                    .contains(&checker.unit_expression(expression)))
        {
            let live_in = builder.statement(*body, LiveSet::new())?;
            builder.lambda_live_in.insert(expression.index(), live_in);
        }
    }
    Ok(Liveness {
        expression_after: builder.expression_after,
        statement_after: builder.statement_after,
        skipped_functions: builder.skipped_functions,
        function_live_in: builder.function_live_in,
        lambda_live_in: builder.lambda_live_in,
        deferred: builder.deferred,
    })
}

impl Builder<'_, '_> {
    fn item(&mut self, id: ItemId) -> Result<(), OwnershipCheckingError> {
        match self.checker.parsed.ast().items().get(id)?.payload().clone() {
            Item::Modified { declaration, .. } => self.item(declaration)?,
            Item::Function { form, .. } => {
                let deferred_before = self.deferred.len();
                let live_in = match form {
                    FunctionForm::ImplicitUnitAbsent => LiveSet::new(),
                    FunctionForm::ImplicitUnitBlock(body) => {
                        self.statement(body, LiveSet::new())?
                    }
                    FunctionForm::Explicit { body, .. } => match body {
                        FunctionBody::Absent => LiveSet::new(),
                        FunctionBody::Expression { expression, .. } => self.expression(
                            expression,
                            ExpressionUse::Consume {
                                parameter_span: None,
                            },
                            LiveSet::new(),
                        )?,
                        FunctionBody::Block(body) => self.statement(body, LiveSet::new())?,
                    },
                };
                self.function_live_in.insert(id.index(), live_in);
                if self.deferred.len() != deferred_before {
                    self.skipped_functions.insert(id.index());
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
                self.statement(body, LiveSet::new())?;
            }
            Item::Error | Item::Variable { .. } | Item::Constant { .. } => {}
        }
        Ok(())
    }

    fn statement(
        &mut self,
        id: StatementId,
        live_after: LiveSet,
    ) -> Result<LiveSet, OwnershipCheckingError> {
        self.statement_after[id.index()] = live_after.clone();
        match self
            .checker
            .parsed
            .ast()
            .statements()
            .get(id)?
            .payload()
            .clone()
        {
            Statement::Error => Ok(live_after),
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => {
                let mut live = live_after;
                for element in elements.into_iter().rev() {
                    live = self.statement(element, live)?;
                }
                Ok(live)
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
                    return Ok(live_after);
                };
                let mut live = live_after;
                if let Some(symbol) = self.checker.marker_symbol(name) {
                    live.remove(symbol);
                }
                self.expression(
                    initializer,
                    ExpressionUse::Consume {
                        parameter_span: None,
                    },
                    live,
                )
            }
            Statement::LocalDestructuring { initializer, .. } => {
                let mut live = live_after;
                let usage = if let Some(descriptor) = self
                    .checker
                    .typed
                    .destructuring(UnitStatementId::new(self.checker.source_unit, id))
                {
                    for component in descriptor.components() {
                        live.remove(&component.symbol());
                    }
                    match descriptor.mode() {
                        DestructuringMode::Copy => ExpressionUse::Read,
                        DestructuringMode::Consume => ExpressionUse::Consume {
                            parameter_span: None,
                        },
                    }
                } else {
                    ExpressionUse::Read
                };
                self.expression(initializer, usage, live)
            }
            Statement::Expression { expression } => {
                self.expression(expression, ExpressionUse::Read, live_after)
            }
            Statement::While {
                condition, body, ..
            } => {
                let mut header = live_after.clone();
                loop {
                    self.loop_stack.push((live_after.clone(), header.clone()));
                    let body_in = self.statement(body, header.clone())?;
                    self.loop_stack.pop();
                    let mut condition_after = live_after.clone();
                    condition_after.extend(body_in);
                    let next = self.expression(condition, ExpressionUse::Read, condition_after)?;
                    if next == header {
                        return Ok(next);
                    }
                    header.extend(next);
                }
            }
            Statement::For { source, body, .. } => {
                let bindings = self
                    .checker
                    .typed
                    .sequential_iteration(UnitStatementId::new(self.checker.source_unit, id))
                    .map(|plan| plan.binding().symbols().collect::<Vec<_>>())
                    .unwrap_or_default();
                let mut header = live_after.clone();
                loop {
                    self.loop_stack.push((live_after.clone(), header.clone()));
                    let mut body_in = self.statement(body, header.clone())?;
                    self.loop_stack.pop();
                    // Each iteration defines these borrowed values; they are not live before Acquire.
                    for symbol in &bindings {
                        body_in.remove(symbol);
                    }
                    let mut next = live_after.clone();
                    next.extend(body_in);
                    if next == header {
                        return self.expression(source, ExpressionUse::Read, next);
                    }
                    header.extend(next);
                }
            }
            Statement::Loop { body, .. } => {
                // Only break edges reach live_after; the backedge may overwrite those owners.
                let mut header = LiveSet::new();
                loop {
                    self.loop_stack.push((live_after.clone(), header.clone()));
                    let next = self.statement(body, header.clone())?;
                    self.loop_stack.pop();
                    if next == header {
                        return Ok(next);
                    }
                    header.extend(next);
                }
            }
        }
    }

    fn expression(
        &mut self,
        id: ExpressionId,
        _usage: ExpressionUse,
        live_after: LiveSet,
    ) -> Result<LiveSet, OwnershipCheckingError> {
        self.expression_after[id.index()] = live_after.clone();
        if self.checker.is_constant_use(id) {
            return Ok(live_after);
        }
        if let Some(operation) = self
            .checker
            .typed
            .integer_operation(self.checker.unit_expression(id))
        {
            return self.expression(
                operation.receiver().expression(),
                ExpressionUse::Read,
                live_after,
            );
        }
        if let Some(receiver) = self
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
        {
            let mut receiver_live = live_after;
            if let Some(place) = self.checker.loan_place(receiver.expression())? {
                receiver_live.insert(place.root());
            }
            return self.expression(receiver.expression(), ExpressionUse::Read, receiver_live);
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
            if let Some(place) = self.checker.place(operation.receiver().expression())? {
                let mut live = live_after;
                if self.checker.is_move_only_variable(place.root()) {
                    live.insert(place.root());
                }
                return Ok(live);
            }
            return self.expression(
                operation.receiver().expression(),
                ExpressionUse::Read,
                live_after,
            );
        }
        if let Some(descriptor) = self
            .checker
            .construction_descriptors
            .get(&self.checker.unit_expression(id))
            .cloned()
        {
            let mut arguments = descriptor.arguments().to_vec();
            arguments.sort_by_key(|argument| argument.evaluation_index());
            let terminating = arguments
                .iter()
                .position(|argument| self.checker.is_nothing_expression(argument.argument()));
            let evaluated = terminating.map_or(arguments.len(), |index| index + 1);
            let mut live = if terminating.is_some() {
                LiveSet::new()
            } else {
                live_after
            };
            for argument in arguments[..evaluated].iter().rev() {
                live = self.expression(
                    argument.argument().expression(),
                    ExpressionUse::Consume {
                        parameter_span: None,
                    },
                    live,
                )?;
            }
            return Ok(live);
        }
        let node = self.checker.parsed.ast().expressions().get(id)?;
        match node.payload().clone() {
            Expression::Error
            | Expression::This
            | Expression::Literal(_)
            | Expression::SuperMember { .. } => Ok(live_after),
            Expression::Name => {
                let mut live = live_after;
                if let Some(symbol) = self.checker.reference_symbol(node.span())
                    && self.checker.is_move_only_variable(symbol)
                {
                    live.insert(symbol);
                }
                Ok(live)
            }
            Expression::Group { expression } => self.expression(expression, _usage, live_after),
            Expression::String { parts } => {
                let mut live = live_after;
                for part in parts.into_iter().rev() {
                    if let StringPart::Interpolation { expression, .. } = part {
                        live = self.expression(expression, ExpressionUse::Read, live)?;
                    }
                }
                Ok(live)
            }
            Expression::Lambda { .. } => {
                let mut live = live_after;
                for capture in self.checker.captures_of(id) {
                    if let crate::ownership_checking::UnitClosureCaptureSource::Symbol(symbol) =
                        capture.source()
                    {
                        live.insert(symbol);
                    }
                }
                Ok(live)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let mut branches = self.statement(then_branch, live_after.clone())?;
                branches.extend(if let Some(else_branch) = else_branch {
                    self.statement(else_branch, live_after)?
                } else {
                    live_after
                });
                self.expression(condition, ExpressionUse::Read, branches)
            }
            Expression::When {
                subject, entries, ..
            } => {
                let mut live = live_after.clone();
                for entry in &entries {
                    live.extend(self.statement(entry.body, live_after.clone())?);
                }
                for entry in entries.into_iter().rev() {
                    for condition in entry.conditions.into_iter().rev() {
                        if let WhenCondition::Expression(expression)
                        | WhenCondition::Contains { expression, .. } = condition
                        {
                            live = self.expression(expression, ExpressionUse::Read, live)?;
                        }
                    }
                }
                if let Some(subject) = subject {
                    self.expression(subject, ExpressionUse::Read, live)
                } else {
                    Ok(live)
                }
            }
            Expression::Return { value, .. } => {
                if let Some(value) = value {
                    self.expression(
                        value,
                        ExpressionUse::Consume {
                            parameter_span: None,
                        },
                        LiveSet::new(),
                    )
                } else {
                    Ok(LiveSet::new())
                }
            }
            Expression::Break { .. } => Ok(self
                .loop_stack
                .last()
                .map(|context| context.0.clone())
                .unwrap_or_default()),
            Expression::Continue { .. } => Ok(self
                .loop_stack
                .last()
                .map(|context| context.1.clone())
                .unwrap_or_default()),
            Expression::NonNullAssert { operand, .. } => self.expression(
                operand,
                ExpressionUse::Consume {
                    parameter_span: None,
                },
                live_after,
            ),
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
                self.expression(operand, ExpressionUse::Read, live_after)
            }
            Expression::Binary { left, right, .. } => {
                if let Some(plan) = self.checker.short_circuit_plan(id)? {
                    use super::super::constant::UnitShortCircuitRhs;
                    let live = match plan.rhs {
                        UnitShortCircuitRhs::Never => live_after,
                        UnitShortCircuitRhs::Always => {
                            self.expression(right, ExpressionUse::Read, live_after)?
                        }
                        UnitShortCircuitRhs::Conditional => {
                            let mut live =
                                self.expression(right, ExpressionUse::Read, live_after.clone())?;
                            live.extend(live_after);
                            live
                        }
                    };
                    return self.expression(left, ExpressionUse::Read, live);
                }
                let live = self.expression(right, ExpressionUse::Read, live_after)?;
                self.expression(left, ExpressionUse::Read, live)
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => {
                if let Some(descriptor) = self.checker.element_place_descriptor(target)? {
                    let live = self.expression(
                        value,
                        ExpressionUse::Consume {
                            parameter_span: None,
                        },
                        live_after,
                    )?;
                    if descriptor.index().source_unit() != self.checker.source_unit
                        || descriptor.receiver().source_unit() != self.checker.source_unit
                    {
                        return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                            source_unit: self.checker.source_unit.index(),
                            expression: target.index(),
                        });
                    }
                    let live = self.expression(
                        descriptor.index().expression(),
                        ExpressionUse::Read,
                        live,
                    )?;
                    return self.expression(
                        descriptor.receiver().expression(),
                        ExpressionUse::Read,
                        live,
                    );
                }
                let mut live = live_after;
                if operator == AssignmentOperator::Assign
                    && let Some(place) = self.checker.place(target)?
                    && place.fields().is_empty()
                    && place.element().is_none()
                {
                    live.remove(&place.root());
                }
                live = self.expression(
                    value,
                    ExpressionUse::Consume {
                        parameter_span: None,
                    },
                    live,
                )?;
                if operator == AssignmentOperator::Assign {
                    Ok(live)
                } else {
                    self.expression(target, ExpressionUse::Read, live)
                }
            }
            Expression::Member { receiver, .. } => {
                if let Some(place) = self.checker.place(id)? {
                    let mut live = live_after;
                    if self.checker.is_move_only_variable(place.root()) {
                        live.insert(place.root());
                    }
                    Ok(live)
                } else {
                    if self
                        .checker
                        .typed
                        .aggregate_projection(self.checker.unit_expression(id))
                        .is_some()
                        && self.checker.element_place_descriptor(receiver)?.is_some()
                    {
                        let expression = self.checker.unit_expression(id);
                        if !self.deferred.contains(&expression) {
                            self.deferred.push(expression);
                        }
                    }
                    self.expression(receiver, ExpressionUse::Read, live_after)
                }
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                let contracts = self
                    .checker
                    .contracts_by_call
                    .get(&self.checker.unit_expression(id));
                let mut live = live_after;
                for (index, argument) in arguments.into_iter().enumerate().rev() {
                    let argument_usage = if contracts
                        .and_then(|contracts| contracts.get(index))
                        .is_some_and(|contract| {
                            contract.kind() == UnitCallArgumentOwnershipKind::Value
                        }) {
                        ExpressionUse::Consume {
                            parameter_span: None,
                        }
                    } else {
                        ExpressionUse::Read
                    };
                    live = self.expression(argument.value, argument_usage, live)?;
                }
                let receiver = self
                    .checker
                    .receiver_contracts_by_call
                    .get(&self.checker.unit_expression(id))
                    .copied();
                match receiver.map(|contract| (contract.source(), contract.kind())) {
                    Some((
                        crate::type_checking::UnitCallReceiverOrigin::Expression(receiver),
                        kind,
                    )) => {
                        let usage = if kind == UnitCallArgumentOwnershipKind::Value {
                            ExpressionUse::Consume {
                                parameter_span: None,
                            }
                        } else {
                            ExpressionUse::Read
                        };
                        self.expression(receiver.expression(), usage, live)
                    }
                    Some((crate::type_checking::UnitCallReceiverOrigin::ImplicitThis(_), _)) => {
                        Ok(live)
                    }
                    None => self.expression(callee, ExpressionUse::Read, live),
                }
            }
            Expression::Index { receiver, index } => {
                let live = self.expression(index, ExpressionUse::Read, live_after)?;
                self.expression(receiver, ExpressionUse::Read, live)
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.expression(receiver, ExpressionUse::Read, live_after)
                } else {
                    Ok(live_after)
                }
            }
        }
    }
}
