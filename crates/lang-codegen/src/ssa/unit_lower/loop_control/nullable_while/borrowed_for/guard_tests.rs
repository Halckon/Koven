//! Actual source/SSA checkpoints for the private borrowed-for relation.
use super::*;
use crate::ssa::unit_lower::cfg::carried_control_edge;

impl UnitExpressionLowerer<'_> {
    /// Recover the original context from its actual preheader edge. The helper
    /// creates no block or entity and checks the complete ordered carrier tuple.
    pub(in crate::ssa::unit_lower) fn guard_source_loop_context(
        &mut self,
        header: BlockId,
        live: &[UnitSymbolId],
        pending: Vec<EntityId>,
        proof_scope: Option<UnitStatementId>,
        span: Span,
    ) -> LoopContext {
        let saved = self.loop_state();
        let entry = self.function.entry_block().unwrap();
        let TerminatorKind::Branch(edge) = self
            .function
            .block(entry)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
            .clone()
        else {
            panic!("real source preheader")
        };
        assert_eq!(edge.target, header);
        let mut roots = live.to_vec();
        roots.sort();
        self.block = entry;
        self.bindings = roots
            .iter()
            .zip(&edge.arguments)
            .map(|(root, entity)| {
                let EntityId::Value(value) = entity else {
                    panic!("original named owner/value prefix")
                };
                assert_eq!(
                    self.function.entity(*entity).unwrap().ty,
                    EntityType::Value(self.type_ids[&self.typed.symbol_type(*root).unwrap()])
                );
                (*root, LoweredValue::Value(*value))
            })
            .collect();
        let booleans = self
            .function
            .block(entry)
            .unwrap()
            .parameters
            .iter()
            .filter_map(|entity| match self.function.entity(*entity).unwrap().ty {
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                } if self.ssa_types[target.index()] == SsaTypeKind::Boolean => Some(*entity),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(booleans.len(), self.entry_boolean_roots.len());
        self.borrow_bindings = self
            .entry_boolean_roots
            .iter()
            .zip(booleans)
            .map(|(root, entity)| {
                let EntityId::Loan(loan) = entity else {
                    unreachable!()
                };
                (*root, loan)
            })
            .collect();
        // At these source preheaders all owners are already named. No call,
        // nullable proof, lambda or temporary expression is in progress.
        self.nullable_proofs.clear();
        self.pending_operands = pending;
        self.pending_call_frames.clear();
        assert!(self.temporaries.is_empty());
        assert!(self.capture_loans.is_empty() && self.result_source_loans.is_empty());
        assert!(self.current_receiver.is_none() && self.consumed_receiver.is_none());
        let mut carried = self.carried_bindings(&self.bindings, span).unwrap();
        let mut loans = self.carried_loans(&self.borrow_bindings, span).unwrap();
        self.carry_pending_operands(&mut carried, &mut loans, span)
            .unwrap();
        assert_eq!(carried_control_edge(header, &carried, &loans), edge);
        let context = LoopContext {
            proof_scope,
            borrowed_for_while: None,
            borrowed_while_body: None,
            iteration: None,
            header,
            carried,
            loans,
            entry_temporaries: self.temporaries.keys().copied().collect(),
            entry_pending_count: self.pending_operands.len(),
            entry_receiver: self.current_receiver,
            entry_consumed_receiver: self.consumed_receiver,
            entry_symbols: self.bindings.keys().copied().collect(),
            entry_closure_bindings: self.closure_bindings.clone(),
            continues: Vec::new(),
            breaks: Vec::new(),
        };
        self.restore_loop_state(&saved);
        context
    }
}
