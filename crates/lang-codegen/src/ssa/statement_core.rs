//! Shared statement ordering; adapters retain dispatch and exact cleanup ownership.
use lang_frontend::ast::StatementId;

use super::{expression_core::LoweredValue, lowering_support::LoweringError};

pub(super) trait StatementLowering {
    fn lower_inner(&mut self, statement: StatementId) -> Result<LoweredValue, LoweringError>;
    fn emit_after_statement(&mut self, statement: StatementId) -> Result<(), LoweringError>;
}

pub(super) fn lower_statement<H: StatementLowering>(
    host: &mut H,
    statement: StatementId,
) -> Result<LoweredValue, LoweringError> {
    let result = host.lower_inner(statement)?;
    if result != LoweredValue::Diverged {
        host.emit_after_statement(statement)?;
    }
    Ok(result)
}

pub(super) fn lower_sequence<H: StatementLowering>(
    host: &mut H,
    statements: impl IntoIterator<Item = StatementId>,
) -> Result<LoweredValue, LoweringError> {
    for statement in statements {
        if lower_statement(host, statement)? == LoweredValue::Diverged {
            return Ok(LoweredValue::Diverged);
        }
    }
    Ok(LoweredValue::Unit)
}
