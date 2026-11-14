//! Observed child literal/loan identities, rechecked independently after emission.
use super::*;
use crate::ssa::model::Definition;
pub(super) struct Snapshot {
    literal: (InstructionId, Operation),
    prepare: (InstructionId, Operation),
    borrow: (InstructionId, Operation),
}
fn definition(
    function: &Function,
    entity: EntityId,
    span: Span,
) -> Result<InstructionId, LoweringError> {
    match function
        .entity(entity)
        .ok_or_else(|| invalid(span))?
        .definition
    {
        Definition::InstructionResult {
            instruction,
            index: 0,
        } => Ok(instruction),
        _ => Err(invalid(span)),
    }
}
impl Snapshot {
    pub(super) fn capture(
        function: &Function,
        replace: InstructionId,
        span: Span,
    ) -> Result<Self, LoweringError> {
        let fail = || invalid(span);
        let r = function.instruction(replace).ok_or_else(fail)?;
        let Operation::RootReplace {
            loan, replacement, ..
        } = r.operation
        else {
            return Err(fail());
        };
        let literal = definition(function, EntityId::Value(replacement), span)?;
        let l = function.instruction(literal).ok_or_else(fail)?;
        if l.block != r.block
            || l.results != [EntityId::Value(replacement)]
            || !matches!(l.operation, Operation::StringLiteral { .. })
        {
            return Err(fail());
        }
        let borrow = definition(function, EntityId::Loan(loan), span)?;
        let b = function.instruction(borrow).ok_or_else(fail)?;
        let Operation::BorrowBegin { place, .. } = b.operation else {
            return Err(fail());
        };
        let prepare = definition(function, EntityId::Place(place), span)?;
        let p = function.instruction(prepare).ok_or_else(fail)?;
        Ok(Self {
            literal: (literal, l.operation.clone()),
            prepare: (prepare, p.operation.clone()),
            borrow: (borrow, b.operation.clone()),
        })
    }
    pub(super) fn finish(
        &self,
        function: &Function,
        replace: InstructionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || invalid(span);
        let r = function.instruction(replace).ok_or_else(fail)?;
        let Operation::RootReplace {
            owner,
            loan,
            replacement,
        } = r.operation
        else {
            return Err(fail());
        };
        let (owner, rhs, loan) = (
            EntityId::Value(owner),
            EntityId::Value(replacement),
            EntityId::Loan(loan),
        );
        super::super::nested_resume_receipt::composition_loan::verify(
            function, replace, owner, span,
        )?;
        let observed = Self::capture(function, replace, span)?;
        for (actual, expected) in [
            (&observed.literal, &self.literal),
            (&observed.prepare, &self.prepare),
            (&observed.borrow, &self.borrow),
        ] {
            if actual != expected {
                return Err(fail());
            }
        }
        let at = |id| {
            let placements =
                function
                    .blocks
                    .iter()
                    .flat_map(|candidate| {
                        candidate.instructions.iter().enumerate().filter_map(
                            move |(index, actual)| (*actual == id).then_some((candidate.id, index)),
                        )
                    })
                    .collect::<Vec<_>>();
            match placements.as_slice() {
                [(owner, index)] if *owner == r.block => Ok(*index),
                _ => Err(fail()),
            }
        };
        if at(self.literal.0)? >= at(replace)?
            || r.results.len() != 2
            || [owner, rhs, r.results[0], r.results[1]]
                .into_iter()
                .collect::<BTreeSet<_>>()
                .len()
                != 4
        {
            return Err(fail());
        }
        for i in &function.instructions {
            let uses = i.operation.entities();
            if i.results.contains(&rhs) != (i.id == self.literal.0)
                || uses.iter().filter(|e| **e == rhs).count() != usize::from(i.id == replace)
                || i.results.contains(&owner)
                || uses.iter().filter(|e| **e == owner).count()
                    != usize::from(i.id == self.prepare.0 || i.id == replace)
            {
                return Err(fail());
            }
        }
        for b in &function.blocks {
            let term = &b.terminator.as_ref().ok_or_else(fail)?.kind;
            if b.parameters.contains(&rhs)
                || term.entities().contains(&rhs)
                || term.entities().contains(&owner)
                || b.parameters.iter().filter(|e| **e == owner).count()
                    != usize::from(b.id == r.block)
            {
                return Err(fail());
            }
        }
        // Loan itself must be the originally observed one, not a substitute.
        if definition(function, loan, span)? != self.borrow.0 {
            return Err(fail());
        }
        Ok(())
    }
}
