use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::SymbolId,
    parser::{
        AssignmentOperator, BinaryOperator, Expression, FunctionBody, FunctionForm, Item,
        Statement, StringPart, WhenCondition,
    },
    type_checking::{DestructuringMode, ParameterMode},
};

use super::super::{Checker, ExpressionUse, OwnershipCheckingError};

type LiveSet = BTreeSet<SymbolId>;

pub(super) struct Liveness<'a, 'checker> {
    checker: &'a Checker<'checker>,
    pub(super) expression_after: Vec<LiveSet>,
    pub(super) statement_after: Vec<LiveSet>,
    loop_stack: Vec<(LiveSet, LiveSet)>,
    pub(super) skipped_functions: BTreeSet<usize>,
    pub(super) function_live_in: BTreeMap<usize, LiveSet>,
    pub(super) lambda_live_in: BTreeMap<usize, LiveSet>,
    pub(super) skipped_lambdas: BTreeSet<usize>,
    saw_drop_deferred: bool,
}

impl<'a, 'checker> Liveness<'a, 'checker> {
    pub(super) fn build(checker: &'a Checker<'checker>) -> Result<Self, OwnershipCheckingError> {
        let mut this = Self {
            checker,
            expression_after: vec![LiveSet::new(); checker.parsed.ast().expressions().len()],
            statement_after: vec![LiveSet::new(); checker.parsed.ast().statements().len()],
            loop_stack: Vec::new(),
            skipped_functions: BTreeSet::new(),
            function_live_in: BTreeMap::new(),
            lambda_live_in: BTreeMap::new(),
            skipped_lambdas: BTreeSet::new(),
            saw_drop_deferred: false,
        };
        for &root in checker.parsed.roots() {
            this.item(root)?;
        }
        Ok(this)
    }

