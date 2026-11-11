//! Red-only observation seam: no production producer is connected yet.
//! Never infer a receipt by scanning the completed SSA.
use crate::ssa::model::{BlockId, FunctionId, InstructionId, LoanId};
use lang_frontend::{
    ownership_checking::UnitLoanActionId,
    type_checking::{UnitExpressionId, UnitItemId},
};

#[derive(Clone, Debug)]
pub(crate) struct Receipt {
    pub(crate) ids: Vec<UnitLoanActionId>,
    pub(crate) callable: UnitItemId,
    pub(crate) caller: FunctionId,
    pub(crate) call: UnitExpressionId,
    pub(crate) block: BlockId,
    pub(crate) instruction: InstructionId,
    pub(crate) source: LoanId,
    pub(crate) result: LoanId,
}

pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<Receipt>) {
    (lower(), Vec::new())
}
