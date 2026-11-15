//! Feature-only exhaustive bounded-counter caller closure, not the general receipt's local replay.
use super::*;
use crate::ssa::model::{
    EntityType, LoanKind, Operation, Program, SsaTypeId, SsaTypeKind, TerminatorKind,
};
use std::collections::BTreeSet;
type CounterBreakDispatch<'a> = (
    &'a [Vec<FunctionId>; 2],
    [Span; 5],
    (UnitExpressionId, Span, Span),
);
impl Receipt {
    pub(crate) fn certify_counter_break(
        &self,
        program: &Program,
        wrapper: &UnitLoopCounterBreakBranchNoCarryFact,
        abi: &[(SsaTypeId, ParameterMode)],
        roles: &[usize; 4],
        conditions: &[(UnitExpressionId, Span); 2],
        dispatch: CounterBreakDispatch<'_>,
    ) -> Option<[(FunctionId, InstructionId, FunctionId); 2]> {
        let (calls, spans, exit_fact) = dispatch;
        let cb = self.plan.counter_break.as_ref()?;
        if !cb.fact.same_identity(wrapper)
            || cb.fact != *wrapper
            || cb.fact.break_condition() != exit_fact.0
            || cb.condition_span != exit_fact.1
            || cb.break_span != exit_fact.2
            || cb.role != roles[3]
        {
            return None;
        }
        let wrapper = wrapper.counter();
        let fact = wrapper.pair();
        let p = &self.plan;
        let s = self.span;
        if p.prefix.is_some()
            || p.counter.is_none()
            || p.tail_break.is_some()
            || p.counter_fact.is_some()
            || !p.fact.same_identity(fact)
            || p.fact != *fact
            || p.roles != [roles[0], roles[1], roles[2]]
            || abi.len() != 4
            || abi.iter().any(|(_, mode)| *mode != ParameterMode::Borrow)
            || self.parameters.len() != abi.len()
            || roles.iter().copied().collect::<BTreeSet<_>>() != (0..abi.len()).collect()
            || roles[0] == roles[1]
            || roles[0] == roles[2]
            || roles[1] == roles[2]
            || p.conditions != conditions.map(|c| c.0)
            || p.condition_spans != conditions.map(|c| c.1)
        {
            return None;
        }
        let original = cb.fact.counter();
        let counter = p.counter.as_ref()?;
        if !original.same_identity(wrapper)
            || original != wrapper
            || counter.spec.counter != wrapper.counter()
            || counter.spec.rounds != wrapper.rounds()
            || counter.spec.initializer != wrapper.initializer()
            || counter.spec.update_rhs != wrapper.decrement()
            || [
                counter.spec.initializer_span,
                counter.spec.condition_span,
                counter.spec.zero_span,
                counter.spec.one_span,
                counter.spec.update_span,
            ] != spans
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
        let ([op, success, abort], [result_slot, failed_slot, _left_slot, one_slot]) =
            counter.checked?;
        let [gate, yes, no] = cb.split?;
        let continued = cb.continued?;
        if split != actual_split
            || finished == exit
            || op != continued
            || no != continued
            || join != gate
            || cb.used?.0 != gate
            || cb.observed != Some((yes, exit))
        {
            return None;
        }
        let refs = [
            entry, header, split, left, right, gate, yes, continued, success, abort, finished, exit,
        ];
        let ids = refs
            .map(|b| self.evidence.block(b, s).ok())
            .into_iter()
            .collect::<Option<Vec<_>>>()?;
        let covered = ids.iter().copied().collect::<BTreeSet<_>>();
        if covered.len() != 12 || f.entry_block() != Some(ids[0]) {
            return None;
        }
        if self.evidence.blocks.len() != 12
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
            } else if index == roles[1] {
                SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                }
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
                0 | 2 | 5 => 1,
                1 | 7 => 2,
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
        let exit_block = f.block(ids[11])?;
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
        let int_type = abi[roles[1]].0;
        let bool_type = abi[roles[2]].0;
        if module.type_kind(int_type)
            != Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
            || module.type_kind(bool_type) != Some(&SsaTypeKind::Boolean)
        {
            return None;
        }
        let join_ids = self.evidence.ids(f, continued, s).ok()?;
        let one = f.instruction(*join_ids.first()?)?;
        let [EntityId::Value(one_value)] = one.results.as_slice() else {
            return None;
        };
        if one.operation != Operation::Constant(crate::ssa::model::ScalarConstant::Integer(1))
            || one.origin != crate::ssa::model::Origin::Source(spans[3])
            || f.entity(EntityId::Value(*one_value))?.ty != EntityType::Value(int_type)
        {
            return None;
        }
        let arithmetic = f.instruction(*join_ids.get(1)?)?;
        let [EntityId::Value(result), EntityId::Value(failed)] = arithmetic.results.as_slice()
        else {
            return None;
        };
        if f.entity(EntityId::Value(*result))?.ty != EntityType::Value(int_type)
            || f.entity(EntityId::Value(*failed))?.ty != EntityType::Value(bool_type)
            || self.evidence.ends.get(&ids[7])?.get(&result_slot) != Some(&EntityId::Value(*result))
            || self.evidence.ends.get(&ids[7])?.get(&failed_slot) != Some(&EntityId::Value(*failed))
        {
            return None;
        }
        let abort_block = f.block(ids[9])?;
        let term = abort_block.terminator.as_ref()?;
        let reason = term.abort.clone()?;
        let crate::ssa::model::AbortReason::CheckedArithmetic {
            operator: crate::ssa::model::CheckedArithmeticOperator::Subtract,
            divisor,
        } = reason
        else {
            return None;
        };
        // Follow the actual checked edge's operand transport of original constant one.
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &f.block(ids[7])?.terminator.as_ref()?.kind
        else {
            return None;
        };
        if *condition != *failed || when_true.target != ids[9] || when_false.target != ids[8] {
            return None;
        }
        let actual_one = if let Some(index) = self
            .evidence
            .parameters
            .get(&abort)?
            .iter()
            .position(|slot| *slot == one_slot)
        {
            if when_true.arguments.get(index) != Some(&EntityId::Value(*one_value)) {
                return None;
            }
            *abort_block.parameters.get(index)?
        } else {
            EntityId::Value(*one_value)
        };
        if actual_one != EntityId::Value(divisor)
            || term.origin != crate::ssa::model::Origin::Source(spans[4])
        {
            return None;
        }
        super::super::fresh_loop_receipt::audit_counter_whole_function(
            f,
            &covered,
            &instructions,
            s,
            (ids[9], reason, spans[4], int_type),
        )
        .ok()?;
        // Every entity's semantic type must exist and lie in the independently typed ABI domain.
        for entity in f.values.iter().chain(&f.places).chain(&f.loans) {
            match entity.ty {
                EntityType::Value(t)
                    if (t == int_type
                        && module.type_kind(t)
                            == Some(&SsaTypeKind::Integer {
                                bits: 32,
                                signed: true,
                            }))
                        || (t == bool_type
                            && module.type_kind(t) == Some(&SsaTypeKind::Boolean)) => {}
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
