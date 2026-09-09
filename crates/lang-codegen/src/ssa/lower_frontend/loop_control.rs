//! `while` / `loop` 的 backedge、jump 与 loop-carried binding lowering。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    source::Span,
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind,
    control::{BranchExit, LinearBindingSlot, LinearBindings},
    error,
};
use crate::ssa::model::{BlockId, Edge, EntityId, Origin, TerminatorKind};

struct LoopJump {
    exit: BranchExit,
    span: Span,
}

pub(super) struct LoopContext {
    pub(super) entry_views: BTreeMap<SymbolId, super::LoanId>,
    header: BlockId,
    carried: LinearBindings,
    continues: Vec<LoopJump>,
    breaks: Vec<BranchExit>,
}

impl ExpressionLowerer<'_> {
    pub(super) fn lower_while(
        &mut self,
        statement: StatementId,
        condition: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let baseline = self.bindings.clone();
        let context = self.create_loop_header(&baseline, span)?;
        let condition = self.require_value(condition)?;
        let condition_bindings = self.bindings.clone();
        let carried = self.linear_binding_slots(&condition_bindings, span)?;
        let parameter_types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
        let body_block = self
            .function
            .add_block(parameter_types.clone(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let false_block = self
            .function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition,
                    when_true: loop_edge(body_block, &carried),
                    when_false: loop_edge(false_block, &carried),
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        // Save the false edge before body lowering changes any owner or proof state.
        self.block = false_block;
        self.bindings =
            self.rebind_linear_bindings(&condition_bindings, false_block, &carried, span)?;
        let false_exit = self.loop_exit();
        self.block = body_block;
        self.bindings =
            self.rebind_linear_bindings(&condition_bindings, body_block, &carried, span)?;
        self.loops.push(context);
        let body_result = self.lower_statement(body)?;
        if !matches!(body_result, LoweredValue::Diverged) {
            self.record_continue(span)?;
        }
        let context = self.loops.pop().expect("while context must be balanced");
        self.finish_loop_continues(&context)?;
        let mut exits = context.breaks;
        exits.push(false_exit);
        self.finish_loop_exits(statement, exits, &baseline, span)
    }

    pub(super) fn lower_loop(
        &mut self,
        statement: StatementId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let baseline = self.bindings.clone();
        let context = self.create_loop_header(&baseline, span)?;
        self.loops.push(context);
        let body_result = self.lower_statement(body)?;
        if !matches!(body_result, LoweredValue::Diverged) {
            self.record_continue(span)?;
        }
        let context = self.loops.pop().expect("loop context must be balanced");
        self.finish_loop_continues(&context)?;
        self.finish_loop_exits(statement, context.breaks, &baseline, span)
    }

    fn finish_loop_exits(
        &mut self,
        statement: StatementId,
        mut exits: Vec<BranchExit>,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        // Different paths may already have consumed an owner; discharge each exit before joining.
        for exit in &mut exits {
            self.block = exit.block;
            self.bindings.clone_from(&exit.bindings);
            self.temporaries.clone_from(&exit.temporaries);
            self.pending_call_loans.clone_from(&exit.loans);
            self.non_null_bindings.clone_from(&exit.views);
            self.emit_drops(lang_frontend::ownership_checking::DropPoint::LoopExit(
                statement,
            ))?;
            *exit = self.loop_exit();
        }
        self.merge_exits(exits, baseline, span)
    }

    fn loop_exit(&self) -> BranchExit {
        BranchExit {
            block: self.block,
            result: LoweredValue::Unit,
            bindings: self.bindings.clone(),
            temporaries: self.temporaries.clone(),
            loans: self.pending_call_loans.clone(),
            views: self.non_null_bindings.clone(),
        }
    }

    pub(super) fn lower_break(&mut self, span: Span) -> Result<LoweredValue, LoweringError> {
        let exit = self.loop_exit();
        let context = self
            .loops
            .last_mut()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        context.breaks.push(exit);
        Ok(LoweredValue::Diverged)
    }

    pub(super) fn lower_continue(&mut self, span: Span) -> Result<LoweredValue, LoweringError> {
        self.record_continue(span)?;
        Ok(LoweredValue::Diverged)
    }

    fn record_continue(&mut self, span: Span) -> Result<(), LoweringError> {
        let jump = LoopJump {
            exit: self.loop_exit(),
            span,
        };
        let context = self
            .loops
            .last_mut()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        context.continues.push(jump);
        Ok(())
    }

    fn finish_loop_continues(&mut self, context: &LoopContext) -> Result<(), LoweringError> {
        for jump in &context.continues {
            let arguments = context
                .carried
                .slots
                .iter()
                .map(|slot| loop_slot_entity(slot, &jump.exit, jump.span))
                .collect::<Result<Vec<_>, _>>()?;
            self.function
                .set_terminator(
                    jump.exit.block,
                    TerminatorKind::Branch(Edge {
                        target: context.header,
                        arguments,
                    }),
                    Origin::Source(jump.span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, jump.span))?;
        }
        Ok(())
    }

    fn create_loop_header(
        &mut self,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LoopContext, LoweringError> {
        let mut carried = self.linear_binding_slots(baseline, span)?;
        // Scalar bindings may change on each iteration, so loops carry them too.
        for (&symbol, &binding) in baseline {
            match binding {
                LoweredValue::Unit => {}
                LoweredValue::Value(value) => {
                    if carried.slots.iter().any(|slot| slot.symbol == Some(symbol)) {
                        continue;
                    }
                    let source = EntityId::Value(value);
                    let ty = self
                        .function
                        .entity(source)
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                        .ty;
                    carried.slots.push(LinearBindingSlot {
                        symbol: Some(symbol),
                        source,
                        ty,
                        temporaries: Vec::new(),
                        loans: Vec::new(),
                        views: Vec::new(),
                    });
                }
                LoweredValue::Diverged => {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
        }
        let header = self
            .function
            .add_block(
                carried.slots.iter().map(|slot| slot.ty).collect(),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(loop_edge(header, &carried)),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.bindings = self.rebind_linear_bindings(baseline, header, &carried, span)?;
        self.block = header;
        Ok(LoopContext {
            entry_views: self.non_null_bindings.clone(),
            header,
            carried,
            continues: Vec::new(),
            breaks: Vec::new(),
        })
    }
}

fn loop_edge(target: BlockId, carried: &LinearBindings) -> Edge {
    Edge {
        target,
        arguments: carried.slots.iter().map(|slot| slot.source).collect(),
    }
}

/// Resolve header slots from the exit snapshot, after nested CFG has rebound IDs.
fn loop_slot_entity(
    slot: &LinearBindingSlot,
    exit: &BranchExit,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let entity = if let Some(symbol) = slot.symbol {
        match exit.bindings.get(&symbol) {
            Some(LoweredValue::Value(value)) => Some(EntityId::Value(*value)),
            _ => None,
        }
    } else if let Some(key) = slot.temporaries.first() {
        exit.temporaries.get(key).copied().map(EntityId::Value)
    } else if let Some(key) = slot.loans.first() {
        exit.loans.get(key).copied().flatten().map(EntityId::Loan)
    } else if let Some(symbol) = slot.views.first() {
        exit.views.get(symbol).copied().map(EntityId::Loan)
    } else {
        None
    };
    entity.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
}
