//! compilation-unit 无 jump `while` 的 owner-aware 回边与零次退出 lowering。

use std::collections::BTreeSet;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::UnitDropPoint,
    source::Span,
    type_checking::UnitStatementId,
};

use super::{LoweredValue, UnitExpressionLowerer, cfg::carried_edge, lowering_error};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{Origin, TerminatorKind},
};

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
        let entry_symbols = baseline.keys().copied().collect::<BTreeSet<_>>();
        let carried = self.carried_bindings(&baseline, span)?;
        let header = self.add_carried_block(&carried, span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(carried_edge(header, &carried)),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        self.block = header;
        self.bindings = self.rebind_carried(&baseline, header, &carried, span)?;
        let condition = self.require_expression_value(condition)?;
        if !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let condition_bindings = self.bindings.clone();
        let body_block = self.add_carried_block(&carried, self.statement_span(body)?)?;
        let exit_block = self.add_carried_block(&carried, span)?;
        let when_true = self.carried_edge_from(body_block, &carried, &condition_bindings, span)?;
        let when_false = self.carried_edge_from(exit_block, &carried, &condition_bindings, span)?;
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

        self.block = body_block;
        self.bindings = self.rebind_carried(&condition_bindings, body_block, &carried, span)?;
        if self.lower_statement(body)? != LoweredValue::Diverged {
            if !self.temporaries.is_empty() {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            self.discard_non_entry_bindings(&entry_symbols, self.statement_span(body)?)?;
            let backedge = self.carried_edge_from(header, &carried, &self.bindings, span)?;
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::Branch(backedge),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }

        self.block = exit_block;
        self.bindings = self.rebind_carried(&condition_bindings, exit_block, &carried, span)?;
        self.temporaries.clear();
        self.emit_drops(UnitDropPoint::LoopExit(UnitStatementId::new(
            self.source_unit,
            statement,
        )))?;
        Ok(LoweredValue::Unit)
    }
}
