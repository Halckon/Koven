//! Bounded Choice collection at real planner transitions, separate from legacy IDs.
mod replace;

use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::values::{
    UnitOwnerChoiceIdV1, UnitOwnerChoiceInputV1, UnitOwnerChoiceV1, UnitOwnerProducerCatalogV1,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueObservationV1, UnitOwnerValueV1 as Value,
};
use crate::parser::{PrefixOperator, VariableKind};

/// One Choice binding, optionally replaced then transferred once; one String tail per edge.
struct Admission {
    callable: UnitOwnerCallable,
    control: UnitExpressionId,
    result_chain: Vec<UnitExpressionId>,
    symbol: UnitSymbolId,
    drop_point: crate::ownership_checking::UnitDropPoint,
    arms: [Arm; 2],
    transfer: Option<Transfer>,
    replacement: Option<replace::Admission>,
}

struct Transfer {
    source: UnitSymbolId,
    symbol: UnitSymbolId,
    chain: Vec<UnitExpressionId>,
}

#[derive(Default)]
struct TransferFrame {
    // Saved at the actual named take, never reconstructed from a cleanup event.
    result: Option<UnitExpressionId>,
    value: Option<Value>,
    completed: usize,
    bound: bool,
}

struct Arm {
    // The local initializer and tail never share an evaluation-result slot.
    local: Option<(UnitSymbolId, Vec<UnitExpressionId>)>,
    tail: Vec<UnitExpressionId>,
}

#[derive(Default)]
struct Frame {
    result: Option<(UnitExpressionId, UnitOwnerId)>,
    completed: usize,
}

#[derive(Default)]
struct Edge {
    initializer: Frame,
    tail: Frame,
    // A bounded symbol-to-value side table, independent of legacy OwnedValue.owner.
    local: Option<(UnitSymbolId, UnitOwnerId)>,
    delivered: bool,
}

pub(in super::super) struct ChoiceTransaction {
    admission: Admission,
    catalog: UnitOwnerProducerCatalogV1,
    events: [Vec<Event>; 2],
    edges: [Edge; 2],
    transfer: TransferFrame,
    replacement: replace::Frame,
    control_completed: bool,
    result: Option<UnitExpressionId>,
    result_completed: usize,
    suffix: Vec<Event>,
    bound: bool,
    dropped: bool,
    valid: bool,
}

fn chain(
    checker: &Checker<'_>,
    expression: ExpressionId,
    local: Option<UnitSymbolId>,
) -> Option<Vec<UnitExpressionId>> {
    let ast = checker.parsed.ast();
    let mut ids = Vec::new();
    let mut current = expression;
    loop {
        if checker.is_constant_use(current) {
            return None;
        }
        ids.push(checker.unit_expression(current));
        let node = ast.expressions().get(current).ok()?;
        match node.payload() {
            Expression::Group { expression } => current = *expression,
            Expression::Name
                if local.is_some() && checker.reference_symbol(node.span()) == local =>
            {
                break;
            }
            Expression::String { parts }
                if local.is_none()
                    && !parts
                        .iter()
                        .any(|part| matches!(part, StringPart::Interpolation { .. })) =>
            {
                break;
            }
            _ => return None,
        }
    }
    ids.reverse();
    Some(ids)
}

fn immutable_string_binding(
    checker: &Checker<'_>,
    statement: StatementId,
) -> Option<(UnitSymbolId, ExpressionId)> {
    let ast = checker.parsed.ast();
    let Statement::LocalVariable { declaration } = ast.statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    let Item::Variable {
        kind: VariableKind::Val,
        name,
        initializer,
        ..
    } = ast.items().get(*declaration).ok()?.payload()
    else {
        return None;
    };
    let symbol = *checker.marker_symbol(*name)?;
    if checker
        .typed
        .symbol_type(symbol)
        .and_then(|ty| checker.typed.types().get(ty))
        != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        || !checker.is_move_only_variable(symbol)
    {
        return None;
    }
    Some((symbol, *initializer))
}

