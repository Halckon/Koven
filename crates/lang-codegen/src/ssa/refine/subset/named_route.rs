//! SPEC-0341 bounded caller routing triggers: owned String argument calls, owned-class
//! receiver calls, and literal String temporary calls. The complete subset precheck remains mandatory.
use super::{Checker, RefineBody};
use lang_frontend::{
    ownership_checking::{
        LoanKind, OwnershipBindingKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
        UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    },
    parser::{Expression, Item, ParameterModeMarker, Statement, StringPart},
    type_checking::{
        BuiltinType, CallableResultSource, ExpressionCategory, ParameterMode,
        UnitCallReceiverOrigin, UnitCallTarget, UnitExpressionId, UnitTypeKind,
    },
};

impl Checker<'_, '_> {
    pub(super) fn bounded_named_call(&self) -> Option<UnitExpressionId> {
        let inputs = self.inputs;
        if inputs.receiver.is_some()
            || inputs.static_self.is_some()
            || inputs.borrow_return.is_some()
            || inputs.carrier_return.is_some()
            || !self.unit(inputs.return_type)
        {
            return None;
        }
        let Item::Function {
            parameters,
            type_parameters,
            extension_receiver: None,
            ..
        } = inputs
            .parsed
            .ast()
            .items()
            .get(inputs.function_item?)
            .ok()?
            .payload()
        else {
            return None;
        };
        if !type_parameters.is_empty() || !inputs.substitutions.is_empty() {
            return None;
        }
        let [parameter] = parameters.as_slice() else {
            return None;
        };
        if !matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_))) {
            return None;
        }
        let [symbol] = inputs.parameters.as_slice() else {
            return None;
        };
        if inputs.modes.as_slice() != [ParameterMode::Value] {
            return None;
        }
        let RefineBody::Block(body) = inputs.body else {
            return None;
        };
        let Statement::Block { elements } =
            inputs.parsed.ast().statements().get(body).ok()?.payload()
        else {
            return None;
        };
        let [statement] = elements.as_slice() else {
            return None;
        };
        let Statement::Expression { expression } = inputs
            .parsed
            .ast()
            .statements()
            .get(*statement)
            .ok()?
            .payload()
        else {
            return None;
        };
        let expressions = inputs.parsed.ast().expressions();
        let Expression::Call {
            callee, arguments, ..
        } = expressions.get(*expression).ok()?.payload()
        else {
            return None;
        };
        let [argument] = arguments.as_slice() else {
            return None;
        };
        let source = argument.value;
        if !matches!(expressions.get(*callee).ok()?.payload(), Expression::Name)
            || !matches!(expressions.get(source).ok()?.payload(), Expression::Name)
            || self.reference(source) != Some(*symbol)
        {
            return None;
        }
        let ty = self.symbol_type(*symbol)?;
        if !self.string(ty) {
            return None;
        }
        let binding = inputs
            .owned
            .bindings()
            .iter()
            .find(|b| b.symbol() == *symbol)?;
        if binding.kind() != OwnershipBindingKind::Owned {
            return None;
        }
        let id = self.id(*expression);
        let call = inputs.typed.call(id)?;
        let [argument] = call.arguments() else {
            return None;
        };
        if !matches!(call.target(), UnitCallTarget::Declaration(_))
            || call.receiver().is_some()
            || call.aborts()
            || call.result_source() != CallableResultSource::Owned
            || !call.instance().type_arguments().is_empty()
            || !matches!(
                self.kind(self.c(call.return_type())),
                Some(UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Unit))
            )
            || argument.mode() != ParameterMode::Borrow
            || argument.argument_index() != 0
            || argument.parameter_index() != 0
            || argument.category() != ExpressionCategory::Place
            || argument.crosses_thread()
            || self.c(argument.parameter_type()) != ty
            || inputs.typed.expression_type(self.id(source)) != Some(ty)
        {
            return None;
        }
        let site = inputs.source_plan.call_site(inputs.source_token, id)?;
        if site.dispatch_key().is_none() || !site.delegation().is_empty() {
            return None;
        }
        let mut facts = inputs
            .owned
            .loans()
            .iter()
            .filter(|f| f.call() == id && f.argument() == self.id(source));
        let fact = facts.next()?;
        let UnitLoanTarget::Place(place) = fact.target() else {
            return None;
        };
        if facts.next().is_some()
            || fact.kind() != LoanKind::Shared
            || !place.is_root()
            || place.root() != *symbol
        {
            return None;
        }
        let point = UnitDropPoint::CallReturn(id);
        (inputs
            .owned
            .direct_call_loan_ends()
            .iter()
            .any(|end| end.loan() == fact && end.point() == point)
            && inputs.owned.drops().iter().any(|drop| {
                drop.point() == point && drop.target() == UnitDropTarget::Named(*symbol)
            }))
        .then_some(id)
    }

    pub(super) fn bounded_receiver_route(&self) -> bool {
        self.bounded_receiver_candidate().unwrap_or(false)
    }

    fn bounded_receiver_candidate(&self) -> Option<bool> {
        let inputs = self.inputs;
        if inputs.receiver.is_some()
            || inputs.static_self.is_some()
            || inputs.borrow_return.is_some()
            || inputs.carrier_return.is_some()
            || !self.unit(inputs.return_type)
        {
            return None;
        }
        let Item::Function {
            parameters,
            type_parameters,
            extension_receiver: None,
            ..
        } = inputs
            .parsed
            .ast()
            .items()
            .get(inputs.function_item?)
            .ok()?
            .payload()
        else {
            return None;
        };
        if !type_parameters.is_empty() || !inputs.substitutions.is_empty() {
            return None;
        }
        let [parameter] = parameters.as_slice() else {
            return None;
        };
        if !matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_))) {
            return None;
        }
        let [symbol] = inputs.parameters.as_slice() else {
            return None;
        };
        if inputs.modes.as_slice() != [ParameterMode::Value] {
            return None;
        }
        let RefineBody::Block(body) = inputs.body else {
            return None;
        };
        let Statement::Block { elements } =
            inputs.parsed.ast().statements().get(body).ok()?.payload()
        else {
            return None;
        };
        let [statement] = elements.as_slice() else {
            return None;
        };
        let Statement::Expression { expression } = inputs
            .parsed
            .ast()
            .statements()
            .get(*statement)
            .ok()?
            .payload()
        else {
            return None;
        };
        let expressions = inputs.parsed.ast().expressions();
        let Expression::Call {
            callee, arguments, ..
        } = expressions.get(*expression).ok()?.payload()
        else {
            return None;
        };
        if !arguments.is_empty() {
            return None;
        }
        let Expression::Member {
            receiver: source,
            safe: false,
            ..
        } = expressions.get(*callee).ok()?.payload()
        else {
            return None;
        };
        if !matches!(expressions.get(*source).ok()?.payload(), Expression::Name)
            || self.reference(*source) != Some(*symbol)
        {
            return None;
        }
        let ty = self.symbol_type(*symbol)?;
        if !self.class(ty)
            || !matches!(self.kind(ty), Some(UnitTypeKind::Nominal { arguments, .. }) if arguments.is_empty())
            || inputs.typed.is_resource_type(ty) != Some(false)
        {
            return None;
        }
        let binding = inputs
            .owned
            .bindings()
            .iter()
            .find(|b| b.symbol() == *symbol)?;
        if binding.kind() != OwnershipBindingKind::Owned {
            return None;
        }
        let id = self.id(*expression);
        let call = inputs.typed.call(id)?;
        let receiver = call.receiver()?;
        if !call.arguments().is_empty()
            || call.aborts()
            || call.result_source() != CallableResultSource::Owned
            || !call.instance().type_arguments().is_empty()
            || !matches!(
                self.kind(self.c(call.return_type())),
                Some(UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Unit))
            )
            || receiver.mode() != ParameterMode::Borrow
            || self.c(receiver.ty()) != ty
            || receiver.origin() != UnitCallReceiverOrigin::Expression(self.id(*source))
        {
            return None;
        }
        let site = inputs.source_plan.call_site(inputs.source_token, id)?;
        if site.dispatch_key().is_none() || !site.delegation().is_empty() {
            return None;
        }
        let fact = inputs.owned.receiver_fact(id)?;
        let UnitReceiverOwnershipTarget::Place(place) = fact.target() else {
            return None;
        };
        if fact.kind() != UnitReceiverOwnershipKind::SharedLoan
            || fact.source() != receiver.origin()
            || fact.receiver_type() != receiver.ty()
            || fact.is_receiver_reservation()
            || fact.activation_point().is_some()
            || !place.is_root()
            || place.root() != *symbol
        {
            return None;
        }
        let point = UnitDropPoint::CallReturn(id);
        Some(
            inputs
                .owned
                .receiver_call_return_ends()
                .iter()
                .any(|end| end.receiver() == fact && end.point() == point)
                && inputs.owned.drops().iter().any(|drop| {
                    drop.point() == point && drop.target() == UnitDropTarget::Named(*symbol)
                }),
        )
    }

    pub(super) fn bounded_temporary_call(&self) -> Option<UnitExpressionId> {
        let inputs = self.inputs;
        if inputs.receiver.is_some()
            || inputs.static_self.is_some()
            || inputs.borrow_return.is_some()
            || inputs.carrier_return.is_some()
            || !self.unit(inputs.return_type)
        {
            return None;
        }
        let Item::Function {
            parameters,
            type_parameters,
            extension_receiver: None,
            ..
        } = inputs
            .parsed
            .ast()
            .items()
            .get(inputs.function_item?)
            .ok()?
            .payload()
        else {
            return None;
        };
        if !type_parameters.is_empty() || !inputs.substitutions.is_empty() {
            return None;
        }
        if !parameters.is_empty() || !inputs.parameters.is_empty() || !inputs.modes.is_empty() {
            return None;
        }
        let RefineBody::Block(body) = inputs.body else {
            return None;
        };
        let Statement::Block { elements } =
            inputs.parsed.ast().statements().get(body).ok()?.payload()
        else {
            return None;
        };
        let [statement] = elements.as_slice() else {
            return None;
        };
        let Statement::Expression { expression } = inputs
            .parsed
            .ast()
            .statements()
            .get(*statement)
            .ok()?
            .payload()
        else {
            return None;
        };
        let expressions = inputs.parsed.ast().expressions();
        let Expression::Call {
            callee, arguments, ..
        } = expressions.get(*expression).ok()?.payload()
        else {
            return None;
        };
        if arguments.is_empty()
            || !matches!(expressions.get(*callee).ok()?.payload(), Expression::Name)
        {
            return None;
        }
        let id = self.id(*expression);
        let call = inputs.typed.call(id)?;
        if call.arguments().len() != arguments.len()
            || !matches!(call.target(), UnitCallTarget::Declaration(_))
            || call.receiver().is_some()
            || call.aborts()
            || call.result_source() != CallableResultSource::Owned
            || !call.instance().type_arguments().is_empty()
            || !matches!(
                self.kind(self.c(call.return_type())),
                Some(UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Unit))
            )
        {
            return None;
        }
        // Parameter slots form a complete permutation; evaluation remains source ordered.
        let mut parameters = call
            .arguments()
            .iter()
            .map(|a| a.parameter_index())
            .collect::<Vec<_>>();
        parameters.sort_unstable();
        if parameters != (0..arguments.len()).collect::<Vec<_>>() {
            return None;
        }
        for (index, source) in arguments.iter().enumerate() {
            let source = source.value;
            let Expression::String { parts, .. } = expressions.get(source).ok()?.payload() else {
                return None;
            };
            let ty = inputs.typed.expression_type(self.id(source))?;
            let argument = call.arguments().get(index)?;
            if !parts.iter().all(|part| matches!(part, StringPart::Text(_)))
                || !self.string(ty)
                || argument.mode() != ParameterMode::Borrow
                || argument.argument_index() != index
                || argument.category() != ExpressionCategory::Temporary
                || argument.crosses_thread()
                || self.c(argument.parameter_type()) != ty
            {
                return None;
            }
            let mut facts = inputs
                .owned
                .loans()
                .iter()
                .filter(|f| f.call() == id && f.argument() == self.id(source));
            let fact = facts.next()?;
            if facts.next().is_some()
                || fact.kind() != LoanKind::Shared
                || fact.target() != &UnitLoanTarget::Temporary(self.id(source))
            {
                return None;
            }
            let point = UnitDropPoint::CallReturn(id);
            if !inputs
                .owned
                .direct_call_loan_ends()
                .iter()
                .any(|end| end.loan() == fact && end.point() == point)
                || !inputs.owned.drops().iter().any(|drop| {
                    drop.point() == point
                        && drop.target() == UnitDropTarget::Temporary(self.id(source))
                })
            {
                return None;
            }
        }
        let site = inputs.source_plan.call_site(inputs.source_token, id)?;
        if site.dispatch_key().is_none() || !site.delegation().is_empty() {
            return None;
        }
        Some(id)
    }
}

