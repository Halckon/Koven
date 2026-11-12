//! Dynamic while CFG, reached-edge cleanup and independent lifetime proof boundaries.
use super::{Builder, LoopFrame, Lowered};
use crate::ssa::lowering_support::{LoweringError, LoweringErrorKind, error};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::UnitDropPoint,
    source::Span,
    type_checking::UnitStatementId,
};

impl Builder<'_, '_> {
    pub(super) fn while_loop(
        &mut self,
        statement: StatementId,
        condition: ExpressionId,
        loop_body: StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let invariant = self.invariant_while_entry(statement, span)?;
        let no_carry = self.no_carry_entry(statement, span)?;
        for fact in self.inputs.owned.borrow_results().bindings() {
            if fact.binding().source_unit() == self.inputs.source_unit
                && crate::ssa::borrow_result_support::closed_while::following(
                    self.inputs.parsed,
                    self.inputs.typed,
                    self.inputs.owned,
                    fact,
                )
                .contains(&statement)
                && (self.borrowed.contains_key(&fact.binding())
                    || self.result_source_loans.contains_key(&fact.binding())
                    || !self.pending_call_loans.is_empty())
            {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
        }
        let statement = UnitStatementId::new(self.inputs.source_unit, statement);
        self.end_reassigned_loop_views(span)?;
        let header = self.body.block();
        let body_block = self.body.block();
        let finished = self.body.block();
        let exit = self.body.block();
        self.fresh_loop_entry(
            statement,
            [self.current(span)?, header, body_block, finished, exit],
            span,
        )?;
        self.loop_receipt_entry(
            invariant.as_ref(),
            [self.current(span)?, header, body_block, finished, exit],
            span,
        )?;
        self.header_while(
            statement,
            self.unit(condition),
            UnitStatementId::new(self.inputs.source_unit, loop_body),
            [header, body_block, finished, exit],
            span,
        )?;
        self.open_loop_while(
            statement,
            self.unit(condition),
            UnitStatementId::new(self.inputs.source_unit, loop_body),
            [header, body_block, finished, exit],
            span,
        )?;
        self.owner_while(
            statement,
            self.unit(condition),
            UnitStatementId::new(self.inputs.source_unit, loop_body),
            finished,
            exit,
            span,
        )?;
        self.goto(header, span)?;
        self.block = Some(header);
        self.condition(condition, body_block, finished)?;
        self.no_carry_boundary(no_carry.as_ref(), span)?;
        self.invariant_while_boundary(invariant.as_ref(), span)?;
        self.loop_receipt_boundary(invariant.as_ref(), 2, span)?;
        self.fresh_loop_boundary(2, span)?;
        if self.body.blocks[body_block.0].has_predecessor {
            let mark = self.enter(body_block);
            self.loops.push(LoopFrame {
                statement,
                continue_target: header,
                exit,
                views: mark,
                loans: self.pending_call_loans.len(),
                broken: false,
                seq_for: None,
                map_for: None,
            });
            let reached = self.statement(loop_body)? != Lowered::Diverged;
            if no_carry.is_some() && !reached {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            self.no_carry_boundary(no_carry.as_ref(), span)?;
            if reached {
                self.fresh_loop_boundary(3, span)?;
            }
            self.loops.pop();
            self.leave(mark, span)?;
            self.invariant_while_body_end(invariant.as_ref(), body_block, reached, span)?;
            if reached {
                self.loop_receipt_boundary(invariant.as_ref(), 3, span)?;
                let cb = self.cb_backedge(statement, header, span)?;
                let cr = self.cr_backedge(statement, header, span)?;
                self.goto(header, span)?;
                self.cb_backedge_emitted(cb, span)?;
                self.cr_backedge_emitted(cr, span)?;
            }
        } else {
            let span = self.statement_span(loop_body)?;
            self.unreachable_drop_scopes.push(span);
        }
        let exit_point = UnitDropPoint::LoopExit(statement);
        let facts = self.collect_drops(exit_point);
        if self.body.blocks[finished.0].has_predecessor {
            // The condition-false edge drops here; each break drops on its own path.
            let mark = self.enter(finished);
            self.leave_unclaimed(mark, span)?;
            self.emit_drops(&facts)?;
            self.invariant_while_boundary(invariant.as_ref(), span)?;
            self.loop_receipt_boundary(invariant.as_ref(), 4, span)?;
            self.fresh_loop_boundary(4, span)?;
            self.no_carry_boundary(no_carry.as_ref(), span)?;
            self.owner_false_edge(statement, exit, span)?;
            self.goto(exit, span)?;
        }
        if !self.body.blocks[exit.0].has_predecessor {
            self.unreachable_drop_points.push(exit_point);
            return Ok(Lowered::Diverged);
        }
        self.finish_drops(&facts);
        let mark = self.enter(exit);
        self.leave_unclaimed(mark, span)?;
        self.invariant_while_boundary(invariant.as_ref(), span)?;
        self.loop_receipt_boundary(invariant.as_ref(), 5, span)?;
        self.fresh_loop_boundary(5, span)?;
        self.no_carry_boundary(no_carry.as_ref(), span)?;
        Ok(Lowered::Unit)
    }
}
