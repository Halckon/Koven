//! The direct parent true edge supplies the child preheader's current vector.
//! This conditional entry lemma does not establish the parent's initial owners.
use super::*;
impl Emitted {
    pub(super) fn parent_entry(
        &self,
        body: &RefinedBody,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || invalid(span);
        let p = self.plan.as_ref().ok_or_else(fail)?;
        let parent = function
            .block(self.blocks[&p.parent_header])
            .ok_or_else(fail)?;
        let entry = function.block(self.blocks[&p.entry]).ok_or_else(fail)?;
        let Some((
            RTerm::Branch {
                condition: guard,
                when_true,
                ..
            },
            _,
        )) = body
            .blocks
            .get(p.parent_header.0)
            .and_then(|b| b.terminator.as_ref())
        else {
            return Err(fail());
        };
        let TerminatorKind::Conditional {
            condition,
            when_true: edge,
            when_false,
        } = &parent.terminator.as_ref().ok_or_else(fail)?.kind
        else {
            return Err(fail());
        };
        if *when_true != p.entry
            || when_false.target == entry.id
            || self.seen[&p.parent_header].get(guard) != Some(&EntityId::Value(*condition))
        {
            return Err(fail());
        }
        self.edge(function, p.parent_header, p.entry, edge, span)?;
        let mut predecessors = Vec::new();
        for block in &function.blocks {
            let term = &block.terminator.as_ref().ok_or_else(fail)?.kind;
            for target in term.targets() {
                if target == entry.id {
                    predecessors.push(block.id);
                }
            }
        }
        if predecessors != [parent.id] {
            return Err(fail());
        }
        // No root is regenerated, borrowed or consumed in the scalar guard or
        // scalar child initialization. Every child header input comes from its
        // same-root parent parameter, through the actual emitted edge vector.
        for &root in &p.roots {
            for role in [p.parent_header, p.entry] {
                let value = self.parameter(role, root, span)?;
                if self.seen[&role].get(&root) != Some(&value) {
                    return Err(fail());
                }
                let block = function.block(self.blocks[&role]).ok_or_else(fail)?;
                for id in &block.instructions {
                    let op = function.instruction(*id).ok_or_else(fail)?;
                    if op.results.contains(&value) || op.operation.entities().contains(&value) {
                        return Err(fail());
                    }
                }
                let terminator = &block.terminator.as_ref().ok_or_else(fail)?.kind;
                let edges = match terminator {
                    TerminatorKind::Branch(e) => vec![e],
                    TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } => vec![when_true, when_false],
                    _ => return Err(fail()),
                };
                if edges
                    .iter()
                    .any(|edge| edge.arguments.iter().filter(|e| **e == value).count() != 1)
                {
                    return Err(fail());
                }
            }
        }
        #[cfg(test)]
        super::super::build::owner_consumer_test_support::record_action(
            "child-parent-entry-ssa-complete",
            p.roots.len(),
        );
        Ok(())
    }
}
