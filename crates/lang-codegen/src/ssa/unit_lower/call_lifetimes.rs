//! 未提交的同步调用前缀；只结束本帧实际建立的 loan，owner 仍由 frontend drop facts 决定。
use super::*;

pub(super) struct PendingCallFrame {
    pub(super) call: UnitExpressionId,
    pub(super) receiver: bool,
    pub(super) loan_arguments: Vec<(UnitExpressionId, Vec<usize>)>,
    pub(super) loop_depth: usize,
    pub(super) pending_start: usize,
    pub(super) created_loans: Vec<usize>,
    /// 同一父 owner 的并行字段 loan 尚无 projection-aware alias 合同。
    pub(super) field_replace_owner: Option<usize>,
    /// 保留共享字段 loan 的稳定 root，使后续 sibling exclusive loan 明确拒绝。
    pub(super) shared_field_roots: Vec<UnitSymbolId>,
    /// 独占 root 的 owner 槽位须与 binding 保持同一 CFG identity。
    pub(super) exclusive_root_owners: Vec<usize>,
}

impl UnitExpressionLowerer<'_> {
    /// Iteration facts authorize the same CFG-safe call prefixes through both unit views.
    /// Span containment selects the capability slice, never an ownership or cleanup action.
    pub(super) fn supports_control_prefix(&self, span: Span) -> bool {
        self.constant_owned.is_some()
            || self.typed.sequential_iterations().iter().any(|descriptor| {
                descriptor.statement().source_unit() == self.source_unit
                    && self
                        .statement_span(descriptor.statement().statement())
                        .is_ok_and(|iteration| {
                            (iteration.start() <= span.start() && span.end() <= iteration.end())
                                || (span.start() <= iteration.start()
                                    && iteration.end() <= span.end())
                        })
            })
    }

    /// 槽位在 CFG 重绑定后仍稳定；不可保存旧 block 的 LoanId。
    pub(super) fn end_pending_call_loans(
        &mut self,
        minimum_loop_depth: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut retained = self.pending_operands.len();
        for frame in self.pending_call_frames.iter().rev() {
            if frame.loop_depth < minimum_loop_depth {
                continue;
            }
            retained = retained.min(frame.pending_start);
            for &index in frame.created_loans.iter().rev() {
                let Some(EntityId::Loan(loan)) = self.pending_operands.get(index).copied() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                self.function
                    .append_instruction(
                        self.block,
                        Operation::BorrowEnd { loan },
                        Vec::new(),
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            }
        }
        self.pending_operands.truncate(retained);
        Ok(())
    }
}
