//! Copyable enums in the refined CFG (SPEC-0356 step 3): case construction, case tests on
//! the discriminant and smart-cast payload reads through the case's payload place.
use lang_frontend::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
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
        let mut operands = vec![operand];
        let mut inner = operand;
        while let (Expression::Group { expression }, _) = self.node(inner)? {
            inner = expression;
            operands.push(inner);
        }
        let inner_span = self.span(inner);
        let owner = match self.node(inner)?.0 {
            Expression::This => self.this_value_of(root, inner_span)?,
            Expression::Name => self.name(inner, inner_span)?,
            _ => match self.expression_inner(inner)? {
                Lowered::Value(slot) => slot,
                Lowered::Diverged => return Ok(Lowered::Diverged),
                Lowered::Unit => return Err(error(LoweringErrorKind::MissingFact, inner_span)),
            },
        };
        let result = self.type_test(owner, root, type_ref, negated, span)?;
        for op in operands.into_iter().rev() {
            self.register_temporary(op, owner)?;
            self.drops(UnitDropPoint::AfterExpression(self.unit(op)))?;
        }
        Ok(Lowered::Value(result))
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

pub(in crate::ssa::refine::build) mod inout {
    //! `Inout` in the refined CFG (SPEC-0358): `&local` arguments lend the binding's root place
    //! exclusively and read the replaced value back after the call; an `Inout` parameter's
    //! target is replaced through the caller's loan.
    use lang_frontend::{
        ast::ExpressionId, name_resolution::UnitSymbolId, parser::Expression, source::Span,
    };

    use super::super::{Builder, Lowered};
    use crate::ssa::{
        lowering_support::{LoweringError, LoweringErrorKind, error},
        model::{EntityType, LoanKind},
        refine::body::{ROp, Slot},
    };

    /// A binding lent to an `Inout` parameter; its new value is read back from `place`.
    #[derive(Clone, Debug)]
    pub(in crate::ssa::refine::build) struct Writeback {
        symbol: UnitSymbolId,
        place: Slot,
        move_only: bool,
        span: Span,
    }

    impl Writeback {
        pub(in crate::ssa::refine::build) const fn new(
            symbol: UnitSymbolId,
            place: Slot,
            move_only: bool,
            span: Span,
        ) -> Self {
            Self {
                symbol,
                place,
                move_only,
                span,
            }
        }
    }

    impl Builder<'_, '_> {
        /// `&local`: an exclusive loan of the binding's root place, live until the call returns;
        /// `&p` for an `Inout` parameter passes its loan on (a reborrow, nothing to write back).
        pub(in crate::ssa::refine::build) fn inout_argument(
            &mut self,
            expression: ExpressionId,
        ) -> Result<(Slot, Option<Writeback>), LoweringError> {
            let (_, span) = self.node(expression)?;
            let symbol = self.direct_place_symbol(expression, span)?;
            if let Some(loan) = self.inout_loan(symbol) {
                return Ok((loan, None));
            }
            let owner = *self
                .bindings
                .get(&symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let EntityType::Value(target) = self.body.slots[owner.0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let place = self.value(ROp::RootPlace { owner }, EntityType::Place(target), span)?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Exclusive,
                },
                EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target,
                },
                span,
            )?;
            let move_only = self.move_only(self.expression_type(expression)?);
            Ok((
                loan,
                Some(Writeback {
                    symbol,
                    place,
                    move_only,
                    span,
                }),
            ))
        }

