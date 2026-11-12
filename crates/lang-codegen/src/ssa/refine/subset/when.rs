//! Subset rules for `when` expressions (SPEC-0356, SPEC-0360) and intrinsic `Rc<T>` (SPEC-0359, SPEC-0378).
use std::collections::BTreeSet;

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{ConstructionRootKind, LoanKind, RcOwnershipEffectKind},
    parser::{Expression, WhenCondition, WhenEntry},
    type_checking::{
        Copyability, ExpressionCategory, IntegerRangeOperationKind, IntrinsicTypeConstructor,
        ParameterMode, RcOperationKind, UnitConstructionTarget, UnitRcOperationDescriptor,
        UnitTypeId, UnitTypeKind,
    },
};

use super::Checker;

impl Checker<'_, '_> {
    pub(super) fn rc_payload(&self, ty: UnitTypeId) -> Option<UnitTypeId> {
        self.rc_payload_seen(ty, &mut BTreeSet::new())
    }

    pub(super) fn rc_payload_seen(
        &self,
        ty: UnitTypeId,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> Option<UnitTypeId> {
        let Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Rc,
            arguments,
        }) = self.kind(ty)
        else {
            return None;
        };
        let [payload] = arguments.as_slice() else {
            return None;
        };
        if !active.insert(ty) {
            return None;
        }
        let ok = self.storage_type_seen(*payload, active);
        active.remove(&ty);
        ok.then_some(*payload)
    }

    pub(super) fn rc_construction(&mut self, expression: ExpressionId) -> bool {
        self.migrated = true;
        let id = self.id(expression);
        let Some(descriptor) = self
            .inputs
            .typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
        else {
            return false;
        };
        let result = self.c(descriptor.result_type());
        let (Some(payload), [argument], [type_argument]) = (
            self.rc_payload(result),
            descriptor.arguments(),
            descriptor.instance().type_arguments(),
        ) else {
            return false;
        };
        let mut plans = self
            .inputs
            .owned
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == id);
        let (Some(plan), None) = (plans.next(), plans.next()) else {
            return false;
        };
        let value = argument.argument();
        descriptor.target() == UnitConstructionTarget::IntrinsicRc
            && self.expression_type(expression) == Some(result)
            && self.c(*type_argument) == payload
            && self.c(argument.parameter_type()) == payload
            && value.source_unit() == self.inputs.source_unit
            && plan.target() == descriptor.target()
            && plan.terminating_operand().is_none()
            && plan.deliveries().len() == 1
            && plan.deliveries()[0].argument() == value
            && plan.root_obligation().is_some_and(|root| {
                root.construction() == id
                    && self.c(root.result_type()) == result
                    && root.kind() == ConstructionRootKind::SharedOwner
            })
            && self.expression_type(value.expression()) == Some(payload)
            && self.expression(value.expression())
    }

    pub(super) fn rc_operation_on_owner(
        &self,
        expression: ExpressionId,
    ) -> Option<UnitRcOperationDescriptor> {
        let id = self.id(expression);
        let descriptor = self.inputs.typed.rc_operation(id)?;
        let mut effects = self
            .inputs
            .owned
            .rc_effects()
            .iter()
            .filter(|effect| effect.expression() == id);
        let effect = effects.next()?;
        let expected = match descriptor.kind() {
            RcOperationKind::Share => RcOwnershipEffectKind::Retain,
            RcOperationKind::Value => RcOwnershipEffectKind::BorrowPayload,
        };
        let receiver = descriptor.receiver();
        let shape = descriptor.expression() == id
            && effects.next().is_none()
            && effect.receiver() == receiver
            && self.c(effect.payload_type()) == self.c(descriptor.payload_type())
            && effect.kind() == expected
            && receiver.source_unit() == self.inputs.source_unit
            && self.inputs.typed.expression_category(receiver) == Some(ExpressionCategory::Place);
        let root = self.ungrouped(receiver.expression());
        let node = self.inputs.parsed.ast().expressions().get(root).ok()?;
        let payload_ty = self.c(descriptor.payload_type());
        let owner = matches!(node.payload(), Expression::Name)
            && self.reference(root).is_some_and(|symbol| {
                self.known.contains(&symbol)
                    && matches!(
                        self.parameter_mode(symbol),
                        None | Some(ParameterMode::Value)
                    )
                    && match self.inputs.typed.non_null_use(receiver) {
                        None => self
                            .symbol_type(symbol)
                            .is_some_and(|ty| self.rc_payload(ty) == Some(payload_ty)),
                        Some(fact) => {
                            fact.symbol() == symbol
                                && self.view_symbols.contains(&symbol)
                                && self.rc_payload(self.c(fact.narrowed_type())) == Some(payload_ty)
                        }
                    }
            });
        (shape && owner).then_some(descriptor)
    }

    pub(super) fn rc_operation(&mut self, expression: ExpressionId) -> bool {
        self.migrated = true;
        self.rc_operation_on_owner(expression)
            .is_some_and(|descriptor| match descriptor.kind() {
                RcOperationKind::Share => true,
                RcOperationKind::Value => {
                    let payload = self.c(descriptor.payload_type());
                    self.storage_type(payload)
                        && self.copyability_of(payload) == Copyability::Copyable
                }
            })
    }

    pub(super) fn rc_value_owner(&mut self, receiver: ExpressionId) -> Option<UnitTypeId> {
        self.migrated = true;
        let descriptor = self.rc_operation_on_owner(receiver)?;
        (descriptor.kind() == RcOperationKind::Value
            && self.class(self.c(descriptor.payload_type())))
        .then(|| self.c(descriptor.payload_type()))
    }

    pub(super) fn borrowed_rc_payload(
        &mut self,
        call: ExpressionId,
        value: ExpressionId,
        parameter: UnitTypeId,
    ) -> bool {
        let Ok(call_node) = self.inputs.parsed.ast().expressions().get(call) else {
            return false;
        };
        if self.span_has_control_transfer(call_node.span())
            || self.copyability_of(parameter) != Copyability::MoveOnly
            || !self.storage_type(parameter)
        {
            return false;
        }
        let root = self.ungrouped(value);
        let Some(descriptor) = self.rc_operation_on_owner(root) else {
            return false;
        };
        let ok = descriptor.kind() == RcOperationKind::Value
            && self.c(descriptor.payload_type()) == parameter
            && self.expression_type(root) == Some(parameter)
            && self.inputs.owned.loans().iter().any(|loan| {
                loan.call() == self.id(call)
                    && (loan.argument() == self.id(value) || loan.argument() == self.id(root))
                    && loan.kind() == LoanKind::Shared
            });
        self.migrated |= ok;
        ok
    }

    /// A scalar subject compared by value, an integer subject tested for Range membership,
    /// or a Copyable enum subject tested for its case. A value-context `when` needs `else`
    /// unless the subject is an enum, whose closed domain the frontend proved covered.
    pub(super) fn subject_when(
        &mut self,
        control: ExpressionId,
        ty: UnitTypeId,
        subject: ExpressionId,
        entries: &[WhenEntry],
    ) -> bool {
        self.migrated = true;
        let Some(subject_type) = self.expression_type(subject) else {
            return false;
        };
        let copy_enum = self.enum_type(subject_type);
        let move_enum = self.box_enum_type(subject_type);
        let tagged = copy_enum || move_enum;
        let string = self.string(subject_type);
        let subject_ok = if string {
            self.string_when_subject(subject)
        } else if move_enum {
            self.move_enum_when_subject(subject, subject_type, entries)
        } else {
            (self.scalar(subject_type) || copy_enum) && self.expression(subject)
        };
        let control = self.id(control);
        let closed_after = self.inputs.typed.when_closed_after(control);
        let Some(entries) = entries.get(..closed_after.unwrap_or(entries.len())) else {
            return false;
        };
        let has_else = entries.iter().any(|entry| entry.else_span.is_some());
        let boolean_exhaustive = matches!(
            self.kind(subject_type),
            Some(lang_frontend::type_checking::UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::Boolean
            ))
        ) && (closed_after.is_some() || self.boolean_two_arms(entries));
        if !subject_ok || !(self.unit(ty) || has_else || tagged || boolean_exhaustive) {
            return false;
        }
        let mut can_reach_entry = true;
        entries.iter().enumerate().all(|(index, entry)| {
            if !can_reach_entry {
                return true;
            }
            let saved = self.view_symbols.clone();
            let mut can_match = entry.else_span.is_some();
            let selector_count = self
                .inputs
                .typed
                .when_final_selector_count(control, index)
                .unwrap_or(entry.conditions.len());
            let conditions_ok = entry.else_span.is_some() == entry.conditions.is_empty()
                && entry
                    .conditions
                    .iter()
                    .take(selector_count)
                    .all(|condition| {
                        if !can_reach_entry {
                            return true;
                        }
                        let ok = match condition {
                            WhenCondition::Expression(condition) if string => {
                                self.string_when_literal(*condition)
                            }
                            WhenCondition::Expression(condition) if tagged => {
                                copy_enum && self.case_value_condition(*condition, subject_type)
                            }
                            WhenCondition::Expression(condition) => {
                                self.expression_type(*condition) == Some(subject_type)
                                    && self.expression(*condition)
                            }
                            WhenCondition::Contains { expression, .. } => {
                                !tagged
                                    && !string
                                    && self.range_condition(*expression, subject_type)
                            }
                            WhenCondition::TypeTest { type_ref, .. } => {
                                tagged && self.type_test_target(*type_ref, subject_type)
                            }
                        };
                        if let WhenCondition::Expression(expression)
                        | WhenCondition::Contains { expression, .. } = condition
                            && self
                                .inputs
                                .typed
                                .expression_falls_through(self.id(*expression))
                                == Some(false)
                        {
                            can_reach_entry = false;
                        } else {
                            can_match = true;
                        }
                        ok
                    });
            let ok = conditions_ok && (!can_match || self.statement(entry.body));
            self.view_symbols = saved;
            ok
        })
    }

    fn move_enum_when_subject(
        &mut self,
        subject: ExpressionId,
        subject_type: UnitTypeId,
        entries: &[WhenEntry],
    ) -> bool {
        let mut operands = Vec::new();
        let mut current = subject;
        loop {
            let Ok(node) = self.inputs.parsed.ast().expressions().get(current) else {
                return false;
            };
            operands.push(current);
            match node.payload() {
                Expression::Group { expression } => current = *expression,
                Expression::Name => break,
                _ => return false,
            }
        }
        let Some(symbol) = self.reference(current) else {
            return false;
        };
        if !self.known.contains(&symbol)
            || !matches!(
                self.parameter_mode(symbol),
                None | Some(ParameterMode::Value)
            )
            || self.symbol_type(symbol) != Some(subject_type)
            || !self.expression(subject)
        {
            return false;
        }
        let snapshot_tag = operands.iter().any(|&operand| {
            let point =
                lang_frontend::ownership_checking::UnitDropPoint::AfterExpression(self.id(operand));
            self.inputs.owned.drops().iter().any(|fact| {
                fact.point() == point
                    && fact.target()
                        == lang_frontend::ownership_checking::UnitDropTarget::Named(symbol)
            })
        });
        !snapshot_tag
            || entries
                .iter()
                .flat_map(|entry| &entry.conditions)
                .all(|condition| matches!(condition, WhenCondition::TypeTest { .. }))
    }

    fn boolean_two_arms(&self, entries: &[WhenEntry]) -> bool {
        let ast = self.inputs.parsed.ast().expressions();
        let pair = |left: ExpressionId, right: ExpressionId| {
            matches!(
                (
                    ast.get(left).map(|n| n.payload()),
                    ast.get(right).map(|n| n.payload())
                ),
                (
                    Ok(Expression::Literal(
                        lang_frontend::parser::LiteralKind::Boolean(a)
                    )),
                    Ok(Expression::Literal(
                        lang_frontend::parser::LiteralKind::Boolean(b)
                    ))
                ) if a != b
            )
        };
        match entries {
            [first, second] if first.else_span.is_none() && second.else_span.is_none() => {
                match (first.conditions.as_slice(), second.conditions.as_slice()) {
                    ([WhenCondition::Expression(left)], [WhenCondition::Expression(right)]) => {
                        pair(*left, *right)
                    }
                    _ => false,
                }
            }
            [only] if only.else_span.is_none() => match only.conditions.as_slice() {
                [
                    WhenCondition::Expression(left),
                    WhenCondition::Expression(right),
                ] => pair(*left, *right),
                _ => false,
            },
            _ => false,
        }
    }

    fn string_when_subject(&mut self, expression: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::Group { expression } => self.string_when_subject(*expression),
            Expression::Name
                if self.reference(expression).is_some_and(|symbol| {
                    self.known.contains(&symbol)
                        && (self.parameter_mode(symbol) == Some(ParameterMode::Borrow)
                            || self.for_borrowed.contains(&symbol))
                }) =>
            {
                true
            }
            Expression::Member { .. } | Expression::Index { .. } => false,
            _ => self.expression(expression),
        }
    }

    fn string_when_literal(&mut self, expression: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::Group { expression } => self.string_when_literal(*expression),
            Expression::String { .. } => self.expression(expression),
            _ => false,
        }
    }

    fn range_condition(&mut self, range: ExpressionId, subject_type: UnitTypeId) -> bool {
        let Some(operation) = self
            .inputs
            .typed
            .integer_range_operation(self.id(range))
            .copied()
        else {
            return false;
        };
        matches!(
            operation.kind(),
            IntegerRangeOperationKind::Inclusive | IntegerRangeOperationKind::Exclusive
        ) && self.integer_type(subject_type)
            && self.c(operation.left_type()) == subject_type
            && self.c(operation.right_type()) == subject_type
            && operation.left().source_unit() == self.inputs.source_unit
            && operation.right().source_unit() == self.inputs.source_unit
            && self.expression(operation.left().expression())
            && (self.inputs.typed.expression_falls_through(operation.left()) == Some(false)
                || self.expression(operation.right().expression()))
    }

    pub(super) fn subjectless_when(&mut self, ty: UnitTypeId, entries: &[WhenEntry]) -> bool {
        self.migrated = true;
        let has_else = entries.iter().any(|entry| entry.else_span.is_some());
        let mut can_reach_entry = true;
        (self.unit(ty) || has_else)
            && entries.iter().all(|entry| {
                if !can_reach_entry {
                    return true;
                }
                let mut can_match = entry.else_span.is_some();
                entry.else_span.is_some() == entry.conditions.is_empty()
                    && entry.conditions.iter().all(|condition| {
                        if !can_reach_entry {
                            return true;
                        }
                        let WhenCondition::Expression(condition) = condition else {
                            return false;
                        };
                        let (yes, no) = self.condition_outcomes(*condition);
                        can_match |= yes;
                        can_reach_entry = no;
                        self.condition(*condition, false)
                    })
                    && (!can_match || self.statement(entry.body))
            })
    }
}
