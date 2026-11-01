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
                (self.scalar(ty) || self.class(ty)) && self.expression_type_of(value_id) == Some(ty)
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

    /// Root `replace(&local, value)` and `swap(&left, &right)` on local `var` bindings.
    pub(super) fn ownership_primitive(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(primitive) = self.inputs.typed.ownership_primitive(id) else {
            return false;
        };
        let Some(call) = self.inputs.typed.call(id) else {
            return false;
        };
        let Some(plan) = self.inputs.owned.ownership_primitive(id) else {
            return false;
        };
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Expression::Call { arguments, .. } = node.payload() else {
            return false;
        };
        if primitive.expression() != id
            || plan.descriptor() != &primitive
            || arguments.len() != 2
            || call.arguments().len() != 2
            || call.receiver().is_some()
        {
            return false;
        }
        let call_span = node.span();
        if self
            .inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(_, expr_node)| {
                let span = expr_node.span();
                call_span.start() <= span.start()
                    && span.end() <= call_span.end()
                    && matches!(
                        expr_node.payload(),
                        Expression::Return { .. }
                            | Expression::Break { .. }
                            | Expression::Continue { .. }
                    )
            })
        {
            return false;
        }
        let value_ty = self.c(primitive.value_type());
        self.note_nullable(value_ty);
        if !self.storage_type(value_ty) {
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
        let mut roots = Vec::new();
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
        if plan.places().len() != roots.len()
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
        self.migrated = true;
        true
    }
}
