//! Optional unchanged-root transport up to, but never through, the parent header.
use super::*;
use crate::ssa::model::CheckedArithmeticOperator;
#[derive(Clone, Debug)]
pub(in super::super) struct Carry {
    pub header: BlockRef,
    pub edges: Vec<(BlockRef, BlockRef)>,
    pub checked: Vec<CarryChecked>,
}
#[derive(Clone, Debug)]
pub(in super::super) struct CarryChecked {
    pub body: BlockRef,
    pub success: BlockRef,
    pub abort: BlockRef,
    pub instruction: usize,
    pub span: Span,
    pub operator: CheckedArithmeticOperator,
}
pub(super) struct CarryView<'a> {
    pub function: &'a Function,
    pub blocks: &'a BTreeMap<BlockRef, BlockId>,
    pub slots: &'a BTreeMap<BlockRef, Vec<Slot>>,
    pub parameters: &'a BTreeMap<BlockRef, Vec<EntityId>>,
    pub seen: &'a BTreeMap<BlockRef, (InstructionId, Operation)>,
}
impl Carry {
    pub(super) fn fold(
        &mut self,
        block: BlockRef,
        successor: BlockRef,
        prefix: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        let remap = |b: &mut BlockRef| {
            if *b == successor {
                *b = block
            }
        };
        remap(&mut self.header);
        for (a, b) in &mut self.edges {
            remap(a);
            remap(b);
        }
        self.edges.retain(|(a, b)| a != b);
        for c in &mut self.checked {
            if c.body == successor {
                c.instruction = c
                    .instruction
                    .checked_add(prefix)
                    .ok_or_else(|| invalid(span))?;
            }
            remap(&mut c.body);
            remap(&mut c.success);
            remap(&mut c.abort);
            if c.body == c.success || c.body == c.abort || c.success == c.abort {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
    pub(super) fn observe(
        &self,
        function: &Function,
        position: (BlockRef, usize),
        instruction: Option<InstructionId>,
        seen: &mut BTreeMap<BlockRef, (InstructionId, Operation)>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for c in &self.checked {
            if position == (c.body, c.instruction)
                && seen
                    .insert(c.body, {
                        let id = instruction.ok_or_else(|| invalid(span))?;
                        (
                            id,
                            function
                                .instruction(id)
                                .ok_or_else(|| invalid(span))?
                                .operation
                                .clone(),
                        )
                    })
                    .is_some()
            {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
    pub(super) fn finish(
        &self,
        p: &Plan,
        view: CarryView<'_>,
        (replace, new): (InstructionId, EntityId),
        span: Span,
    ) -> Result<(), LoweringError> {
        let CarryView {
            function,
            blocks,
            slots,
            parameters,
            seen,
        } = view;
        let fail = || invalid(span);
        if self.edges.is_empty() || seen.len() != self.checked.len() {
            return Err(fail());
        }
        let mut at = p.resume;
        let mut value = new;
        let mut visited = std::collections::BTreeSet::new();
        for &(from, to) in &self.edges {
            if from != at || !visited.insert(from) || from == self.header {
                return Err(fail());
            }
            let b = function
                .block(*blocks.get(&from).ok_or_else(fail)?)
                .ok_or_else(fail)?;
            let start = if from == p.resume {
                b.instructions
                    .iter()
                    .position(|id| *id == replace)
                    .ok_or_else(fail)?
                    + 1
            } else {
                0
            };
            for id in &b.instructions[start..] {
                let i = function.instruction(*id).ok_or_else(fail)?;
                if i.results.contains(&value) || i.operation.entities().contains(&value) {
                    return Err(fail());
                }
            }
            let term = &b.terminator.as_ref().ok_or_else(fail)?.kind;
            let edge = match term {
                TerminatorKind::Branch(e) if !self.checked.iter().any(|c| c.body == from) => e,
                TerminatorKind::Conditional {
                    condition,
                    when_true,
                    when_false,
                } => {
                    let checks = self
                        .checked
                        .iter()
                        .filter(|c| c.body == from)
                        .collect::<Vec<_>>();
                    let [c] = checks.as_slice() else {
                        return Err(fail());
                    };
                    let (id, snapshot) = seen.get(&from).ok_or_else(fail)?;
                    let id = *id;
                    let i = function.instruction(id).ok_or_else(fail)?;
                    if b.instructions.iter().filter(|x| **x == id).count() != 1
                        || b.instructions.last() != Some(&id)
                        || i.block != b.id
                        || i.origin.span() != c.span
                        || &i.operation != snapshot
                        || !matches!(i.operation,Operation::CheckedArithmetic{operator,..} if operator==c.operator)
                        || i.results.len() != 2
                        || i.results[1] != EntityId::Value(*condition)
                        || c.success != to
                        || when_true.target != *blocks.get(&c.abort).ok_or_else(fail)?
                        || when_false.target != *blocks.get(&c.success).ok_or_else(fail)?
                    {
                        return Err(fail());
                    }
                    let abort = function.block(when_true.target).ok_or_else(fail)?;
                    if !matches!(
                        abort.terminator.as_ref().map(|t| &t.kind),
                        Some(TerminatorKind::Abort)
                    ) {
                        return Err(fail());
                    }
                    when_false
                }
                _ => return Err(fail()),
            };
            let target = function
                .block(*blocks.get(&to).ok_or_else(fail)?)
                .ok_or_else(fail)?;
            let ss = slots.get(&to).ok_or_else(fail)?;
            let es = parameters.get(&to).ok_or_else(fail)?;
            let indexes = ss
                .iter()
                .enumerate()
                .filter_map(|(i, s)| (*s == p.root).then_some(i))
                .collect::<Vec<_>>();
            let [index] = indexes.as_slice() else {
                return Err(fail());
            };
            if edge.target != target.id
                || edge.arguments.len() != ss.len()
                || ss.len() != es.len()
                || target.parameters != *es
                || edge.arguments.get(*index) != Some(&value)
            {
                return Err(fail());
            }
            value = *es.get(*index).ok_or_else(fail)?;
            at = to;
        }
        if at != self.header {
            return Err(fail());
        }
        #[cfg(test)]
        super::super::build::owner_consumer_test_support::record_action(
            "nested-parent-normal-carry-complete",
            1,
        );
        Ok(())
    }
}
