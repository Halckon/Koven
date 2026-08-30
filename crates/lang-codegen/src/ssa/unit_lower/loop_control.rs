//! compilation-unit loop 的 owner-aware 回边、jump 与共同退出 lowering。

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
    cfg::{CarriedBinding, carried_edge},
    control::BranchExit,
    lowering_error,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{BlockId, Origin, TerminatorKind},
};

struct LoopJump {
    block: BlockId,
    bindings: BTreeMap<UnitSymbolId, LoweredValue>,
    closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
    span: Span,
}

pub(super) struct LoopContext {
    header: BlockId,
    carried: Vec<CarriedBinding>,
    entry_symbols: BTreeSet<UnitSymbolId>,
    entry_closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
    continues: Vec<LoopJump>,
    breaks: Vec<LoopJump>,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_while(
        &mut self,
        statement: StatementId,
        condition: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let baseline = self.bindings.clone();
        let baseline_closures = self.closure_bindings.clone();
        let context = self.create_loop_context(&baseline, span)?;
        let header = context.header;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(carried_edge(header, &context.carried)),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        self.block = header;
        self.bindings = self.rebind_carried(&baseline, header, &context.carried, span)?;
        self.closure_bindings = baseline_closures;
        let condition = self.require_expression_value(condition)?;
        if !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let condition_bindings = self.bindings.clone();
        let condition_closures = self.closure_bindings.clone();
        let body_block = self.add_carried_block(&context.carried, self.statement_span(body)?)?;
        let false_block = self.add_carried_block(&context.carried, span)?;
        let when_true =
            self.carried_edge_from(body_block, &context.carried, &condition_bindings, span)?;
        let when_false =
            self.carried_edge_from(false_block, &context.carried, &condition_bindings, span)?;
        self.function
            .set_terminator(
                header,
                TerminatorKind::Conditional {
                    condition,
                    when_true,
                    when_false,
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        self.loops.push(context);
        self.block = body_block;
        let carried = &self.loops.last().expect("while context exists").carried;
        self.bindings = self.rebind_carried(&condition_bindings, body_block, carried, span)?;
        self.closure_bindings = condition_closures.clone();
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self.loops.pop().expect("while context must be balanced");
        self.finish_continues(&context)?;

        let false_bindings =
            self.rebind_carried(&condition_bindings, false_block, &context.carried, span)?;
        let mut exits = context
            .breaks
            .into_iter()
            .map(branch_exit)
            .collect::<Vec<_>>();
        exits.push(BranchExit {
            block: false_block,
            result: LoweredValue::Unit,
            bindings: false_bindings,
            closure_bindings: condition_closures,
        });
        self.merge_loop_exits(exits, span)?;
        self.emit_loop_exit(statement)?;
        Ok(LoweredValue::Unit)
    }

    pub(super) fn lower_loop(
        &mut self,
        statement: StatementId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let baseline = self.bindings.clone();
        let baseline_closures = self.closure_bindings.clone();
        let context = self.create_loop_context(&baseline, span)?;
        let header = context.header;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(carried_edge(header, &context.carried)),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = header;
        self.bindings = self.rebind_carried(&baseline, header, &context.carried, span)?;
        self.closure_bindings = baseline_closures;
        self.loops.push(context);
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self.loops.pop().expect("loop context must be balanced");
        self.finish_continues(&context)?;
        if context.breaks.is_empty() {
            self.bindings.clear();
            self.closure_bindings.clear();
            self.temporaries.clear();
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
        self.emit_drops(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))?;
        self.prepare_jump(span)?;
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
        self.emit_drops(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
            self.source_unit,
            expression,
        )))?;
        self.prepare_jump(span)?;
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
        let carried = self.carried_bindings(baseline, span)?;
        let header = self.add_carried_block(&carried, span)?;
        Ok(LoopContext {
            header,
            carried,
            entry_symbols: baseline.keys().copied().collect(),
            entry_closure_bindings: self.closure_bindings.clone(),
            continues: Vec::new(),
            breaks: Vec::new(),
        })
    }

    fn prepare_jump(&mut self, span: Span) -> Result<(), LoweringError> {
        let entry_symbols = self
            .loops
            .last()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?
            .entry_symbols
            .clone();
        if !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.discard_non_entry_bindings(&entry_symbols, span)
    }

    fn current_jump(&self, span: Span) -> Result<LoopJump, LoweringError> {
        if self.loops.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(LoopJump {
            block: self.block,
            bindings: self.bindings.clone(),
            closure_bindings: self.closure_bindings.clone(),
            span,
        })
    }

    fn record_natural_continue(&mut self, span: Span) -> Result<(), LoweringError> {
        self.prepare_jump(span)?;
        let jump = self.current_jump(span)?;
        self.loops
            .last_mut()
            .expect("natural continue has a loop context")
            .continues
            .push(jump);
        Ok(())
    }

    fn finish_continues(&mut self, context: &LoopContext) -> Result<(), LoweringError> {
        for jump in &context.continues {
            if jump.closure_bindings != context.entry_closure_bindings {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    jump.span,
                ));
            }
            let edge = self.carried_edge_from(
                context.header,
                &context.carried,
                &jump.bindings,
                jump.span,
            )?;
            self.function
                .set_terminator(
                    jump.block,
                    TerminatorKind::Branch(edge),
                    Origin::Source(jump.span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, jump.span))?;
        }
        Ok(())
    }

    fn emit_loop_exit(&mut self, statement: StatementId) -> Result<(), LoweringError> {
        let point = UnitDropPoint::LoopExit(UnitStatementId::new(self.source_unit, statement));
        self.temporaries.clear();
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
    BranchExit {
        block: jump.block,
        result: LoweredValue::Unit,
        bindings: jump.bindings,
        closure_bindings: jump.closure_bindings,
    }
}