        /// After the loan ended: the binding now names the value the callee left in its place;
        /// a MoveOnly owner is taken back from the place.
        pub(in crate::ssa::refine::build) fn write_back(
            &mut self,
            writeback: Writeback,
        ) -> Result<(), LoweringError> {
            let binding = *self
                .bindings
                .get(&writeback.symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, writeback.span))?;
            let EntityType::Place(target) = self.body.slots[writeback.place.0] else {
                return Err(error(LoweringErrorKind::InvalidModel, writeback.span));
            };
            let op = if writeback.move_only {
                ROp::RootPlaceTake {
                    owner: binding,
                    place: writeback.place,
                }
            } else {
                ROp::ReadPlace {
                    place: writeback.place,
                }
            };
            let value = self.value(op, EntityType::Value(target), writeback.span)?;
            self.push(ROp::Alias { source: value }, vec![binding], writeback.span)?;
            self.narrowed.remove(&writeback.symbol);
            Ok(())
        }

        /// The exclusive loan of an `Inout` parameter, if `symbol` is one.
        pub(in crate::ssa::refine::build) fn inout_loan(
            &self,
            symbol: UnitSymbolId,
        ) -> Option<Slot> {
            self.borrowed.get(&symbol).copied().filter(|loan| {
                matches!(
                    self.body.slots[loan.0],
                    EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        ..
                    }
                )
            })
        }

        fn direct_place_symbol(
            &self,
            mut expression: ExpressionId,
            span: Span,
        ) -> Result<UnitSymbolId, LoweringError> {
            loop {
                let (payload, node_span) = self.node(expression)?;
                match payload {
                    Expression::Name => return self.reference(node_span),
                    Expression::Group { expression: inner } => expression = inner,
                    _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
                }
            }
        }

        pub(in crate::ssa::refine::build) fn ownership_primitive(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<Lowered, LoweringError> {
            let id = self.unit(expression);
            let primitive = self
                .inputs
                .typed
                .ownership_primitive(id)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let (Expression::Call { arguments, .. }, _) = self.node(expression)? else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            if arguments.len() != 2 {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let value_ty = self.c(primitive.value_type());
            let target = self.ssa_type(value_ty, span)?;
            let copyable = !self.move_only(value_ty);
            if primitive.kind() == lang_frontend::type_checking::OwnershipPrimitiveKind::Replace
                && (self.inputs.owned.field_replacement(id).is_some()
                    || self.is_field_replace(arguments[0].value))
            {
                return self
                    .field_replace_primitive(id, &arguments, value_ty, target, copyable, span);
            }
            let modes = match primitive.kind() {
                lang_frontend::type_checking::OwnershipPrimitiveKind::Replace => [
                    lang_frontend::type_checking::ParameterMode::Inout,
                    lang_frontend::type_checking::ParameterMode::Value,
                ],
                lang_frontend::type_checking::OwnershipPrimitiveKind::Swap => [
                    lang_frontend::type_checking::ParameterMode::Inout,
                    lang_frontend::type_checking::ParameterMode::Inout,
                ],
            };
            let mut roots = Vec::with_capacity(2);
            let mut replacement = None;
            let is_unit = self.is_unit(value_ty);
            for (index, argument) in arguments.iter().enumerate() {
                if modes[index] == lang_frontend::type_checking::ParameterMode::Inout {
                    let symbol = self.direct_place_symbol(argument.value, argument.span)?;
                    let arg_id = self.unit(argument.value);
                    let fact = self
                        .inputs
                        .owned
                        .loans()
                        .iter()
                        .find(|loan| loan.call() == id && loan.argument() == arg_id)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, argument.span))?;
                    let begin_span = fact.begin_span();
                    self.end_views_of(symbol, begin_span)?;
                    self.narrowed.remove(&symbol);
                    let binding = if is_unit {
                        let b = self.value(
                            ROp::Constant(crate::ssa::model::ScalarConstant::Unit),
                            EntityType::Value(target),
                            argument.span,
                        )?;
                        self.push(ROp::Copy { source: b }, vec![b], argument.span)?;
                        b
                    } else {
                        let b = *self
                            .bindings
                            .get(&symbol)
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, argument.span))?;
                        if copyable {
                            self.push(ROp::Copy { source: b }, vec![b], argument.span)?;
                        }
                        b
                    };
                    let place = self.value(
                        ROp::RootPlace { owner: binding },
                        EntityType::Place(target),
                        begin_span,
                    )?;
                    let loan = self.value(
                        ROp::BorrowBegin {
                            place,
                            kind: LoanKind::Exclusive,
                        },
                        EntityType::Loan {
                            kind: LoanKind::Exclusive,
                            target,
                        },
                        begin_span,
                    )?;
                    self.pending_call_loans.push(loan);
                    roots.push((symbol, binding, loan));
                } else {
                    let value = if is_unit {
                        match self.expression(argument.value)? {
                            Lowered::Diverged => return Ok(Lowered::Diverged),
                            Lowered::Unit => self.value(
                                ROp::Constant(crate::ssa::model::ScalarConstant::Unit),
                                EntityType::Value(target),
                                argument.span,
                            )?,
                            Lowered::Value(slot) => slot,
                        }
                    } else {
                        let Some(value) = self.required(argument.value)? else {
                            return Ok(Lowered::Diverged);
                        };
                        let value = self.adapt(value, argument.value, value_ty)?;
                        self.deliver(argument.value);
                        value
                    };
                    let value = if copyable {
                        self.value(
                            ROp::Copy { source: value },
                            EntityType::Value(target),
                            argument.span,
                        )?
                    } else {
                        value
                    };
                    replacement = Some(value);
                }
            }
            for (_, _, loan) in &roots {
                self.pending_call_loans.retain(|&active| active != *loan);
            }
            let result = match (primitive.kind(), roots.as_slice(), replacement) {
                (
                    lang_frontend::type_checking::OwnershipPrimitiveKind::Replace,
                    [(_, binding, loan)],
                    Some(replacement),
                ) => {
                    self.owner_replace(id, *binding, replacement, span)?;
                    let old_slot = self.body.slot(EntityType::Value(target));
                    self.push(
                        ROp::RootReplace {
                            owner: *binding,
                            loan: *loan,
                            replacement,
                        },
                        vec![*binding, old_slot],
                        span,
                    )?;
                    self.return_replace_emitted(id, *binding, replacement, old_slot, span)?;
                    if is_unit {
                        Lowered::Unit
                    } else {
                        Lowered::Value(old_slot)
                    }
                }
                (
                    lang_frontend::type_checking::OwnershipPrimitiveKind::Swap,
                    [(_, left_binding, left_loan), (_, right_binding, right_loan)],
                    None,
                ) => {
                    self.push(
                        ROp::RootSwap {
                            left_owner: *left_binding,
                            right_owner: *right_binding,
                            left_loan: *left_loan,
                            right_loan: *right_loan,
                        },
                        vec![*left_binding, *right_binding],
                        span,
                    )?;
                    Lowered::Unit
                }
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            self.drops(lang_frontend::ownership_checking::UnitDropPoint::CallReturn(id))?;
            Ok(result)
        }

        fn field_replace_primitive(
            &mut self,
            id: lang_frontend::type_checking::UnitExpressionId,
            arguments: &[lang_frontend::parser::CallArgument],
            value_ty: lang_frontend::type_checking::UnitTypeId,
            target: crate::ssa::model::SsaTypeId,
            copyable: bool,
            span: Span,
        ) -> Result<Lowered, LoweringError> {
            let mut projected = arguments[0].value;
            while let (Expression::Group { expression: inner }, _) = self.node(projected)? {
                projected = inner;
            }
            let projection = self
                .inputs
                .typed
                .aggregate_projection(self.unit(projected))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
            let lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(receiver) =
                projection.receiver()
            else {
                return Err(error(LoweringErrorKind::MissingFact, arguments[0].span));
            };
            let symbol = self.direct_place_symbol(receiver.expression(), arguments[0].span)?;
            let owner_ty = self
                .symbol_type(symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
            let owner_ssa = self.ssa_type(owner_ty, arguments[0].span)?;
            let payload_ssa = *self
                .types
                .heap_payloads
                .get(&owner_ssa)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
            let field = *self
                .types
                .field_indices
                .get(&(owner_ty, projection.field()))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
            let arg0_id = self.unit(arguments[0].value);
            let fact = self
                .inputs
                .owned
                .loans()
                .iter()
                .find(|loan| loan.call() == id && loan.argument() == arg0_id)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
            let begin_span = fact.begin_span();
            self.end_views_of(symbol, begin_span)?;
            self.narrowed.remove(&symbol);
            let owner = *self
                .bindings
                .get(&symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
            let base = self.value(
                ROp::HeapPayloadPlace { owner },
                EntityType::Place(payload_ssa),
                begin_span,
            )?;
            let place = self.value(
                ROp::FieldPlace { base, field },
                EntityType::Place(target),
                begin_span,
            )?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Exclusive,
                },
                EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target,
                },
                begin_span,
            )?;
            self.pending_call_loans.push(loan);
            let Some(value) = self.required(arguments[1].value)? else {
                return Ok(Lowered::Diverged);
            };
            self.pending_call_loans.retain(|&active| active != loan);
            let value = self.adapt(value, arguments[1].value, value_ty)?;
            self.deliver(arguments[1].value);
            let replacement = if copyable {
                self.value(
                    ROp::Copy { source: value },
                    EntityType::Value(target),
                    arguments[1].span,
                )?
            } else {
                value
            };
            let old_slot = self.value(
                ROp::HeapFieldExchange {
                    owner,
                    field,
                    loan,
                    replacement,
                },
                EntityType::Value(target),
                span,
            )?;
            self.drops(lang_frontend::ownership_checking::UnitDropPoint::CallReturn(id))?;
            Ok(if self.is_unit(value_ty) {
                Lowered::Unit
            } else {
                Lowered::Value(old_slot)
            })
        }

        fn is_field_replace(&self, arg0: ExpressionId) -> bool {
            let mut projected = arg0;
            while let Ok((Expression::Group { expression: inner }, _)) = self.node(projected) {
                projected = inner;
            }
            self.inputs
                .typed
                .aggregate_projection(self.unit(projected))
                .is_some_and(|projection| {
                    projection.kind()
                        == lang_frontend::type_checking::UnitAggregateProjectionKind::Field
                })
        }
    }
}
