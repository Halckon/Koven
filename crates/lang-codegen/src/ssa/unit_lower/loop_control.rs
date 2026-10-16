//! compilation-unit loop 的 owner-aware 回边、jump 与共同退出 lowering。

mod iteration;
mod iteration_cleanup;
mod nullable_while;
mod numeric_iteration;

pub(super) use nullable_while::DirectNullableWhileEntry;
use nullable_while::NullableWhileEntry;

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};

use super::{
    LoweredValue, UnitExpressionLowerer,
    cfg::{BranchExit, CarriedAccess, CarriedBinding, carried_control_edge},
    lowering_error,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{BlockId, Origin, TerminatorKind},
};

struct LoopJump {
    state: BranchExit,
    span: Span,
}

pub(super) struct LoopContext {
    proof_scope: Option<UnitStatementId>,
    iteration: Option<iteration::IterationContext>,
    header: BlockId,
    carried: Vec<CarriedBinding>,
    loans: Vec<CarriedAccess>,
    entry_temporaries: BTreeSet<UnitExpressionId>,
    entry_pending_count: usize,
    entry_receiver: Option<super::ReceiverBinding>,
    entry_consumed_receiver: Option<super::ConsumedReceiver>,
    entry_symbols: BTreeSet<UnitSymbolId>,
    entry_closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
    continues: Vec<LoopJump>,
    breaks: Vec<LoopJump>,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn current_while_proof_scope(&self) -> Option<UnitStatementId> {
        self.loops.last().and_then(|context| context.proof_scope)
    }

    pub(super) fn has_iteration_context(&self) -> bool {
        self.loops.iter().any(|context| context.iteration.is_some())
    }

    pub(super) fn lower_while(
        &mut self,
        statement: StatementId,
        condition: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.direct_nullable_while_condition(condition, span)?;
        let inherited = if let Some((descriptor, subject)) = descriptor {
            match self.prepare_nullable_while_entry(statement, descriptor, subject, span)? {
                NullableWhileEntry::Readonly(root, proof) => Some((root, proof)),
                NullableWhileEntry::Direct(entry) => {
                    return self
                        .lower_direct_nullable_while(statement, condition, body, entry, span);
                }
            }
        } else {
            self.reject_nested_while_proof(span)?;
            None
        };
        if !self.supports_control_prefix(span) && !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let baseline = self.bindings.clone();
        let baseline_closures = self.closure_bindings.clone();
        let context = if inherited.is_some() {
            // Only the qualified same-root readonly while may carry an existing proof.
            self.create_carried_loop_context(&baseline, span)?
        } else {
            self.create_loop_context(&baseline, span)?
        };
        let header = context.header;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(carried_control_edge(
                    header,
                    &context.carried,
                    &context.loans,
                )),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        self.block = header;
        self.current_receiver = context.entry_receiver;
        self.consumed_receiver = context.entry_consumed_receiver;
        self.bindings = self.rebind_loop(&baseline, header, &context, span)?;
        self.closure_bindings = baseline_closures;
        let condition = self.require_expression_value(condition)?;
        if !self.supports_control_prefix(span) && !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let condition_state = self.loop_state();
        let body_block = self.add_carried_control_block(
            &context.carried,
            &context.loans,
            self.statement_span(body)?,
        )?;
        let false_block = self.add_carried_control_block(&context.carried, &context.loans, span)?;
        let when_true = self.carried_edge_from(
            body_block,
            &context.carried,
            &context.loans,
            &condition_state,
            span,
        )?;
        let when_false = self.carried_edge_from(
            false_block,
            &context.carried,
            &context.loans,
            &condition_state,
            span,
        )?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition,
                    when_true,
                    when_false,
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        // Both successors start from the complete condition state, before body
        // lowering can consume an owner or retire a proof on a sibling path.
        self.restore_loop_state(&condition_state);
        self.block = false_block;
        self.bindings = self.rebind_loop(&condition_state.bindings, false_block, &context, span)?;
        let false_exit = self.loop_state();

        self.restore_loop_state(&condition_state);
        self.block = body_block;
        self.bindings = self.rebind_loop(&condition_state.bindings, body_block, &context, span)?;
        self.loops.push(context);
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self.loops.pop().expect("while context must be balanced");
        if let Some((root, proof)) = inherited {
            for jump in &context.continues {
                self.validate_readonly_while_exit(root, proof, &jump.state, jump.span)?;
            }
        }
        self.finish_continues(&context, context.header)?;

        let mut exits = context
            .breaks
            .into_iter()
            .map(branch_exit)
            .collect::<Vec<_>>();
        exits.push(false_exit);
        if let Some((root, proof)) = inherited {
            // Consume each exit's own facts, never a reconstruction from entry or
            // the state left behind by another exit, before merging inherited pairs.
            for exit in &mut exits {
                self.validate_readonly_while_exit(root, proof, exit, span)?;
                self.restore_loop_state(exit);
                self.emit_loop_exit(statement)?;
                *exit = self.loop_state();
                self.validate_readonly_while_exit(root, proof, exit, span)?;
            }
        }
        self.merge_loop_exits(exits, span)?;
        if let Some((root, proof)) = inherited {
            self.validate_readonly_while_exit(root, proof, &self.loop_state(), span)?;
        } else {
            self.emit_loop_exit(statement)?;
        }
        Ok(LoweredValue::Unit)
    }

