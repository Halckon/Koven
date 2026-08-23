use std::collections::BTreeMap;

mod liveness;

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::SymbolId,
    parser::{
        AssignmentOperator, Expression, FunctionBody, FunctionForm, Item, NameMarker, Statement,
        StringPart, WhenCondition,
    },
    source::Span,
    type_checking::{Copyability, DestructuringMode, ExpressionCategory, ParameterMode},
};

use crate::ownership_checking::{DropFact, DropPoint, DropTarget};

use super::{Checker, ExpressionUse, OwnershipCheckingError};

use self::liveness::Liveness;

pub(super) fn plan(checker: &Checker<'_>) -> Result<Vec<DropFact>, OwnershipCheckingError> {
    let liveness = Liveness::build(checker)?;
    DropPlanner::new(checker, liveness).run()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnedValue {
    symbol: SymbolId,
    origin: Span,
    scope_depth: usize,
}

#[derive(Clone, Debug, Default)]
struct ValueState {
    values: Vec<OwnedValue>,
}

impl ValueState {
    fn position(&self, symbol: SymbolId) -> Option<usize> {
        self.values.iter().position(|value| value.symbol == symbol)
    }

    fn remove(&mut self, symbol: SymbolId) -> Option<OwnedValue> {
        self.position(symbol).map(|index| self.values.remove(index))
    }

    fn insert(&mut self, value: OwnedValue) {
        self.remove(value.symbol);
        self.values.push(value);
    }
}

struct DropPlanner<'a, 'checker> {
    checker: &'a Checker<'checker>,
    liveness: Liveness<'a, 'checker>,
    facts: Vec<DropFact>,
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
            loop_boundaries: Vec::new(),
            scope_depth: 0,
            binding_depths: BTreeMap::new(),
        }
    }

    fn run(mut self) -> Result<Vec<DropFact>, OwnershipCheckingError> {
        for &root in self.checker.parsed.roots() {
            self.item(root)?;
        }
        Ok(self.facts)
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
                self.expression(initializer, ExpressionUse::Consume, state)?;
                if let Some(symbol) = self.checker.marker_symbol(name)
                    && self.checker.is_move_only_variable(symbol)
                {
                    self.binding_depths.insert(symbol, self.scope_depth);
                    state.insert(OwnedValue {
                        symbol,
                        origin: marker_span(name),
                        scope_depth: self.scope_depth,
                    });
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
                self.expression(initializer, usage, state)?;
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
                self.drop_loop_exit(id, state);
                Ok(true)
            }
        }
    }

    fn expression(
        &mut self,
        id: ExpressionId,
        usage: ExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let node = self.checker.parsed.ast().expressions().get(id)?;
        match node.payload().clone() {
            Expression::Error
            | Expression::This
            | Expression::Literal(_)
            | Expression::SuperMember { .. }
            | Expression::Lambda { .. } => Ok(true),
            Expression::Name => {
                if let Some(symbol) = self.checker.reference_symbol(node.span()) {
                    match usage {
                        ExpressionUse::Consume => {
                            state.remove(symbol);
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
                        self.expression(expression, ExpressionUse::Read, state)?;
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
                if self.statement(then_branch, &mut then_state)? {
                    self.drop_branch_exit(id, 0, &mut then_state);
                    branch_states.push(then_state);
                }
                if let Some(else_branch) = else_branch {
                    let mut else_state = base;
                    if self.statement(else_branch, &mut else_state)? {
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
                if let Some(subject) = subject {
                    self.expression(subject, ExpressionUse::Read, state)?;
                }
                for entry in &entries {
                    for condition in &entry.conditions {
                        if let WhenCondition::Expression(expression)
                        | WhenCondition::Contains { expression, .. } = condition
                        {
                            self.expression(*expression, ExpressionUse::Read, state)?;
                        }
                    }
                }
                let base = state.clone();
                let mut branch_states = Vec::new();
                for (index, entry) in entries.iter().enumerate() {
                    let mut branch = base.clone();
                    if self.statement(entry.body, &mut branch)? {
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
                if let Some(value) = value {
                    self.expression(value, ExpressionUse::Consume, state)?;
                }
                self.drop_all(DropPoint::ControlTransfer(id), state);
                Ok(false)
            }
            Expression::Break { .. } | Expression::Continue { .. } => {
                let boundary = self.loop_boundaries.last().copied().unwrap_or(0);
                self.drop_deeper_than(boundary, DropPoint::ControlTransfer(id), state);
                Ok(false)
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
            | Expression::NonNullAssert { operand, .. } => {
                self.expression(operand, ExpressionUse::Read, state)
            }
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
            Expression::Binary { left, right, .. } => {
                self.expression(left, ExpressionUse::Read, state)?;
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
                    self.expression(value, ExpressionUse::Consume, state)?;
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
                self.expression(value, ExpressionUse::Consume, state)?;
                if let Some(place) = self.checker.place(target)?
                    && place.is_root()
                {
                    let symbol = place.root();
                    if operator != AssignmentOperator::Assign {
                        self.expression(target, ExpressionUse::Read, state)?;
                    }
                    let old = state.remove(symbol);
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
                        state.remove(root);
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
                self.expression(callee, ExpressionUse::Read, state)?;
                let modes = self.checker.calls_by_expression.get(&id.index()).cloned();
                let mut borrowed_roots = Vec::new();
                for (index, argument) in arguments.into_iter().enumerate() {
                    let mode = modes
                        .as_ref()
                        .and_then(|modes| modes.get(index))
                        .copied()
                        .unwrap_or(ParameterMode::Borrow);
                    match mode {
                        ParameterMode::Value => {
                            self.expression(argument.value, ExpressionUse::Consume, state)?;
                        }
                        ParameterMode::Borrow | ParameterMode::Inout => {
                            if let Some(place) = self.checker.place(argument.value)? {
                                let root = place.root();
                                self.expression(argument.value, ExpressionUse::Place, state)?;
                                if state.position(root).is_some() && !borrowed_roots.contains(&root)
                                {
                                    borrowed_roots.push(root);
                                }
                            } else {
                                self.expression(argument.value, ExpressionUse::Place, state)?;
                                if let Some(temporary) = self
                                    .checker
                                    .temporary_element_owner(argument.value)?
                                    .or_else(|| {
                                        (mode == ParameterMode::Borrow
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
                                    self.push_fact(DropFact::new(
                                        DropPoint::CallReturn(id),
                                        DropTarget::Temporary(temporary),
                                        origin,
                                    ));
                                }
                            }
                        }
                    }
                }
                for root in borrowed_roots {
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

    fn drop_named(&mut self, point: DropPoint, symbol: SymbolId, state: &mut ValueState) {
        if let Some(value) = state.remove(symbol) {
            self.push_fact(DropFact::new(
                point,
                DropTarget::Named(symbol),
                value.origin,
            ));
        }
    }

    fn drop_all(&mut self, point: DropPoint, state: &mut ValueState) {
        while let Some(value) = state.values.pop() {
            self.push_fact(DropFact::new(
                point,
                DropTarget::Named(value.symbol),
                value.origin,
            ));
        }
    }

    fn drop_scope(&mut self, depth: usize, point: DropPoint, state: &mut ValueState) {
        let mut index = state.values.len();
        while index > 0 {
            index -= 1;
            if state.values[index].scope_depth != depth {
                continue;
            }
            let value = state.values.remove(index);
            self.push_fact(DropFact::new(
                point,
                DropTarget::Named(value.symbol),
                value.origin,
            ));
        }
    }

    fn drop_deeper_than(&mut self, depth: usize, point: DropPoint, state: &mut ValueState) {
        let mut index = state.values.len();
        while index > 0 {
            index -= 1;
            if state.values[index].scope_depth <= depth {
                continue;
            }
            let value = state.values.remove(index);
            self.push_fact(DropFact::new(
                point,
                DropTarget::Named(value.symbol),
                value.origin,
            ));
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

    fn push_fact(&mut self, fact: DropFact) {
        if !self.facts.contains(&fact) {
            self.facts.push(fact);
        }
    }
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
