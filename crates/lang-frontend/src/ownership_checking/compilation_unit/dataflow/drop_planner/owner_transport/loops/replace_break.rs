//! Read-only actual P -> Q replacement and Break witnesses; no loop-state merge.
use super::*;
use crate::ownership_checking::{
    UnitOwnerProducerCatalogV1, UnitOwnerReplaceBreakObservationV1, UnitOwnerReplaceBreakStateV1,
};
use crate::source::Span;

pub(super) struct ReplaceBreakInput {
    pub(super) consumer_target:
        crate::ownership_checking::compilation_unit::owner_transport::ReplaceBreakSuffix,
    pub(super) statement: StatementId,
    pub(super) condition: ExpressionId,
    pub(super) body: StatementId,
    pub(super) jump: UnitExpressionId,
    initial: UnitExpressionId,
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
impl ReplaceBreakInput {
    pub(super) fn admit(
        checker: &Checker<'_>,
        elements: &[StatementId],
        parameters: &BTreeSet<UnitSymbolId>,
    ) -> Option<Self> {
        let [prefix, statement, suffix] = elements else {
            return None;
        };
        if parameters.len() != 1 {
            return None;
        }
        let ast = checker.parsed.ast();
        let (root, initial) = variable(checker, *prefix, VariableKind::Var)?;
        let consumer_target = if let Some((retired, used)) =
            variable(checker, *suffix, VariableKind::Val)
        {
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
        if !literal(checker, initial) {
            return None;
        }
        let Statement::While {
            condition, body, ..
        } = ast.statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        // Keep the actual outer Group/Not condition; true-body and false-exit roles do not swap.
        if conditional::boolean_parameter(checker, *condition, parameters).is_none() {
            return None;
        }
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
        if !matches!(
            ast.expressions().get(*jump).ok()?.payload(),
            Expression::Break { .. }
        ) || !exact_type(checker, *jump, BuiltinType::Nothing)
        {
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
            consumer_target,
            statement: *statement,
            condition: *condition,
            body: *body,
            jump: checker.unit_expression(*jump),
            initial: checker.unit_expression(initial),
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
        let Some(catalog) = UnitOwnerProducerCatalogV1::from_legacy(observation) else {
            return;
        };
        if scope != 1
            || value.scope_depth != scope
            || value.symbol != self.root
            || value.origin != value.declaration
            || catalog.definitions().len() != 1
            || catalog
                .definition(owner.clone())
                .is_none_or(|d| d.origin() != UnitOwnerOrigin::String(self.initial))
            || observation.events
                != [
                    UnitOwnerTransportEvent::Result {
                        expression: self.initial,
                        owner: owner.clone(),
                    },
                    UnitOwnerTransportEvent::Binding {
                        symbol: self.root,
                        initializer: self.initial,
                        owner,
                    },
                ]
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
        let Some((identity, _, _)) = &self.transfer else {
            return false;
        };
        self.body_completion = self.output(identity, state);
        self.body_completion.is_some()
    }
    // Reached only after the exact active Break witness and actual Ok(false) were checked.
    pub(super) fn finish(
        &mut self,
        body: UnitOwnerObservation,
        state: &ValueState,
    ) -> Option<UnitOwnerReplaceBreakObservationV1> {
        let condition_false = self.input(state)?;
        if body.status != UnitOwnerObservationStatus::Complete {
            return None;
        }
        // This raw sink contains foreign P. Definitions-only validation belongs to the new
        // bounded protocol; do not weaken or call the old closed from_legacy factory.
        let producers = UnitOwnerProducerCatalogV1 {
            identity: body.identity,
            callable: body.callable,
            definitions: body.definitions,
        };
        let prefix = self.prefix.take()?;
        let (completed_identity, completed_jump, post_break) = self.transfer.take()?;
        let source = self.initial.source_unit();
        let statement = UnitStatementId::new(source, self.statement);
        let condition = UnitExpressionId::new(source, self.condition);
        let body_statement = UnitStatementId::new(source, self.body);
        let value = UnitOwnerReplaceBreakObservationV1 {
            suffix: self.consumer_target.clone(),
            consumer: None,
            call_consumer: None,
            return_consumer: None,
            identity: prefix.identity,
            callable: prefix.callable,
            completed_identity,
            completed_jump,
            statement,
            condition,
            body: body_statement,
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
            post_break,
            body_completion: self.body_completion.take()?,
            condition_false,
            edges: LoopBodyCompletion::Break(self.jump).edges(statement, condition, body_statement),
        };
        value.is_consistent().then_some(value)
    }
}
