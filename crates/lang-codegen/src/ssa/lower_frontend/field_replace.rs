//! Trusted direct class-field exchange, separate from whole-root owner replacement.
use lang_frontend::{
    ownership_checking::{LoanKind as FrontendLoanKind, OwnershipPrimitiveValueTransfer},
    parser::CallArgument,
    type_checking::{AggregateProjectionKind, AggregateProjectionReceiver, ExpressionCategory},
};

use super::*;

impl ExpressionLowerer<'_> {
    pub(super) fn lower_field_replace(
        &mut self,
        call: ExpressionId,
        first: &CallArgument,
        second: &CallArgument,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .ownership_primitive(call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let ty = self.resolve_type(descriptor.value_type(), span)?;
        let target = *self
            .type_ids
            .get(&ty)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let (symbol, field, owner_type) =
            self.begin_field_replace(call, first.value, target, first.span)?;
        let mut replacement = match self.lower(second.value)? {
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Value(value) => value,
            LoweredValue::Unit => {
                if builtin_type(self.typed, ty) != Some(BuiltinType::Unit) {
                    return Err(error(LoweringErrorKind::MissingFact, second.span));
                }
                let (_, results) = self.append(
                    Operation::Constant(ScalarConstant::Unit),
                    vec![EntityType::Value(target)],
                    second.span,
                )?;
                value(results[0])
            }
        };
        // A terminating prefix has no commit fact. Require the trusted plan only here.
        let plan = self
            .owned
            .field_replacement(call)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let fact = self
            .owned
            .loan_begin(first.value)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if plan.descriptor() != &descriptor
            || plan.owner_type() != owner_type
            || !matches!(fact.target(), LoanTarget::Place(place) if place == plan.place())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let copyable = self.typed.copyability(ty) == Some(Copyability::Copyable);
        let expected = match (self.typed.expression_category(second.value), copyable) {
            (Some(ExpressionCategory::Temporary), _) => OwnershipPrimitiveValueTransfer::Temporary,
            (Some(ExpressionCategory::Place), true) => OwnershipPrimitiveValueTransfer::Copy,
            (Some(ExpressionCategory::Place), false) => OwnershipPrimitiveValueTransfer::Move,
            _ => return Err(error(LoweringErrorKind::MissingFact, second.span)),
        };
        if plan.new_value_transfer() != expected {
            return Err(error(LoweringErrorKind::MissingFact, second.span));
        }
        if copyable {
            let (_, results) = self.append(
                Operation::Copy {
                    source: replacement,
                },
                vec![EntityType::Value(target)],
                second.span,
            )?;
            replacement = value(results[0]);
        } else {
            self.forget_delivered_owners(&[replacement]);
            if self
                .function
                .entity(EntityId::Value(replacement))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(target))
            {
                let Some(TypeKind::Nullable(inner)) = self.typed.types().get(ty) else {
                    return Err(error(LoweringErrorKind::MissingFact, second.span));
                };
                if self.typed.expression_type(second.value) != Some(*inner) {
                    return Err(error(LoweringErrorKind::MissingFact, second.span));
                }
                let (_, results) = self.append(
                    Operation::NullableWrap {
                        nullable: target,
                        owner: replacement,
                    },
                    vec![EntityType::Value(target)],
                    second.span,
                )?;
                replacement = value(results[0]);
            }
        }
        // Both identities may have crossed CFG while the replacement was evaluated.
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let loan = self
            .pending_call_loans
            .get(&(call.index(), first.value.index()))
            .copied()
            .flatten()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, results) = self.append(
            Operation::HeapFieldExchange {
                owner,
                field,
                loan,
                replacement,
            },
            vec![EntityType::Value(target)],
            span,
        )?;
        self.pending_call_loans
            .remove(&(call.index(), first.value.index()));
        self.emit_drops(DropPoint::CallReturn(call))?;
        Ok(if builtin_type(self.typed, ty) == Some(BuiltinType::Unit) {
            LoweredValue::Unit
        } else {
            LoweredValue::Value(value(results[0]))
        })
    }

    fn begin_field_replace(
        &mut self,
        call: ExpressionId,
        operand: ExpressionId,
        target: SsaTypeId,
        span: Span,
    ) -> Result<(SymbolId, usize, TypeId), LoweringError> {
        let fact = self
            .owned
            .loan_begin(operand)
            .filter(|fact| fact.call() == call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let LoanTarget::Place(path) = fact.target() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let [field_symbol] = path.fields() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !path.elements().is_empty() || fact.kind() != FrontendLoanKind::Exclusive {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let symbol = path.root();
        // The SSA alias model still treats sibling fields as a shared parent root.
        // Keep that backend boundary explicit rather than generating invalid SSA for
        // otherwise legal simultaneous disjoint source loans.
        if self.owned.loans().iter().any(|active| {
            self.pending_call_loans
                .contains_key(&(active.call().index(), active.argument().index()))
                && matches!(active.target(), LoanTarget::Place(place) if place.root() == symbol)
        }) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let begin = fact.begin_span();
        let projected = self.field_replace_ungroup(operand, span)?;
        let projection = self
            .typed
            .aggregate_projection(projected)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let AggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let receiver = self.field_replace_ungroup(receiver, span)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(receiver)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(node.payload(), Expression::Name)
            || self.references.get(&span_key(node.span())) != Some(&symbol)
            || projection.field() != *field_symbol
            || projection.kind() != AggregateProjectionKind::Field
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner_type = self
            .typed
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(TypeKind::Nominal { nominal, arguments }) = self.typed.types().get(owner_type)
        else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .nominals()
            .iter()
            .find(|declaration| declaration.id() == *nominal)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || self.typed.parameter_mode(symbol).is_some()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let field = nominal
            .fields()
            .iter()
            .position(|candidate| candidate == field_symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let field_type = self.resolve_type(projection.ty(), span)?;
        if self.type_ids.get(&field_type) != Some(&target) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let owner_ssa = self
            .type_ids
            .get(&owner_type)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let payload = self
            .heap_payloads
            .get(owner_ssa)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, roots) = self.append(
            Operation::HeapPayloadPlace { owner },
            vec![EntityType::Place(payload)],
            span,
        )?;
        let (_, fields) = self.append(
            Operation::FieldPlace {
                base: place(roots[0]),
                field,
            },
            vec![EntityType::Place(target)],
            span,
        )?;
        let (_, loans) = self.append(
            Operation::BorrowBegin {
                place: place(fields[0]),
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target,
            }],
            begin,
        )?;
        let EntityId::Loan(loan) = loans[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        self.pending_call_loans
            .insert((call.index(), operand.index()), Some(loan));
        Ok((symbol, field, owner_type))
    }

    pub(super) fn field_replace_ungroup(
        &self,
        mut expression: ExpressionId,
        span: Span,
    ) -> Result<ExpressionId, LoweringError> {
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Group { expression: inner } = node.payload() else {
                return Ok(expression);
            };
            expression = *inner;
        }
    }
}
