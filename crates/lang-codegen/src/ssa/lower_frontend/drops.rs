//! Phase 3 ASAP drop facts to explicit SSA owner drops.

use lang_frontend::ownership_checking::{DropPoint, DropTarget};

use super::{ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error};
use crate::ssa::model::Operation;

impl ExpressionLowerer<'_> {
    pub(super) fn emit_drops(&mut self, point: DropPoint) -> Result<(), LoweringError> {
        let facts = self
            .owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        for fact in facts {
            let owner = match fact.target() {
                DropTarget::Named(symbol) => {
                    // nullable 分支证明是 owner 的共享视图。ASAP drop 可能恰好位于证明分支的
                    // 最后一次使用处，因此必须先结束视图，再消费 owner。
                    if let Some(loan) = self.non_null_bindings.remove(&symbol) {
                        self.append(
                            Operation::BorrowEnd { loan },
                            Vec::new(),
                            fact.value_origin(),
                        )?;
                    }
                    match self.bindings.remove(&symbol) {
                        Some(LoweredValue::Value(value)) => value,
                        Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                            return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                        }
                    }
                }
                DropTarget::Temporary(expression) => {
                    self.temporaries
                        .remove(&expression.index())
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact.value_origin()))?
                }
                DropTarget::ReplacedElement(_) | DropTarget::Captured { .. } => {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
            };
            self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
        }
        Ok(())
    }
}
