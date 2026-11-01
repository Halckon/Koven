//! Subset rules for integer intrinsics (`inv`, checked conversions), prefix `+`/`-`,
//! `IntRange`/`LongRange` value operations, and `String.clone()` (SPEC-0371).
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, LiteralKind, PrefixOperator},
    type_checking::{
        BuiltinType, Copyability, IntegerOperationKind, ParameterMode, StringOperationKind,
        UnitTypeId, UnitTypeKind,
    },
};

use super::Checker;

impl Checker<'_, '_> {
    pub(super) fn range_type(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.kind(ty),
            Some(UnitTypeKind::Builtin(
                BuiltinType::IntRange | BuiltinType::LongRange
            ))
        )
    }

    pub(super) fn prefix_integer(
        &mut self,
        operator: PrefixOperator,
        operand: ExpressionId,
    ) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(operand) else {
            return false;
        };
        if operator == PrefixOperator::Minus
            && let Expression::Literal(LiteralKind::Integer(kind)) = node.payload()
        {
            return self
                .inputs
                .sources
                .slice(node.span())
                .ok()
                .and_then(|text| {
                    lang_frontend::type_checking::integer_literal_magnitude(text, *kind)
                })
                .is_some_and(|magnitude| {
                    magnitude == (1u128 << 127)
                        || i128::try_from(magnitude)
                            .ok()
                            .and_then(i128::checked_neg)
                            .is_some()
                });
        }
        self.expression(operand)
    }

    pub(super) fn integer_operation(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.integer_operation(id) else {
            return false;
        };
        let receiver_ty = self.c(descriptor.receiver_type());
        let result_ty = self.c(descriptor.result_type());
        let kind_ok = match descriptor.kind() {
            IntegerOperationKind::Invert => receiver_ty == result_ty,
            IntegerOperationKind::CheckedConvert => true,
        };
        descriptor.expression() == id
            && descriptor.receiver().source_unit() == self.inputs.source_unit
            && self.integer_type(receiver_ty)
            && self.integer_type(result_ty)
            && kind_ok
            && self.expression_type_of(descriptor.receiver()) == Some(receiver_ty)
            && self.expression_type(expression) == Some(result_ty)
            && crate::ssa::integer::intrinsic_receiver_matches(
                self.inputs.parsed,
                expression,
                descriptor.receiver().expression(),
            )
            && self.expression(descriptor.receiver().expression())
    }

    pub(super) fn integer_range_operation(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.integer_range_operation(id).copied() else {
            return false;
        };
        let left_ty = self.c(descriptor.left_type());
        let right_ty = self.c(descriptor.right_type());
        let result_ty = self.c(descriptor.result_type());
        let types = [left_ty, right_ty, result_ty].map(|ty| match self.kind(ty) {
            Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        });
        descriptor.expression() == id
            && descriptor.left().source_unit() == self.inputs.source_unit
            && descriptor.right().source_unit() == self.inputs.source_unit
            && self.expression_type_of(descriptor.left()) == Some(left_ty)
            && self.expression_type_of(descriptor.right()) == Some(right_ty)
            && self.expression_type(expression) == Some(result_ty)
            && crate::ssa::integer_range::matches_fact(
                self.inputs.parsed,
                expression,
                descriptor.left().expression(),
                descriptor.right().expression(),
                descriptor.kind(),
                types,
            )
            && self.expression(descriptor.left().expression())
            && self.expression(descriptor.right().expression())
    }

    pub(super) fn string_operation(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.string_operation(id) else {
            return false;
        };
        let Some(effect) = self.inputs.owned.string_effect(id) else {
            return false;
        };
        if descriptor.kind() != StringOperationKind::Clone
            || descriptor.receiver_mode() != ParameterMode::Borrow
            || descriptor.result_mode() != ParameterMode::Value
            || effect.receiver() != descriptor.receiver()
            || effect.receiver_type() != descriptor.receiver_type()
            || effect.result_type() != descriptor.result_type()
            || effect.kind() != descriptor.kind()
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || !self.string(self.c(descriptor.receiver_type()))
            || !self.string(self.c(descriptor.result_type()))
        {
            return false;
        }
        let receiver = descriptor.receiver().expression();
        let receiver_id = self.id(receiver);
        if self.borrowed_string_name(receiver) {
            return true;
        }
        if let Some(fact) = self.inputs.typed.non_null_use(receiver_id)
            && self.copyability_of(self.c(fact.narrowed_type())) == Copyability::MoveOnly
        {
            return self.reference(receiver) == Some(fact.symbol())
                && self.known.contains(&fact.symbol());
        }
        self.expression(receiver)
    }

    pub(super) fn numeric_for(
        &mut self,
        statement: lang_frontend::ast::StatementId,
        source: ExpressionId,
        body: lang_frontend::ast::StatementId,
    ) -> bool {
        let statement_id =
            lang_frontend::type_checking::UnitStatementId::new(self.inputs.source_unit, statement);
        if self.inputs.typed.map_iteration(statement_id).is_some() {
            return false;
        }
        let Some(plan) = self.inputs.owned.iteration(statement_id) else {
            return false;
        };
        let descriptor = plan.descriptor();
        let source_id = self.id(source);
        if self.inputs.typed.sequential_iteration(statement_id) != Some(descriptor)
            || !matches!(
                descriptor.provider(),
                lang_frontend::type_checking::IterationProvider::NumericRange(
                    BuiltinType::Int | BuiltinType::Long
                )
            )
            || descriptor.statement() != statement_id
            || descriptor.source() != source_id
            || plan.source_access()
                != lang_frontend::ownership_checking::UnitIterationSourceAccess::Owned
            || plan.source_kind()
                != &lang_frontend::ownership_checking::IterationSource::OwnedSnapshot(source_id)
        {
            return false;
        }
        let element_ty = self.c(descriptor.element_type());
        let source_ty = self.c(descriptor.source_type());
        if !self.integer_type(element_ty)
            || !self.range_type(source_ty)
            || self.expression_type(source) != Some(source_ty)
            || !self.expression(source)
        {
            return false;
        }
        match descriptor.binding() {
            lang_frontend::type_checking::UnitSequentialIterationBinding::Discard => {}
            lang_frontend::type_checking::UnitSequentialIterationBinding::Name(symbol) => {
                if self.symbol_type(*symbol) != Some(element_ty) {
                    return false;
                }
                self.known.insert(*symbol);
            }
            lang_frontend::type_checking::UnitSequentialIterationBinding::Destructure(_) => {
                return false;
            }
        }
        self.loop_body(body)
    }

    pub(super) fn constant_materialization(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(facts) = self.inputs.typed.constants() else {
            return false;
        };
        let Ok(index) = facts
            .uses()
            .binary_search_by_key(&id, |use_| use_.expression())
        else {
            return false;
        };
        let descriptor = &facts.uses()[index];
        if let Some(owned) = self.inputs.constant_owned {
            let Some(plan) = owned.materialization_at(id) else {
                return false;
            };
            if plan.descriptor() != descriptor {
                return false;
            }
        }
        let Some(ty) = self.expression_type(expression) else {
            return false;
        };
        let supported = match descriptor.value() {
            lang_frontend::type_checking::ConstValue::Boolean(_) => matches!(
                self.kind(ty),
                Some(UnitTypeKind::Builtin(BuiltinType::Boolean))
            ),
            lang_frontend::type_checking::ConstValue::Integer { .. } => self.integer_type(ty),
            lang_frontend::type_checking::ConstValue::String(_) => self.string(ty),
            _ => false,
        };
        if supported {
            self.migrated = true;
        }
        supported
    }
}
