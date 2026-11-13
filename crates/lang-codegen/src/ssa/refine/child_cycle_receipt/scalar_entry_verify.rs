//! Owner-preserving prefix diamond leading to the existing parent recurrence.
use super::*;
use crate::ssa::model::{Definition, Origin, ScalarConstant};
impl Emitted {
    pub(super) fn scalar_initial_base(
        &self,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || invalid(span);
        let p = self.plan.as_ref().ok_or_else(fail)?;
        let prefix = p.scalar_entry.as_ref().ok_or_else(fail)?;
        let [head, a, b] = prefix.split.ok_or_else(fail)?;
        let join = prefix.join.ok_or_else(fail)?;
        let roles = [head, a, b, join];
        if roles.into_iter().collect::<BTreeSet<_>>().len() != 4
            || p.initial_base != Some(join)
            || function.entry_block() != Some(self.blocks[&head])
        {
            return Err(fail());
        }
        let block = |role| function.block(self.blocks[&role]).ok_or_else(fail);
        for (target, predecessors) in [
            (head, vec![]),
            (a, vec![head]),
            (b, vec![head]),
            (join, vec![a, b]),
        ] {
            let mut actual = Vec::new();
            for source in &function.blocks {
                for destination in source.terminator.as_ref().ok_or_else(fail)?.kind.targets() {
                    if destination == self.blocks[&target] {
                        actual.push(source.id);
                    }
                }
            }
            actual.sort();
            let mut expected = predecessors
                .iter()
                .map(|r| self.blocks[r])
                .collect::<Vec<_>>();
            expected.sort();
            if actual != expected {
                return Err(fail());
            }
        }
        let (used_block, used_slot) = prefix.used.ok_or_else(fail)?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &block(head)?.terminator.as_ref().ok_or_else(fail)?.kind
        else {
            return Err(fail());
        };
        if used_block != head
            || self.seen[&head].get(&used_slot) != Some(&EntityId::Value(*condition))
        {
            return Err(fail());
        }
        self.edge(function, head, a, when_true, span)?;
        self.edge(function, head, b, when_false, span)?;
        for (index, arm) in [a, b].into_iter().enumerate() {
            let body = block(arm)?;
            let [constant_id, call_id] = body.instructions.as_slice() else {
                return Err(fail());
            };
            if constant_id == call_id {
                return Err(fail());
            }
            let constant = function.instruction(*constant_id).ok_or_else(fail)?;
            let call = function.instruction(*call_id).ok_or_else(fail)?;
            let (callee, call_span, value, arg_span) = prefix.calls[index];
            if constant.block != body.id
                || constant.results.len() != 1
                || !matches!(constant.results[0], EntityId::Value(_))
                || constant.origin != Origin::Source(arg_span)
                || constant.operation != Operation::Constant(ScalarConstant::Integer(value))
                || call.block != body.id
                || call.origin != Origin::Source(call_span)
                || !call.results.is_empty()
                || call.operation
                    != (Operation::DirectCall {
                        callee,
                        receiver: None,
                        arguments: constant.results.clone(),
                    })
            {
                return Err(fail());
            }
            let TerminatorKind::Branch(edge) = &body.terminator.as_ref().ok_or_else(fail)?.kind
            else {
                return Err(fail());
            };
            self.edge(function, arm, join, edge, span)?;
        }
        let TerminatorKind::Branch(edge) = &block(join)?.terminator.as_ref().ok_or_else(fail)?.kind
        else {
            return Err(fail());
        };
        self.edge(function, join, p.parent_header, edge, span)?;
        let mut owners = BTreeSet::new();
        for root in &p.roots {
            let owner = *self.seen[&head].get(root).ok_or_else(fail)?;
            if !matches!(owner, EntityId::Value(_)) || !owners.insert(owner) {
                return Err(fail());
            }
            let data = function.entity(owner).ok_or_else(fail)?;
            let producer = match data.definition {
                Definition::BlockParameter {
                    block: defined,
                    index,
                } => {
                    if defined != block(head)?.id
                        || block(head)?.parameters.get(index) != Some(&owner)
                    {
                        return Err(fail());
                    }
                    None
                }
                Definition::InstructionResult {
                    instruction,
                    index: 0,
                } => {
                    let source = function.instruction(instruction).ok_or_else(fail)?;
                    if source.block != block(head)?.id
                        || source.results != [owner]
                        || !matches!(source.operation, Operation::StringLiteral { .. })
                        || self.initial_sources.get(&owner)
                            != Some(&(instruction, source.operation.clone()))
                        || block(head)?
                            .instructions
                            .iter()
                            .filter(|id| **id == instruction)
                            .count()
                            != 1
                    {
                        return Err(fail());
                    }
                    Some(instruction)
                }
                _ => return Err(fail()),
            };
            for instruction in &function.instructions {
                if instruction.operation.entities().contains(&owner)
                    || instruction.results.contains(&owner) != (Some(instruction.id) == producer)
                {
                    return Err(fail());
                }
            }
            for other in &function.blocks {
                if other.parameters.iter().filter(|e| **e == owner).count()
                    != usize::from(other.id == block(head)?.id && producer.is_none())
                {
                    return Err(fail());
                }
                let count = other
                    .terminator
                    .as_ref()
                    .ok_or_else(fail)?
                    .kind
                    .entities()
                    .iter()
                    .filter(|e| **e == owner)
                    .count();
                if count != if other.id == block(head)?.id { 2 } else { 0 } {
                    return Err(fail());
                }
            }
            for role in [a, b, join] {
                let value = self.parameter(role, *root, span)?;
                let role_block = block(role)?;
                let index = role_block
                    .parameters
                    .iter()
                    .position(|e| *e == value)
                    .ok_or_else(fail)?;
                if !matches!(value, EntityId::Value(_))
                    || !matches!(
                        function.entity(value).ok_or_else(fail)?.ty,
                        crate::ssa::model::EntityType::Value(_)
                    )
                    || function.entity(value).ok_or_else(fail)?.definition
                        != (Definition::BlockParameter {
                            block: role_block.id,
                            index,
                        })
                {
                    return Err(fail());
                }
                if self.seen[&role].get(root) != Some(&value) {
                    return Err(fail());
                }
                for instruction in &function.instructions {
                    if instruction.results.contains(&value)
                        || instruction.operation.entities().contains(&value)
                    {
                        return Err(fail());
                    }
                }
                for other in &function.blocks {
                    if other.parameters.iter().filter(|e| **e == value).count()
                        != usize::from(other.id == block(role)?.id)
                        || other
                            .terminator
                            .as_ref()
                            .ok_or_else(fail)?
                            .kind
                            .entities()
                            .iter()
                            .filter(|e| **e == value)
                            .count()
                            != usize::from(other.id == block(role)?.id)
                    {
                        return Err(fail());
                    }
                }
                let TerminatorKind::Branch(edge) =
                    &block(role)?.terminator.as_ref().ok_or_else(fail)?.kind
                else {
                    return Err(fail());
                };
                if edge.arguments.iter().filter(|e| **e == value).count() != 1 {
                    return Err(fail());
                }
            }
        }
        #[cfg(test)]
        super::super::build::owner_consumer_test_support::record_action(
            "parent-scalar-entry-ssa-complete",
            p.roots.len(),
        );
        #[cfg(test)]
        super::super::build::owner_consumer_test_support::record_action(
            "parent-initial-owner-base-ssa-complete",
            p.roots.len(),
        );
        Ok(())
    }
}
