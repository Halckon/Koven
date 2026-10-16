//! Retire a while view absent from another real ordinary-if exit before merging.
use super::*;
use std::collections::BTreeSet;

impl ExpressionLowerer<'_> {
    pub(super) fn retire_non_common_if_while_proofs(
        &mut self,
        exits: &mut [BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        if exits.len() < 2 {
            return Ok(());
        }
        let roots = exits
            .iter()
            .flat_map(|exit| exit.views.keys().copied())
            .collect::<BTreeSet<_>>();
        let mut ending = vec![Vec::new(); exits.len()];
        for root in roots {
            let present = exits
                .iter()
                .enumerate()
                .filter_map(|(index, exit)| exit.views.get(&root).map(|proof| (index, *proof)))
                .collect::<Vec<_>>();
            let first = present[0].1;
            for &(index, proof) in &present {
                self.validate_non_null_proof(root, proof, &exits[index].bindings, span)?;
                if proof.origin != first.origin || proof.inner != first.inner {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            if present.len() == exits.len() {
                continue;
            }
            for (index, proof) in present {
                // Other proof scopes retain their original strict merge contract.
                if !proof.is_while_origin() {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                ending[index].push(root);
            }
        }
        // Validate every actual exit before appending any BorrowEnd. In particular,
        // absence on one path is not evidence that a child on another path ended.
        for (exit, roots) in exits.iter().zip(&ending) {
            if roots.is_empty() {
                continue;
            }
            if !exit.for_sources.is_empty()
                || !exit.for_elements.is_empty()
                || self
                    .function
                    .block(exit.block)
                    .is_none_or(|block| block.terminator.is_some())
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            self.restore_if_proof_exit(exit);
            self.ensure_no_unresolved_while_proof_carriers(span)?;
            for root in roots {
                self.validate_non_null_proof(
                    *root,
                    self.non_null_bindings[root],
                    &self.bindings,
                    span,
                )?;
            }
        }
        for (exit, roots) in exits.iter_mut().zip(ending) {
            if roots.is_empty() {
                continue;
            }
            self.restore_if_proof_exit(exit);
            for root in roots {
                self.end_non_null_proof(root, span)?;
            }
            // Owner and all other exit records remain exactly as published by the
            // branch. The strict merge consumes only these surviving views.
            exit.views.clone_from(&self.non_null_bindings);
        }
        Ok(())
    }

    fn restore_if_proof_exit(&mut self, exit: &BranchExit) {
        self.block = exit.block;
        self.bindings.clone_from(&exit.bindings);
        self.temporaries.clone_from(&exit.temporaries);
        self.pending_call_loans.clone_from(&exit.loans);
        self.capture_loans.clone_from(&exit.capture_loans);
        self.non_null_bindings.clone_from(&exit.views);
        self.borrow_bindings.clone_from(&exit.borrow_bindings);
        self.result_source_loans
            .clone_from(&exit.result_source_loans);
    }
}
