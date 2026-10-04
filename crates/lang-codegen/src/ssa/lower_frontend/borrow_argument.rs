//! Borrow argument evaluation and temporary-place carriers.
use super::{
    EntityId, EntityType, ExpressionLowerer, LoanId, LoanKind, LoweredValue, LoweringError,
    LoweringErrorKind, Operation, PlaceId, error, place,
};
use lang_frontend::{
    ast::ExpressionId, ownership_checking::LoanTarget, parser::Expression, source::Span,
    type_checking::RcOperationKind,
};
impl ExpressionLowerer<'_> {
    pub(super) fn lower_borrow_argument(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        span: Span,
    ) -> Result<(LoanId, bool), LoweringError> {
        if let Some(non_null) = self.typed.non_null_use(argument)
            && let Some(view) = self.non_null_bindings.get(&non_null.symbol()).copied()
        {
            return Ok((view, false));
        }
        let fact = self
            .owned
            .loan_begin(argument)
            .filter(|fact| fact.call() == call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if fact.kind() != lang_frontend::ownership_checking::LoanKind::Shared {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if let Some((loan, created)) = self.deinit_view(argument)? {
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
        let target = self.expression_ssa_type(argument, span)?;
        let (_, results) = self.append(
            Operation::RootPlace { owner },
            vec![EntityType::Place(target)],
            span,
        )?;
        Ok(place(results[0]))
    }
}