pub(in crate::ssa::refine) fn bounded_named_call(
    inputs: &super::RefineInputs<'_>,
) -> Option<UnitExpressionId> {
    super::query_checker(inputs).bounded_named_call()
}

pub(in crate::ssa::refine) fn bounded_temporary_call(
    inputs: &super::RefineInputs<'_>,
) -> Option<UnitExpressionId> {
    super::query_checker(inputs).bounded_temporary_call()
}

mod when {
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

    use super::super::Checker;

    impl Checker<'_, '_> {
        pub(in crate::ssa::refine::subset) fn rc_payload(
            &self,
            ty: UnitTypeId,
        ) -> Option<UnitTypeId> {
            self.rc_payload_seen(ty, &mut BTreeSet::new())
        }

        pub(in crate::ssa::refine::subset) fn rc_payload_seen(
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

        pub(in crate::ssa::refine::subset) fn rc_construction(
            &mut self,
            expression: ExpressionId,
        ) -> bool {
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

        pub(in crate::ssa::refine::subset) fn rc_operation_on_owner(
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
                && self.inputs.typed.expression_category(receiver)
                    == Some(ExpressionCategory::Place);
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
                                    && self.rc_payload(self.c(fact.narrowed_type()))
                                        == Some(payload_ty)
                            }
                        }
                });
            (shape && owner).then_some(descriptor)
        }

        pub(in crate::ssa::refine::subset) fn rc_operation(
            &mut self,
            expression: ExpressionId,
        ) -> bool {
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

        pub(in crate::ssa::refine::subset) fn rc_value_owner(
            &mut self,
            receiver: ExpressionId,
        ) -> Option<UnitTypeId> {
            self.migrated = true;
            let descriptor = self.rc_operation_on_owner(receiver)?;
            (descriptor.kind() == RcOperationKind::Value
                && self.class(self.c(descriptor.payload_type())))
            .then(|| self.c(descriptor.payload_type()))
        }

        pub(in crate::ssa::refine::subset) fn borrowed_rc_payload(
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
        pub(in crate::ssa::refine::subset) fn subject_when(
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
            ) && (closed_after.is_some()
                || self.boolean_two_arms(entries));
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
                let point = lang_frontend::ownership_checking::UnitDropPoint::AfterExpression(
                    self.id(operand),
                );
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

        pub(in crate::ssa::refine::subset) fn subjectless_when(
            &mut self,
            ty: UnitTypeId,
            entries: &[WhenEntry],
        ) -> bool {
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
}
