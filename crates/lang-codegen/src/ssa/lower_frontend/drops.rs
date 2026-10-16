//! Phase 3 ASAP drop facts to explicit SSA owner drops.

use lang_frontend::ownership_checking::{DropPoint, DropTarget};

use super::{ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error};
use crate::ssa::model::Operation;

impl ExpressionLowerer<'_> {
    pub(super) fn emit_drops(&mut self, point: DropPoint) -> Result<(), LoweringError> {
        self.end_result_bindings(point)?;
        let facts = self
            .owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        for fact in facts {
            // Generic ownership facts precede monomorphization. A concrete Copyable
            // instance has no drop obligation; keep its binding/aliases intact.
            let template = match fact.target() {
                DropTarget::Named(symbol) => Some(self.typed.symbol_type(symbol)),
                DropTarget::Temporary(expression) => Some(self.typed.expression_type(expression)),
                _ => None,
            };
            if let Some(template) = template {
                let template = template
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact.value_origin()))?;
                let concrete = self.resolve_type(template, fact.value_origin())?;
                if concrete != template
                    && self.typed.copyability(concrete)
                        == Some(lang_frontend::type_checking::Copyability::Copyable)
                {
                    continue;
                }
            }
            // Conditional cleanup needs its saved choice transported with the owner.
            if fact.condition().is_some()
                && !super::map_loop_owner::supported_drop(
                    self.parsed,
                    self.names,
                    self.typed,
                    self.owned,
                    fact,
                )
            {
                return Err(error(
                    LoweringErrorKind::UnsupportedNode,
                    fact.value_origin(),
                ));
            }
            let owner = match fact.target() {
                DropTarget::Named(symbol) => {
                    // Loop-exit facts include entry owners consumed on every actual exit.
                    if matches!(point, DropPoint::LoopExit(_))
                        && !self.bindings.contains_key(&symbol)
                        && (fact.condition().is_none()
                            || super::constant_presence::constant_drop(
                                self.parsed,
                                self.names,
                                self.typed,
                                self.owned,
                                fact,
                            ))
                    {
                        continue;
                    }
                    // nullable 分支证明是 owner 的共享视图。ASAP drop 可能恰好位于证明分支的
                    // 最后一次使用处，因此必须先结束视图，再消费 owner。
                    self.end_non_null_proof(symbol, fact.value_origin())?;
                    match self.bindings.remove(&symbol) {
                        Some(LoweredValue::Value(value)) => value,
                        Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                            return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                        }
                    }
                }
                DropTarget::Temporary(expression) => {
                    let owner = self
                        .temporaries
                        .remove(&expression.index())
                        .ok_or_else(|| {
                            error(LoweringErrorKind::MissingFact, fact.value_origin())
                        })?;
                    // Group expressions can name the same temporary owner.
                    self.temporaries.retain(|_, candidate| *candidate != owner);
                    owner
                }
                // The closure owner recursively drops its owned environment slots.
                DropTarget::Captured { .. } => continue,
                DropTarget::RetainedSource(_) => {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
                DropTarget::ReplacedElement(_) => {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
            };
            self.record_map_loop_drop(fact, owner)?;
            self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
            self.release_owner_capture_loans(owner, fact.value_origin())?;
        }
        Ok(())
    }
}