    pub(super) fn lower_loop(
        &mut self,
        statement: StatementId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.reject_nested_while_proof(span)?;
        if !self.supports_control_prefix(span) && !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let baseline = self.bindings.clone();
        let baseline_closures = self.closure_bindings.clone();
        let context = self.create_loop_context(&baseline, span)?;
        let header = context.header;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(carried_control_edge(
                    header,
                    &context.carried,
                    &context.loans,
                )),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = header;
        self.current_receiver = context.entry_receiver;
        self.consumed_receiver = context.entry_consumed_receiver;
        self.bindings = self.rebind_loop(&baseline, header, &context, span)?;
        self.closure_bindings = baseline_closures;
        self.loops.push(context);
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self.loops.pop().expect("loop context must be balanced");
        self.finish_continues(&context, context.header)?;
        if context.breaks.is_empty() {
            self.current_receiver = None;
            self.consumed_receiver = None;
            self.bindings.clear();
            self.borrow_bindings.clear();
            self.nullable_proofs.clear();
            self.closure_bindings.clear();
            self.capture_loans.clear();
            self.temporaries.clear();
            self.pending_operands.clear();
            return Ok(LoweredValue::Diverged);
        }
        self.merge_loop_exits(context.breaks.into_iter().map(branch_exit).collect(), span)?;
        self.emit_loop_exit(statement)?;
        Ok(LoweredValue::Unit)
    }

