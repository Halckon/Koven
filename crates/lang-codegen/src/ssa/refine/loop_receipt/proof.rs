//! Only original producer provenance, not a copy of the lowering plan or slot map.
use super::*;
use crate::ssa::model::EntityId;
#[derive(Clone, Debug)]
pub(super) struct ProducerProof {
    pub(super) blocks: LoopBlocks,
    pub(super) original_edges: Vec<(BlockRef, BlockRef)>,
    pub(super) original_return: BlockRef,
    pub(super) folds: Vec<(BlockRef, BlockRef)>,
    pub(super) owner_alias: Option<(EntityId, EntityId)>,
    pub(super) source_callee: FunctionId,
    pub(super) consumers: Vec<FunctionId>,
    pub(super) flag_parameter: usize,
    pub(super) condition_span: Span,
}
impl ProducerProof {
    pub(super) fn captured(plan: &Plan, span: Span) -> Result<Self, LoweringError> {
        let mut original_edges = Vec::new();
        let mut returned = None;
        for (block, term) in &plan.terms {
            match term {
                RTerm::Goto(to) => original_edges.push((*block, *to)),
                RTerm::Branch {
                    when_true,
                    when_false,
                    ..
                } => {
                    original_edges.push((*block, *when_true));
                    original_edges.push((*block, *when_false));
                }
                RTerm::Return(None) if returned.is_none() => returned = Some(*block),
                _ => return Err(invalid(span)),
            }
        }
        Ok(Self {
            blocks: plan.blocks.ok_or_else(|| invalid(span))?,
            original_edges,
            original_return: returned.ok_or_else(|| invalid(span))?,
            folds: plan.folds.clone(),
            owner_alias: None,
            source_callee: plan.source_callee,
            consumers: plan.callees.clone(),
            flag_parameter: plan.flag_parameter,
            condition_span: plan.condition_span,
        })
    }
    pub(super) fn canonical(&self, mut block: BlockRef) -> BlockRef {
        for (from, to) in &self.folds {
            if block == *to {
                block = *from;
            }
        }
        block
    }
}
