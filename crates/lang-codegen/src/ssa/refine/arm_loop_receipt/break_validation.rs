//! Final SSA evidence for a single tail break after two independently closed arms.
use super::*;
use crate::ssa::model::{Origin, TerminatorKind};
impl Emitted {
    pub(super) fn validate_break(
        &self,
        parameters: &[Slot],
        f: &Function,
        p: &Plan,
        s: Span,
    ) -> Result<(), LoweringError> {
        let tail = p.tail_break.as_ref().ok_or_else(|| invalid(s))?;
        let [entry, header, split, finished, exit] = p.blocks.ok_or_else(|| invalid(s))?;
        let [actual_split, arm_a, arm_b] = p.split.ok_or_else(|| invalid(s))?;
        let join = p.join.ok_or_else(|| invalid(s))?;
        if p.counter.is_some()
            || split != actual_split
            || tail.observed != Some((join, exit))
            || p.used[0].map(|u| u.0) != Some(header)
            || p.used[1].map(|u| u.0) != Some(split)
        {
            return Err(invalid(s));
        }
        // Only the empty false-path block may fold into the loop exit.
        let mut distinct = Vec::new();
        for b in [entry, header, split, arm_a, arm_b, join, exit] {
            let id = self.block(b, s)?;
            if distinct.contains(&id) {
                return Err(invalid(s));
            }
            distinct.push(id);
        }
        if finished != exit && distinct.contains(&self.block(finished, s)?) {
            return Err(invalid(s));
        }
        for (from, to) in [
            (entry, vec![header]),
            (header, vec![split, finished]),
            (split, vec![arm_a, arm_b]),
            (arm_a, vec![join]),
            (arm_b, vec![join]),
            (join, vec![exit]),
        ] {
            self.links(f, from, &to, s)?;
        }
        // Complete predecessor sets prove no reentry or bypass of either arm's End.
        for (to, from) in [
            (header, vec![entry]),
            (split, vec![header]),
            (arm_a, vec![split]),
            (arm_b, vec![split]),
            (join, vec![arm_a, arm_b]),
        ] {
            self.incoming(f, to, &from, s)?;
        }
        if finished == exit {
            self.incoming(f, exit, &[header, join], s)?;
        } else {
            self.links(f, finished, &[exit], s)?;
            self.incoming(f, finished, &[header], s)?;
            self.incoming(f, exit, &[finished, join], s)?;
            if !self.ids(f, finished, s)?.is_empty() {
                return Err(invalid(s));
            }
        }
        for b in [entry, header, split, arm_a, arm_b, join, finished, exit] {
            if self
                .parameters
                .get(&b)
                .ok_or_else(|| invalid(s))?
                .iter()
                .any(|slot| !parameters.contains(slot))
            {
                return Err(invalid(s));
            }
        }
        // A legitimate preheader may have prefix predecessors. Instead of
        // forbidding them, prove the exit cannot route back into this slice.
        let mut pending = vec![self.block(exit, s)?];
        let mut seen = Vec::new();
        while let Some(id) = pending.pop() {
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            let term = f
                .block(id)
                .and_then(|b| b.terminator.as_ref())
                .ok_or_else(|| invalid(s))?;
            for target in term.kind.targets() {
                if [entry, header, split, arm_a, arm_b, join]
                    .iter()
                    .any(|b| self.blocks.get(b) == Some(&target))
                    || (finished != exit && self.block(finished, s)? == target)
                {
                    return Err(invalid(s));
                }
                pending.push(target);
            }
        }
        self.condition(parameters, f, p, 0, s)?;
        self.condition(parameters, f, p, 1, s)?;
        let term = f
            .block(self.block(join, s)?)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(s))?;
        if !matches!(&term.kind, TerminatorKind::Branch(_))
            || term.origin != Origin::Source(tail.span)
            || !self.ids(f, join, s)?.is_empty()
        {
            return Err(invalid(s));
        }
        Ok(())
    }
}
