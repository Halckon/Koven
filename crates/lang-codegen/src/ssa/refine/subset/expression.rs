//! Expression-level subset admission rules for `ssa::refine`.
use super::*;
use lang_frontend::{
    ownership_checking::{LoanKind, UnitDropFact, UnitLoanFact, UnitLoanTarget},
    type_checking::{ExpressionCategory, UnitDisplayTarget, UnitInterpolationPart},
};

impl Checker<'_, '_> {
    /// A return in a concat's right operand can abandon its already evaluated
    /// left temporary before the enclosing call can borrow the concatenated value.
    pub(super) fn pending_string_concat_operand(
        &self,
        owner: UnitExpressionId,
        transfer: UnitExpressionId,
        drop: &UnitDropFact,
        loan: &UnitLoanFact,
    ) -> bool {
        let argument = loan.argument();
        let typed = self.inputs.typed;
        if self.inputs.borrow_return.is_some()
            || self.inputs.carrier_return.is_some()
            || transfer.source_unit() != argument.source_unit()
            || loan.kind() != LoanKind::Shared
            || loan.target() != &UnitLoanTarget::Temporary(argument)
            || drop.is_conditional_temporary()
            || typed.expression_category(owner) != Some(ExpressionCategory::Temporary)
            || typed.expression_falls_through(owner) != Some(true)
        {
            return false;
        }
        let nodes = self.inputs.parsed.ast().expressions();
        let (Ok(origin), Ok(exit)) = (
            nodes.get(owner.expression()),
            nodes.get(transfer.expression()),
        ) else {
            return false;
        };
        if origin.span() != drop.value_origin()
            || !matches!(exit.payload(), Expression::Return { .. })
        {
            return false;
        }
        let contains_exit = |id| {
            nodes.get(id).is_ok_and(|node| {
                let span = node.span();
                span.start() <= exit.span().start() && exit.span().end() <= span.end()
            })
        };
        let string = |id| self.expression_type(id).is_some_and(|ty| self.string(ty));
        let mut current = argument.expression();
        loop {
            current = self.ungrouped(current);
            let Ok(node) = nodes.get(current) else {
                return false;
            };
            let Expression::Binary {
                left,
                operator: BinaryOperator::Add,
                right,
                ..
            } = *node.payload()
            else {
                return false;
            };
            if !string(current) || !string(left) || !string(right) {
                return false;
            }
            if contains_exit(right) {
                if self.ungrouped(left) == owner.expression() {
                    return true;
                }
                current = right;
            } else if contains_exit(left) {
                current = left;
            } else {
                return false;
            }
        }
    }

