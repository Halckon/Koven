//! Distinct child and parent receipt states share only mechanical SSA validation.
use super::*;
use crate::ssa::refine::body::RefinedBody;
#[derive(Default)]
pub(in super::super) struct Emitted {
    child: Receipt,
    pub(super) parent: Receipt,
}
impl Emitted {
    pub(in super::super) fn layout(
        &mut self,
        body: &RefinedBody,
        function: &Function,
        blocks: &BTreeMap<BlockRef, BlockId>,
        slots: &BTreeMap<BlockRef, Vec<Slot>>,
        entities: &BTreeMap<BlockRef, Vec<EntityId>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if body.nested_exit_plans.parent_required != body.nested_exit_plans.parent.is_some() {
            return Err(invalid(span));
        }
        self.child.layout(
            &body.nested_exit_plans.child,
            function,
            blocks,
            slots,
            entities,
            span,
        )?;
        self.parent.layout(
            &body.nested_exit_plans.parent,
            function,
            blocks,
            slots,
            entities,
            span,
        )
    }
    pub(in super::super) fn statement(
        &mut self,
        body: &RefinedBody,
        function: &Function,
        position: (BlockRef, usize),
        instruction: InstructionId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.child.statement(
            &body.nested_exit_plans.child,
            function,
            position,
            instruction,
            current,
            span,
        )?;
        self.parent.statement(
            &body.nested_exit_plans.parent,
            function,
            position,
            instruction,
            current,
            span,
        )
    }
    pub(in super::super) fn finish(
        &self,
        body: &RefinedBody,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        if body.nested_exit_plans.parent_required != body.nested_exit_plans.parent.is_some()
            || self.parent.function.is_some() != body.nested_exit_plans.parent.is_some()
        {
            return Err(invalid(span));
        }
        self.child
            .finish(&body.nested_exit_plans.child, function, span)?;
        self.parent
            .finish(&body.nested_exit_plans.parent, function, span)
    }
}