fn admission(checker: &Checker<'_>, item: ItemId) -> Option<Admission> {
    let ast = checker.parsed.ast();
    let Item::Function {
        name,
        parameters,
        form,
        ..
    } = ast.items().get(item).ok()?.payload()
    else {
        return None;
    };
    // A top-level ordinary, non-generic function with exactly one Boolean parameter.
    if !checker.parsed.roots().contains(&item)
        || parameters.len() != 1
        || parameters[0].mode_marker.is_some()
        || checker.receiver_context(*name).is_some()
    {
        return None;
    }
    let function = *checker.marker_symbol(*name)?;
    let signature = checker
        .typed
        .signatures()
        .declarations()
        .iter()
        .find(|declaration| declaration.symbol() == function)?
        .callable()?;
    if signature.receiver().is_some()
        || !signature.type_parameters().is_empty()
        || checker.typed.types().get(signature.return_type())
            != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
    {
        return None;
    }
    let parameter = *checker.marker_symbol(parameters[0].name)?;
    if checker
        .typed
        .symbol_type(parameter)
        .and_then(|ty| checker.typed.types().get(ty))
        != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
    {
        return None;
    }
    let body = match form {
        FunctionForm::ImplicitUnitBlock(body)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => *body,
        _ => return None,
    };
    let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
        return None;
    };
    let (statement, transfer_statement, final_statement) = match elements.as_slice() {
        [statement] => (*statement, None, None),
        [statement, transfer] => (*statement, Some(*transfer), None),
        [statement, replacement, transfer] => (*statement, Some(*replacement), Some(*transfer)),
        _ => return None,
    };
    let Statement::LocalVariable { declaration } = ast.statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    let Item::Variable {
        kind,
        name,
        initializer,
        ..
    } = ast.items().get(*declaration).ok()?.payload()
    else {
        return None;
    };
    let symbol = *checker.marker_symbol(*name)?;
    if !matches!(kind, VariableKind::Val | VariableKind::Var) {
        return None;
    }
    if checker
        .typed
        .symbol_type(symbol)
        .and_then(|ty| checker.typed.types().get(ty))
        != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        || !checker.is_move_only_variable(symbol)
    {
        return None;
    }
    // A whole-If Group forwards the selected value; it is not another producer
    // and must still complete through the real expression-result frames.
    let mut control = *initializer;
    let mut result_chain = Vec::new();
    loop {
        if checker.is_constant_use(control)
            || checker
                .typed
                .expression_type(checker.unit_expression(control))
                .and_then(|ty| checker.typed.types().get(ty))
                != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        {
            return None;
        }
        result_chain.push(checker.unit_expression(control));
        match ast.expressions().get(control).ok()?.payload() {
            Expression::Group { expression } => control = *expression,
            Expression::If { .. } => break,
            _ => return None,
        }
    }
    result_chain.reverse();
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = ast.expressions().get(control).ok()?.payload()
    else {
        return None;
    };
    // Only pure wrappers around the same checked Boolean parameter are admitted.
    // The existing traversal evaluates them before the fork. Keep inputs keyed by
    // the actual If edges; a Not changes the predicate, not the owner edge order.
    let mut condition = *condition;
    loop {
        if checker.is_constant_use(condition)
            || checker
                .typed
                .expression_type(checker.unit_expression(condition))
                .and_then(|ty| checker.typed.types().get(ty))
                != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
        {
            return None;
        }
        let node = ast.expressions().get(condition).ok()?;
        match node.payload() {
            Expression::Group { expression } => condition = *expression,
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => condition = *operand,
            Expression::Name if checker.reference_symbol(node.span()) == Some(parameter) => break,
            _ => return None,
        }
    }
    fn arm(checker: &Checker<'_>, body: StatementId) -> Option<Arm> {
        let ast = checker.parsed.ast();
        let (binding, tail) = match ast.statements().get(body).ok()?.payload() {
            Statement::ControlBody { elements } => match elements.as_slice() {
                [tail] => (None, *tail),
                [binding, tail] => (Some(*binding), *tail),
                _ => return None,
            },
            Statement::Expression { .. } => (None, body),
            _ => return None,
        };
        let local = if let Some(binding) = binding {
            let Statement::LocalVariable { declaration } =
                ast.statements().get(binding).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                kind: VariableKind::Val,
                name,
                initializer,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            let symbol = *checker.marker_symbol(*name)?;
            if checker
                .typed
                .symbol_type(symbol)
                .and_then(|ty| checker.typed.types().get(ty))
                != Some(&UnitTypeKind::Builtin(BuiltinType::String))
                || !checker.is_move_only_variable(symbol)
            {
                return None;
            }
            Some((symbol, chain(checker, *initializer, None)?))
        } else {
            None
        };
        let Statement::Expression { expression } = ast.statements().get(tail).ok()?.payload()
        else {
            return None;
        };
        Some(Arm {
            tail: chain(
                checker,
                *expression,
                local.as_ref().map(|(symbol, _)| *symbol),
            )?,
            local,
        })
    }
    let mut replacement = None;
    let mut transfer = if let Some(statement) = transfer_statement {
        let (target, initializer) = immutable_string_binding(checker, statement)?;
        if target == symbol {
            return None;
        }
        if *kind == VariableKind::Var {
            replacement = Some(replace::admit(
                checker,
                symbol,
                target,
                initializer,
                statement,
            )?);
            None
        } else {
            Some(Transfer {
                source: symbol,
                symbol: target,
                chain: chain(checker, initializer, Some(symbol))?,
            })
        }
    } else {
        if *kind != VariableKind::Val {
            return None;
        }
        None
    };
    if let Some(statement) = final_statement {
        // The one following declaration may consume either checked replace output:
        // its selected old result, or the definition now installed in the original root.
        let previous = replacement.as_ref()?.symbol;
        let (target, initializer) = immutable_string_binding(checker, statement)?;
        if target == previous || target == symbol {
            return None;
        }
        let (source, ids) = if let Some(ids) = chain(checker, initializer, Some(previous)) {
            (previous, ids)
        } else {
            (symbol, chain(checker, initializer, Some(symbol))?)
        };
        transfer = Some(Transfer {
            source,
            symbol: target,
            chain: ids,
        });
    }
    Some(Admission {
        callable: UnitOwnerCallable::Function(UnitItemId::new(checker.source_unit, item)),
        control: checker.unit_expression(control),
        result_chain,
        symbol,
        drop_point: crate::ownership_checking::UnitDropPoint::AfterStatement(UnitStatementId::new(
            checker.source_unit,
            final_statement.or(transfer_statement).unwrap_or(statement),
        )),
        arms: [arm(checker, *then_branch)?, arm(checker, *else_branch)?],
        transfer,
        replacement,
    })
}

