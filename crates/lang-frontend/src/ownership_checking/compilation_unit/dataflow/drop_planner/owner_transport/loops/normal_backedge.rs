//! Normal-backedge collection at real planner entry, body completion and Return hooks.
use super::*;
use crate::ownership_checking::{UnitOwnerNormalBackedgeV1, UnitOwnerProducerCatalogV1};
use crate::parser::AssignmentOperator;
mod control;

pub(super) struct NormalInput {
    pub(super) statement: StatementId,
    pub(super) condition: ExpressionId,
    pub(super) body: StatementId,
    assignment: ExpressionId,
    increment: Option<ExpressionId>,
    root: UnitSymbolId,
    initial: UnitExpressionId,
    argument: UnitExpressionId,
    call: UnitExpressionId,
    call_origin: crate::source::Span,
    previous: UnitSymbolId,
    previous_statement: UnitStatementId,
    target: crate::ownership_checking::compilation_unit::owner_transport::ReturnConsumerTarget,
    expected: Option<OwnedValue>,
    prefix: Option<UnitOwnerObservation>,
    entered: bool,
    assigned: bool,
    completed: Option<crate::ownership_checking::UnitOwnerReplaceBreakStateV1>,
    product: Option<UnitOwnerNormalBackedgeV1>,
    valid: bool,
}
impl NormalInput {
    pub(super) fn admit(
        checker: &Checker<'_>,
        elements: &[StatementId],
        parameters: &BTreeSet<UnitSymbolId>,
    ) -> Option<Self> {
        let [prefix, control, statement, suffix] = elements else {
            return None;
        };
        if parameters.len() != 1 {
            return None;
        }
        let ast = checker.parsed.ast();
        let (root, initial) = super::replace_break::variable(checker, *prefix, VariableKind::Var)?;
        if !super::replace_break::literal(checker, initial) {
            return None;
        }
        let Statement::LocalVariable { declaration } =
            ast.statements().get(*control).ok()?.payload()
        else {
            return None;
        };
        let Item::Variable {
            name,
            kind: VariableKind::Var,
            initializer,
            ..
        } = ast.items().get(*declaration).ok()?.payload()
        else {
            return None;
        };
        let control_symbol = *checker.marker_symbol(*name)?;
        let Statement::While {
            condition, body, ..
        } = ast.statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
            return None;
        };
        let [declaration, assignment_statement] = elements.as_slice() else {
            return None;
        };
        let (previous, call) =
            super::replace_break::variable(checker, *declaration, VariableKind::Val)?;
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
        let [place, incoming] = arguments.as_slice() else {
            return None;
        };
        let place_node = ast.expressions().get(place.value).ok()?;
        if !type_arguments.is_empty()
            || !matches!(
                ast.expressions().get(*callee).ok()?.payload(),
                Expression::Name
            )
            || place.named_prefix.is_some()
            || incoming.named_prefix.is_some()
            || incoming.mode_marker.is_some()
            || !matches!(
                place.mode_marker,
                Some(crate::parser::ParameterModeMarker::Inout(_))
            )
            || !matches!(place_node.payload(), Expression::Name)
            || checker.reference_symbol(place_node.span()) != Some(root)
            || !super::replace_break::literal(checker, incoming.value)
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
        let Statement::Expression {
            expression: assignment,
        } = ast.statements().get(*assignment_statement).ok()?.payload()
        else {
            return None;
        };
        let Expression::Assignment {
            target,
            operator: AssignmentOperator::Assign,
            value,
            ..
        } = ast.expressions().get(*assignment).ok()?.payload()
        else {
            return None;
        };
        let increment = control::admit(
            checker,
            parameters,
            control_symbol,
            *initializer,
            *condition,
            *target,
            *value,
        )?;
        let target = super::super::replace_return_consumer::admit(checker, *suffix, root)?;
        if target.binding.is_some() || target.replacement.is_some() {
            return None;
        }
        Some(Self {
            statement: *statement,
            condition: *condition,
            body: *body,
            assignment: *assignment,
            increment,
            root,
            initial: checker.unit_expression(initial),
            argument: checker.unit_expression(incoming.value),
            call: checker.unit_expression(call),
            call_origin: call_node.span(),
            previous,
            previous_statement: UnitStatementId::new(checker.source_unit, *declaration),
            target,
            expected: None,
            prefix: None,
            entered: false,
            assigned: false,
            completed: None,
            product: None,
            valid: true,
        })
    }
    pub(super) fn capture(
        &mut self,
        observation: &UnitOwnerObservation,
        state: &ValueState,
        scope: usize,
    ) {
        let Some([value]) = boundary_values(state) else {
            self.valid = false;
            return;
        };
        let Some(owner) = value.owner.clone() else {
            self.valid = false;
            return;
        };
        let Some(catalog) = UnitOwnerProducerCatalogV1::from_legacy(observation) else {
            self.valid = false;
            return;
        };
        if scope != 1
            || value.scope_depth != 1
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
            || self.prefix.is_some()
        {
            self.valid = false;
            return;
        }
        self.expected = Some(value.clone());
        self.prefix = Some(observation.clone());
    }
    pub(super) fn enter(&mut self, state: &ValueState) -> bool {
        self.entered = self.valid
            && self
                .expected
                .as_ref()
                .is_some_and(|v| boundary_values(state) == Some(std::slice::from_ref(v)));
        self.entered
    }
    pub(super) fn complete(&mut self, state: &ValueState) -> bool {
        let Some([value]) = boundary_values(state) else {
            return false;
        };
        let Some(expected) = &self.expected else {
            return false;
        };
        if !self.valid
            || !self.entered
            || !self.assigned
            || self.completed.is_some()
            || value.symbol != expected.symbol
            || value.declaration != expected.declaration
            || value.scope_depth != expected.scope_depth
            || value.origin != self.call_origin
            || value.owner == expected.owner
        {
            return false;
        }
        self.completed = super::super::replace_consumer::snapshot(value);
        self.completed.is_some()
    }
    pub(super) fn finish_body(&mut self, body: UnitOwnerObservation, state: &ValueState) {
        let Some(prefix) = self.prefix.take() else {
            self.valid = false;
            return;
        };
        let Some(expected) = &self.expected else {
            self.valid = false;
            return;
        };
        let Some(backedge) = self.completed.take() else {
            self.valid = false;
            return;
        };
        if body.status != UnitOwnerObservationStatus::Complete
            || boundary_values(state) != Some(std::slice::from_ref(expected))
        {
            self.valid = false;
            return;
        }
        let Some(entry) = super::super::replace_consumer::snapshot(expected) else {
            self.valid = false;
            return;
        };
        let source = self.initial.source_unit();
        self.product = Some(UnitOwnerNormalBackedgeV1 {
            identity: prefix.identity.clone(),
            callable: prefix.callable,
            statement: UnitStatementId::new(source, self.statement),
            condition: UnitExpressionId::new(source, self.condition),
            body: UnitStatementId::new(source, self.body),
            initial: self.initial,
            argument: self.argument,
            call: self.call,
            previous: self.previous,
            previous_statement: self.previous_statement,
            inputs: UnitOwnerProducerCatalogV1 {
                identity: prefix.identity,
                callable: prefix.callable,
                definitions: prefix.definitions,
            },
            producers: UnitOwnerProducerCatalogV1 {
                identity: body.identity,
                callable: body.callable,
                definitions: body.definitions,
            },
            prefix: prefix.events,
            events: body.events,
            entry: entry.clone(),
            backedge,
            false_sample: entry,
            target: self.target.clone(),
            assignment: UnitExpressionId::new(source, self.assignment),
            increment: self.increment.map(|id| UnitExpressionId::new(source, id)),
            assignment_completed: self.assigned,
            body_completed: true,
            taken: false,
            delivered: false,
            cleaned: false,
        });
    }
    fn take(&mut self, expression: UnitExpressionId, value: Option<&OwnedValue>) {
        if expression != self.target.payload {
            return;
        }
        let Some(product) = &mut self.product else {
            self.valid = false;
            return;
        };
        if !self.valid || product.taken || value != self.expected.as_ref() {
            self.valid = false;
            return;
        }
        product.taken = true;
    }
    fn deliver(
        &mut self,
        expression: UnitExpressionId,
        payload: UnitExpressionId,
        state: &ValueState,
    ) {
        let Some(product) = &mut self.product else {
            self.valid = false;
            return;
        };
        if !self.valid
            || expression != self.target.expression
            || payload != self.target.payload
            || !product.taken
            || product.delivered
            || !empty_boundary(state)
        {
            self.valid = false;
            return;
        }
        product.delivered = true;
    }
    fn finish(
        &mut self,
        expression: UnitExpressionId,
        state: &ValueState,
    ) -> Option<UnitOwnerOpenLoopObservationV1> {
        let mut product = self.product.take()?;
        if !self.valid || expression != self.target.expression || !empty_boundary(state) {
            return None;
        }
        product.cleaned = true;
        product
            .is_consistent()
            .then_some(UnitOwnerOpenLoopObservationV1::NormalBackedge(Box::new(
                product,
            )))
    }
}
impl DropPlanner<'_, '_> {
    pub(in super::super) fn normal_control_expression_supported(&self, id: ExpressionId) -> bool {
        let Some(transaction) = &self.owner_loop else {
            return false;
        };
        let Some(normal) = &transaction.normal else {
            return false;
        };
        if normal.increment == Some(id) {
            return self.normal_assignment_supported(normal.assignment);
        }
        normal.increment.is_some()
            && normal.condition == id
            && normal.valid
            && !normal.entered
            && normal.prefix.is_some()
            && transaction.active.is_none()
            && self.loop_boundaries.is_empty()
            && self.owner_branch_depth == 0
            && self.scope_depth == 1
    }
    pub(in super::super) fn normal_assignment_supported(&self, id: ExpressionId) -> bool {
        self.owner_loop.as_ref().is_some_and(|t| {
            t.normal
                .as_ref()
                .is_some_and(|n| n.valid && n.entered && !n.assigned && n.assignment == id)
                && t.active.as_ref().is_some_and(|(identity, boundary)| {
                    self.loop_boundaries.as_slice() == [*boundary]
                        && self.scope_depth == boundary + 1
                        && self.owner_branch_depth == 0
                        && self
                            .owner_observation
                            .as_ref()
                            .is_some_and(|o| std::sync::Arc::ptr_eq(identity, &o.identity))
                })
        })
    }
    pub(in super::super) fn normal_assignment_completed(
        &mut self,
        id: ExpressionId,
        continues: bool,
    ) {
        if self.normal_assignment_supported(id) {
            let normal = self
                .owner_loop
                .as_mut()
                .and_then(|t| t.normal.as_mut())
                .expect("checked normal assignment");
            normal.assigned = continues;
            normal.valid &= continues;
        }
    }
    fn normal_tail(&self) -> bool {
        self.owner_branch_depth == 0
            && self.loop_boundaries.is_empty()
            && self.scope_depth == 1
            && self
                .owner_loop
                .as_ref()
                .is_some_and(|t| t.active.is_none() && t.normal.is_some())
    }
    pub(super) fn take_normal_return(&mut self, id: ExpressionId, value: Option<&OwnedValue>) {
        if self.normal_tail() {
            self.owner_loop
                .as_mut()
                .and_then(|t| t.normal.as_mut())
                .expect("checked tail")
                .take(self.checker.unit_expression(id), value);
        }
    }
    pub(super) fn deliver_normal_return(
        &mut self,
        id: ExpressionId,
        payload: ExpressionId,
        state: &ValueState,
    ) {
        if self.normal_tail() {
            self.owner_loop
                .as_mut()
                .and_then(|t| t.normal.as_mut())
                .expect("checked tail")
                .deliver(
                    self.checker.unit_expression(id),
                    self.checker.unit_expression(payload),
                    state,
                );
        }
    }
    pub(super) fn finish_normal_return(&mut self, id: ExpressionId, state: &ValueState) {
        if self.normal_tail() {
            let transaction = self.owner_loop.as_mut().expect("checked tail");
            transaction.replace_result = transaction
                .normal
                .as_mut()
                .expect("checked tail")
                .finish(self.checker.unit_expression(id), state);
        }
    }
}
