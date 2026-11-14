//! Nullable subjects reuse the ordinary When CFG and existing nullable SSA ABI.
use lang_frontend::{
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    source::Span,
    type_checking::UnitExpressionId,
};

use super::{Builder, Lowered, View, WhenSubject};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::{
        body::{BlockRef, ROp},
        nullable_when,
    },
};

impl Builder<'_, '_> {
    pub(super) fn nullable_when_subject(
        &mut self,
        control: UnitExpressionId,
    ) -> Result<Option<WhenSubject>, LoweringError> {
        let inputs = self.inputs;
        let source = nullable_when::source(inputs, control)?;
        let descriptor = source.plan.descriptor();
        let span = self.span(control.expression());
        let leaf = source
            .operands
            .last()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        // Leave the subject-chain AfterExpression points to both actual successors.
        // The call/constructor's children still execute normally, exactly once.
        let owner = match self.expression_inner(leaf.expression())? {
            Lowered::Value(owner) => owner,
            Lowered::Diverged => return Ok(None),
            Lowered::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let nullable = self.ssa_type(self.c(descriptor.nullable_type()), span)?;
        let inner = self.ssa_type(self.c(descriptor.inner_type()), span)?;
        if self.body.slots.get(owner.0) != Some(&EntityType::Value(nullable)) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if let Some(root) = descriptor.stable_symbol() {
            if self.borrowed.contains_key(&root)
                || self.bindings.get(&root) != Some(&owner)
                || self.active_view(root).is_some()
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        } else {
            for operand in &source.operands {
                if self
                    .temporaries
                    .get(operand)
                    .is_some_and(|&actual| actual != owner)
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                self.temporaries.insert(*operand, owner);
            }
        }
        let proof = self.body.slot(EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        });
        Ok(Some(WhenSubject::Nullable {
            control,
            owner,
            proof,
        }))
    }

    pub(super) fn nullable_when_operand_drops(
        &self,
        subject: Option<WhenSubject>,
    ) -> Result<Vec<(usize, UnitDropTarget, Span)>, LoweringError> {
        let Some(WhenSubject::Nullable { control, .. }) = subject else {
            return Ok(Vec::new());
        };
        let source = nullable_when::source(self.inputs, control)?;
        let mut facts = Vec::new();
        for &operand in source.operands.iter().rev() {
            facts.extend(self.collect_drops(UnitDropPoint::AfterExpression(operand)));
        }
        Ok(facts)
    }

    pub(super) fn enter_nullable_when_arm(
        &mut self,
        subject: Option<WhenSubject>,
        index: usize,
        target: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(WhenSubject::Nullable {
            control,
            owner,
            proof,
        }) = subject
        else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        if index != 1 {
            return Ok(());
        }
        let inputs = self.inputs;
        let source = nullable_when::source(inputs, control)?;
        if let Some(symbol) = source.plan.descriptor().stable_symbol() {
            if self.bindings.get(&symbol) != Some(&owner)
                || self
                    .active_views
                    .iter()
                    .any(|view| view.when_scope == Some((control, owner)))
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.active_views.push(View {
                symbol,
                slot: proof,
                target,
                when_scope: Some((control, owner)),
            });
        } else {
            // A temporary has no symbol/use witness. End its unused proof, retaining
            // the actual owner until the published full-expression cleanup point.
            self.push(ROp::BorrowEnd { loan: proof }, Vec::new(), span)?;
        }
        Ok(())
    }
}
