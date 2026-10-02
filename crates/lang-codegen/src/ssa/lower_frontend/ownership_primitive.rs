//! Owned whole-root exchange consumes typed identity and Phase 3 commit facts.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    ownership_checking::{LoanKind as FrontendLoanKind, OwnershipPrimitiveValueTransfer},
    parser::Expression,
    source::Span,
    type_checking::{ExpressionCategory, OwnershipPrimitiveKind},
};

use super::*;

impl ExpressionLowerer<'_> {
    pub(super) fn lower_ownership_primitive(
        &mut self,
        expression: ExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .ownership_primitive(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let [first, second] = arguments else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if descriptor.expression() != expression
            || descriptor.operands() != [first.value, second.value]
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if descriptor.kind() == OwnershipPrimitiveKind::Replace
            && self.owned.loan_begin(first.value).is_some_and(
                |fact| matches!(fact.target(), LoanTarget::Place(place) if !place.is_root()),
            )
        {
            return self.lower_field_replace(expression, first, second, span);
        }
        let ty = self.resolve_type(descriptor.value_type(), span)?;
        let target = *self
            .type_ids
            .get(&ty)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let first_root = self.lower_primitive_root(expression, first.value, target, first.span)?;
        let second_root;
        let replacement;
        match descriptor.kind() {
            OwnershipPrimitiveKind::Replace => {
                replacement = match self.lower(second.value)? {
                    LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                    LoweredValue::Unit => {
                        if builtin_type(self.typed, ty) != Some(BuiltinType::Unit) {
                            return Err(error(LoweringErrorKind::MissingFact, second.span));
                        }
                        let (_, result) = self.append(
                            Operation::Constant(ScalarConstant::Unit),
                            vec![EntityType::Value(target)],
                            second.span,
                        )?;
                        value(result[0])
                    }
                    LoweredValue::Value(owner) => owner,
                };
                second_root = None;
            }
            OwnershipPrimitiveKind::Swap => {
                second_root = Some(self.lower_primitive_root(
                    expression,
                    second.value,
                    target,
                    second.span,
                )?);
                replacement = self.primitive_root_value(first_root, first.value, span)?;
            }
        }
        // A completely terminating prefix has no executable commit plan. Only require
        // the plan once evaluation actually reaches this normal continuation.
        let plan = self
            .owned
            .ownership_primitive(expression)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if plan.descriptor() != &descriptor
            || plan.places().first().map(|place| place.root()) != Some(first_root)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let first_owner = self.primitive_root_value(first_root, first.value, span)?;
        let first_loan = self.primitive_pending_loan(expression, first.value, span)?;
        let result = match descriptor.kind() {
            OwnershipPrimitiveKind::Replace => {
                if plan.places().len() != 1 {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let copyable = self.typed.copyability(ty) == Some(Copyability::Copyable);
                let expected = match (self.typed.expression_category(second.value), copyable) {
                    (Some(ExpressionCategory::Place), true) => {
                        OwnershipPrimitiveValueTransfer::Copy
                    }
                    (Some(ExpressionCategory::Temporary), _) => {
                        OwnershipPrimitiveValueTransfer::Temporary
                    }
                    (Some(ExpressionCategory::Place), false) => {
                        OwnershipPrimitiveValueTransfer::Move
                    }
                    _ => return Err(error(LoweringErrorKind::MissingFact, second.span)),
                };
                if plan.new_value_transfer() != Some(expected) {
                    return Err(error(LoweringErrorKind::MissingFact, second.span));
                }
                let mut replacement = replacement;
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
                let (_, results) = self.append(
                    Operation::RootReplace {
                        owner: first_owner,
                        loan: first_loan,
                        replacement,
                    },
                    vec![EntityType::Value(target); 2],
                    span,
                )?;
                self.bindings
                    .insert(first_root, LoweredValue::Value(value(results[0])));
                if builtin_type(self.typed, ty) == Some(BuiltinType::Unit) {
                    self.bindings.insert(first_root, LoweredValue::Unit);
                    LoweredValue::Unit
                } else {
                    LoweredValue::Value(value(results[1]))
                }
            }
            OwnershipPrimitiveKind::Swap => {
                let second_root =
                    second_root.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if plan.places().len() != 2
                    || plan.places()[1].root() != second_root
                    || first_root == second_root
                    || plan.new_value_transfer().is_some()
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let second_owner = self.primitive_root_value(second_root, second.value, span)?;
                let second_loan = self.primitive_pending_loan(expression, second.value, span)?;
                let (_, results) = self.append(
                    Operation::RootSwap {
                        owners: [first_owner, second_owner],
                        loans: [first_loan, second_loan],
                    },
                    vec![EntityType::Value(target); 2],
                    span,
                )?;
                self.bindings
                    .insert(first_root, LoweredValue::Value(value(results[0])));
                self.bindings
                    .insert(second_root, LoweredValue::Value(value(results[1])));
                if builtin_type(self.typed, ty) == Some(BuiltinType::Unit) {
                    self.bindings.insert(first_root, LoweredValue::Unit);
                    self.bindings.insert(second_root, LoweredValue::Unit);
                }
                LoweredValue::Unit
            }
        };
        self.release_primitive_unit_operand(expression.index(), first.value.index());
        self.pending_call_loans
            .remove(&(expression.index(), first.value.index()));
        if second_root.is_some() {
            self.release_primitive_unit_operand(expression.index(), second.value.index());
            self.pending_call_loans
                .remove(&(expression.index(), second.value.index()));
        }
        self.emit_drops(DropPoint::CallReturn(expression))?;
        Ok(result)
    }

    pub(super) fn release_primitive_unit_operand(&mut self, call: usize, operand: usize) {
        if self.typed.ownership_primitives().iter().any(|primitive| {
            primitive.expression().index() == call
                && builtin_type(self.typed, primitive.value_type()) == Some(BuiltinType::Unit)
                && (primitive.operands()[0].index() == operand
                    || (primitive.kind() == OwnershipPrimitiveKind::Swap
                        && primitive.operands()[1].index() == operand))
        }) {
            self.temporaries.remove(&operand);
        }
    }

    fn primitive_pending_loan(
        &self,
        call: ExpressionId,
        operand: ExpressionId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        self.pending_call_loans
            .get(&(call.index(), operand.index()))
            .copied()
            .flatten()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    fn primitive_root_value(
        &self,
        symbol: SymbolId,
        operand: ExpressionId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        match self.bindings.get(&symbol) {
            Some(LoweredValue::Value(value)) => Ok(*value),
            Some(LoweredValue::Unit) => self
                .temporaries
                .get(&operand.index())
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span)),
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }

    fn lower_primitive_root(
        &mut self,
        call: ExpressionId,
        operand: ExpressionId,
        target: SsaTypeId,
        span: Span,
    ) -> Result<SymbolId, LoweringError> {
        let fact = self
            .owned
            .loan_begin(operand)
            .filter(|fact| fact.call() == call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let LoanTarget::Place(root) = fact.target() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !root.is_root() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let symbol = root.root();
        if fact.kind() != FrontendLoanKind::Exclusive {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let begin = fact.begin_span();
        let mut node = operand;
        loop {
            let expression = self
                .parsed
                .ast()
                .expressions()
                .get(node)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            match expression.payload() {
                Expression::Group { expression } => node = *expression,
                Expression::Name
                    if self.references.get(&span_key(expression.span())) == Some(&symbol) =>
                {
                    break;
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        let mut owner = match self.bindings.get(&symbol).copied() {
            Some(LoweredValue::Value(owner)) => owner,
            Some(LoweredValue::Unit) => {
                let (_, results) = self.append(
                    Operation::Constant(ScalarConstant::Unit),
                    vec![EntityType::Value(target)],
                    span,
                )?;
                value(results[0])
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let ty = self
            .typed
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.copyability(ty) == Some(Copyability::Copyable) {
            let (_, results) = self.append(
                Operation::Copy { source: owner },
                vec![EntityType::Value(target)],
                span,
            )?;
            owner = value(results[0]);
        }
        if builtin_type(self.typed, ty) == Some(BuiltinType::Unit) {
            // The semantic binding stays Unit on every exit. Only the call prefix
            // transports its physical zero-sized root together with the loan.
            self.temporaries.insert(operand.index(), owner);
        } else {
            self.bindings.insert(symbol, LoweredValue::Value(owner));
        }
        let (_, result) = self.append(
            Operation::RootPlace { owner },
            vec![EntityType::Place(target)],
            span,
        )?;
        let EntityId::Place(place) = result[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let (_, result) = self.append(
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target,
            }],
            begin,
        )?;
        let EntityId::Loan(loan) = result[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        self.pending_call_loans
            .insert((call.index(), operand.index()), Some(loan));
        Ok(symbol)
    }
}
