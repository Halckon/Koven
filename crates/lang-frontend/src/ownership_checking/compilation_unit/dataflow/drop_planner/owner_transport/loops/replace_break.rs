//! Shared actual P -> Q prefix for live Break and delivered Return endpoints.
mod returning;
use super::*;
use crate::ownership_checking::{
    UnitOwnerOpenLoopObservationV1, UnitOwnerOpenLoopPrefixV1, UnitOwnerProducerCatalogV1,
    UnitOwnerReplaceBreakObservationV1, UnitOwnerReplaceBreakStateV1,
};
use crate::parser::{BinaryOperator, LiteralKind, PrefixOperator};
use crate::source::Span;

pub(super) struct OpenLoopInput {
    returning: Option<returning::ReturnInput>,
    pub(super) consumer_target:
        crate::ownership_checking::compilation_unit::owner_transport::ReplaceBreakSuffix,
    pub(super) statement: StatementId,
    pub(super) condition: ExpressionId,
    pub(super) parameter: UnitSymbolId,
    pub(super) body: StatementId,
    pub(super) jump: UnitExpressionId,
    initial: UnitExpressionId,
    initial_origin: UnitOwnerOrigin,
    call: UnitExpressionId,
    call_origin: Span,
    argument: UnitExpressionId,
    root: UnitSymbolId,
    previous: UnitSymbolId,
    previous_statement: UnitStatementId,
    prefix: Option<UnitOwnerObservation>,
    expected: Option<OwnedValue>,
    inputs: Option<UnitOwnerProducerCatalogV1>,
    entry: Option<UnitOwnerReplaceBreakStateV1>,
    transfer: Option<(
        std::sync::Arc<()>,
        UnitExpressionId,
        UnitOwnerReplaceBreakStateV1,
    )>,
    body_completion: Option<UnitOwnerReplaceBreakStateV1>,
}
fn variable(
    checker: &Checker<'_>,
    statement: StatementId,
    kind: VariableKind,
) -> Option<(UnitSymbolId, ExpressionId)> {
    let ast = checker.parsed.ast();
    let Statement::LocalVariable { declaration } = ast.statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    let Item::Variable {
        name,
        kind: actual,
        initializer,
        ..
    } = ast.items().get(*declaration).ok()?.payload()
    else {
        return None;
    };
    let symbol = *checker.marker_symbol(*name)?;
    (*actual == kind
        && checker.is_move_only_variable(symbol)
        && checker
            .typed
            .symbol_type(symbol)
            .and_then(|ty| checker.typed.types().get(ty))
            == Some(&UnitTypeKind::Builtin(BuiltinType::String))
        && exact_type(checker, *initializer, BuiltinType::String))
    .then_some((symbol, *initializer))
}
fn literal(checker: &Checker<'_>, id: ExpressionId) -> bool {
    exact_type(checker, id, BuiltinType::String) && checker.parsed.ast().expressions().get(id).is_ok_and(|node| matches!(node.payload(), Expression::String { parts } if parts.iter().all(|part| !matches!(part, StringPart::Interpolation { .. }))))
}
// Logical leaves permit Group/Not wrappers, never another predicate.
fn logical_leaf_parameter(
    checker: &Checker<'_>,
    mut expression: ExpressionId,
    parameters: &BTreeSet<UnitSymbolId>,
) -> Option<UnitSymbolId> {
    loop {
        if !exact_type(checker, expression, BuiltinType::Boolean) {
            return None;
        }
        let node = checker.parsed.ast().expressions().get(expression).ok()?;
        match node.payload() {
            Expression::Group { expression: inner } => expression = *inner,
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => expression = *operand,
            Expression::Name => {
                return checker
                    .reference_symbol(node.span())
                    .filter(|symbol| parameters.contains(symbol));
            }
            _ => return None,
        }
    }
}
// P/Q-only condition admission. Other loop families retain their original Name-chain gate.
fn boolean_condition_parameter(
    checker: &Checker<'_>,
    expression: ExpressionId,
    parameters: &BTreeSet<UnitSymbolId>,
) -> Option<UnitSymbolId> {
    if checker.ordinary_condition_outcomes(expression) != (true, true) {
        return None;
    }
    let ast = checker.parsed.ast();
    let mut current = expression;
    loop {
        if !exact_type(checker, current, BuiltinType::Boolean) {
            return None;
        }
        match ast.expressions().get(current).ok()?.payload() {
            Expression::Group { expression } => current = *expression,
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => current = *operand,
            Expression::Name => {
                return conditional::boolean_parameter(checker, current, parameters);
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr,
                right,
                ..
            } => {
                let left = logical_leaf_parameter(checker, *left, parameters)?;
                let right = logical_leaf_parameter(checker, *right, parameters)?;
                return (left == right).then_some(left);
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::Equal | BinaryOperator::NotEqual,
                right,
                ..
            } => {
                let literal = |id| {
                    exact_type(checker, id, BuiltinType::Boolean)
                        && ast.expressions().get(id).is_ok_and(|node| {
                            matches!(node.payload(), Expression::Literal(LiteralKind::Boolean(_)))
                        })
                };
                let parameter = match (literal(*left), literal(*right)) {
                    (true, false) => *right,
                    (false, true) => *left,
                    _ => return None,
                };
                // The parameter side is only the old Group/Not -> Name chain, never another comparison.
                return conditional::boolean_parameter(checker, parameter, parameters);
            }
            _ => return None,
        }
    }
}
impl OpenLoopInput {
    pub(super) fn admit(
        checker: &Checker<'_>,
        elements: &[StatementId],
        parameters: &BTreeSet<UnitSymbolId>,
        owner_parameter: Option<UnitSymbolId>,
    ) -> Option<Self> {
        let (prefix, statement, suffix, binding) = match elements {
            [prefix, statement, suffix] => (prefix, statement, suffix, None),
            [prefix, statement, binding, suffix] => (prefix, statement, suffix, Some(binding)),
            _ => return None,
        };
        if parameters.len() != 1 {
            return None;
        }
        let ast = checker.parsed.ast();
        let (root, initial) = variable(checker, *prefix, VariableKind::Var)?;
        let consumer_target = if let Some(binding) = binding {
            let (symbol, initializer) = variable(checker, *binding, VariableKind::Val)?;
            let node = ast.expressions().get(initializer).ok()?;
            if symbol == root
                || !matches!(node.payload(), Expression::Name)
                || checker.reference_symbol(node.span()) != Some(root)
            {
                return None;
            }
            let mut target =
                super::super::replace_return_consumer::admit(checker, *suffix, symbol)?;
            if target.replacement.is_some() {
                return None;
            }
            target.binding = Some((
                checker.unit_expression(initializer),
                UnitStatementId::new(checker.source_unit, *binding),
                symbol,
            ));
            crate::ownership_checking::compilation_unit::owner_transport::ReplaceBreakSuffix::Return(
                target,
            )
        } else if let Some((retired, used)) = variable(checker, *suffix, VariableKind::Val) {
            let used_node = ast.expressions().get(used).ok()?;
            if root == retired
                || !matches!(used_node.payload(), Expression::Name)
                || checker.reference_symbol(used_node.span()) != Some(root)
            {
                return None;
            }
            crate::ownership_checking::compilation_unit::owner_transport::ReplaceBreakSuffix::Bind {
                initializer: checker.unit_expression(used),
                statement: UnitStatementId::new(checker.source_unit, *suffix),
                retired,
            }
        } else if let Some(target) =
            super::super::replace_return_consumer::admit(checker, *suffix, root)
        {
            crate::ownership_checking::compilation_unit::owner_transport::ReplaceBreakSuffix::Return(
                target,
            )
        } else {
            crate::ownership_checking::compilation_unit::owner_transport::ReplaceBreakSuffix::Call(
                super::super::replace_call_consumer::admit(checker, *suffix, root)?,
            )
        };
        let initial_origin = match owner_parameter {
            Some(parameter) => {
                let node = ast.expressions().get(initial).ok()?;
                if !matches!(node.payload(), Expression::Name)
                    || checker.reference_symbol(node.span()) != Some(parameter)
                {
                    return None;
                }
                UnitOwnerOrigin::Parameter(parameter)
            }
            None if literal(checker, initial) => {
                UnitOwnerOrigin::String(checker.unit_expression(initial))
            }
            _ => return None,
        };
        let Statement::While {
            condition, body, ..
        } = ast.statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        // Keep the actual outer condition; comparison and Not never swap P/Q edge roles.
        let parameter = boolean_condition_parameter(checker, *condition, parameters)?;
        let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
            return None;
        };
        let [declaration, tail] = elements.as_slice() else {
            return None;
        };
        let (previous, call) = variable(checker, *declaration, VariableKind::Val)?;
        let Statement::Expression { expression: jump } =
            ast.statements().get(*tail).ok()?.payload()
        else {
            return None;
        };
        if !exact_type(checker, *jump, BuiltinType::Nothing) {
            return None;
        }
        let returning = match ast.expressions().get(*jump).ok()?.payload() {
            Expression::Break { .. } => None,
            Expression::Return { .. } => Some(returning::ReturnInput::admit(
                checker,
                *tail,
                *condition,
                root,
                &consumer_target,
            )?),
            _ => return None,
        };
        if owner_parameter.is_some() && returning.is_none() {
            return None;
        }
        let call_node = ast.expressions().get(call).ok()?;
        let Expression::Call {
            callee,
            type_arguments,
            arguments,
            ..
        } = call_node.payload()
        else {
            return None;
        };
        if !type_arguments.is_empty()
            || !matches!(
                ast.expressions().get(*callee).ok()?.payload(),
                Expression::Name
            )
        {
            return None;
        }
        let [place, incoming] = arguments.as_slice() else {
            return None;
        };
        let place_node = ast.expressions().get(place.value).ok()?;
        if place.named_prefix.is_some()
            || incoming.named_prefix.is_some()
            || incoming.mode_marker.is_some()
            || !matches!(
                place.mode_marker,
                Some(crate::parser::ParameterModeMarker::Inout(_))
            )
            || !matches!(place_node.payload(), Expression::Name)
            || checker.reference_symbol(place_node.span()) != Some(root)
            || !literal(checker, incoming.value)
        {
            return None;
        }
        let plan = checker
            .ownership_primitives
            .iter()
            .find(|plan| plan.descriptor().expression() == checker.unit_expression(call))?;
        if plan.descriptor().kind() != OwnershipPrimitiveKind::Replace
            || plan.places().len() != 1
            || !plan.places()[0].is_root()
            || plan.places()[0].root() != root
            || checker.typed.types().get(plan.descriptor().value_type())
                != Some(&UnitTypeKind::Builtin(BuiltinType::String))
            || plan.descriptor().operands()
                != [
                    checker.unit_expression(place.value),
                    checker.unit_expression(incoming.value),
                ]
        {
            return None;
        }
        Some(Self {
            returning,
            consumer_target,
            statement: *statement,
            condition: *condition,
            parameter,
            body: *body,
            jump: checker.unit_expression(*jump),
            initial: checker.unit_expression(initial),
            initial_origin,
            call: checker.unit_expression(call),
            call_origin: call_node.span(),
            argument: checker.unit_expression(incoming.value),
            root,
            previous,
            previous_statement: UnitStatementId::new(checker.source_unit, *declaration),
            prefix: None,
            expected: None,
            inputs: None,
            entry: None,
            transfer: None,
            body_completion: None,
        })
    }
    pub(super) fn capture(
        &mut self,
        observation: &UnitOwnerObservation,
        state: &ValueState,
        scope: usize,
    ) {
        let Some([value]) = boundary_values(state) else {
            return;
        };
        let Some(owner) = value.owner.clone() else {
            return;
        };
        let catalog = match self.initial_origin {
            UnitOwnerOrigin::Parameter(_) => {
                // Retain the actual function-entry definition and identity. The old
                // String-only legacy factory must not acquire Parameter admission.
                if observation.status != UnitOwnerObservationStatus::Complete
                    || !observation.is_consistent()
                {
                    return;
                }
                UnitOwnerProducerCatalogV1 {
                    identity: observation.identity.clone(),
                    callable: observation.callable,
                    definitions: observation.definitions.clone(),
                }
            }
            _ => {
                let Some(catalog) = UnitOwnerProducerCatalogV1::from_legacy(observation) else {
                    return;
                };
                catalog
            }
        };
        let mut expected_events = Vec::new();
        if let UnitOwnerOrigin::Parameter(parameter) = self.initial_origin {
            expected_events.push(UnitOwnerTransportEvent::Move {
                expression: self.initial,
                symbol: parameter,
                owner: owner.clone(),
            });
        }
        expected_events.extend([
            UnitOwnerTransportEvent::Result {
                expression: self.initial,
                owner: owner.clone(),
            },
            UnitOwnerTransportEvent::Binding {
                symbol: self.root,
                initializer: self.initial,
                owner: owner.clone(),
            },
        ]);
        if scope != 1
            || value.scope_depth != scope
            || value.symbol != self.root
            || value.origin != value.declaration
            || catalog.definitions().len() != 1
            || !catalog.is_consistent()
            || catalog
                .definition(owner.clone())
                .is_none_or(|d| d.origin() != self.initial_origin)
            || observation.events != expected_events
        {
            return;
        }
        self.expected = Some(value.clone());
        self.prefix = Some(observation.clone());
        self.inputs = Some(catalog);
    }
    fn read<'a>(
        &self,
        state: &'a ValueState,
    ) -> Option<(&'a OwnedValue, UnitOwnerReplaceBreakStateV1)> {
        let [value] = boundary_values(state)? else {
            return None;
        };
        let expected = self.expected.as_ref()?;
        (value.symbol == expected.symbol
            && value.declaration == expected.declaration
            && value.scope_depth == expected.scope_depth)
            .then_some((
                value,
                UnitOwnerReplaceBreakStateV1 {
                    symbol: value.symbol,
                    owner: value.owner.clone()?,
                    origin: value.origin,
                    declaration: value.declaration,
                    scope_depth: value.scope_depth,
                },
            ))
    }
    fn input(&self, state: &ValueState) -> Option<UnitOwnerReplaceBreakStateV1> {
        let (value, snapshot) = self.read(state)?;
        (Some(value) == self.expected.as_ref()).then_some(snapshot)
    }
    fn output(
        &self,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
    ) -> Option<UnitOwnerReplaceBreakStateV1> {
        let (_, snapshot) = self.read(state)?;
        (snapshot.origin == self.call_origin
            && Some(&snapshot.owner) != self.expected.as_ref()?.owner.as_ref()
            && std::sync::Arc::ptr_eq(&snapshot.owner.identity, identity))
        .then_some(snapshot)
    }
    pub(super) fn enter(&mut self, state: &ValueState) -> bool {
        self.entry = self.input(state);
        self.entry.is_some()
    }
    pub(super) fn jump_boundary(
        &mut self,
        completion: LoopBodyCompletion,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
    ) -> bool {
        if completion != LoopBodyCompletion::Break(self.jump)
            || self.transfer.is_some()
            || self.entry.is_none()
        {
            return false;
        }
        let Some(snapshot) = self.output(identity, state) else {
            return false;
        };
        self.transfer = Some((identity.clone(), self.jump, snapshot));
        true
    }
    pub(super) fn complete_body(&mut self, state: &ValueState) -> bool {
        if let Some(returning) = &mut self.returning {
            return returning.complete_body(state);
        }
        let Some((identity, _, _)) = &self.transfer else {
            return false;
        };
        self.body_completion = self.output(identity, state);
        self.body_completion.is_some()
    }
    pub(super) fn is_returning(&self) -> bool {
        self.returning.is_some()
    }

    // Both variants consume exactly the same captured prefix and raw body definitions.
    fn take_prefix(
        &mut self,
        body: UnitOwnerObservation,
        state: &ValueState,
        installed: UnitOwnerReplaceBreakStateV1,
    ) -> Option<UnitOwnerOpenLoopPrefixV1> {
        let condition_false = self.input(state)?;
        if body.status != UnitOwnerObservationStatus::Complete {
            return None;
        }
        let producers = UnitOwnerProducerCatalogV1 {
            identity: body.identity,
            callable: body.callable,
            definitions: body.definitions,
        };
        let prefix = self.prefix.take()?;
        let source = self.initial.source_unit();
        let value = UnitOwnerOpenLoopPrefixV1 {
            identity: prefix.identity,
            callable: prefix.callable,
            statement: UnitStatementId::new(source, self.statement),
            condition: UnitExpressionId::new(source, self.condition),
            body: UnitStatementId::new(source, self.body),
            jump: self.jump,
            call: self.call,
            call_origin: self.call_origin,
            initial: self.initial,
            argument: self.argument,
            previous: self.previous,
            previous_statement: self.previous_statement,
            inputs: self.inputs.take()?,
            producers,
            prefix: prefix.events,
            events: body.events,
            entry: self.entry.take()?,
            installed,
            condition_false,
        };
        value.prefix_is_consistent().then_some(value)
    }

    // Reached only after the actual body result and its terminal witness agree.
    pub(super) fn finish(
        &mut self,
        body: UnitOwnerObservation,
        state: &ValueState,
    ) -> Option<UnitOwnerOpenLoopObservationV1> {
        if let Some(returning) = &self.returning {
            let installed = returning.body_snapshot()?;
            let prefix = self.take_prefix(body, state, installed)?;
            self.returning.as_mut()?.save_prefix(prefix);
            return None;
        }
        let (completed_identity, completed_jump, installed) = self.transfer.take()?;
        let prefix = self.take_prefix(body, state, installed)?;
        let edges = LoopBodyCompletion::Break(self.jump).edges(
            prefix.statement,
            prefix.condition,
            prefix.body,
        );
        let value = UnitOwnerReplaceBreakObservationV1 {
            prefix,
            suffix: self.consumer_target.clone(),
            consumer: None,
            call_consumer: None,
            return_consumer: None,
            completed_identity,
            completed_jump,
            body_completion: self.body_completion.take()?,
            edges,
        };
        value
            .is_consistent()
            .then_some(UnitOwnerOpenLoopObservationV1::BreakLive(Box::new(value)))
    }
}