impl ChoiceTransaction {
    pub(in super::super) fn begin(
        checker: &Checker<'_>,
        item: ItemId,
        identity: std::sync::Arc<()>,
    ) -> Option<Self> {
        let admission = admission(checker, item)?;
        Some(Self {
            catalog: UnitOwnerProducerCatalogV1 {
                identity,
                callable: admission.callable,
                definitions: Vec::new(),
            },
            admission,
            events: [Vec::new(), Vec::new()],
            edges: [Edge::default(), Edge::default()],
            transfer: TransferFrame::default(),
            replacement: replace::Frame::default(),
            control_completed: false,
            result: None,
            result_completed: 0,
            suffix: Vec::new(),
            bound: false,
            dropped: false,
            valid: true,
        })
    }
    fn abort(&mut self) {
        self.valid = false;
        self.catalog.definitions.clear();
        self.events.iter_mut().for_each(Vec::clear);
        self.edges = [Edge::default(), Edge::default()];
        self.transfer = TransferFrame::default();
        self.replacement = replace::Frame::default();
        self.control_completed = false;
        self.result = None;
        self.result_completed = 0;
        self.suffix.clear();
        self.bound = false;
        self.dropped = false;
    }
    pub(in super::super) fn string_completed(&mut self, expression: UnitExpressionId) {
        if !self.valid {
            return;
        }
        if self.replacement_string(expression) {
            return;
        }
        let Some(edge) = self.admission.arms.iter().position(|arm| {
            arm.local.as_ref().map_or(&arm.tail, |(_, ids)| ids).first() == Some(&expression)
        }) else {
            self.abort();
            return;
        };
        let state = &self.edges[edge];
        if state.initializer.result.is_some()
            || state.initializer.completed != 0
            || state.local.is_some()
            || state.tail.result.is_some()
            || state.tail.completed != 0
            || state.delivered
            || (edge == 1 && !self.edges[0].delivered)
        {
            self.abort();
            return;
        }
        let owner = UnitOwnerId {
            identity: self.catalog.identity.clone(),
            callable: self.catalog.callable,
            index: self.catalog.definitions.len(),
        };
        self.catalog.definitions.push(UnitOwnerDefinition {
            id: owner.clone(),
            origin: UnitOwnerOrigin::String(expression),
        });
        self.events[edge].push(Event::Create(expression, owner.clone()));
        let frame = if self.admission.arms[edge].local.is_some() {
            &mut self.edges[edge].initializer
        } else {
            &mut self.edges[edge].tail
        };
        frame.result = Some((expression, owner));
    }
    pub(in super::super) fn expression_completed(
        &mut self,
        expression: UnitExpressionId,
        consuming: bool,
        continues: bool,
    ) {
        if !self.valid {
            return;
        }
        if !continues {
            self.abort();
            return;
        }
        if self.replacement_expression(expression, consuming) {
            return;
        }
        if let Some(transfer) = &self.admission.transfer
            && let Some(position) = transfer.chain.iter().position(|id| *id == expression)
        {
            let from = if position == 0 {
                expression
            } else {
                transfer.chain[position - 1]
            };
            if !consuming
                || !self.bound
                || self.dropped
                || self.transfer.bound
                || self.transfer.completed != position
                || self.transfer.result != Some(from)
            {
                self.abort();
                return;
            }
            let Some(value) = self.transfer.value.clone() else {
                self.abort();
                return;
            };
            if position != 0 {
                self.suffix.push(Event::Forward(from, expression, value));
                self.transfer.result = Some(expression);
            }
            self.transfer.completed += 1;
            return;
        }
        if let Some(position) = self
            .admission
            .result_chain
            .iter()
            .position(|id| *id == expression)
        {
            if !consuming
                || self.bound
                || self.dropped
                || self.result_completed != position
                || !self.edges.iter().all(|edge| edge.delivered)
            {
                self.abort();
                return;
            }
            if position == 0 {
                if self.control_completed || self.result.is_some() {
                    self.abort();
                    return;
                }
                self.control_completed = true;
            } else {
                let from = self.admission.result_chain[position - 1];
                if !self.control_completed || self.result != Some(from) {
                    self.abort();
                    return;
                }
                self.suffix.push(Event::Forward(
                    from,
                    expression,
                    Value::Choice(self.choice_id()),
                ));
            }
            self.result = Some(expression);
            self.result_completed += 1;
            return;
        }
        let Some((edge, initializer, position)) =
            self.admission
                .arms
                .iter()
                .enumerate()
                .find_map(|(edge, arm)| {
                    arm.local
                        .as_ref()
                        .and_then(|(_, ids)| ids.iter().position(|id| *id == expression))
                        .map(|position| (edge, true, position))
                        .or_else(|| {
                            arm.tail
                                .iter()
                                .position(|id| *id == expression)
                                .map(|position| (edge, false, position))
                        })
                })
        else {
            return;
        };
        let state = &mut self.edges[edge];
        let (frame, ids) = if initializer {
            let Some((_, ids)) = &self.admission.arms[edge].local else {
                self.abort();
                return;
            };
            (&mut state.initializer, ids)
        } else {
            (&mut state.tail, &self.admission.arms[edge].tail)
        };
        if !consuming || state.delivered || frame.completed != position {
            self.abort();
            return;
        }
        let Some((from, owner)) = frame.result.clone() else {
            self.abort();
            return;
        };
        if position == 0 {
            if from != expression {
                self.abort();
                return;
            }
        } else if ids.get(position - 1) == Some(&from) {
            self.events[edge].push(Event::Forward(
                from,
                expression,
                Value::Definition(owner.clone()),
            ));
            frame.result = Some((expression, owner));
        } else {
            self.abort();
            return;
        }
        frame.completed += 1;
    }
    /// Called only after the actual named take. None is a failed take, never a token.
    pub(in super::super) fn moved(
        &mut self,
        expression: UnitExpressionId,
        taken: Option<UnitSymbolId>,
    ) {
        if !self.valid {
            return;
        }
        if let Some(transfer) = &self.admission.transfer
            && transfer.chain.first() == Some(&expression)
        {
            let source = transfer.source;
            if !self.bound
                || self.dropped
                || self.transfer.bound
                || self.transfer.result.is_some()
                || self.transfer.value.is_some()
                || self.transfer.completed != 0
                || taken != Some(source)
            {
                self.abort();
                return;
            }
            let Some(value) = self.replacement_transfer_value(source) else {
                self.abort();
                return;
            };
            self.transfer.value = Some(value.clone());
            self.suffix.push(Event::Move(expression, source, value));
            self.transfer.result = Some(expression);
            return;
        }
        let Some(edge) = self
            .admission
            .arms
            .iter()
            .position(|arm| arm.local.is_some() && arm.tail.first() == Some(&expression))
        else {
            self.abort();
            return;
        };
        let state = &mut self.edges[edge];
        if state.delivered
            || state.initializer.result.is_some()
            || state.tail.result.is_some()
            || state.tail.completed != 0
            || state.local.as_ref().map(|(symbol, _)| *symbol) != taken
            || taken.is_none()
        {
            self.abort();
            return;
        }
        let Some((symbol, owner)) = state.local.take() else {
            self.abort();
            return;
        };
        self.events[edge].push(Event::Move(
            expression,
            symbol,
            Value::Definition(owner.clone()),
        ));
        state.tail.result = Some((expression, owner));
    }
    pub(in super::super) fn edge_completed(
        &mut self,
        control: UnitExpressionId,
        edge: usize,
        tail: Option<UnitExpressionId>,
        continues: bool,
    ) {
        if !self.valid {
            return;
        }
        let Some(state) = self.edges.get(edge) else {
            self.abort();
            return;
        };
        let arm = &self.admission.arms[edge];
        if control != self.admission.control
            || !continues
            || state.delivered
            || state.local.is_some()
            || state.initializer.result.is_some()
            || state.initializer.completed != arm.local.as_ref().map_or(0, |(_, ids)| ids.len())
            || state.tail.completed != arm.tail.len()
            || tail != arm.tail.last().copied()
            || state.tail.result.as_ref().map(|(actual, _)| *actual) != tail
        {
            self.abort();
            return;
        }
        self.edges[edge].delivered = true;
    }
    fn choice_id(&self) -> UnitOwnerChoiceIdV1 {
        UnitOwnerChoiceIdV1 {
            identity: self.catalog.identity.clone(),
            callable: self.catalog.callable,
            control: self.admission.control,
        }
    }
    /// Post-insert only: a completed initializer is handed to the actual named binding.
    pub(in super::super) fn bound(&mut self, symbol: UnitSymbolId, initializer: UnitExpressionId) {
        if !self.valid {
            return;
        }
        if self.replacement_bound(symbol, initializer) {
            return;
        }
        if let Some(edge) = self.admission.arms.iter().position(|arm| {
            arm.local
                .as_ref()
                .is_some_and(|(local, _)| *local == symbol)
        }) {
            let arm = &self.admission.arms[edge];
            let Some((_, ids)) = &arm.local else {
                self.abort();
                return;
            };
            let state = &mut self.edges[edge];
            if state.delivered
                || state.local.is_some()
                || state.tail.result.is_some()
                || state.tail.completed != 0
                || state.initializer.completed != ids.len()
                || ids.last() != Some(&initializer)
                || state.initializer.result.as_ref().map(|(actual, _)| *actual) != Some(initializer)
            {
                self.abort();
                return;
            }
            let Some((_, owner)) = state.initializer.result.take() else {
                self.abort();
                return;
            };
            self.events[edge].push(Event::Bind(
                symbol,
                initializer,
                Value::Definition(owner.clone()),
            ));
            state.local = Some((symbol, owner));
            return;
        }
        if let Some(transfer) = &self.admission.transfer
            && symbol == transfer.symbol
        {
            if !self.bound
                || self.dropped
                || self.transfer.bound
                || self.transfer.completed != transfer.chain.len()
                || self.transfer.result != Some(initializer)
                || transfer.chain.last() != Some(&initializer)
            {
                self.abort();
                return;
            }
            let Some(value) = self.transfer.value.clone() else {
                self.abort();
                return;
            };
            self.suffix.push(Event::Bind(symbol, initializer, value));
            self.transfer.result = None;
            self.transfer.bound = true;
            return;
        }
        if self.bound
            || !self.control_completed
            || !self.edges.iter().all(|edge| edge.delivered)
            || symbol != self.admission.symbol
            || self.result_completed != self.admission.result_chain.len()
            || self.result != Some(initializer)
            || self.admission.result_chain.last() != Some(&initializer)
        {
            self.abort();
            return;
        }
        self.suffix.push(Event::Bind(
            symbol,
            initializer,
            Value::Choice(self.choice_id()),
        ));
        self.result = None;
        self.bound = true;
    }
    pub(in super::super) fn dropped(
        &mut self,
        point: crate::ownership_checking::UnitDropPoint,
        symbol: UnitSymbolId,
    ) {
        if !self.valid {
            return;
        }
        if self.replacement_dropped(point, symbol) {
            return;
        }
        let expected = self
            .admission
            .transfer
            .as_ref()
            .map_or(self.admission.symbol, |transfer| transfer.symbol);
        if !self.bound
            || self.dropped
            || symbol != expected
            || point != self.admission.drop_point
            || (self.admission.transfer.is_some()
                && (!self.transfer.bound || self.transfer.result.is_some()))
        {
            self.abort();
            return;
        }
        let value = if self.admission.transfer.is_some() {
            let Some(value) = self.transfer.value.take() else {
                self.abort();
                return;
            };
            value
        } else {
            Value::Choice(self.choice_id())
        };
        self.suffix.push(Event::Drop(point, symbol, value));
        self.dropped = true;
    }
    pub(in super::super) fn finish(self) -> Option<UnitOwnerValueObservationV1> {
        if !self.valid
            || !self.bound
            || !self.dropped
            || !self.edges.iter().all(|edge| edge.delivered)
            || self.transfer.value.is_some()
        {
            return None;
        }
        let (Some((left_tail, left)), Some((right_tail, right))) =
            (&self.edges[0].tail.result, &self.edges[1].tail.result)
        else {
            return None;
        };
        let choice = UnitOwnerChoiceV1 {
            id: self.choice_id(),
            events: self.events.clone(),
            inputs: [
                UnitOwnerChoiceInputV1 {
                    edge: 0,
                    tail: *left_tail,
                    owner: left.clone(),
                },
                UnitOwnerChoiceInputV1 {
                    edge: 1,
                    tail: *right_tail,
                    owner: right.clone(),
                },
            ],
        };
        let graph = UnitOwnerValueObservationV1 {
            catalog: self.catalog,
            prefix: Vec::new(),
            choice: Some(choice),
            suffix: self.suffix,
        };
        graph.is_consistent().then_some(graph)
    }
}
