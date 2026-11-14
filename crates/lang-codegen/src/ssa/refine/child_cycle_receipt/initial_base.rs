//! A bounded initial-owner base: direct function entry, own parameter or literal.
use super::*;
use crate::ssa::model::{Definition, EntityType};
impl Emitted {
    pub(super) fn initial_base(
        &self,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || invalid(span);
        let p = self.plan.as_ref().ok_or_else(fail)?;
        if p.scalar_entry.is_some() {
            return self.scalar_initial_base(function, span);
        }
        let Some(initial) = p.initial_base else {
            return Ok(());
        };
        let entry = function.block(self.blocks[&initial]).ok_or_else(fail)?;
        if function.entry_block() != Some(entry.id) {
            return Err(fail());
        }
        let TerminatorKind::Branch(edge) = &entry.terminator.as_ref().ok_or_else(fail)?.kind else {
            return Err(fail());
        };
        self.edge(function, initial, p.parent_header, edge, span)?;
        let mut owners = BTreeSet::new();
        for root in &p.roots {
            let owner = *self.seen[&initial].get(root).ok_or_else(fail)?;
            if !matches!(owner, EntityId::Value(_)) || !owners.insert(owner) {
                return Err(fail());
            }
            let data = function.entity(owner).ok_or_else(fail)?;
            if !matches!(data.ty, EntityType::Value(_)) {
                return Err(fail());
            }
            match data.definition {
                Definition::BlockParameter { block, index } => {
                    if block != entry.id || entry.parameters.get(index) != Some(&owner) {
                        return Err(fail());
                    }
                }
                Definition::InstructionResult { instruction, index } => {
                    let i = function.instruction(instruction).ok_or_else(fail)?;
                    if self.initial_sources.get(&owner) != Some(&(instruction, i.operation.clone()))
                        || index != 0
                        || i.block != entry.id
                        || i.results != [owner]
                        || !matches!(
                            i.operation,
                            Operation::StringLiteral { .. } | Operation::StringConcat { .. }
                        )
                        || entry
                            .instructions
                            .iter()
                            .filter(|id| **id == instruction)
                            .count()
                            != 1
                    {
                        return Err(fail());
                    }
                    if matches!(i.operation, Operation::StringConcat { .. }) {
                        super::initial_concat::verify(
                            function,
                            entry.id,
                            instruction,
                            owner,
                            span,
                        )?;
                    }
                }
            }
            // No direct use, borrow, or old-owner consumption can precede entry
            // transport; later blocks must refer to their renamed parameters.
            if function
                .instructions
                .iter()
                .any(|i| i.operation.entities().contains(&owner))
            {
                return Err(fail());
            }
            for block in &function.blocks {
                let term = &block.terminator.as_ref().ok_or_else(fail)?.kind;
                if term.targets().contains(&entry.id)
                    || (block.id != entry.id && block.parameters.contains(&owner))
                    || term.entities().iter().filter(|e| **e == owner).count()
                        != usize::from(block.id == entry.id)
                {
                    return Err(fail());
                }
            }
            let definitions = function
                .instructions
                .iter()
                .filter(|i| i.results.contains(&owner))
                .count();
            if definitions
                != usize::from(matches!(
                    data.definition,
                    Definition::InstructionResult { .. }
                ))
            {
                return Err(fail());
            }
        }
        #[cfg(test)]
        super::super::build::owner_consumer_test_support::record_action(
            "parent-initial-owner-base-ssa-complete",
            p.roots.len(),
        );
        Ok(())
    }
}
