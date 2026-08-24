//! `while` / `loop` 的 backedge、jump 与 loop-carried binding lowering。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    source::Span,
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, control::BranchExit, error,
    value,
};
use crate::ssa::model::{BlockId, Edge, EntityId, EntityType, Origin, TerminatorKind, ValueId};

struct LoopJump {
    block: BlockId,
    bindings: BTreeMap<SymbolId, LoweredValue>,
    span: Span,
}

pub(super) struct LoopContext {
    header: BlockId,
    carried_symbols: Vec<SymbolId>,
    continues: Vec<LoopJump>,
    breaks: Vec<BranchExit>,
}

struct LoopHeader {
    block: BlockId,
    bindings: BTreeMap<SymbolId, LoweredValue>,
    carried_symbols: Vec<SymbolId>,
}

impl ExpressionLowerer<'_> {
    pub(super) fn lower_while(
        &mut self,
        condition: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let baseline = self.bindings.clone();
        let header = self.create_loop_header(&baseline, span)?;
        self.branch_with_bindings(
            self.block,
            header.block,
            &header.carried_symbols,
            &baseline,
            span,
        )?;
        self.block = header.block;
        self.bindings = header.bindings;

        let condition = self.require_value(condition)?;
        let condition_bindings = self.bindings.clone();
        let body_block = self.add_empty_block(span)?;
        let false_block = self.add_empty_block(span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition,
                    when_true: empty_edge(body_block),
                    when_false: empty_edge(false_block),
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        self.loops.push(LoopContext {
            header: header.block,
            carried_symbols: header.carried_symbols,
            continues: Vec::new(),
            breaks: Vec::new(),
        });
        self.block = body_block;
        self.bindings.clone_from(&condition_bindings);
        let body_result = self.lower_statement(body)?;
        if !matches!(body_result, LoweredValue::Diverged) {
            self.record_continue(span)?;
        }
        let context = self.loops.pop().expect("while context must be balanced");
        self.finish_loop_continues(&context)?;

        let mut exits = context.breaks;
        exits.push(BranchExit {
            block: false_block,
            result: LoweredValue::Unit,
            bindings: condition_bindings,
        });
        self.merge_exits(exits, &baseline, span)
    }

    pub(super) fn lower_loop(
        &mut self,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let baseline = self.bindings.clone();
        let header = self.create_loop_header(&baseline, span)?;
        self.branch_with_bindings(
            self.block,
            header.block,
            &header.carried_symbols,
            &baseline,
            span,
        )?;
        self.loops.push(LoopContext {
            header: header.block,
            carried_symbols: header.carried_symbols,
            continues: Vec::new(),
            breaks: Vec::new(),
        });
        self.block = header.block;
        self.bindings = header.bindings;
        let body_result = self.lower_statement(body)?;
        if !matches!(body_result, LoweredValue::Diverged) {
            self.record_continue(span)?;
        }
        let context = self.loops.pop().expect("loop context must be balanced");
        self.finish_loop_continues(&context)?;
        self.merge_exits(context.breaks, &baseline, span)
    }

    pub(super) fn lower_break(&mut self, span: Span) -> Result<LoweredValue, LoweringError> {
        let exit = BranchExit {
            block: self.block,
            result: LoweredValue::Unit,
            bindings: self.bindings.clone(),
        };
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
            block: self.block,
            bindings: self.bindings.clone(),
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
            self.branch_with_bindings(
                jump.block,
                context.header,
                &context.carried_symbols,
                &jump.bindings,
                jump.span,
            )?;
        }
        Ok(())
    }

    fn create_loop_header(
        &mut self,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LoopHeader, LoweringError> {
        let mut carried_symbols = Vec::new();
        let mut parameter_types = Vec::new();
        for (&symbol, &binding) in baseline {
            match binding {
                LoweredValue::Unit => {}
                LoweredValue::Value(value) => {
                    carried_symbols.push(symbol);
                    parameter_types.push(self.value_type(value, span)?);
                }
                LoweredValue::Diverged => {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
        }
        let header = self
            .function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let parameters = self
            .function
            .block(header)
            .expect("new loop header must exist")
            .parameters
            .clone();
        let mut bindings = baseline.clone();
        for (symbol, parameter) in carried_symbols.iter().copied().zip(parameters) {
            bindings.insert(symbol, LoweredValue::Value(value(parameter)));
        }
        Ok(LoopHeader {
            block: header,
            bindings,
            carried_symbols,
        })
    }

    fn branch_with_bindings(
        &mut self,
        block: BlockId,
        target: BlockId,
        symbols: &[SymbolId],
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let arguments = symbols
            .iter()
            .map(|symbol| {
                let binding = bindings
                    .get(symbol)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                match binding {
                    LoweredValue::Value(value) => Ok(EntityId::Value(value)),
                    LoweredValue::Unit | LoweredValue::Diverged => {
                        Err(error(LoweringErrorKind::MissingFact, span))
                    }
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.function
            .set_terminator(
                block,
                TerminatorKind::Branch(Edge { target, arguments }),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }

    fn value_type(&self, value: ValueId, span: Span) -> Result<EntityType, LoweringError> {
        self.function
            .entity(EntityId::Value(value))
            .map(|data| data.ty)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))
    }
}

fn empty_edge(target: BlockId) -> Edge {
    Edge {
        target,
        arguments: Vec::new(),
    }
}
