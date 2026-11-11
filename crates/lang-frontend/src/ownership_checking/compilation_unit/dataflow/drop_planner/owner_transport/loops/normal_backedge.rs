//! Normal-backedge collection at real planner entry, body completion and Return hooks.
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::{
    HeaderCompletion, HeaderCore, HeaderEquation,
};
use crate::ownership_checking::{
    UnitOwnerContinueBackedgeV1, UnitOwnerNormalBackedgeV1, UnitOwnerProducerCatalogV1,
};
use crate::parser::AssignmentOperator;
mod binding;
mod conditional_break;
mod conditional_break_hooks;
mod conditional_return;
mod conditional_return_hooks;
mod control;
use binding::NormalSuffix;
mod mixed;

pub(super) struct NormalInput {
    pub(super) statement: StatementId,
    pub(super) condition: ExpressionId,
    pub(super) body: StatementId,
    assignment: ExpressionId,
    increment: Option<ExpressionId>,
    root: UnitSymbolId,
    initial: UnitExpressionId,
    initial_origin: UnitOwnerOrigin,
    argument: UnitExpressionId,
    call: UnitExpressionId,
    call_origin: crate::source::Span,
    previous: UnitSymbolId,
    previous_statement: UnitStatementId,
    suffix: NormalSuffix,
    expected: Option<OwnedValue>,
    prefix: Option<UnitOwnerObservation>,
    entered: bool,
    assigned: bool,
    completed: Option<crate::ownership_checking::UnitOwnerReplaceBreakStateV1>,
    product: Option<HeaderCore>,
    jump: Option<UnitExpressionId>,
    mixed: Option<mixed::MixedInput>,
    conditional_break: Option<conditional_break::ConditionalBreakInput>,
    conditional_return: Option<conditional_return::ConditionalReturnInput>,
    transfer: Option<(
        std::sync::Arc<()>,
        UnitExpressionId,
        crate::ownership_checking::UnitOwnerReplaceBreakStateV1,
    )>,
    finished_role: Option<LoopBodyCompletion>,
    valid: bool,
}
impl NormalInput {
    pub(super) fn admit(
        checker: &Checker<'_>,
        elements: &[StatementId],
        parameters: &BTreeSet<UnitSymbolId>,
        owner_parameter: Option<UnitSymbolId>,
        returns_unit: bool,
        callable: UnitOwnerCallable,
    ) -> Option<Self> {
        let [prefix, control, statement, suffix] = elements else {
            return None;
        };
        if !(1..=2).contains(&parameters.len()) {
            return None;
        }
        let ast = checker.parsed.ast();
        let (root, initial) = super::replace_break::variable(checker, *prefix, VariableKind::Var)?;
        let initial_origin = match owner_parameter {
            Some(parameter) => {
                let node = ast.expressions().get(initial).ok()?;
                if !matches!(node.payload(), Expression::Name)
                    || checker.reference_symbol(node.span()) != Some(parameter)
                    || !exact_type(checker, initial, BuiltinType::String)
                    || parameter == root
                {
                    return None;
                }
                UnitOwnerOrigin::Parameter(parameter)
            }
            None if super::replace_break::literal(checker, initial) => {
                UnitOwnerOrigin::String(checker.unit_expression(initial))
            }
            _ => return None,
        };
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
        let mut conditional_break = None;
        let mut conditional_return = None;
        let (declaration, assignment_statement, jump, mixed) = match elements.as_slice() {
            [declaration, assignment] => (*declaration, *assignment, None, None),
            [declaration, assignment, tail] => {
                let Statement::Expression { expression } =
                    ast.statements().get(*tail).ok()?.payload()
                else {
                    return None;
                };
                match ast.expressions().get(*expression).ok()?.payload() {
                    Expression::Continue { .. } => (
                        *declaration,
                        *assignment,
                        Some(checker.unit_expression(*expression)),
                        None,
                    ),
                    Expression::If {
                        else_branch: None, ..
                    } => {
                        conditional_break = conditional_break::ConditionalBreakInput::admit(
                            checker,
                            *expression,
                            control_symbol,
                            *condition,
                            parameters,
                            callable,
                        );
                        if conditional_break.is_none() {
                            conditional_return =
                                Some(conditional_return::ConditionalReturnInput::admit(
                                    checker,
                                    *expression,
                                    control_symbol,
                                    *condition,
                                    parameters,
                                    callable,
                                    root,
                                )?);
                        }
                        (*declaration, *assignment, None, None)
                    }
                    Expression::If { .. } => (
                        *declaration,
                        *assignment,
                        None,
                        Some(mixed::MixedInput::admit(
                            checker,
                            *expression,
                            control_symbol,
                            *condition,
                            parameters,
                            callable,
                        )?),
                    ),
                    _ => return None,
                }
            }
            _ => return None,
        };
        let (previous, call) =
            super::replace_break::variable(checker, declaration, VariableKind::Val)?;
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
        } = ast.statements().get(assignment_statement).ok()?.payload()
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
        if ((mixed.is_some() || conditional_break.is_some() || conditional_return.is_some())
            && increment.is_none())
            || (parameters.len() != 1
                && mixed.is_none()
                && conditional_break.is_none()
                && conditional_return.is_none())
        {
            return None;
        }
        if owner_parameter.is_some()
            && (parameters.len() != 1
                || increment.is_none()
                || jump.is_some()
                || mixed.is_some()
                || conditional_break.is_some()
                || conditional_return.is_some())
        {
            return None;
        }
        let suffix = if returns_unit {
            if owner_parameter.is_some()
                || increment.is_none()
                || jump.is_some()
                || mixed.is_some()
                || conditional_break.is_some()
                || conditional_return.is_some()
            {
                return None;
            }
            NormalSuffix::BindDrop(Box::new(binding::BindInput::admit(checker, *suffix, root)?))
        } else {
            let target = super::super::replace_return_consumer::admit(checker, *suffix, root)?;
            if target.binding.is_some() || target.replacement.is_some() {
                return None;
            }
            NormalSuffix::Return(target)
        };
        Some(Self {
            statement: *statement,
            condition: *condition,
            body: *body,
            assignment: *assignment,
            increment,
            root,
            initial: checker.unit_expression(initial),
            initial_origin,
            argument: checker.unit_expression(incoming.value),
            call: checker.unit_expression(call),
            call_origin: call_node.span(),
            previous,
            previous_statement: UnitStatementId::new(checker.source_unit, declaration),
            suffix,
            expected: None,
            prefix: None,
            entered: false,
            assigned: false,
            completed: None,
            product: None,
            jump,
            mixed,
            conditional_break,
            conditional_return,
            transfer: None,
            finished_role: None,
            valid: true,
        })
    }
    pub(super) fn expected_completion(&self) -> LoopBodyCompletion {
        if let Some(input) = &self.conditional_return {
            return input.role;
        }
        if let Some(input) = &self.conditional_break {
            return input.role;
        }
        if let Some(mixed) = &self.mixed {
            return mixed.role;
        }
        self.jump.map_or(
            LoopBodyCompletion::Fallthrough,
            LoopBodyCompletion::Continue,
        )
    }
    pub(super) fn jump_boundary(
        &mut self,
        completion: LoopBodyCompletion,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
        branch: Option<usize>,
    ) -> bool {
        if let Some(input) = &mut self.conditional_break {
            return self.valid
                && self.entered
                && self.assigned
                && branch == Some(0)
                && input.jump(completion, identity, state);
        }
        if let Some(mixed) = &mut self.mixed {
            return self.valid
                && self.entered
                && self.assigned
                && branch.is_some_and(|b| mixed.jump(b, completion, identity, state));
        }
        let LoopBodyCompletion::Continue(jump) = completion else {
            return false;
        };
        if self.jump != Some(jump)
            || !self.valid
            || !self.entered
            || !self.assigned
            || self.transfer.is_some()
        {
            return false;
        }
        let Some([value]) = boundary_values(state) else {
            return false;
        };
        let Some(snapshot) = super::super::replace_consumer::snapshot(value) else {
            return false;
        };
        self.transfer = Some((identity.clone(), jump, snapshot));
        true
    }
    pub(super) fn finish_role(&mut self, completion: LoopBodyCompletion) -> bool {
        if completion != self.expected_completion() || self.finished_role.is_some() {
            self.valid = false;
            return false;
        }
        self.finished_role = Some(completion);
        true
    }
    pub(super) fn mixed_active(&self) -> bool {
        self.mixed.is_some()
    }
    pub(super) fn split_enter(
        &mut self,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
    ) -> bool {
        self.valid
            && self.entered
            && self.assigned
            && self
                .mixed
                .as_mut()
                .is_some_and(|m| m.enter(identity, state))
    }
    pub(super) fn split_arm_enter(
        &mut self,
        branch: usize,
        control: UnitExpressionId,
        body: UnitStatementId,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
    ) -> bool {
        self.mixed
            .as_mut()
            .is_some_and(|m| m.arm_entry(branch, control, body, identity, state))
    }
    pub(super) fn split_arm_complete(
        &mut self,
        branch: usize,
        continues: bool,
        state: &ValueState,
    ) -> bool {
        self.mixed
            .as_mut()
            .is_some_and(|m| m.arm_complete(branch, continues, state))
    }
    pub(super) fn split_complete(&self) -> bool {
        self.mixed.as_ref().is_some_and(|m| m.complete())
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
        let catalog = if matches!(self.initial_origin, UnitOwnerOrigin::Parameter(_)) {
            if observation.status != UnitOwnerObservationStatus::Complete
                || !observation.is_consistent()
            {
                self.valid = false;
                return;
            }
            UnitOwnerProducerCatalogV1 {
                identity: observation.identity.clone(),
                callable: observation.callable,
                definitions: observation.definitions.clone(),
            }
        } else {
            let Some(catalog) = UnitOwnerProducerCatalogV1::from_legacy(observation) else {
                self.valid = false;
                return;
            };
            catalog
        };
        let mut prefix = Vec::new();
        if let UnitOwnerOrigin::Parameter(symbol) = self.initial_origin {
            prefix.push(UnitOwnerTransportEvent::Move {
                expression: self.initial,
                symbol,
                owner: owner.clone(),
            });
        }
        prefix.extend([
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
            || value.scope_depth != 1
            || value.symbol != self.root
            || value.origin != value.declaration
            || catalog.definitions().len() != 1
            || catalog
                .definition(owner.clone())
                .is_none_or(|d| d.origin() != self.initial_origin)
            || !catalog.is_consistent()
            || observation.events != prefix
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
        if let Some(mixed) = &self.mixed {
            // Both terminal arms have discarded their local state. Use only their actual
            // independently recorded completions, never reconstruct a post-If Q state.
            let Some(snapshot) = mixed.completed_snapshot() else {
                return false;
            };
            let Some(expected) = &self.expected else {
                return false;
            };
            if !self.valid
                || !self.entered
                || !self.assigned
                || self.completed.is_some()
                || snapshot.symbol != expected.symbol
                || snapshot.declaration != expected.declaration
                || snapshot.scope_depth != expected.scope_depth
                || snapshot.origin != self.call_origin
                || Some(snapshot.owner.clone()) == expected.owner
            {
                return false;
            }
            self.completed = Some(snapshot.clone());
            return true;
        }
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
        if self
            .conditional_break
            .as_mut()
            .is_some_and(|input| !input.complete(state))
        {
            return false;
        }
        if self
            .conditional_return
            .as_mut()
            .is_some_and(|input| !input.complete(state))
        {
            return false;
        }
        self.completed = super::super::replace_consumer::snapshot(value);
        match (&self.completed, self.jump, &self.transfer) {
            (Some(snapshot), Some(jump), Some((_, actual, transferred))) => {
                jump == *actual && snapshot == transferred
            }
            (Some(_), None, None) => true,
            _ => false,
        }
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
            || !self.valid
            || self.finished_role != Some(self.expected_completion())
            || self
                .transfer
                .as_ref()
                .is_some_and(|(identity, _, _)| !std::sync::Arc::ptr_eq(identity, &body.identity))
            || self
                .mixed
                .as_ref()
                .is_some_and(|m| !m.identity_matches(&body.identity))
            || self
                .conditional_break
                .as_ref()
                .is_some_and(|input| !input.identity_matches(&body.identity))
            || self
                .conditional_return
                .as_ref()
                .is_some_and(|input| !input.identity_matches(&body.identity))
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
        let equation = HeaderEquation {
            completion: match self.expected_completion() {
                LoopBodyCompletion::ConditionalReturn { control, .. } => {
                    HeaderCompletion::ConditionalReturn(control)
                }
                LoopBodyCompletion::ConditionalBreak { control, .. } => {
                    HeaderCompletion::ConditionalBreak(control)
                }
                LoopBodyCompletion::Conditional { control, .. } => HeaderCompletion::Mixed(control),
                _ => self
                    .jump
                    .map_or(HeaderCompletion::Fallthrough, HeaderCompletion::Continue),
            },
            identity: prefix.identity.clone(),
            callable: prefix.callable,
            statement: UnitStatementId::new(source, self.statement),
            condition: UnitExpressionId::new(source, self.condition),
            body: UnitStatementId::new(source, self.body),
            initial: self.initial,
            initial_origin: self.initial_origin,
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
            assignment: UnitExpressionId::new(source, self.assignment),
            increment: self.increment.map(|id| UnitExpressionId::new(source, id)),
            assignment_completed: self.assigned,
            body_completed: true,
        };
        match &mut self.suffix {
            NormalSuffix::Return(target) => {
                self.product = Some(HeaderCore {
                    equation,
                    target: target.clone(),
                    taken: false,
                    delivered: false,
                    cleaned: false,
                })
            }
            NormalSuffix::BindDrop(input) => input.install(equation),
        }
    }
    fn take(&mut self, expression: UnitExpressionId, value: Option<&OwnedValue>) {
        let NormalSuffix::Return(target) = &mut self.suffix else {
            if let NormalSuffix::BindDrop(input) = &mut self.suffix {
                input.take(expression, value);
            }
            return;
        };
        if expression != target.payload {
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
        let NormalSuffix::Return(target) = &self.suffix else {
            return;
        };
        let Some(product) = &mut self.product else {
            self.valid = false;
            return;
        };
        if !self.valid
            || expression != target.expression
            || payload != target.payload
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
        let NormalSuffix::Return(target) = &self.suffix else {
            return None;
        };
        let mut product = self.product.take()?;
        if !self.valid || expression != target.expression || !empty_boundary(state) {
            return None;
        }
        product.cleaned = true;
        if !product.is_consistent() {
            return None;
        }
        if let Some(input) = self.conditional_return.take() {
            let value = input.finish(product)?;
            value.is_consistent().then_some(
                UnitOwnerOpenLoopObservationV1::ConditionalReturnBackedge(Box::new(value)),
            )
        } else if let Some(input) = self.conditional_break.take() {
            let value = input.finish(product)?;
            value.is_consistent().then_some(
                UnitOwnerOpenLoopObservationV1::ConditionalBreakBackedge(Box::new(value)),
            )
        } else if let Some(mixed) = self.mixed.take() {
            let value = mixed.finish(product)?;
            value
                .is_consistent()
                .then_some(UnitOwnerOpenLoopObservationV1::MixedBackedge(Box::new(
                    value,
                )))
        } else if let Some(jump) = self.jump {
            let (identity, actual, snapshot) = self.transfer.take()?;
            let value = UnitOwnerContinueBackedgeV1 {
                target: product.statement,
                core: product,
                jump,
                identity,
                snapshot,
                stopped: actual == jump
                    && self.finished_role == Some(LoopBodyCompletion::Continue(jump)),
            };
            value
                .is_consistent()
                .then_some(UnitOwnerOpenLoopObservationV1::ContinueBackedge(Box::new(
                    value,
                )))
        } else {
            let value = UnitOwnerNormalBackedgeV1 { core: product };
            value
                .is_consistent()
                .then_some(UnitOwnerOpenLoopObservationV1::NormalBackedge(Box::new(
                    value,
                )))
        }
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
        if normal
            .mixed
            .as_ref()
            .is_some_and(|m| m.condition() == self.checker.unit_expression(id))
            || normal
                .conditional_break
                .as_ref()
                .is_some_and(|m| m.condition() == self.checker.unit_expression(id))
            || normal
                .conditional_return
                .as_ref()
                .is_some_and(|m| m.condition() == self.checker.unit_expression(id))
        {
            return normal.valid
                && normal.entered
                && normal.assigned
                && transaction
                    .active
                    .as_ref()
                    .is_some_and(|(identity, boundary)| {
                        self.loop_boundaries.as_slice() == [*boundary]
                            && self.scope_depth == boundary + 1
                            && self.owner_branch_depth == 0
                            && self
                                .owner_observation
                                .as_ref()
                                .is_some_and(|o| std::sync::Arc::ptr_eq(identity, &o.identity))
                    });
        }
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
