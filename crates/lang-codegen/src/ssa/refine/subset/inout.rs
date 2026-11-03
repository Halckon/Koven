//! Subset rules for `Inout` (SPEC-0358): scalar `Inout` parameters read and replaced in
//! the callee, and `&local` arguments at the call site. They mirror `build/inout.rs`.
use lang_frontend::{
    ast::ExpressionId, name_resolution::UnitSymbolId, ownership_checking::LoanKind,
    parser::Expression, type_checking::ParameterMode,
};

use super::Checker;
use crate::ssa::refine::RefineBody;

impl Checker<'_, '_> {
    /// The mode of a parameter of this function (or a thunk capture); `None` for locals.
    pub(super) fn parameter_mode(&self, symbol: UnitSymbolId) -> Option<ParameterMode> {
        if self.for_borrowed.contains(&symbol) {
            return Some(ParameterMode::Borrow);
        }
        if let RefineBody::Lambda(thunk) = &self.inputs.body
            && thunk
                .captures
                .iter()
                .any(|capture| capture.symbol == symbol)
        {
            return Some(ParameterMode::Borrow);
        }
        let index = self
            .inputs
            .parameters
            .iter()
            .position(|parameter| *parameter == symbol)?;
        self.inputs.modes.get(index).copied()
    }

    /// `&x` for an `Inout` parameter: a known scalar or plain-class local, or an `Inout`
    /// parameter passed on, whose exclusive loan the ownership facts publish.
    pub(super) fn inout_argument(&mut self, call: ExpressionId, value: ExpressionId) -> bool {
        self.migrated = true;
        let Ok(node) = self.inputs.parsed.ast().expressions().get(value) else {
            return false;
        };
        let Expression::Name = node.payload() else {
            return false;
        };
        let Some(symbol) = self.reference(value) else {
            return false;
        };
        let (call, value_id) = (self.id(call), self.id(value));
        let typed = self.inputs.typed;
        self.known.contains(&symbol)
            && matches!(
                self.parameter_mode(symbol),
                None | Some(ParameterMode::Inout)
            )
            && typed.non_null_use(value_id).is_none()
            && self.symbol_type(symbol).is_some_and(|ty| {
                ((self.storage_type(ty)
                    && self.copyability_of(ty)
                        == lang_frontend::type_checking::Copyability::Copyable)
                    || self.class(ty))
                    && self.expression_type_of(value_id) == Some(ty)
            })
            && self.inputs.owned.loans().iter().any(|loan| {
                loan.call() == call
                    && loan.argument() == value_id
                    && loan.kind() == LoanKind::Exclusive
            })
    }

    pub(super) fn direct_place_symbol(&self, mut expression: ExpressionId) -> Option<UnitSymbolId> {
        loop {
            let node = self
                .inputs
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .ok()?;
            match node.payload() {
                Expression::Name => return self.reference(expression),
                Expression::Group { expression: inner } => expression = *inner,
                _ => return None,
            }
        }
    }

