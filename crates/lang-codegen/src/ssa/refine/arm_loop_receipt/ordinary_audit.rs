//! Feature-only exhaustive ordinary caller closure, not the general receipt's local replay.
use super::*;
use crate::ssa::model::{
    EntityType, LoanKind, Operation, Program, SsaTypeId, SsaTypeKind, TerminatorKind,
};
use std::collections::BTreeSet;
impl Receipt {
    pub(crate) fn certify_ordinary(
        &self,
        program: &Program,
        fact: &UnitLoopBranchNoCarryFact,
        abi: &[(SsaTypeId, ParameterMode)],
        roles: &[usize; 3],
        conditions: &[(UnitExpressionId, Span); 2],
        dispatch: (
            &[Vec<FunctionId>; 2],
            Option<&super::super::fresh_loop_receipt::SourceConsumer>,
        ),
    ) -> Option<[(FunctionId, InstructionId, FunctionId); 2]> {
        let (calls, prefix) = dispatch;
        let p = &self.plan;
        let s = self.span;
        if p.prefix.is_some()
            || p.counter.is_some()
            || p.counter_fact.is_some()
            || p.counter_break.is_some()
            || p.tail_break.is_some()
            || !p.fact.same_identity(fact)
            || p.fact != *fact
            || p.roles != *roles
            || !matches!(abi.len(), 2 | 3)
            || self.parameters.len() != abi.len()
            || roles.iter().copied().collect::<BTreeSet<_>>() != (0..abi.len()).collect()
            || roles[0] == roles[1]
            || roles[0] == roles[2]
            || (roles[1] == roles[2]) != (abi.len() == 2)
            || p.conditions != conditions.map(|c| c.0)
            || p.condition_spans != conditions.map(|c| c.1)
        {
            return None;
        }
        for (arm, ids) in p.arms.iter().zip(calls) {
            if !matches!(arm.consumers.len(), 1 | 2)
                || ids.first() != Some(&arm.callee)
                || ids.get(1..) != Some(arm.consumers.as_slice())
            {
                return None;
            }
        }
        let modules: Vec<_> = program
            .modules
            .iter()
            .filter(|m| m.id == self.caller.module)
            .collect();
        let [module] = modules.as_slice() else {
            return None;
        };
        let functions: Vec<_> = module
            .functions
            .iter()
            .filter(|f| f.id == self.caller)
            .collect();
        let [f] = functions.as_slice() else {
            return None;
        };
        self.validate(f).ok()?;
        if !f.return_types.is_empty()
            || f.never_returns
            || f.borrow_return.is_some()
            || f.carrier_return.is_some()
            || f.receiver.is_some()
        {
            return None;
        }
        let [entry, header, split, finished, exit] = p.blocks?;
        let [actual_split, left, right] = p.split?;
        let join = p.join?;
        // The ordinary source has no suffix, so the producer folds finished into exit.
        if split != actual_split || finished != exit {
            return None;
        }
        let refs = [entry, header, split, left, right, join, exit];
        let ids = refs
            .map(|b| self.evidence.block(b, s).ok())
            .into_iter()
            .collect::<Option<Vec<_>>>()?;
        let covered = ids.iter().copied().collect::<BTreeSet<_>>();
        if covered.len() != 7 || f.entry_block() != Some(ids[0]) {
            return None;
        }
        if self.evidence.blocks.len() != 7
            || self
                .evidence
                .blocks
                .values()
                .copied()
                .collect::<BTreeSet<_>>()
                != covered
        {
            return None;
        }
        if self.evidence.parameters.get(&entry) != Some(&self.parameters) {
            return None;
        }
        let entry_block = f.block(ids[0])?;
        if entry_block.parameters.len() != abi.len() {
            return None;
        }
        for (index, (entity, (ty, mode))) in entry_block.parameters.iter().zip(abi).enumerate() {
            let kind = if index == roles[0] {
                SsaTypeKind::StringOwner
            } else {
                SsaTypeKind::Boolean
            };
            let expected = match (mode, entity) {
                (ParameterMode::Borrow, EntityId::Loan(_)) => EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: *ty,
                },
                (ParameterMode::Value, EntityId::Value(_)) if index != roles[0] => {
                    EntityType::Value(*ty)
                }
                _ => return None,
            };
            if ty.module != f.id.module
                || module.type_kind(*ty) != Some(&kind)
                || f.entity(*entity)?.ty != expected
            {
                return None;
            }
        }
        let mut instructions = BTreeSet::new();
        for (i, b) in refs.iter().enumerate() {
            let list = self.evidence.ids(f, *b, s).ok()?;
            let count = match i {
                0 => usize::from(prefix.is_some()),
                1 | 2 => usize::from(abi[roles[i]].1 == ParameterMode::Borrow),
                3 => calls[0].len() + 1,
                4 => calls[1].len() + 1,
                _ => 0,
            };
            if list.len() != count {
                return None;
            }
            for instruction in list {
                if !instructions.insert(instruction) {
                    return None;
                }
            }
        }
        if let Some(prefix) = prefix {
            let [instruction] = entry_block.instructions.as_slice() else {
                return None;
            };
            let call = f.instruction(*instruction)?;
            if call.operation
                != (Operation::DirectCall {
                    callee: prefix.callee,
                    receiver: None,
                    arguments: vec![entry_block.parameters[roles[0]]],
                })
                || !call.results.is_empty()
                || call.origin != crate::ssa::model::Origin::Source(prefix.span)
            {
                return None;
            }
            let ending = self.evidence.ends.get(&ids[0])?;
            for (slot, entity) in self.parameters.iter().zip(&entry_block.parameters) {
                if ending.get(slot) != Some(entity) {
                    return None;
                }
            }
        }
        let exit_block = f.block(ids[6])?;
        if !exit_block.parameters.is_empty()
            || !matches!(&exit_block.terminator.as_ref()?.kind,TerminatorKind::Return{values} if values.is_empty())
            || f.blocks
                .iter()
                .filter(|b| {
                    matches!(
                        b.terminator.as_ref().map(|t| &t.kind),
                        Some(TerminatorKind::Return { .. })
                    )
                })
                .count()
                != 1
        {
            return None;
        }
        // This reusable structural audit receives producer-role-derived sets, never the final universe.
        super::super::fresh_loop_receipt::audit_whole_function(f, &covered, &instructions, s)
            .ok()?;
        // Every entity's semantic type must exist and lie in the independently typed ABI domain.
        for entity in f.values.iter().chain(&f.places).chain(&f.loans) {
            match entity.ty {
                EntityType::Value(t)
                    if t == abi[roles[1]].0
                        && module.type_kind(t) == Some(&SsaTypeKind::Boolean) => {}
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                } if abi.iter().any(|(ty, _)| *ty == target)
                    && module.type_kind(target).is_some() => {}
                _ => return None,
            }
        }
        let mut edges = Vec::new();
        for (i, b) in [left, right].iter().enumerate() {
            let list = self.evidence.ids(f, *b, s).ok()?;
            let inst = f.instruction(*list.first()?)?;
            let Operation::BorrowCall { callee, .. } = inst.operation else {
                return None;
            };
            if callee != calls[i][0] {
                return None;
            }
            edges.push((f.id, inst.id, callee));
        }
        edges.try_into().ok()
    }
}
