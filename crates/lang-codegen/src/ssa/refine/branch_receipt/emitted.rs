//! Consume producer-bound roles and the actual fold, never infer paths by scanning SSA.
use super::*;
#[cfg(test)]
#[path = "ci_tests.rs"]
mod ci_tests;
use crate::ssa::model::{
    BlockId, Edge, EntityId, Function, InstructionId, LoanId, Operation, PlaceAccess,
    TerminatorKind, ValueId,
};
#[derive(Clone, Debug)]
pub(crate) struct PathReceipt {
    pub(crate) ids: Vec<UnitLoanActionId>,
    pub(crate) branch: UnitLoanForwardBranch,
    pub(crate) callee: FunctionId,
    pub(crate) block: BlockId,
    pub(crate) instruction: InstructionId,
    pub(crate) source: LoanId,
    pub(crate) result: LoanId,
    pub(crate) edge: Edge,
}
#[derive(Clone, Debug)]
pub(crate) struct Receipt {
    pub(crate) topology: UnitBorrowBranchForwardTopology,
    pub(crate) callable: UnitItemId,
    pub(crate) caller: FunctionId,
    pub(crate) split: UnitExpressionId,
    pub(crate) condition: UnitExpressionId,
    pub(crate) entry: BlockId,
    pub(crate) source: LoanId,
    pub(crate) flag: LoanId,
    pub(crate) read: InstructionId,
    pub(crate) condition_value: ValueId,
    fold: Option<(BlockRef, BlockRef)>,
    pub(crate) paths: Vec<PathReceipt>,
}
struct Layout {
    entry: BlockId,
    source: LoanId,
    flag: LoanId,
    arms: [BlockId; 2],
    sources: [LoanId; 2],
}
#[derive(Default)]
pub(in crate::ssa::refine) struct Emitted {
    layout: Option<Layout>,
    read: Option<(InstructionId, ValueId)>,
    calls: [Option<(InstructionId, LoanId)>; 2],
    edges: Option<[Edge; 2]>,
    returned: [bool; 2],
}
fn loan(entity: Option<EntityId>, span: Span) -> Result<LoanId, LoweringError> {
    match entity {
        Some(EntityId::Loan(id)) => Ok(id),
        _ => Err(invalid(span)),
    }
}
impl Emitted {
    pub(in crate::ssa::refine) fn new(
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<Self, LoweringError> {
        plans.finish(span)?;
        if plans
            .plan
            .as_ref()
            .is_some_and(|p| p.caller != function.id())
        {
            return Err(invalid(span));
        }
        Ok(Self::default())
    }
    pub(in crate::ssa::refine) fn layout(
        &mut self,
        plans: &Plans,
        function: &Function,
        blocks: &BTreeMap<BlockRef, BlockId>,
        parameters: &BTreeMap<BlockRef, Vec<Slot>>,
        entities: &BTreeMap<BlockRef, Vec<EntityId>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else { return Ok(()) };
        let entry = p.entry.ok_or_else(|| invalid(span))?;
        let arms = p.arms.ok_or_else(|| invalid(span))?;
        let join = p.join.ok_or_else(|| invalid(span))?;
        if self.layout.is_some()
            || match p.topology {
                UnitBorrowBranchForwardTopology::TailContinuation => {
                    plans.folds != [(arms[1], join)]
                }
                UnitBorrowBranchForwardTopology::BothArmsReturn => !plans.folds.is_empty(),
            }
            || blocks.len() != 3
            || blocks.contains_key(&join)
            || p.paths.len() != 2
            || p.paths[0].block != Some(arms[0])
            || p.paths[1].block
                != Some(match p.topology {
                    UnitBorrowBranchForwardTopology::TailContinuation => join,
                    UnitBorrowBranchForwardTopology::BothArmsReturn => arms[1],
                })
        {
            return Err(invalid(span));
        }
        let block = |b| blocks.get(&b).copied().ok_or_else(|| invalid(span));
        let mapped = |b, slot: Option<Slot>| -> Result<LoanId, LoweringError> {
            let slots = parameters.get(&b).ok_or_else(|| invalid(span))?;
            let values = entities.get(&b).ok_or_else(|| invalid(span))?;
            if slots.len() != values.len()
                || function.block(block(b)?).map(|b| &b.parameters) != Some(values)
            {
                return Err(invalid(span));
            }
            let slot = slot.ok_or_else(|| invalid(span))?;
            let indices = slots
                .iter()
                .enumerate()
                .filter(|(_, s)| **s == slot)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            let [index] = indices.as_slice() else {
                return Err(invalid(span));
            };
            loan(values.get(*index).copied(), span)
        };
        let source = mapped(entry, p.source)?;
        let flag = mapped(entry, p.flag_slot)?;
        let arm_ids = [block(arms[0])?, block(arms[1])?];
        let entry_id = block(entry)?;
        let sources = [
            mapped(arms[0], p.paths[0].source)?,
            mapped(arms[1], p.paths[1].source)?,
        ];
        if function.id() != p.caller
            || source == flag
            || source.function() != p.caller
            || flag.function() != p.caller
            || arm_ids[0] == arm_ids[1]
            || arm_ids.contains(&entry_id)
            || entities.get(&entry).map(Vec::as_slice)
                != Some(&[EntityId::Loan(source), EntityId::Loan(flag)][..])
            || sources.iter().any(|s| s.function() != p.caller)
            || (0..2).any(|i| {
                entities.get(&arms[i]).map(Vec::as_slice) != Some(&[EntityId::Loan(sources[i])][..])
            })
        {
            return Err(invalid(span));
        }
        self.layout = Some(Layout {
            entry: entry_id,
            source,
            flag,
            arms: arm_ids,
            sources,
        });
        Ok(())
    }
    pub(in crate::ssa::refine) fn statement(
        &mut self,
        plans: &Plans,
        tag: Tag,
        function: &Function,
        instruction: InstructionId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let p = plans.plan.as_ref().ok_or_else(|| invalid(span))?;
        let l = self.layout.as_ref().ok_or_else(|| invalid(span))?;
        let data = function
            .instruction(instruction)
            .ok_or_else(|| invalid(span))?;
        if function.id() != p.caller {
            return Err(invalid(span));
        }
        match tag {
            Tag::Condition => {
                let [EntityId::Value(value)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if self.read.is_some()
                    || data.block != l.entry
                    || data.operation
                        != (Operation::Read {
                            source: PlaceAccess::Loan(l.flag),
                        })
                    || p.condition_slot.and_then(|s| current.get(&s))
                        != Some(&EntityId::Value(*value))
                {
                    return Err(invalid(span));
                }
                self.read = Some((instruction, *value));
            }
            Tag::Call(index) => {
                let path = p.paths.get(index).ok_or_else(|| invalid(span))?;
                let source = loan(path.source.and_then(|s| current.get(&s).copied()), span)?;
                let result = loan(path.result.and_then(|s| current.get(&s).copied()), span)?;
                if index >= 2
                    || self.calls[index].is_some()
                    || data.block != l.arms[index]
                    || source != l.sources[index]
                    || source == result
                    || result.function() != p.caller
                    || data.operation
                        != (Operation::BorrowCall {
                            callee: path.callee,
                            arguments: vec![EntityId::Loan(source)],
                            source,
                        })
                    || data.results != [EntityId::Loan(result)]
                {
                    return Err(invalid(span));
                }
                self.calls[index] = Some((instruction, result));
            }
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn terminator(
        &mut self,
        plans: &Plans,
        function: &Function,
        block: BlockId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else { return Ok(()) };
        let l = self.layout.as_ref().ok_or_else(|| invalid(span))?;
        let term = function
            .block(block)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(span))?;
        if function.id() != p.caller {
            return Err(invalid(span));
        }
        if block == l.entry {
            let (_, value) = self.read.ok_or_else(|| invalid(span))?;
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = &term.kind
            else {
                return Err(invalid(span));
            };
            if self.edges.is_some()
                || *condition != value
                || p.source.and_then(|s| current.get(&s)) != Some(&EntityId::Loan(l.source))
                || p.flag_slot.and_then(|s| current.get(&s)) != Some(&EntityId::Loan(l.flag))
                || when_true.target != l.arms[0]
                || when_false.target != l.arms[1]
                || when_true.arguments != [EntityId::Loan(l.source)]
                || when_false.arguments != [EntityId::Loan(l.source)]
            {
                return Err(invalid(span));
            }
            self.edges = Some([when_true.clone(), when_false.clone()]);
        } else {
            let index = l
                .arms
                .iter()
                .position(|b| *b == block)
                .ok_or_else(|| invalid(span))?;
            let (_, result) = self.calls[index].ok_or_else(|| invalid(span))?;
            let path = &p.paths[index];
            if self.returned[index]
                || term.kind != (TerminatorKind::BorrowReturn { loan: result })
                || path.source.and_then(|s| current.get(&s))
                    != Some(&EntityId::Loan(l.sources[index]))
                || path.result.and_then(|s| current.get(&s)) != Some(&EntityId::Loan(result))
            {
                return Err(invalid(span));
            }
            self.returned[index] = true;
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn finish(
        self,
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else {
            return if self.layout.is_none()
                && self.read.is_none()
                && self.calls.iter().all(Option::is_none)
                && self.edges.is_none()
                && self.returned == [false; 2]
            {
                Ok(())
            } else {
                Err(invalid(span))
            };
        };
        let l = self.layout.ok_or_else(|| invalid(span))?;
        let (read, condition_value) = self.read.ok_or_else(|| invalid(span))?;
        let edges = self.edges.ok_or_else(|| invalid(span))?;
        if self.returned != [true; 2]
            || function.id() != p.caller
            || function.blocks.len() != 3
            || function.instructions.len() != 3
            || match p.topology {
                UnitBorrowBranchForwardTopology::TailContinuation => plans.folds.len() != 1,
                UnitBorrowBranchForwardTopology::BothArmsReturn => !plans.folds.is_empty(),
            }
        {
            return Err(invalid(span));
        }
        let mut paths = Vec::new();
        for (i, edge) in edges.into_iter().enumerate() {
            let (instruction, result) = self.calls[i].ok_or_else(|| invalid(span))?;
            paths.push(PathReceipt {
                ids: p.paths[i].ids.clone(),
                branch: p.paths[i].branch,
                callee: p.paths[i].callee,
                block: l.arms[i],
                instruction,
                source: l.sources[i],
                result,
                edge,
            });
        }
        let receipt = Receipt {
            topology: p.topology,
            callable: p.callable,
            caller: p.caller,
            split: p.split,
            condition: p.condition,
            entry: l.entry,
            source: l.source,
            flag: l.flag,
            read,
            condition_value,
            fold: plans.folds.first().copied(),
            paths,
        };
        receipt.validate(plans, function, span)?;
        #[cfg(test)]
        ci_tests::record(plans, function, &receipt, span);
        #[cfg(test)]
        RECEIPTS.with(|v| {
            if let Some(v) = v.borrow_mut().as_mut() {
                v.pending.push(receipt)
            }
        });
        Ok(())
    }
}
impl Receipt {
    #[cfg(all(test, feature = "loan-observation-test-support"))]
    pub(crate) fn folded_distinct_blocks(&self) -> bool {
        self.fold.is_some_and(|(from, to)| from != to)
    }
    fn validate(
        &self,
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let p = plans.plan.as_ref().ok_or_else(|| invalid(span))?;
        let data = function
            .instruction(self.read)
            .ok_or_else(|| invalid(span))?;
        let entry = function.block(self.entry).ok_or_else(|| invalid(span))?;
        if self.topology != p.topology
            || self.callable != p.callable
            || self.caller != function.id()
            || self.split != p.split
            || self.condition != p.condition
            || match (self.topology, self.fold) {
                (UnitBorrowBranchForwardTopology::TailContinuation, Some(fold)) => {
                    plans.folds != [fold]
                }
                (UnitBorrowBranchForwardTopology::BothArmsReturn, None) => !plans.folds.is_empty(),
                _ => true,
            }
            || self.paths.len() != 2
            || data.block != self.entry
            || data.operation
                != (Operation::Read {
                    source: PlaceAccess::Loan(self.flag),
                })
            || data.results != [EntityId::Value(self.condition_value)]
            || entry.parameters != [EntityId::Loan(self.source), EntityId::Loan(self.flag)]
            || entry.terminator.as_ref().map(|t| &t.kind)
                != Some(&TerminatorKind::Conditional {
                    condition: self.condition_value,
                    when_true: self.paths[0].edge.clone(),
                    when_false: self.paths[1].edge.clone(),
                })
        {
            return Err(invalid(span));
        }
        let mut instructions = vec![self.read];
        for (i, path) in self.paths.iter().enumerate() {
            let data = function
                .instruction(path.instruction)
                .ok_or_else(|| invalid(span))?;
            let block = function.block(path.block).ok_or_else(|| invalid(span))?;
            if instructions.contains(&path.instruction)
                || path.ids != p.paths[i].ids
                || path.branch != p.paths[i].branch
                || path.callee != p.paths[i].callee
                || path.edge.target != path.block
                || path.edge.arguments != [EntityId::Loan(self.source)]
                || data.block != path.block
                || data.results != [EntityId::Loan(path.result)]
                || data.operation
                    != (Operation::BorrowCall {
                        callee: path.callee,
                        arguments: vec![EntityId::Loan(path.source)],
                        source: path.source,
                    })
                || block.parameters != [EntityId::Loan(path.source)]
                || block.terminator.as_ref().map(|t| &t.kind)
                    != Some(&TerminatorKind::BorrowReturn { loan: path.result })
            {
                return Err(invalid(span));
            }
            instructions.push(path.instruction);
        }
        if function
            .instructions
            .iter()
            .any(|data| !instructions.contains(&data.id))
        {
            return Err(invalid(span));
        }
        Ok(())
    }
}
