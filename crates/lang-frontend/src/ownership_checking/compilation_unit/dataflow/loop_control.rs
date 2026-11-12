//! Real loop body checks, repeating-edge diagnostics and origin/resource merging.
use super::{Checker, Flows, OwnershipCheckingError, merge_state};
use crate::{
    ast::StatementId,
    diagnostic::{Diagnostic, Severity},
};
impl Checker<'_> {
    // Only repeating edges require the owner again; exits keep their ordinary flow checks.
    pub(super) fn check_loop_backedge(
        &mut self,
        body: StatementId,
        repeated: StatementId,
        flows: &Flows,
    ) -> Result<(), OwnershipCheckingError> {
        let live = &self.statement_live_after[body.index()];
        let repeated_span = self.parsed.ast().statements().get(repeated)?.span();
        let mut moved = std::collections::BTreeMap::new();
        let mut moved_this = None;
        let mut exclusive_capture = None;
        for state in [&flows.next, &flows.continues].into_iter().flatten() {
            // Repeating edges have no capture-loan fixed point yet. Do not carry
            // a newly formed exclusive environment back to an unchecked header.
            if exclusive_capture.is_none()
                && let Some(loan) = state.loans.iter().find(|loan| {
                    matches!(loan.owner, super::ActiveLoanOwner::Closure(_))
                        && loan.kind == super::LoanKind::Exclusive
                        && loan.origin.source_id() == repeated_span.source_id()
                        && loan.origin.start() >= repeated_span.start()
                        && loan.origin.end() <= repeated_span.end()
                })
            {
                exclusive_capture = Some(loan.origin);
            }
            for (&symbol, &origin) in &state.moved {
                if live.contains(&symbol) {
                    moved.entry(symbol).or_insert(origin);
                }
            }
            // Unlike named roots, this cannot be rebound. Only moves in the repeated
            // region need it again; a for source and moves before the loop run once.
            if let Some(origin) = state.this_moved
                && origin.source_id() == repeated_span.source_id()
                && origin.start() >= repeated_span.start()
                && origin.end() <= repeated_span.end()
            {
                moved_this.get_or_insert(origin);
            }
        }
        if let Some(origin) = exclusive_capture {
            self.emit_borrow_binding_diagnostic(
                crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                "exclusive closure capture remains live on a loop backedge",
                origin,
            )?;
        }
        for origin in moved.into_values().chain(moved_this) {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.use_after_move,
                "moved value may be used again on the next loop iteration",
                origin,
            )?;
            diagnostic.add_label(self.sources, origin, "value was moved here")?;
            self.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn check_maybe_loop(
        &mut self,
        mut prefix: Flows,
        body: StatementId,
        repeated: StatementId,
        errors: usize,
        headers: &[(
            super::UnitSymbolId,
            crate::ownership_checking::callable_provenance::graph::NodeId,
        )],
        can_skip: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let Some(base) = prefix.next.take() else {
            return Ok(prefix);
        };
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        let research = self.research_loop_start(Some(repeated), body, &base);
        let body_id = body;
        let body = self.check_statement(body, base.clone())?;
        if self.diagnostics.len() == errors {
            self.check_loop_backedge(body_id, repeated, &body)?;
            self.check_loop_capture_transport(repeated, body_id, &base, &body)?;
        }
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        self.research_loop_finish(research, &body);
        if !can_skip {
            for state in [&body.next, &body.continues].into_iter().flatten() {
                state.origins.backedge(headers);
            }
            prefix.next = body.breaks;
            return Ok(prefix);
        }
        let origins = base.origins.loop_exit(
            headers,
            body.next.as_ref().map(|state| &state.origins),
            body.continues.as_ref().map(|state| &state.origins),
            body.breaks.as_ref().map(|state| &state.origins),
        );
        let mut next = base;
        for state in [body.next, body.breaks, body.continues]
            .into_iter()
            .flatten()
        {
            merge_state(&mut next, state);
        }
        next.origins = origins;
        prefix.next = Some(next);
        Ok(prefix)
    }
}
