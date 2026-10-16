//! Value/Borrow argument evaluation and temporary-place carriers.
use super::{
    EntityId, EntityType, ExpressionLowerer, LoanId, LoanKind, LoweredValue, LoweringError,
    LoweringErrorKind, Operation, PlaceId, ValueId, error, place, span_key,
};
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::LoanTarget,
    parser::Expression,
    source::Span,
    type_checking::{Copyability, RcOperationKind},
};
impl ExpressionLowerer<'_> {
    /// MoveOnly values are detached immediately and carried until all operands are ready.
    pub(super) fn lower_value_argument(
        &mut self,
        argument: ExpressionId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let value = self.require_value(argument)?;
        if self
            .typed
            .expression_type(argument)
            .and_then(|ty| self.typed.copyability(ty))
            == Some(Copyability::MoveOnly)
        {
            let mut source = argument;
            loop {
                self.temporaries.remove(&source.index());
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(source)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                match node.payload() {
                    Expression::Group { expression } => source = *expression,
                    Expression::Name => {
                        let symbol = self
                            .references
                            .get(&span_key(node.span()))
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, node.span()))?;
                        self.bindings.remove(symbol);
                        break;
                    }
                    _ => break,
                }
            }
            self.temporaries.insert(argument.index(), value);
        }
        Ok(value)
    }

    pub(super) fn take_pending_value_argument(
        &mut self,
        argument: ExpressionId,
        span: Span,
    ) -> Result<Option<ValueId>, LoweringError> {
        if self
            .typed
            .expression_type(argument)
            .and_then(|ty| self.typed.copyability(ty))
            != Some(Copyability::MoveOnly)
        {
            return Ok(None);
        }
        self.temporaries
            .remove(&argument.index())
            .map(Some)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    pub(super) fn lower_borrow_argument(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        span: Span,
    ) -> Result<(LoanId, bool), LoweringError> {
        if self.owned.borrow_results().range_uses().iter().any(|fact| {
            fact.expression() == argument
                && fact.site() == lang_frontend::ownership_checking::RangeUseSite::Call(call)
        }) {
            let target = self.expression_ssa_type(argument, span)?;
            return self
                .lower_short_range(argument, target, span)
                .map(|loan| (loan, true));
        }
        if let Some(proof) = self.non_null_proof_for_expression(argument, span)? {
            return Ok((proof.loan, false));
        }
        let fact = self
            .owned
            .loan_begin(argument)
            .filter(|fact| fact.call() == call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if fact.kind() != lang_frontend::ownership_checking::LoanKind::Shared {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let mut actual = argument;
        while let Expression::Group { expression } = self
            .parsed
            .ast()
            .expressions()
            .get(actual)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
            .payload()
        {
            actual = *expression;
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(actual)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if matches!(
            self.typed
                .expression_type(argument)
                .and_then(|ty| self.typed.types().get(ty)),
            Some(lang_frontend::type_checking::TypeKind::Intrinsic {
                constructor: lang_frontend::type_checking::IntrinsicTypeConstructor::View,
                ..
            })
        ) && matches!(node.payload(), Expression::Name | Expression::This)
            && let Some(symbol) = self.references.get(&super::span_key(node.span()))
            && let Some(LoweredValue::Value(view)) = self.bindings.get(symbol).copied()
        {
            let target = self.expression_ssa_type(argument, span)?;
            let (_, results) = self.append(
                Operation::RootPlace { owner: view },
                vec![EntityType::Place(target)],
                span,
            )?;
            let place = place(results[0]);
            let (_, results) = self.append(
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                span,
            )?;
            let EntityId::Loan(loan) = results[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            return Ok((loan, true));
        }
        if matches!(node.payload(), Expression::Name | Expression::This)
            && let Some(symbol) = self.references.get(&super::span_key(node.span()))
            && self
                .owned
                .borrow_results()
                .bindings()
                .iter()
                .any(|binding| binding.binding() == *symbol && binding.origin() == fact.target())
            && let Some(loan) = self.borrow_bindings.get(symbol).copied()
        {
            return Ok((loan, false));
        }
        if let Some((loan, created)) = self.readonly_view(argument)? {
            // The current call carrier stores one pending loan. Deeper borrowed projections
            // need an ancestor-loan carrier across argument CFG and remain fail-closed.
            if created.len() > 1 {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            return Ok((loan, !created.is_empty()));
        }
        if let LoanTarget::Place(target) = fact.target()
            && target.is_root()
            && let Some(loan) = self.borrow_bindings.get(&target.root()).copied()
            && self
                .function
                .entity(EntityId::Loan(loan))
                .map(|data| data.ty)
                == Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: self.expression_ssa_type(argument, span)?,
                })
        {
            return Ok((loan, false));
        }
        if let LoanTarget::Place(target) = fact.target()
            && !target.is_root()
            && self.borrow_bindings.contains_key(&target.root())
            && self.typed.aggregate_projection(argument).is_some()
        {
            let mut created = Vec::new();
            let loan = self.clone_field_loan(argument, &mut created)?;
            if created.len() != 1 {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            return Ok((loan, true));
        }
        let place = self.lower_borrow_place(argument, fact.target(), span)?;
        let target = self.expression_ssa_type(argument, span)?;
        let (_, results) = self.append(
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }],
            fact.begin_span(),
        )?;
        let EntityId::Loan(loan) = results[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, fact.begin_span()));
        };
        Ok((loan, true))
    }

    fn lower_borrow_place(
        &mut self,
        argument: ExpressionId,
        target: &LoanTarget,
        span: Span,
    ) -> Result<PlaceId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(argument)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Group { expression } = node.payload() {
            return self.lower_borrow_place(*expression, target, span);
        }
        if let LoanTarget::Temporary(temporary) = target
            && let Some(element) = self.typed.element_place(argument)
        {
            let owner = self.require_value(*temporary)?;
            let index = self.require_value(element.index())?;
            let element_type = self.expression_ssa_type(argument, span)?;
            let (_, results) = self.append(
                Operation::ContainerElementPlace {
                    owner: EntityId::Value(owner),
                    index,
                },
                vec![EntityType::Place(element_type)],
                span,
            )?;
            return Ok(place(results[0]));
        }
        if let LoanTarget::Place(target) = target
            && !target.is_root()
            && let Some(place) = self.lower_borrowed_container_element(argument, target, span)?
        {
            return Ok(place);
        }
        if let Some(operation) = self.typed.rc_operation(argument)
            && operation.kind() == RcOperationKind::Value
        {
            let owner = self.shared_owner_operand(operation.receiver(), span)?;
            let payload = self.expression_ssa_type(argument, span)?;
            let (_, results) = self.append(
                Operation::SharedPayloadPlace { owner },
                vec![EntityType::Place(payload)],
                span,
            )?;
            return Ok(place(results[0]));
        }
        let owner = match target {
            LoanTarget::Place(place) if place.is_root() => self
                .bindings
                .get(&place.root())
                .and_then(|value| match value {
                    LoweredValue::Value(value) => Some(*value),
                    LoweredValue::Unit | LoweredValue::Diverged => None,
                })
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?,
            LoanTarget::Temporary(temporary) => self.require_value(*temporary)?,
            LoanTarget::Place(_) | LoanTarget::This(_) => {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        let target_type = self.expression_ssa_type(argument, span)?;
        // Match the validated nullable use to its whole-root query result before
        // copying the registered payload for this synchronous shared argument.
        let mut owner = owner;
        if let LoanTarget::Place(root) = target
            && root.is_root()
            && let Some(proof) = self.typed.non_null_use(argument)
            && self.is_map_result_value(owner)
        {
            let declared = self.resolve_type(proof.declared_type(), span)?;
            let narrowed = self.resolve_type(proof.narrowed_type(), span)?;
            if self.typed.copyability(narrowed)
                != Some(lang_frontend::type_checking::Copyability::Copyable)
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let actual = self
                .function
                .entity(EntityId::Value(owner))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .ty
                .semantic_type();
            if proof.symbol() != root.root()
                || self.type_ids.get(&declared) != Some(&actual)
                || self.type_ids.get(&narrowed) != Some(&target_type)
                || self.map_results.get(&actual) != Some(&target_type)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            owner = match self.unwrap_map_result(owner, span)? {
                LoweredValue::Value(value) => value,
                _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
            };
        }
        let (_, results) = self.append(
            Operation::RootPlace { owner },
            vec![EntityType::Place(target_type)],
            span,
        )?;
        Ok(place(results[0]))
    }
}
