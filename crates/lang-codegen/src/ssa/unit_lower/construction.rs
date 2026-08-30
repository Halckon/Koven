//! compilation-unit construction descriptor 与 ordered ownership plan 的共同消费边界。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{ConstructionDeliveryKind, ConstructionRootKind},
    source::Span,
    type_checking::{
        Copyability, ExpressionCategory, NominalKind, UnitConstructionDescriptor,
        UnitConstructionTarget, UnitExpressionId,
    },
};

use super::{UnitExpressionLowerer, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityType, Operation, Origin, ValueId},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn construction_descriptor(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<UnitConstructionDescriptor, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        self.typed
            .types()
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    /// 按源码求值顺序 lower operand，再按声明参数顺序返回已经完成 Value delivery 的字段。
    pub(super) fn lower_construction_fields(
        &mut self,
        descriptor: &UnitConstructionDescriptor,
        span: Span,
    ) -> Result<Vec<ValueId>, LoweringError> {
        let id = descriptor.expression();
        let mut plans = self
            .owned
            .ownership()
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == id);
        let plan = plans
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if plans.next().is_some()
            || plan.target() != descriptor.target()
            || plan.terminating_operand().is_some()
            || self.typed.types().expression_type(id) != Some(descriptor.result_type())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_construction_root(descriptor, plan.root_obligation(), span)?;

        let mut arguments = descriptor.arguments().iter().collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        if arguments.len() != plan.deliveries().len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let mut fields = vec![None; arguments.len()];
        for (evaluation_index, (argument, delivery)) in
            arguments.into_iter().zip(plan.deliveries()).enumerate()
        {
            let expected_delivery = match (
                argument.category(),
                self.typed.types().copyability(argument.parameter_type()),
            ) {
                (ExpressionCategory::Temporary, _) => ConstructionDeliveryKind::DeliverTemporary,
                (ExpressionCategory::Place, Copyability::Copyable) => {
                    ConstructionDeliveryKind::Copy
                }
                (ExpressionCategory::Place, Copyability::MoveOnly) => {
                    ConstructionDeliveryKind::Move
                }
                (ExpressionCategory::Place, Copyability::Unknown | Copyability::Error) => {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            };
            if argument.evaluation_index() != evaluation_index
                || delivery.construction() != id
                || delivery.argument() != argument.argument()
                || delivery.parameter_index() != argument.parameter_index()
                || delivery.parameter_symbol() != argument.parameter_symbol()
                || delivery.evaluation_index() != evaluation_index
                || delivery.kind() != expected_delivery
                || self.typed.types().expression_type(argument.argument())
                    != Some(argument.parameter_type())
                || self.typed.types().expression_category(argument.argument())
                    != Some(argument.category())
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let mut field = self.require_expression_value(argument.argument().expression())?;
            match delivery.kind() {
                ConstructionDeliveryKind::Copy => {
                    let ty = self.expression_ssa_type(argument.argument().expression(), span)?;
                    let (_, results) = self
                        .function
                        .append_instruction(
                            self.block,
                            Operation::Copy { source: field },
                            vec![EntityType::Value(ty)],
                            Origin::Source(span),
                        )
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                    field = require_value(results[0], span)?;
                }
                ConstructionDeliveryKind::Move | ConstructionDeliveryKind::DeliverTemporary => {
                    self.transfer_owned_expression(argument.argument().expression(), field, span)?;
                }
            }
            let slot = fields
                .get_mut(argument.parameter_index())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if slot.replace(field).is_some() {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        fields
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    fn validate_construction_root(
        &self,
        descriptor: &UnitConstructionDescriptor,
        actual: Option<lang_frontend::ownership_checking::UnitConstructionRootDropObligation>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let expected_kind = match descriptor.target() {
            UnitConstructionTarget::IntrinsicBox => ConstructionRootKind::HeapOwner,
            UnitConstructionTarget::IntrinsicRc => ConstructionRootKind::SharedOwner,
            UnitConstructionTarget::EnumCase(_) => ConstructionRootKind::Inline,
            UnitConstructionTarget::Nominal(declaration) => self
                .typed
                .types()
                .signatures()
                .declaration(declaration)
                .and_then(|signature| signature.nominal())
                .and_then(|nominal| match nominal.kind() {
                    NominalKind::Class => Some(ConstructionRootKind::HeapOwner),
                    NominalKind::ValueClass => Some(ConstructionRootKind::Inline),
                    NominalKind::Interface | NominalKind::EnumClass | NominalKind::Object => None,
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
        };
        match (
            self.typed.types().copyability(descriptor.result_type()),
            actual,
        ) {
            (Copyability::Copyable, None) => Ok(()),
            (Copyability::MoveOnly, Some(root))
                if root.construction() == descriptor.expression()
                    && root.result_type() == descriptor.result_type()
                    && root.kind() == expected_kind =>
            {
                Ok(())
            }
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }
}
