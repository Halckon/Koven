//! Copyable enums in the refined CFG (SPEC-0356 step 3): case construction, case tests on
//! the discriminant and smart-cast payload reads through the case's payload place.
use lang_frontend::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::UnitSymbolId,
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, NominalKind, UnitAggregateProjectionReceiver, UnitConstructionTarget,
        UnitNominalSignature, UnitTypeId, UnitTypeKind, UnitTypeRefId,
    },
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{ComparisonOperator, EntityType, ScalarConstant, SsaTypeId},
    refine::body::{ROp, Slot},
};

impl Builder<'_, '_> {
    fn enum_signature(
        &self,
        root: UnitTypeId,
        span: Span,
    ) -> Result<&UnitNominalSignature, LoweringError> {
        let Some(UnitTypeKind::Nominal { declaration, .. }) = self.inputs.typed.types().get(root)
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        self.inputs
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .filter(|nominal| nominal.kind() == NominalKind::EnumClass)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    /// Whether `ty` is an enum root (the subset admits only Copyable concrete ones).
    pub(super) fn is_enum(&self, ty: UnitTypeId) -> bool {
        self.enum_signature(ty, self.inputs.span).is_ok()
    }

    /// The registered variant index and payload aggregate of a case symbol.
    pub(super) fn variant(
        &self,
        root: UnitTypeId,
        case: UnitSymbolId,
        span: Span,
    ) -> Result<(SsaTypeId, usize, SsaTypeId), LoweringError> {
        let tagged = self.ssa_type(root, span)?;
        let (variant, payload) = self
            .types
            .enum_payloads
            .get(&(tagged, case))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        Ok((tagged, variant, payload))
    }

    /// The current receiver's value; a Borrow receiver is copied out of its loan.
    pub(super) fn this_value(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let ty = self.expression_type(expression)?;
        self.this_value_of(ty, span)
    }

    /// Payloads evaluate in source order into declaration order; the payload aggregate is
    /// then wrapped in the case's variant.
    pub(super) fn enum_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let UnitConstructionTarget::EnumCase(case) = descriptor.target() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let result_ty = self.c(descriptor.result_type());
        let move_enum = self.move_only(result_ty);
        let mut arguments = descriptor.arguments().iter().collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        let mut fields: Vec<Option<Slot>> = vec![None; arguments.len()];
        let mut delivered = Vec::with_capacity(arguments.len());
        for argument in arguments {
            if argument.argument().source_unit() != self.inputs.source_unit {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let value_expression = argument.argument().expression();
            let Some(value) = self.required(value_expression)? else {
                return Ok(Lowered::Diverged);
            };
            let param_ty = self.c(argument.parameter_type());
            let mut value = self.adapt(value, value_expression, param_ty)?;
            if move_enum
                && argument.category() == lang_frontend::type_checking::ExpressionCategory::Place
                && !self.move_only(param_ty)
            {
                let param_ssa = self.ssa_type(param_ty, span)?;
                value = self.value(
                    ROp::Copy { source: value },
                    EntityType::Value(param_ssa),
                    self.span(value_expression),
                )?;
            } else {
                self.repoint_temporary(value_expression, value);
                delivered.push(value_expression);
            }
            let slot = fields
                .get_mut(argument.parameter_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if slot.replace(value).is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        for value_expression in delivered {
            self.deliver(value_expression);
        }
        let fields = fields
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (tagged, variant, aggregate) = self.variant(result_ty, case, span)?;
        let payload = self.value(
            ROp::AggregateConstruct { aggregate, fields },
            EntityType::Value(aggregate),
            span,
        )?;
        Ok(Lowered::Value(self.value(
            ROp::TaggedConstruct {
                tagged,
                variant,
                payload,
            },
            EntityType::Value(tagged),
            span,
        )?))
    }

    /// `discriminant(owner) == variant`, negated for `!is`.
    fn variant_test(
        &mut self,
        owner: Slot,
        variant: usize,
        negated: bool,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let integer = self
            .inputs
            .typed
            .types()
            .builtin(BuiltinType::Int)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
            .and_then(|ty| self.ssa_type(ty, span))?;
        let tag = self.value(
            ROp::TaggedDiscriminant { owner },
            EntityType::Value(integer),
            span,
        )?;
        self.variant_tag_test(tag, variant, negated, span)
    }

    pub(super) fn variant_tag_test(
        &mut self,
        tag: Slot,
        variant: usize,
        negated: bool,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let builtin = |builder: &Self, builtin| {
            builder
                .inputs
                .typed
                .types()
                .builtin(builtin)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
                .and_then(|ty| builder.ssa_type(ty, span))
        };
        let integer = builtin(self, BuiltinType::Int)?;
        let boolean = builtin(self, BuiltinType::Boolean)?;
        let expected = self.value(
            ROp::Constant(ScalarConstant::Integer(
                i128::try_from(variant)
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?,
            )),
            EntityType::Value(integer),
            span,
        )?;
        let matched = self.value(
            ROp::Compare {
                operator: ComparisonOperator::Equal,
                left: tag,
                right: expected,
            },
            EntityType::Value(boolean),
            span,
        )?;
        if !negated {
            return Ok(matched);
        }
        self.value(
            ROp::BooleanNot { operand: matched },
            EntityType::Value(boolean),
            span,
        )
    }

    pub(super) fn type_test_variant(
        &self,
        root: UnitTypeId,
        type_ref: TypeRefId,
        span: Span,
    ) -> Result<usize, LoweringError> {
        let target = self
            .inputs
            .typed
            .type_ref_type(UnitTypeRefId::new(self.inputs.source_unit, type_ref))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(UnitTypeKind::EnumCase { case, root: own }) =
            self.inputs.typed.types().get(target).cloned()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if self.c(own) != root {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let (_, variant, _) = self.variant(root, case, span)?;
        Ok(variant)
    }

    /// `owner is C` / `!is C` for a case type of `root`.
    pub(super) fn type_test(
        &mut self,
        owner: Slot,
        root: UnitTypeId,
        type_ref: TypeRefId,
        negated: bool,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let variant = self.type_test_variant(root, type_ref, span)?;
        self.variant_test(owner, variant, negated, span)
    }

    /// A payloadless case value compared with the subject: only its identity is used.
    /// The test is anchored at the `when` entry, like a type test.
    pub(super) fn case_value_test(
        &mut self,
        owner: Slot,
        root: UnitTypeId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .construction(id)
            .filter(|descriptor| {
                descriptor.expression() == id
                    && descriptor.arguments().is_empty()
                    && self.c(descriptor.result_type()) == root
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let UnitConstructionTarget::EnumCase(case) = descriptor.target() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let (_, variant, _) = self.variant(root, case, span)?;
        self.variant_test(owner, variant, false, span)
    }

    /// `operand is C` as a Boolean value.
    pub(super) fn type_test_expression(
        &mut self,
        operand: ExpressionId,
        type_ref: TypeRefId,
        negated: bool,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let root = self.expression_type(operand)?;
        let Some(owner) = self.required(operand)? else {
            return Ok(Lowered::Diverged);
        };
        Ok(Lowered::Value(
            self.type_test(owner, root, type_ref, negated, span)?,
        ))
    }

    /// Whether the projection reads an enum payload (implicit `this` or a narrowed case).
    pub(super) fn is_enum_projection(&self, expression: ExpressionId) -> bool {
        let Some(projection) = self
            .inputs
            .typed
            .aggregate_projection(self.unit(expression))
        else {
            return false;
        };
        match projection.receiver() {
            UnitAggregateProjectionReceiver::This(_) => self
                .inputs
                .receiver
                .is_some_and(|receiver| self.is_enum(receiver.ty)),
            UnitAggregateProjectionReceiver::Expression(receiver) => matches!(
                self.inputs
                    .typed
                    .expression_type(receiver)
                    .and_then(|ty| self.inputs.typed.types().get(ty)),
                Some(UnitTypeKind::EnumCase { .. })
            ),
        }
    }

    /// Copies a scalar payload field out of the case the flow facts proved.
    pub(super) fn enum_projection(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let projection = self
            .inputs
            .typed
            .aggregate_projection(self.unit(expression))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (root, owner) = match projection.receiver() {
            UnitAggregateProjectionReceiver::This(owner) => {
                let receiver = self
                    .inputs
                    .receiver
                    .filter(|receiver| receiver.owner == owner)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                (receiver.ty, self.this_value_of(receiver.ty, span)?)
            }
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let Some(UnitTypeKind::EnumCase { root, .. }) = self
                    .inputs
                    .typed
                    .expression_type(receiver)
                    .and_then(|ty| self.inputs.typed.types().get(ty))
                    .cloned()
                else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let root = self.c(root);
                let receiver = receiver.expression();
                let (payload, receiver_span) = self.node(receiver)?;
                let owner = match payload {
                    Expression::This => self.this_value_of(root, receiver_span)?,
                    Expression::Name => {
                        let symbol = self.reference(receiver_span)?;
                        self.copyable_read(symbol, receiver_span)?
                    }
                    _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
                };
                self.drops(
                    lang_frontend::ownership_checking::UnitDropPoint::AfterExpression(
                        self.unit(receiver),
                    ),
                )?;
                (root, owner)
            }
        };
        let (case, field) = self
            .enum_signature(root, span)?
            .enum_cases()
            .iter()
            .find_map(|case| {
                case.payloads()
                    .iter()
                    .position(|payload| payload.symbol() == projection.field())
                    .map(|index| (case.type_symbol(), index))
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.types.field_indices.get(&(root, projection.field())) != Some(&field) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let (_, variant, payload) = self.variant(root, case, span)?;
        let result = self.ssa_type(self.expression_type(expression)?, span)?;
        let base = self.value(
            ROp::TaggedPayloadPlace { owner, variant },
            EntityType::Place(payload),
            span,
        )?;
        let place = self.value(
            ROp::FieldPlace { base, field },
            EntityType::Place(result),
            span,
        )?;
        Ok(Lowered::Value(self.value(
            ROp::ReadPlace { place },
            EntityType::Value(result),
            span,
        )?))
    }

    /// The receiver value when its type is `root`; a Borrow receiver is copied out.
    pub(super) fn this_value_of(
        &mut self,
        root: UnitTypeId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        if self.inputs.receiver.map(|receiver| receiver.ty) != Some(root) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let slot = self
            .receiver
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        match self.body.slots[slot.0] {
            EntityType::Value(_) => Ok(slot),
            EntityType::Loan { target, .. } => self.value(
                ROp::ReadLoan { loan: slot },
                EntityType::Value(target),
                span,
            ),
            EntityType::Place(_) => Err(error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    /// A Copyable binding or Borrow parameter read by value.
    fn copyable_read(&mut self, symbol: UnitSymbolId, span: Span) -> Result<Slot, LoweringError> {
        if let Some(loan) = self.borrowed.get(&symbol).copied() {
            let EntityType::Loan { target, .. } = self.body.slots[loan.0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            return self.value(ROp::ReadLoan { loan }, EntityType::Value(target), span);
        }
        self.bindings
            .get(&symbol)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }
}
