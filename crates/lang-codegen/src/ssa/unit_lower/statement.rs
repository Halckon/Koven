//! Original statement dispatch and cleanup adapted to the shared ordering core.
use lang_frontend::{
    ast::StatementId, ownership_checking::UnitDropPoint, type_checking::UnitStatementId,
};

use super::{LoweredValue, LoweringError, UnitExpressionLowerer};
use crate::ssa::statement_core::StatementLowering;

impl StatementLowering for UnitExpressionLowerer<'_> {
    fn lower_inner(&mut self, statement: StatementId) -> Result<LoweredValue, LoweringError> {
        self.lower_statement_inner(statement)
    }

    fn emit_after_statement(&mut self, statement: StatementId) -> Result<(), LoweringError> {
        self.emit_drops(UnitDropPoint::AfterStatement(UnitStatementId::new(
            self.source_unit,
            statement,
        )))
    }
}
