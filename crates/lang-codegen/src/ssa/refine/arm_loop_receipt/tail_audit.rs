//! Feature-only exhaustive tail-break caller closure, not the general receipt's local replay.
use super::*;
use crate::ssa::model::{
    EntityType, LoanKind, Operation, Program, SsaTypeId, SsaTypeKind, TerminatorKind,
};
use std::collections::BTreeSet;
type TailDispatch<'a> = (
    &'a [Vec<FunctionId>; 2],
    (UnitExpressionId, Span),
    Option<&'a super::super::fresh_loop_receipt::SourceConsumer>,
    Option<&'a super::super::fresh_loop_receipt::SourceConsumer>,
);
impl Receipt {
    pub(crate) fn certify_tail_break(
        &self,
        program: &Program,
        wrapper: &UnitLoopBreakBranchNoCarryFact,
        abi: &[(SsaTypeId, ParameterMode)],
        roles: &[usize; 3],
        conditions: &[(UnitExpressionId, Span); 2],
        dispatch: TailDispatch<'_>,
    ) -> Option<[(FunctionId, InstructionId, FunctionId); 2]> {
        let (calls, transfer, prefix, suffix) = dispatch;
        let fact = wrapper.pair();
        let p = &self.plan;
        let s = self.span;
        if p.prefix.is_some()
            || p.counter.is_some()
            || p.counter_fact.is_some()
            || p.counter_break.is_some()
            || !p.fact.same_identity(fact)
            || p.fact != *fact
            || p.roles != *roles
            || abi.len() != 3
            || abi
                .iter()
                .any(|(_, mode)| !matches!(mode, ParameterMode::Borrow | ParameterMode::Value))
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
        let tail = p.tail_break.as_ref()?;
        if !tail.fact.same_identity(wrapper)
            || tail.fact != *wrapper
            || tail.expression != transfer.0
            || tail.span != transfer.1
            || wrapper.exit_target() != fact.arms()[0].loop_statement()
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
        // This exact tail-break producer retains its independent false-path block.
        if split != actual_split || finished == exit || tail.observed != Some((join, exit)) {
            return None;
        }
        let refs = [entry, header, split, left, right, join, finished, exit];
        let ids = refs
            .map(|b| self.evidence.block(b, s).ok())
            .into_iter()
            .collect::<Option<Vec<_>>>()?;
        let covered = ids.iter().copied().collect::<BTreeSet<_>>();
        if covered.len() != 8 || f.entry_block() != Some(ids[0]) {
            return None;
        }
        if self.evidence.blocks.len() != 8
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
                7 => usize::from(suffix.is_some()),
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
        if let Some(consumer) = prefix {
            let [instruction] = entry_block.instructions.as_slice() else {
                return None;
            };
            let call = f.instruction(*instruction)?;
            if call.operation
                != (Operation::DirectCall {
                    callee: consumer.callee,
                    receiver: None,
                    arguments: vec![entry_block.parameters[roles[0]]],
                })
                || !call.results.is_empty()
                || call.origin != crate::ssa::model::Origin::Source(consumer.span)
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
        let exit_block = f.block(ids[7])?;
        let source_slot = *self.parameters.get(roles[0])?;
        if let Some(consumer) = suffix {
            if self.evidence.parameters.get(&exit)?.as_slice() != [source_slot]
                || exit_block.parameters.len() != 1
                || exit_block.parameters[0] != self.evidence.entity(exit, source_slot, s).ok()?
            {
                return None;
            }
            // Independently carry original ABI roles through actual edges, not captured ends.
            let mut actual_roles: std::collections::BTreeMap<_, _> = entry_block
                .parameters
                .iter()
                .copied()
                .enumerate()
                .map(|(i, entity)| (entity, i))
                .collect();
            for _ in 0..refs.len() {
                for id in &ids {
                    let block = f.block(*id)?;
                    let edges = match &block.terminator.as_ref()?.kind {
                        TerminatorKind::Branch(edge) => vec![edge],
                        TerminatorKind::Conditional {
                            when_true,
                            when_false,
                            ..
                        } => vec![when_true, when_false],
                        TerminatorKind::Return { .. } => vec![],
                        _ => return None,
                    };
                    for edge in edges {
                        let target = f.block(edge.target)?;
                        if edge.arguments.len() != target.parameters.len() {
                            return None;
                        }
                        for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                            if let Some(role) = actual_roles.get(argument).copied()
                                && let Some(previous) = actual_roles.insert(*parameter, role)
                                && previous != role
                            {
                                return None;
                            }
                        }
                    }
                }
            }
            // Every predecessor must substantiate a known target role; an unknown
            // incoming value cannot borrow the role established by another arm.
            for id in &ids {
                let block = f.block(*id)?;
                let edges = match &block.terminator.as_ref()?.kind {
                    TerminatorKind::Branch(edge) => vec![edge],
                    TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } => vec![when_true, when_false],
                    TerminatorKind::Return { .. } => vec![],
                    _ => return None,
                };
                for edge in edges {
                    let target = f.block(edge.target)?;
                    for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                        if let Some(role) = actual_roles.get(parameter)
                            && actual_roles.get(argument) != Some(role)
                        {
                            return None;
                        }
                    }
                }
            }
            if actual_roles.get(&exit_block.parameters[0]) != Some(&roles[0]) {
                return None;
            }
            for predecessor in [join, finished] {
                let block = f.block(self.evidence.block(predecessor, s).ok()?)?;
                let TerminatorKind::Branch(edge) = &block.terminator.as_ref()?.kind else {
                    return None;
                };
                if edge.target != exit_block.id
                    || edge.arguments.len() != 1
                    || actual_roles.get(&edge.arguments[0]) != Some(&roles[0])
                {
                    return None;
                }
            }
            let [instruction] = exit_block.instructions.as_slice() else {
                return None;
            };
            let call = f.instruction(*instruction)?;
            if call.operation
                != (Operation::DirectCall {
                    callee: consumer.callee,
                    receiver: None,
                    arguments: vec![exit_block.parameters[0]],
                })
                || !call.results.is_empty()
                || call.origin != crate::ssa::model::Origin::Source(consumer.span)
            {
                return None;
            }
        } else if !exit_block.parameters.is_empty() {
            return None;
        }
        if !matches!(&exit_block.terminator.as_ref()?.kind,TerminatorKind::Return{values} if values.is_empty())
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