    pub(super) fn expression(&mut self, expression: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Some(ty) = self.expression_type(expression) else {
            return false;
        };
        if let Expression::When {
            subject: Some(subject),
            ..
        } = *node.payload()
            && self.inputs.typed.expression_falls_through(self.id(subject)) == Some(false)
        {
            self.migrated = true;
            return self.expression(subject);
        }
        let id = self.id(expression);
        if let Some(descriptor) = self.inputs.abandoned_map_require_value(id) {
            return self.abandoned_map_require_value(descriptor);
        }
        if matches!(node.payload(), Expression::Call { .. })
            && let Some(receiver) =
                crate::ssa::lowering_support::non_completing_call_receiver(self.inputs.typed, id)
        {
            self.migrated = true;
            return receiver.source_unit() == self.inputs.source_unit
                && self.expression(receiver.expression());
        }
        if let Some(descriptor) = self.inputs.typed.call(id)
            && let receiver = descriptor
                .receiver()
                .and_then(|r| self.inputs.concrete(r.ty()))
            && let Some((operand, builtin)) = crate::ssa::lowering_support::builtin_display_call(
                self.inputs.typed,
                descriptor,
                receiver,
            )
        {
            if operand.source_unit() != self.inputs.source_unit
                || crate::ssa::lowering_support::is_float_builtin(builtin)
            {
                return false;
            }
            let operand = operand.expression();
            let ok = if builtin == BuiltinType::String {
                receiver.is_some_and(|receiver| {
                    self.string_clone_receiver(expression, operand, receiver)
                })
            } else {
                self.expression(operand)
            };
            self.migrated |= ok;
            return ok;
        }
        // Jumps and abort calls are typed `Nothing`. An if or group can carry that
        // divergence; the inner expression still has to pass its own admission.
        let returns = (matches!(
            node.payload(),
            Expression::Return { .. }
                | Expression::Break { .. }
                | Expression::Continue { .. }
                | Expression::NonNullAssert { .. }
                | Expression::If { .. }
                | Expression::Group { .. }
        ) || matches!(node.payload(), Expression::When { .. })
            && self.inputs.typed.expression_falls_through(id) == Some(false)
            || self.inputs.typed.call(id).is_some_and(|desc| {
                desc.aborts()
                    || self.named_nothing_call(desc)
                    || desc.target() == UnitCallTarget::FunctionValue
            }))
            && matches!(
                self.kind(ty),
                Some(UnitTypeKind::Builtin(BuiltinType::Nothing))
            );
        if let Expression::Lambda { .. } = node.payload() {
            return self.returned_lambda(expression);
        }
        if self.inputs.typed.constants().is_some_and(|facts| {
            facts
                .uses()
                .binary_search_by_key(&id, |use_| use_.expression())
                .is_ok()
        }) {
            return self.storage_type(ty) && self.constant_materialization(expression);
        }
        if self.inputs.typed.ownership_primitive(id).is_some() {
            return (self.unit(ty) || self.storage_type(ty) || self.nothing(ty))
                && self.ownership_primitive(expression);
        }
        if self.has_container_fact(expression) {
            return (self.unit(ty) || self.storage_type(ty))
                && self.container_operation(expression);
        }
        // `Rc`, integer, integer-range, and String operations carry their own typed facts.
        if self.inputs.typed.rc_operation(id).is_some() {
            return self.storage_type(ty) && self.rc_operation(expression);
        }
        if self.inputs.typed.integer_operation(id).is_some() {
            return self.storage_type(ty) && self.integer_operation(expression);
        }
        if self.inputs.typed.integer_range_operation(id).is_some() {
            return self.storage_type(ty) && self.integer_range_operation(expression);
        }
        if self.inputs.typed.string_operation(id).is_some() {
            return self.storage_type(ty) && self.string_operation(expression);
        }
        if self.inputs.typed.range_size(id).is_some() {
            return self.storage_type(ty) && self.range_size(expression);
        }
        if let Some(descriptor) = self.inputs.typed.nullable_when(id) {
            let accepted = self.nullable_when(expression);
            self.has_refine_only_when |= accepted
                && (!matches!(
                    descriptor.source_category(),
                    lang_frontend::type_checking::NullableWhenSubjectCategory::OwnedRoot
                        | lang_frontend::type_checking::NullableWhenSubjectCategory::Temporary
                ) || !crate::ssa::lowering_support::ordinary_nullable_when_heap_types(
                    self.inputs.typed,
                    self.c(descriptor.nullable_type()),
                    self.c(descriptor.inner_type()),
                ));
            return accepted;
        }
        if self.view_or_function(ty)
            && matches!(
                node.payload(),
                Expression::If { .. } | Expression::When { .. }
            )
        {
            return self.callable_value_expr(expression);
        }
        if !self.unspecialized(expression)
            || !(returns
                || self.unit(ty)
                || self.storage_type(ty)
                || self.callable_value(expression))
        {
            return false;
        }
        self.note_nullable(ty);
        if let Some(descriptor) = self.inputs.typed.construction(id) {
            match descriptor.target() {
                lang_frontend::type_checking::UnitConstructionTarget::EnumCase(_) => {
                    return self.enum_construction(expression);
                }
                lang_frontend::type_checking::UnitConstructionTarget::IntrinsicBox => {
                    return self.box_construction(expression);
                }
                lang_frontend::type_checking::UnitConstructionTarget::IntrinsicRc => {
                    return self.rc_construction(expression);
                }
                _ if self.value_class(self.c(descriptor.result_type()))
                    || self.move_value_class(self.c(descriptor.result_type())) =>
                {
                    return self.value_construction(expression);
                }
                _ => {}
            }
            return self.construction(expression);
        }
        if self.inputs.typed.aggregate_projection(id).is_some() {
            return self.projection(expression)
                || self.enum_projection(expression)
                || self.value_projection(expression);
        }
        if self.inputs.typed.non_null_assertion(id).is_some() {
            return self.non_null_assertion(expression);
        }
        match node.payload().clone() {
            Expression::Group { expression } => self.expression(expression),
            Expression::Literal(LiteralKind::Integer(_)) => self.integer_type(ty),
            Expression::Literal(LiteralKind::Char) => {
                self.kind(ty) == Some(&UnitTypeKind::Builtin(BuiltinType::Char))
            }
            Expression::Literal(LiteralKind::Float(_)) => matches!(
                self.kind(ty),
                Some(UnitTypeKind::Builtin(
                    BuiltinType::Float | BuiltinType::Double
                ))
            ),
            Expression::Literal(LiteralKind::Boolean(_) | LiteralKind::Null) => true,
            Expression::String { .. } => {
                if let Some(parts) = self.inputs.typed.interpolation_plan(id) {
                    let mut admitted = true;
                    for part in parts {
                        match *part {
                            UnitInterpolationPart::Text(part_span) => {
                                admitted = part_span.source_id() == node.span().source_id();
                            }
                            UnitInterpolationPart::Format { operand, target } => {
                                if self.inputs.owned.interpolation_operand_completes(operand)
                                    == Some(false)
                                {
                                    admitted = operand.source_unit() == self.inputs.source_unit
                                        && self.expression(operand.expression());
                                    break;
                                }
                                admitted = self.interpolation_operand(operand, target);
                            }
                            UnitInterpolationPart::Exit(operand) => {
                                admitted = operand.source_unit() == self.inputs.source_unit
                                    && self.expression(operand.expression());
                                break;
                            }
                        }
                        if !admitted {
                            break;
                        }
                    }
                    self.migrated |= admitted;
                    return admitted;
                }
                self.inputs
                    .sources
                    .source_text(self.inputs.parsed.source_id())
                    .ok()
                    .and_then(|text| {
                        string_literal::decode_plain(self.inputs.parsed, text, expression).ok()
                    })
                    .flatten()
                    .is_some()
            }
            Expression::Name => self.name(expression),
            Expression::This => self.this_expression(expression),
            Expression::TypeTest {
                expression: operand,
                type_ref,
                ..
            } => self.type_test_expression(operand, type_ref),
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => self.expression(operand),
            Expression::Prefix {
                operator: operator @ (PrefixOperator::Plus | PrefixOperator::Minus),
                operand,
                ..
            } => {
                (self.integer_type(ty)
                    || matches!(self.kind(ty), Some(UnitTypeKind::Builtin(b)) if crate::ssa::lowering_support::is_float_builtin(*b)))
                    && self.expression_type(operand) == Some(ty)
                    && self.prefix_integer(operator, operand)
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => self.binary(expression, left, operator, right),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let saved = self.view_symbols.clone();
                let Some([when_false, when_true]) = self.if_condition_proofs(condition) else {
                    return false;
                };
                self.view_symbols.extend(when_true);
                let (reaches_then, reaches_else) = self.condition_outcomes(condition);
                let then_ok = !reaches_then || self.statement(then_branch);
                let then_views = self.view_symbols.clone();
                self.view_symbols = saved.clone();
                self.note_else_view(condition);
                self.view_symbols.extend(when_false);
                let else_ok =
                    !reaches_else || else_branch.is_none_or(|branch| self.statement(branch));
                let else_views = self.view_symbols.clone();
                let then_continues = reaches_then && self.statement_may_continue(then_branch);
                let else_continues = reaches_else
                    && else_branch.is_none_or(|branch| self.statement_may_continue(branch));
                self.view_symbols = saved
                    .into_iter()
                    .filter(|symbol| {
                        (!then_continues || then_views.contains(symbol))
                            && (!else_continues || else_views.contains(symbol))
                    })
                    .collect();
                self.note_continuing_if_views(
                    node.span(),
                    [(then_continues, &then_views), (else_continues, &else_views)],
                );
                then_ok
                    && else_ok
                    && (self.unit(ty) || else_branch.is_some())
                    && (!returns || else_branch.is_some())
            }
            Expression::When {
                subject: None,
                entries,
                ..
            } => {
                let accepted = self.subjectless_when(ty, &entries, node.span());
                self.has_refine_only_when |= accepted;
                accepted
            }
            Expression::When {
                subject: Some(subject),
                entries,
                ..
            } => {
                let accepted = self.subject_when(expression, ty, subject, &entries);
                self.has_refine_only_when |= accepted
                    && (self.expression_type(subject).and_then(|ty| self.kind(ty))
                        != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                        || matches!(
                            crate::ssa::lowering_support::boolean_literal_when_arms(
                                self.inputs.parsed,
                                &entries,
                            ),
                            Ok(None)
                        ));
                accepted
            }
            Expression::Call { arguments, .. } => {
                let values: Vec<_> = arguments.iter().map(|argument| argument.value).collect();
                let function_value = self
                    .inputs
                    .typed
                    .call(self.id(expression))
                    .is_some_and(|descriptor| descriptor.target() == UnitCallTarget::FunctionValue);
                let accepted = self.call(expression, &values)
                    && (!function_value || self.function_callee(expression, &values))
                    && self.arguments(expression, &values);
                if accepted && let Some(proof) = self.whole_nominal_exit(expression, &values) {
                    self.whole_nominal_exit = Some(proof);
                }
                accepted
            }
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } if self.is_field_target(target) => self.field_assignment(expression, target, value),
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } if self.is_element_target(target) => {
                self.element_assignment(expression, target, value)
            }
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                ..
            } if self.is_map_put_target(target) => self.map_put_assignment(expression, target),
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } => self.assignment(expression, target, value),
            Expression::Assignment {
                target,
                operator:
                    operator @ (AssignmentOperator::AddAssign
                    | AssignmentOperator::SubtractAssign
                    | AssignmentOperator::MultiplyAssign
                    | AssignmentOperator::DivideAssign
                    | AssignmentOperator::RemainderAssign),
                value,
                ..
            } => {
                if self.is_element_target(target) {
                    self.element_compound_assignment(expression, target, operator, value)
                } else {
                    self.compound_assignment(target, operator, value)
                }
            }
            Expression::Return { value, .. } => {
                if self.inputs.borrow_return.is_some() {
                    value.is_some_and(|value| self.borrow_return_body(expression, value))
                } else if self.inputs.carrier_return.is_some() {
                    value.is_some_and(|value| self.carrier_return_body(expression, value))
                } else {
                    value.is_none_or(|value| {
                        self.expression(value) || self.returned_nullable_owner(value)
                    })
                }
            }
            Expression::Break { .. } | Expression::Continue { .. } => self.loops > 0,
            _ => false,
        }
    }

    pub(super) fn returned_nullable_owner(&self, expression: ExpressionId) -> bool {
        let root = self.ungrouped(expression);
        let Some(fact) = self.inputs.typed.non_null_use(self.id(root)) else {
            return false;
        };
        let symbol = fact.symbol();
        let narrowed = self.c(fact.narrowed_type());
        self.reference(root) == Some(symbol)
            && self.known.contains(&symbol)
            && !self.callables.contains(&symbol)
            && !matches!(
                self.parameter_mode(symbol),
                Some(ParameterMode::Borrow | ParameterMode::Inout)
            )
            && self.view_symbols.contains(&symbol)
            && self.copyability_of(narrowed) == Copyability::MoveOnly
            && (self.c(fact.declared_type()) == self.inputs.return_type
                || (narrowed == self.inputs.return_type
                    && self.string(narrowed)
                    && !self.is_map_remove_symbol(symbol)))
    }

    fn interpolation_operand(
        &mut self,
        operand: UnitExpressionId,
        target: UnitDisplayTarget,
    ) -> bool {
        if operand.source_unit() != self.inputs.source_unit {
            return false;
        }
        let expression = operand.expression();
        let Some((ty, enum_case)) = self.inputs.display_receiver_type(operand) else {
            return false;
        };
        let admitted = match self.inputs.builtin_display(target, ty) {
            Some(BuiltinType::String) => {
                self.borrowed_string_name(expression)
                    || self
                        .string_proof_operand(expression)
                        .unwrap_or_else(|| self.expression(expression))
            }
            Some(builtin)
                if matches!(builtin, BuiltinType::Boolean | BuiltinType::Char)
                    || super::integer(builtin) =>
            {
                self.expression(expression)
            }
            _ if matches!(target, UnitDisplayTarget::Requirement(_)) => {
                self.inputs
                    .source_plan
                    .display_site(self.inputs.source_token, operand)
                    .is_some()
                    && self.nominal_display_operand(expression, ty, enum_case)
            }
            _ => false,
        };
        if admitted {
            self.interpolation_operands.insert(operand);
            let root = self.id(self.ungrouped(expression));
            if self
                .inputs
                .constant_owned
                .is_some_and(|owned| owned.materialization_at(root).is_some())
            {
                self.interpolation_operands.insert(root);
            }
        }
        admitted
    }

    fn nominal_display_operand(
        &mut self,
        expression: ExpressionId,
        ty: UnitTypeId,
        enum_case: bool,
    ) -> bool {
        if self
            .inputs
            .typed
            .aggregate_projection(self.id(self.ungrouped(expression)))
            .is_some_and(|projection| {
                projection.kind()
                    == lang_frontend::type_checking::UnitAggregateProjectionKind::Field
            })
        {
            return self.shared_field_read_type(expression, None) == Some(ty);
        }
        if enum_case {
            let name = self.ungrouped(expression);
            return self.borrowed_move_only_name(name, ty)
                || (self
                    .reference(name)
                    .and_then(|symbol| self.symbol_type(symbol))
                    == Some(ty)
                    && self.name(name));
        }
        self.borrowed_move_only_name(expression, ty)
            || self.interpolation_proof_operand(expression, ty)
            || self.expression(expression)
    }

    fn interpolation_proof_operand(&self, expression: ExpressionId, ty: UnitTypeId) -> bool {
        let root = self.ungrouped(expression);
        let id = self.id(root);
        self.inputs.typed.nullable_when_use(id).is_none()
            && self.inputs.typed.non_null_use(id).is_some_and(|fact| {
                self.c(fact.narrowed_type()) == ty
                    && self.reference(root) == Some(fact.symbol())
                    && self.known.contains(&fact.symbol())
                    && self.view_symbols.contains(&fact.symbol())
            })
    }

    fn name(&mut self, expression: ExpressionId) -> bool {
        if self.object_name(expression) || self.known_function(expression) {
            self.migrated = true;
            return true;
        }
        let Some(symbol) = self.reference(expression) else {
            return false;
        };
        if !self.known.contains(&symbol) {
            return false;
        };
        if self.callables.contains(&symbol) {
            return self
                .inputs
                .typed
                .non_null_use(self.id(expression))
                .is_none();
        }
        // A loan parameter is read by copy, so only a Copyable one is a value here.
        if matches!(
            self.parameter_mode(symbol),
            Some(ParameterMode::Borrow | ParameterMode::Inout)
        ) && self
            .symbol_type(symbol)
            .is_none_or(|ty| self.copyability_of(ty) != Copyability::Copyable)
        {
            return false;
        }
        match self.inputs.typed.non_null_use(self.id(expression)) {
            // Only Copyable inners are narrowed by value in the migrated subset.
            Some(fact) => {
                fact.symbol() == symbol
                    && self.copyability_of(self.c(fact.narrowed_type())) == Copyability::Copyable
            }
            None => true,
        }
    }

    fn binary(
        &mut self,
        expression: ExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
    ) -> bool {
        if matches!(
            operator,
            BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
        ) || self
            .inputs
            .typed
            .null_comparison(self.id(expression))
            .is_some()
        {
            return self.condition(expression, false);
        }
        if self.nullable_equality(expression, left, operator, right) {
            return true;
        }
        let Some(operand) = self.expression_type(left) else {
            return false;
        };
        if self.nothing(operand)
            && matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
        {
            // The builder propagates the left exit before asking for the right value.
            return self.expression(left);
        }
        if matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
            && self
                .expression_type(right)
                .is_some_and(|right_ty| self.inputs.is_nullable_string_equality(operand, right_ty))
        {
            let admitted = self.string_equality_operand(expression, left)
                && self.string_equality_operand(expression, right);
            self.migrated |= admitted;
            return admitted;
        }
        let char_operand = self.kind(operand) == Some(&UnitTypeKind::Builtin(BuiltinType::Char));
        let float_operand = matches!(
            self.kind(operand),
            Some(UnitTypeKind::Builtin(
                BuiltinType::Float | BuiltinType::Double
            ))
        );
        let supported = match operator {
            BinaryOperator::Add if self.string(operand) => true,
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder => float_operand || self.integer_type(operand),
            BinaryOperator::BitwiseAnd
            | BinaryOperator::BitwiseOr
            | BinaryOperator::BitwiseXor
            | BinaryOperator::Shl
            | BinaryOperator::Shr
            | BinaryOperator::Ushr => self.integer_type(operand),
            BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => {
                char_operand || float_operand || self.integer_type(operand)
            }
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                char_operand || float_operand || self.scalar(operand) || self.string(operand)
            }
            _ => false,
        };
        let string = self.string(operand);
        supported
            && self.operand(expression, left, string)
            && self.operand(expression, right, string)
    }
}
