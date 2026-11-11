//! Private observed primitive commits, not public owner tokens or cleanup authority.
use super::*;
#[derive(Clone, Debug)]
pub(in super::super::super) struct RootWriteReceipt {
    pub(in super::super::super) control: UnitStatementId,
    pub(in super::super::super) root: UnitSymbolId,
    pub(in super::super::super) call: UnitExpressionId,
    pub(in super::super::super) producer: UnitExpressionId,
}
impl ControlTransactions {
    #[cfg(test)]
    pub(super) fn root_writes(&self, _control: UnitStatementId) -> &[RootWriteReceipt] {
        &[]
    }
}
