//! Construction and Rc lowering over validated frontend ownership facts.
use super::*;

impl ExpressionLowerer<'_> {
    pub(in crate::ssa) fn lower_construction(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.construction(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let plan = self
            .owned
            .construction_plan(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if plan.target() != descriptor.target()
            || (plan.terminating_operand().is_none()
                && plan.deliveries().len() != descriptor.arguments().len())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_construction_root(descriptor.result_type(), descriptor.target(), plan, span)?;

        let mut arguments = descriptor.arguments().iter().collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        let mut fields = vec![None; arguments.len()];
        for (evaluation_index, argument) in arguments.into_iter().enumerate() {
            if plan.terminating_operand() == Some(argument.argument()) {
                return match self.lower(argument.argument())? {
                    LoweredValue::Diverged => Ok(LoweredValue::Diverged),
                    LoweredValue::Unit | LoweredValue::Value(_) => {
                        Err(error(LoweringErrorKind::MissingFact, span))
                    }
                };
            }
            let effect = plan
                .deliveries()
                .get(evaluation_index)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if argument.evaluation_index() != evaluation_index
                || effect.argument() != argument.argument()
                || effect.parameter_index() != argument.parameter_index()
                || effect.parameter_symbol() != argument.parameter_symbol()
                || effect.evaluation_index() != evaluation_index
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let LoweredValue::Value(mut field) = self.lower(argument.argument())? else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let pending = if effect.kind() == ConstructionDeliveryKind::Copy {
                let ty = self.expression_ssa_type(argument.argument(), span)?;
                let (_, results) = self.append(
                    Operation::Copy { source: field },
                    vec![EntityType::Value(ty)],
                    span,
                )?;
                field = value(results[0]);
                None
            } else if self
                .typed
                .expression_type(argument.argument())
                .and_then(|ty| self.typed.copyability(ty))
                == Some(Copyability::MoveOnly)
            {
                // The operand is delivered now; keep only its pending construction slot alive
                // while later operands may introduce block parameters or exit early.
                self.forget_delivered_owners(&[field]);
                let key = argument.argument().index();
                self.temporaries.insert(key, field);
                Some(key)
            } else {
                None
            };
            let slot = fields
                .get_mut(argument.parameter_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if slot.replace((field, pending)).is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        if plan.terminating_operand().is_some() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let fields = fields
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .into_iter()
            .map(|(field, pending)| {
                pending.map_or(Ok(field), |key| {
                    self.temporaries
                        .remove(&key)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let result_type = self.expression_ssa_type(expression, span)?;
        let result = match descriptor.target() {
            ConstructionTarget::Nominal(target) => {
                let nominal = self
                    .typed
                    .nominals()
                    .iter()
                    .find(|nominal| nominal.id() == target)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let aggregate = match nominal.kind() {
                    NominalKind::ValueClass => result_type,
                    NominalKind::Class => self
                        .heap_payloads
                        .get(&result_type)
                        .copied()
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?,
                    _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
                };
                let (_, aggregate_results) = self.append(
                    Operation::AggregateConstruct { aggregate, fields },
                    vec![EntityType::Value(aggregate)],
                    span,
                )?;
                let payload = value(aggregate_results[0]);
                if nominal.kind() == NominalKind::Class {
                    let (_, owner_results) = self.append(
                        Operation::HeapAllocate {
                            owner: result_type,
                            payload,
                        },
                        vec![EntityType::Value(result_type)],
                        span,
                    )?;
                    value(owner_results[0])
                } else {
                    payload
                }
            }
            ConstructionTarget::IntrinsicBox => {
                let [payload] = fields.as_slice() else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let (_, results) = self.append(
                    Operation::HeapAllocate {
                        owner: result_type,
                        payload: *payload,
                    },
                    vec![EntityType::Value(result_type)],
                    span,
                )?;
                value(results[0])
            }
            ConstructionTarget::IntrinsicRc => {
                let [payload] = fields.as_slice() else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let (_, results) = self.append(
                    Operation::SharedAllocate {
                        owner: result_type,
                        payload: *payload,
                    },
                    vec![EntityType::Value(result_type)],
                    span,
                )?;
                value(results[0])
            }
            ConstructionTarget::EnumCase(_) => {
                let ConstructionTarget::EnumCase(case) = descriptor.target() else {
                    unreachable!();
                };
                let (variant, payload_type) = self
                    .enum_payloads
                    .get(&(result_type, case))
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let (_, payload_results) = self.append(
                    Operation::AggregateConstruct {
                        aggregate: payload_type,
                        fields,
                    },
                    vec![EntityType::Value(payload_type)],
                    span,
                )?;
                let (_, results) = self.append(
                    Operation::TaggedConstruct {
                        tagged: result_type,
                        variant,
                        payload: value(payload_results[0]),
                    },
                    vec![EntityType::Value(result_type)],
                    span,
                )?;
                value(results[0])
            }
        };
        Ok(LoweredValue::Value(result))
    }

    pub(in crate::ssa) fn lower_rc_operation(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.rc_operation(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let effect = self.owned.rc_effect(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let expected_effect = match descriptor.kind() {
            RcOperationKind::Share => RcOwnershipEffectKind::Retain,
            RcOperationKind::Value => RcOwnershipEffectKind::BorrowPayload,
        };
        if effect.receiver() != descriptor.receiver()
            || effect.payload_type() != descriptor.payload_type()
            || effect.kind() != expected_effect
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let owner = self.shared_owner_operand(descriptor.receiver(), span)?;
        match descriptor.kind() {
            RcOperationKind::Share => {
                let result_type = self.expression_ssa_type(expression, span)?;
                let (_, results) = self.append(
                    Operation::SharedRetain { owner },
                    vec![EntityType::Value(result_type)],
                    span,
                )?;
                Ok(LoweredValue::Value(value(results[0])))
            }
            RcOperationKind::Value => {
                if self.typed.copyability(descriptor.payload_type()) != Some(Copyability::Copyable)
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let result_type = self.expression_ssa_type(expression, span)?;
                if let EntityId::Loan(source) = owner {
                    let (_, results) = self.append(
                        Operation::SharedPayloadLoan { source },
                        vec![EntityType::Loan {
                            kind: crate::ssa::model::LoanKind::Shared,
                            target: result_type,
                        }],
                        span,
                    )?;
                    let EntityId::Loan(loan) = results[0] else {
                        return Err(error(LoweringErrorKind::InvalidModel, span));
                    };
                    let (_, results) = self.append(
                        Operation::Read {
                            source: crate::ssa::model::PlaceAccess::Loan(loan),
                        },
                        vec![EntityType::Value(result_type)],
                        span,
                    )?;
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                    return Ok(LoweredValue::Value(value(results[0])));
                }
                let (_, places) = self.append(
                    Operation::SharedPayloadPlace { owner },
                    vec![EntityType::Place(result_type)],
                    span,
                )?;
                let (_, results) = self.append(
                    Operation::Read {
                        source: crate::ssa::model::PlaceAccess::Place(place(places[0])),
                    },
                    vec![EntityType::Value(result_type)],
                    span,
                )?;
                Ok(LoweredValue::Value(value(results[0])))
            }
        }
    }

    pub(in crate::ssa) fn validate_construction_root(
        &self,
        result_type: TypeId,
        target: ConstructionTarget,
        plan: &lang_frontend::ownership_checking::ConstructionOwnershipPlan,
        span: Span,
    ) -> Result<(), LoweringError> {
        if plan.terminating_operand().is_some() {
            return if plan.root_obligation().is_none() {
                Ok(())
            } else {
                Err(error(LoweringErrorKind::MissingFact, span))
            };
        }
        let expected_kind = match target {
            ConstructionTarget::IntrinsicBox => Some(ConstructionRootKind::HeapOwner),
            ConstructionTarget::IntrinsicRc => Some(ConstructionRootKind::SharedOwner),
            ConstructionTarget::EnumCase(_) => Some(ConstructionRootKind::Inline),
            ConstructionTarget::Nominal(target) => self
                .typed
                .nominals()
                .iter()
                .find(|nominal| nominal.id() == target)
                .and_then(|nominal| match nominal.kind() {
                    NominalKind::ValueClass => Some(ConstructionRootKind::Inline),
                    NominalKind::Class => Some(ConstructionRootKind::HeapOwner),
                    NominalKind::Interface | NominalKind::EnumClass | NominalKind::Object => None,
                }),
        };
        match (self.typed.copyability(result_type), plan.root_obligation()) {
            (Some(Copyability::Copyable), None) => Ok(()),
            (Some(Copyability::MoveOnly), Some(obligation))
                if obligation.construction() == plan.construction()
                    && obligation.result_type() == result_type
                    && Some(obligation.kind()) == expected_kind =>
            {
                Ok(())
            }
            _ => Err(error(LoweringErrorKind::MissingFact, span)),
        }
    }
}
