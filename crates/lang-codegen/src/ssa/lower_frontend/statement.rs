//! Original statement dispatch and cleanup adapted to the shared ordering core.
use lang_frontend::{ast::StatementId, ownership_checking::DropPoint};

use super::{ExpressionLowerer, LoweredValue, LoweringError};
use crate::ssa::statement_core::StatementLowering;

impl StatementLowering for ExpressionLowerer<'_> {
    fn lower_inner(&mut self, statement: StatementId) -> Result<LoweredValue, LoweringError> {
        self.lower_statement_inner(statement)
    }

    fn emit_after_statement(&mut self, statement: StatementId) -> Result<(), LoweringError> {
        self.emit_drops(DropPoint::AfterStatement(statement))
    }
}