    fn item(&mut self, id: ItemId) -> Result<(), OwnershipCheckingError> {
        match self.checker.parsed.ast().items().get(id)?.payload().clone() {
            Item::Modified { declaration, .. } => self.item(declaration)?,
            Item::Function { form, .. } => {
                self.saw_drop_deferred = false;
                let live_in = match form {
                    FunctionForm::ImplicitUnitAbsent => LiveSet::new(),
                    FunctionForm::ImplicitUnitBlock(body) => {
                        self.statement(body, LiveSet::new())?
                    }
                    FunctionForm::Explicit { body, .. } => match body {
                        FunctionBody::Absent => LiveSet::new(),
                        FunctionBody::Expression { expression, .. } => {
                            self.expression(expression, ExpressionUse::Consume, LiveSet::new())?
                        }
                        FunctionBody::Block(body) => self.statement(body, LiveSet::new())?,
                    },
                };
                self.function_live_in.insert(id.index(), live_in);
                if self.saw_drop_deferred {
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
                    live.remove(&symbol);
                }
                self.expression(initializer, ExpressionUse::Consume, live)
            }
            Statement::LocalDestructuring { initializer, .. } => {
                let mut live = live_after;
                let usage = if let Some(descriptor) = self.checker.typed.destructuring(id) {
                    for component in descriptor.components() {
                        live.remove(&component.symbol());
                    }
                    match descriptor.mode() {
                        DestructuringMode::Copy => ExpressionUse::Read,
                        DestructuringMode::Consume => ExpressionUse::Consume,
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
                // The hidden provider reads its source again on every backedge.
                let mut provider_live = live_after.clone();
                if let Some(place) = self.checker.place(source)? {
                    provider_live.insert(place.root());
                }
                let mut header = provider_live.clone();
                loop {
                    self.loop_stack.push((live_after.clone(), header.clone()));
                    let body_in = self.statement(body, header.clone())?;
                    self.loop_stack.pop();
                    let mut next = provider_live.clone();
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
        if let Some(operation) = self.checker.typed.integer_operation(id) {
            return self.expression(operation.receiver(), ExpressionUse::Read, live_after);
        }
        if let Some(receiver) = self
            .checker
            .typed
            .string_operation(id)
            .map(|operation| operation.receiver())
            .or_else(|| {
                self.checker
                    .typed
                    .container_size(id)
                    .map(|size| size.receiver())
            })
            .or_else(|| self.checker.typed.map_size(id).map(|size| size.receiver()))
        {
            let mut receiver_live = live_after;
            if let Some(place) = self.checker.shared_receiver_place(receiver)? {
                receiver_live.insert(place.root());
            }
            return self.expression(receiver, ExpressionUse::Read, receiver_live);
        }
        if let Some(operation) = self.checker.typed.rc_operation(id) {
            if let Some(place) = self.checker.place(operation.receiver())? {
                let mut live = live_after;
                if self.checker.is_move_only_variable(place.root()) {
                    live.insert(place.root());
                }
                return Ok(live);
            }
            return self.expression(operation.receiver(), ExpressionUse::Read, live_after);
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
                live = self.expression(argument.argument(), ExpressionUse::Consume, live)?;
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
            Expression::Lambda { body, .. } => {
                // Lambda 的跳转与 deferred 状态不能污染创建它的 callable。
                let loops = std::mem::take(&mut self.loop_stack);
                let deferred = std::mem::replace(&mut self.saw_drop_deferred, false);
                let body_live = self.statement(body, LiveSet::new())?;
                self.lambda_live_in.insert(id.index(), body_live);
                if self.saw_drop_deferred {
                    self.skipped_lambdas.insert(id.index());
                }
                self.loop_stack = loops;
                self.saw_drop_deferred = deferred;
                let mut live = live_after;
                for capture in self.checker.captures_of(id) {
                    if let super::super::super::ClosureCaptureSource::Symbol(symbol) =
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
                let plan = self.checker.typed.nullable_when(id).cloned();
                for (entry_index, entry) in entries.into_iter().enumerate().rev() {
                    let descriptor = plan
                        .as_ref()
                        .and_then(|plan| plan.entries().get(entry_index));
                    let body = self.statement(entry.body, live_after.clone())?;
                    if entry.else_span.is_some() {
                        live = body.clone();
                    }
                    for (alternative_index, condition) in
                        entry.conditions.into_iter().enumerate().rev()
                    {
                        let alternative = descriptor
                            .and_then(|entry| entry.alternatives().get(alternative_index));
                        if alternative
                            .is_some_and(|alternative| alternative.fallthrough_domain().is_empty())
                        {
                            live.clear();
                        }
                        if alternative
                            .is_none_or(|alternative| !alternative.match_domain().is_empty())
                        {
                            live.extend(body.iter().copied());
                        }
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
                    self.expression(value, ExpressionUse::Consume, LiveSet::new())
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
            Expression::NonNullAssert { operand, .. } => {
                self.expression(operand, ExpressionUse::Consume, live_after)
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
                self.expression(operand, ExpressionUse::Read, live_after)
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::Elvis,
                right,
                ..
            } => {
                let mut live = self.expression(
                    right,
                    self.checker.control_result_usage(id),
                    live_after.clone(),
                )?;
                if !self.checker.is_only_null(left) {
                    live.extend(live_after);
                }
                self.expression(left, ExpressionUse::Place, live)
            }
            Expression::Binary { left, right, .. } => {
                let live = self.expression(right, ExpressionUse::Read, live_after)?;
                self.expression(left, ExpressionUse::Read, live)
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => {
                if let Some(descriptor) = self.checker.typed.map_put(target) {
                    let live =
                        self.expression(descriptor.value(), ExpressionUse::Consume, live_after)?;
                    let live = self.expression(descriptor.key(), ExpressionUse::Consume, live)?;
                    return self.expression(descriptor.receiver(), ExpressionUse::Read, live);
                }
                if let Some(descriptor) = self.checker.element_place_descriptor(target)? {
                    let live = self.expression(value, ExpressionUse::Consume, live_after)?;
                    let live = self.expression(descriptor.index(), ExpressionUse::Read, live)?;
                    return self.expression(descriptor.receiver(), ExpressionUse::Read, live);
                }
                let mut live = live_after;
                if operator == AssignmentOperator::Assign
                    && let Some(place) = self.checker.place(target)?
                    && place.is_root()
                {
                    live.remove(&place.root());
                }
                live = self.expression(value, ExpressionUse::Consume, live)?;
                if operator == AssignmentOperator::Assign {
                    Ok(live)
                } else {
                    self.expression(target, ExpressionUse::Read, live)
                }
            }
            Expression::Member { .. } => {
                if let Some(place) = self.checker.place(id)? {
                    let mut live = live_after;
                    if self.checker.is_move_only_variable(place.root()) {
                        live.insert(place.root());
                    }
                    Ok(live)
                } else {
                    let Expression::Member { receiver, .. } = node.payload() else {
                        unreachable!()
                    };
                    if self.checker.typed.aggregate_projection(id).is_some()
                        && self.checker.element_place_descriptor(*receiver)?.is_some()
                    {
                        self.saw_drop_deferred = true;
                    }
                    self.expression(*receiver, ExpressionUse::Read, live_after)
                }
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                let modes = self.checker.calls_by_expression.get(&id.index());
                let mut live = live_after;
                for (index, argument) in arguments.into_iter().enumerate().rev() {
                    let argument_usage = if modes.and_then(|modes| modes.get(index))
                        == Some(&ParameterMode::Value)
                    {
                        ExpressionUse::Consume
                    } else {
                        ExpressionUse::Read
                    };
                    live = self.expression(argument.value, argument_usage, live)?;
                }
                match self
                    .checker
                    .receivers_by_expression
                    .get(&id.index())
                    .copied()
                    .map(|receiver| (receiver.origin(), receiver.mode()))
                {
                    Some((
                        crate::type_checking::CallReceiverOrigin::Expression(expression),
                        mode,
                    )) => self.expression(
                        expression,
                        if mode == ParameterMode::Value {
                            ExpressionUse::Consume
                        } else {
                            ExpressionUse::Read
                        },
                        live,
                    ),
                    Some((crate::type_checking::CallReceiverOrigin::ImplicitThis(_), _)) => {
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