    /// Root `replace(&local, value)`, direct field `replace(&holder.field, value)`, and
    /// `swap(&left, &right)` on local bindings.
    pub(super) fn ownership_primitive(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(primitive) = self.inputs.typed.ownership_primitive(id) else {
            return false;
        };
        let Some(call) = self.inputs.typed.call(id) else {
            return false;
        };
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Expression::Call { arguments, .. } = node.payload() else {
            return false;
        };
        if primitive.expression() != id
            || arguments.len() != 2
            || call.arguments().len() != 2
            || call.receiver().is_some()
        {
            return false;
        }
        let call_span = node.span();
        let value_ty = self.c(primitive.value_type());
        self.note_nullable(value_ty);
        if !(self.unit(value_ty) || self.storage_type(value_ty)) {
            return false;
        }
        let modes = match primitive.kind() {
            lang_frontend::type_checking::OwnershipPrimitiveKind::Replace => {
                [ParameterMode::Inout, ParameterMode::Value]
            }
            lang_frontend::type_checking::OwnershipPrimitiveKind::Swap => {
                [ParameterMode::Inout, ParameterMode::Inout]
            }
        };
        for (index, argument) in arguments.iter().enumerate() {
            let Some(mapping) = call
                .arguments()
                .iter()
                .find(|mapping| mapping.argument_index() == index)
            else {
                return false;
            };
            let arg_id = self.id(argument.value);
            if primitive.operands()[index] != arg_id
                || mapping.parameter_index() != index
                || mapping.mode() != modes[index]
                || mapping.parameter_type() != primitive.value_type()
            {
                return false;
            }
        }
        let projected = self.ungrouped(arguments[0].value);
        let projection = self.inputs.typed.aggregate_projection(self.id(projected));
        if primitive.kind() == lang_frontend::type_checking::OwnershipPrimitiveKind::Replace
            && let Some(projection) = projection
            && projection.kind() == lang_frontend::type_checking::UnitAggregateProjectionKind::Field
        {
            if matches!(
                self.kind(value_ty),
                Some(lang_frontend::type_checking::UnitTypeKind::Nullable(inner))
                    if self.rc_payload(*inner).is_some()
            ) {
                return false;
            }
            let lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(receiver) =
                projection.receiver()
            else {
                return false;
            };
            let Some(symbol) = self
                .direct_place_symbol(receiver.expression())
                .filter(|_| receiver.source_unit() == self.inputs.source_unit)
            else {
                return false;
            };
            let Some(owner_ty) = self.symbol_type(symbol) else {
                return false;
            };
            if self.c(projection.ty()) != value_ty
                || !self.known.contains(&symbol)
                || self.callables.contains(&symbol)
                || self.parameter_mode(symbol).is_some()
                || !self.class(owner_ty)
                || !self
                    .class_signature(owner_ty)
                    .is_some_and(|(nominal, args)| {
                        args.is_empty() && nominal.type_parameters().is_empty()
                    })
                || !self.class_field(owner_ty, projection.field())
            {
                return false;
            }
            let arg0_id = self.id(arguments[0].value);
            let mut loans = self
                .inputs
                .owned
                .loans()
                .iter()
                .filter(|loan| loan.call() == id && loan.argument() == arg0_id);
            let Some(fact) = loans.next() else {
                return false;
            };
            if loans.next().is_some()
                || fact.kind() != LoanKind::Exclusive
                || fact.end_span() != call_span
                || !matches!(
                    fact.target(),
                    lang_frontend::ownership_checking::UnitLoanTarget::Place(place)
                        if place.root() == symbol
                            && place.fields() == [projection.field()]
                            && place.element().is_none()
                )
            {
                return false;
            }
            if let Some(field_plan) = self.inputs.owned.field_replacement(id) {
                if field_plan.descriptor() != &primitive
                    || field_plan.owner_type() != owner_ty
                    || field_plan.place().root() != symbol
                    || field_plan.place().fields() != [projection.field()]
                    || field_plan.place().element().is_some()
                {
                    return false;
                }
                let Some(delivery) = self
                    .inputs
                    .owned
                    .value_deliveries()
                    .iter()
                    .find(|delivery| {
                        delivery.call() == id && delivery.argument() == primitive.operands()[1]
                    })
                else {
                    return false;
                };
                let expected = match delivery.kind() {
                    lang_frontend::ownership_checking::UnitValueDeliveryKind::Copy => {
                        lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Copy
                    }
                    lang_frontend::ownership_checking::UnitValueDeliveryKind::Move => {
                        lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Move
                    }
                    lang_frontend::ownership_checking::UnitValueDeliveryKind::Temporary => {
                        lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Temporary
                    }
                };
                if field_plan.new_value_transfer() != expected {
                    return false;
                }
            } else if self.inputs.owned.value_deliveries().iter().any(|delivery| {
                delivery.call() == id && delivery.argument() == primitive.operands()[1]
            }) {
                return false;
            }
            if self.field_replace_rhs_conflicts(arguments[1].value, symbol, projection.field())
                || !self.expression(arguments[1].value)
            {
                return false;
            }
            self.migrated = true;
            return true;
        }
        let diverges = arguments.iter().any(|arg| {
            self.expression_type(arg.value)
                .is_some_and(|ty| self.nothing(ty))
        });
        let mut roots = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            let arg_id = self.id(argument.value);
            if modes[index] == ParameterMode::Inout {
                let Some(symbol) = self.direct_place_symbol(argument.value) else {
                    return false;
                };
                if !self.known.contains(&symbol)
                    || self.callables.contains(&symbol)
                    || self.parameter_mode(symbol).is_some()
                    || self.inputs.typed.non_null_use(arg_id).is_some()
                    || self.symbol_type(symbol) != Some(value_ty)
                    || self.expression_type_of(arg_id) != Some(value_ty)
                {
                    return false;
                }
                let mut loans = self
                    .inputs
                    .owned
                    .loans()
                    .iter()
                    .filter(|loan| loan.call() == id && loan.argument() == arg_id);
                let Some(fact) = loans.next() else {
                    return false;
                };
                if loans.next().is_some()
                    || fact.kind() != LoanKind::Exclusive
                    || fact.end_span() != call_span
                    || !matches!(
                        fact.target(),
                        lang_frontend::ownership_checking::UnitLoanTarget::Place(place)
                            if place.is_root() && place.root() == symbol
                    )
                {
                    return false;
                }
                roots.push(symbol);
            } else if !self.expression(argument.value) {
                return false;
            }
        }
        if !diverges {
            let Some(plan) = self.inputs.owned.ownership_primitive(id) else {
                return false;
            };
            if plan.descriptor() != &primitive
                || plan.places().len() != roots.len()
                || plan
                    .places()
                    .iter()
                    .zip(&roots)
                    .any(|(place, symbol)| !place.is_root() || place.root() != *symbol)
            {
                return false;
            }
            let expected_transfer = match primitive.kind() {
                lang_frontend::type_checking::OwnershipPrimitiveKind::Swap => None,
                lang_frontend::type_checking::OwnershipPrimitiveKind::Replace => {
                    let Some(delivery) =
                        self.inputs
                            .owned
                            .value_deliveries()
                            .iter()
                            .find(|delivery| {
                                delivery.call() == id
                                    && delivery.argument() == primitive.operands()[1]
                            })
                    else {
                        return false;
                    };
                    Some(match delivery.kind() {
                        lang_frontend::ownership_checking::UnitValueDeliveryKind::Copy => {
                            lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Copy
                        }
                        lang_frontend::ownership_checking::UnitValueDeliveryKind::Move => {
                            lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Move
                        }
                        lang_frontend::ownership_checking::UnitValueDeliveryKind::Temporary => {
                            lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Temporary
                        }
                    })
                }
            };
            if plan.new_value_transfer() != expected_transfer {
                return false;
            }
        }
        self.migrated = true;
        true
    }

    fn field_replace_rhs_conflicts(
        &self,
        rhs: ExpressionId,
        owner_symbol: UnitSymbolId,
        field: UnitSymbolId,
    ) -> bool {
        let Ok(rhs_node) = self.inputs.parsed.ast().expressions().get(rhs) else {
            return true;
        };
        let rhs_span = rhs_node.span();
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(expr_id, node)| {
                let span = node.span();
                rhs_span.source_id() == span.source_id()
                    && rhs_span.start() <= span.start()
                    && span.end() <= rhs_span.end()
                    && matches!(node.payload(), Expression::Name)
                    && self.reference(expr_id) == Some(owner_symbol)
                    && !self.is_disjoint_nested_field_replace_receiver(
                        rhs_span,
                        expr_id,
                        owner_symbol,
                        field,
                    )
                    && !self.is_disjoint_sibling_field_borrow(rhs_span, expr_id, field)
            })
    }

    fn is_disjoint_sibling_field_borrow(
        &self,
        rhs_span: lang_frontend::source::Span,
        name_expr: ExpressionId,
        outer_field: UnitSymbolId,
    ) -> bool {
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(call_expr, call_node)| {
                let cspan = call_node.span();
                if rhs_span.source_id() != cspan.source_id()
                    || cspan.start() < rhs_span.start()
                    || cspan.end() > rhs_span.end()
                {
                    return false;
                }
                let Expression::Call { arguments, .. } = call_node.payload() else {
                    return false;
                };
                let call_id = self.id(call_expr);
                arguments.iter().any(|arg| {
                    let projected = self.ungrouped(arg.value);
                    let Some(projection) =
                        self.inputs.typed.aggregate_projection(self.id(projected))
                    else {
                        return false;
                    };
                    let lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(
                        receiver,
                    ) = projection.receiver()
                    else {
                        return false;
                    };
                    if receiver.source_unit() != self.inputs.source_unit
                        || self.ungrouped(receiver.expression()) != name_expr
                        || projection.field() == outer_field
                    {
                        return false;
                    }
                    let arg_id = self.id(arg.value);
                    self.inputs.owned.loans().iter().any(|loan| {
                        loan.call() == call_id
                            && (loan.argument() == arg_id || loan.argument() == self.id(projected))
                            && loan.kind() == lang_frontend::ownership_checking::LoanKind::Shared
                    })
                })
            })
    }

    fn is_disjoint_nested_field_replace_receiver(
        &self,
        rhs_span: lang_frontend::source::Span,
        name_expr: ExpressionId,
        owner_symbol: UnitSymbolId,
        outer_field: UnitSymbolId,
    ) -> bool {
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(call_expr, call_node)| {
                let cspan = call_node.span();
                if rhs_span.source_id() != cspan.source_id()
                    || cspan.start() < rhs_span.start()
                    || cspan.end() > rhs_span.end()
                {
                    return false;
                }
                let Expression::Call { arguments, .. } = call_node.payload() else {
                    return false;
                };
                let Some(arg0) = arguments.first() else {
                    return false;
                };
                let call_id = self.id(call_expr);
                let Some(plan) = self.inputs.owned.field_replacement(call_id) else {
                    return false;
                };
                let projected = self.ungrouped(arg0.value);
                let Some(projection) = self.inputs.typed.aggregate_projection(self.id(projected))
                else {
                    return false;
                };
                let lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(
                    receiver,
                ) = projection.receiver()
                else {
                    return false;
                };
                receiver.source_unit() == self.inputs.source_unit
                    && self.ungrouped(receiver.expression()) == name_expr
                    && plan.place().root() == owner_symbol
                    && projection.field() != outer_field
            })
    }
}
