//! Merge the live proof slots supplied by continuing predecessor blocks.
use lang_frontend::source::Span;

use super::PredecessorBranch;
use crate::ssa::{
    lowering_support::LoweringError,
    refine::{
        body::{BlockRef, ROp},
        build::Builder,
    },
};

impl Builder<'_, '_> {
    /// Joins active views from reached branch predecessors at `join`.
    /// Non-common views in any predecessor end on that predecessor's edge before jumping.
    pub(in crate::ssa::refine::build) fn join_views(
        &mut self,
        predecessors: Vec<PredecessorBranch>,
        join: BlockRef,
        span: Span,
    ) -> Result<bool, LoweringError> {
        if predecessors.is_empty() {
            self.block = None;
            return Ok(false);
        }
        let mut common = Vec::new();
        if let Some(first) = predecessors.first() {
            for v in &first.views {
                if predecessors[1..]
                    .iter()
                    .all(|other| other.views.iter().any(|ov| ov.slot == v.slot))
                {
                    common.push(*v);
                }
            }
        }
        for pred in predecessors {
            self.block = Some(pred.block);
            self.dead = pred.dead;
            self.this_dead = pred.this_dead;
            self.dead_block = Some(pred.block);
            for v in pred.views.iter().rev() {
                if !common.iter().any(|cv| cv.slot == v.slot) {
                    self.push(ROp::BorrowEnd { loan: v.slot }, Vec::new(), span)?;
                }
            }
            self.goto(join, span)?;
        }
        self.block = Some(join);
        self.active_views = common;
        Ok(true)
    }
}