    pub(super) fn lower_break(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if self
            .owned
            .iteration_cleanup_at(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                self.source_unit,
                expression,
            )))
            .is_none()
        {
            self.end_pending_call_loans(self.loops.len(), span)?;
        } else {
            self.end_pending_abi_call_slots(self.loops.len(), span)?;
        }
        self.emit_drops(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))?;
        self.retire_while_proofs(false, span)?;
        self.prepare_jump(span, true)?;
        let jump = self.current_jump(span)?;
        self.loops
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?
            .breaks
            .push(jump);
        Ok(LoweredValue::Diverged)
    }

    pub(super) fn lower_continue(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if self
            .owned
            .iteration_cleanup_at(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                self.source_unit,
                expression,
            )))
            .is_none()
        {
            self.end_pending_call_loans(self.loops.len(), span)?;
        } else {
            self.end_pending_abi_call_slots(self.loops.len(), span)?;
        }
        self.emit_drops(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))?;
        self.advance_iteration(span)?;
        self.retire_while_proofs(false, span)?;
        self.prepare_jump(span, false)?;
        let jump = self.current_jump(span)?;
        self.loops
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?
            .continues
            .push(jump);
        Ok(LoweredValue::Diverged)
    }

    fn create_loop_context(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LoopContext, LoweringError> {
        self.reject_nested_while_proof(span)?;
        self.create_carried_loop_context(baseline, span)
    }

    // Shared construction only. The old wrapper keeps its rejection for loop/for
    // and unqualified proof births; nullable while qualifies an existing proof first.
    fn create_carried_loop_context(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LoopContext, LoweringError> {
        let mut carried = self.carried_bindings(baseline, span)?;
        let mut loans = self.carried_loans(&self.borrow_bindings, span)?;
        self.carry_pending_operands(&mut carried, &mut loans, span)?;
        self.separate_loop_pending_copies(&mut carried, span)?;
        let header = self.add_carried_control_block(&carried, &loans, span)?;
        Ok(LoopContext {
            proof_scope: None,
            iteration: None,
            header,
            carried,
            loans,
            entry_temporaries: self.temporaries.keys().copied().collect(),
            entry_pending_count: self.pending_operands.len(),
            entry_receiver: self.current_receiver,
            entry_consumed_receiver: self.consumed_receiver,
            entry_symbols: baseline.keys().copied().collect(),
            entry_closure_bindings: self.closure_bindings.clone(),
            continues: Vec::new(),
            breaks: Vec::new(),
        })
    }

    fn prepare_jump(&mut self, span: Span, breaking: bool) -> Result<(), LoweringError> {
        let entry_symbols = self
            .loops
            .last()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?
            .entry_symbols
            .clone();
        let context = self
            .loops
            .last()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (temporaries, pending_count) = match (&context.iteration, breaking) {
            (Some(iteration), true) => (&iteration.outer_temporaries, iteration.retained_start),
            _ => (&context.entry_temporaries, context.entry_pending_count),
        };
        if self
            .temporaries
            .keys()
            .copied()
            .ne(temporaries.iter().copied())
            || self.pending_operands.len() < pending_count
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        // Continue retains this provider's state; break keeps only its outer frame
        // after consuming any source-loan and hidden-owner cleanup.
        self.pending_operands.truncate(pending_count);
        self.discard_non_entry_bindings(&entry_symbols, span)
    }

    fn current_jump(&self, span: Span) -> Result<LoopJump, LoweringError> {
        if self.loops.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(LoopJump {
            state: self.loop_state(),
            span,
        })
    }

    fn loop_state(&self) -> BranchExit {
        BranchExit {
            block: self.block,
            result: LoweredValue::Unit,
            receiver: self.current_receiver,
            consumed_receiver: self.consumed_receiver,
            bindings: self.bindings.clone(),
            borrow_bindings: self.borrow_bindings.clone(),
            nullable_proofs: self.nullable_proofs.clone(),
            closure_bindings: self.closure_bindings.clone(),
            capture_loans: self.capture_loans.clone(),
            result_source_loans: self.result_source_loans.clone(),
            pending_operands: self.pending_operands.clone(),
            pending_call_frames: self.pending_call_frames.clone(),
            temporaries: self.temporaries.clone(),
        }
    }

    fn restore_loop_state(&mut self, state: &BranchExit) {
        self.block = state.block;
        self.bindings.clone_from(&state.bindings);
        self.borrow_bindings.clone_from(&state.borrow_bindings);
        self.nullable_proofs.clone_from(&state.nullable_proofs);
        self.current_receiver = state.receiver;
        self.consumed_receiver = state.consumed_receiver;
        self.closure_bindings.clone_from(&state.closure_bindings);
        self.capture_loans.clone_from(&state.capture_loans);
        self.result_source_loans
            .clone_from(&state.result_source_loans);
        self.pending_operands.clone_from(&state.pending_operands);
        self.pending_call_frames
            .clone_from(&state.pending_call_frames);
        self.temporaries.clone_from(&state.temporaries);
    }

    fn rebind_loop(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        context: &LoopContext,
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        let bindings =
            self.rebind_carried_control(baseline, block, &context.carried, &context.loans, span)?;
        self.borrow_bindings =
            self.rebind_carried_loans(block, context.carried.len(), &context.loans, span)?;
        Ok(bindings)
    }

    fn record_natural_continue(&mut self, span: Span) -> Result<(), LoweringError> {
        self.advance_iteration(span)?;
        self.retire_while_proofs(false, span)?;
        self.prepare_jump(span, false)?;
        let jump = self.current_jump(span)?;
        self.loops
            .last_mut()
            .expect("natural continue has a loop context")
            .continues
            .push(jump);
        Ok(())
    }

    fn finish_continues(
        &mut self,
        context: &LoopContext,
        target: BlockId,
    ) -> Result<(), LoweringError> {
        for jump in &context.continues {
            if jump.state.closure_bindings != context.entry_closure_bindings {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    jump.span,
                ));
            }
            if jump.state.consumed_receiver != context.entry_consumed_receiver {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    jump.span,
                ));
            }
            let edge = self.carried_edge_from(
                target,
                &context.carried,
                &context.loans,
                &jump.state,
                jump.span,
            )?;
            self.function
                .set_terminator(
                    jump.state.block,
                    TerminatorKind::Branch(edge),
                    Origin::Source(jump.span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, jump.span))?;
        }
        Ok(())
    }

    fn emit_loop_exit(&mut self, statement: StatementId) -> Result<(), LoweringError> {
        let point = UnitDropPoint::LoopExit(UnitStatementId::new(self.source_unit, statement));
        self.emit_loop_exit_drops(point, self.statement_span(statement)?)
    }

    fn merge_loop_exits(
        &mut self,
        exits: Vec<BranchExit>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(first) = exits.first() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        if exits.iter().skip(1).any(|exit| {
            exit.bindings.keys().collect::<Vec<_>>() != first.bindings.keys().collect::<Vec<_>>()
                || exit.bindings.iter().any(|(symbol, binding)| {
                    first.bindings.get(symbol).is_none_or(|first| {
                        std::mem::discriminant(first) != std::mem::discriminant(binding)
                    })
                })
        }) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.merge_unit_exits(exits, span)?;
        Ok(())
    }
}

fn branch_exit(jump: LoopJump) -> BranchExit {
    jump.state
}
