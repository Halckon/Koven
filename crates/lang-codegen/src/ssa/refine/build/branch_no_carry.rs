//! Real resource snapshots for each selected arm and reached loop boundary.
use super::*;
use lang_frontend::ownership_checking::UnitLoopBranchNoCarryFact;
pub(super) struct BranchLease {
    proof: UnitLoopBranchNoCarryFact,
    borrowed: BTreeMap<UnitSymbolId, Slot>,
    sources: BTreeMap<UnitSymbolId, Vec<Slot>>,
    pending: Vec<Slot>,
}
impl Builder<'_, '_> {
    pub(super) fn arm_lease(
        &self,
        proof: &UnitLoopBranchNoCarryFact,
        span: Span,
    ) -> Result<BranchLease, LoweringError> {
        if !self.inputs.owned.owns_no_carry_branch(proof)
            || !self.borrowed.contains_key(&proof.arms()[0].source())
            || !self.pending_call_loans.is_empty()
            || proof.arms().iter().any(|p| {
                self.borrowed.contains_key(&p.binding())
                    || self.result_source_loans.contains_key(&p.binding())
            })
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(BranchLease {
            proof: proof.clone(),
            borrowed: self.borrowed.clone(),
            sources: self.result_source_loans.clone(),
            pending: self.pending_call_loans.clone(),
        })
    }
    pub(super) fn arm_loop_lease(
        &self,
        statement: StatementId,
        span: Span,
    ) -> Result<Option<BranchLease>, LoweringError> {
        let id = UnitStatementId::new(self.inputs.source_unit, statement);
        let mut selected = self
            .inputs
            .owned
            .no_carry_branches()
            .iter()
            .filter(|p| p.arms()[0].loop_statement() == id);
        let Some(p) = selected.next() else {
            return Ok(None);
        };
        if selected.next().is_some() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.arm_lease(p, span).map(Some)
    }
    pub(super) fn arm_if_lease(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<BranchLease>, LoweringError> {
        let id = self.unit(expression);
        let mut selected = self
            .inputs
            .owned
            .no_carry_branches()
            .iter()
            .filter(|p| p.branch() == id);
        let Some(p) = selected.next() else {
            return Ok(None);
        };
        if selected.next().is_some() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.arm_lease(p, span).map(Some)
    }
    pub(super) fn arm_boundary(
        &self,
        lease: Option<&BranchLease>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(l) = lease else { return Ok(()) };
        if self.borrowed != l.borrowed
            || self.result_source_loans != l.sources
            || self.pending_call_loans != l.pending
            || l.proof.arms().iter().any(|p| {
                self.dead.contains(&p.source())
                    || self.borrowed.contains_key(&p.binding())
                    || self.result_source_loans.contains_key(&p.binding())
            })
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }
    pub(super) fn arm_loop_begin(
        &mut self,
        statement: UnitStatementId,
        blocks: [BlockRef; 5],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = self
            .body
            .arm_loop_plans
            .loops
            .iter_mut()
            .find(|p| p.fact.arms()[0].loop_statement() == statement)
        else {
            return Ok(());
        };
        if p.blocks.is_some() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        p.source = Some(
            *self
                .borrowed
                .get(&p.fact.arms()[0].source())
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?,
        );
        p.blocks = Some(blocks);
        Ok(())
    }
}
