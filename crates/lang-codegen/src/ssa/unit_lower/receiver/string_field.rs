//! String field receivers borrow storage instead of attempting a MoveOnly field read.
use super::*;
use crate::ssa::model::SsaTypeKind;
use lang_frontend::ownership_checking::UnitReceiverOwnershipFact;

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_string_field_receiver(
        &mut self,
        fact: &UnitReceiverOwnershipFact,
        expression: UnitExpressionId,
        target: SsaTypeId,
    ) -> Result<Option<LoweredReceiver>, LoweringError> {
        let UnitReceiverOwnershipTarget::Place(place) = fact.target() else {
            return Ok(None);
        };
        if fact.kind() != UnitReceiverOwnershipKind::SharedLoan
            || place.is_root()
            || self.ssa_types.get(target.index()) != Some(&SsaTypeKind::StringOwner)
        {
            return Ok(None);
        }
        let mut operand = expression.expression();
        while let Expression::Group { expression: inner } = self
            .parsed
            .ast()
            .expressions()
            .get(operand)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, fact.begin_span()))?
            .payload()
        {
            operand = *inner;
        }
        let field =
            self.lower_owned_shared_field_place(operand, place, target, fact.begin_span())?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowBegin {
                    place: field,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(fact.begin_span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, fact.begin_span()))?;
        let EntityId::Loan(loan) = results[0] else {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                fact.begin_span(),
            ));
        };
        Ok(Some(LoweredReceiver {
            entity: EntityId::Loan(loan),
            created_loans: vec![(loan, fact.end_span())],
            writeback: None,
        }))
    }
}
