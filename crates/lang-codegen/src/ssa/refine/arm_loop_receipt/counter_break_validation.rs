//! Exact conditional exit and continuing path, additional to the counter receipt.
use super::*;
use crate::ssa::model::{Origin, TerminatorKind};
impl Emitted {
    pub(super) fn validate_counter_break(
        &self,
        parameters: &[Slot],
        f: &Function,
        p: &Plan,
        s: Span,
    ) -> Result<(), LoweringError> {
        let cb = p.counter_break.as_ref().ok_or_else(|| invalid(s))?;
        let [gate, yes, no] = cb.split.ok_or_else(|| invalid(s))?;
        let continued = cb.continued.ok_or_else(|| invalid(s))?;
        let [entry, header, split, finished, exit] = p.blocks.ok_or_else(|| invalid(s))?;
        let [_, a, b] = p.split.ok_or_else(|| invalid(s))?;
        let (checked, _) = p
            .counter
            .as_ref()
            .and_then(|c| c.checked)
            .ok_or_else(|| invalid(s))?;
        if p.join != Some(gate)
            || cb.observed != Some((yes, exit))
            || cb.used.map(|u| u.0) != Some(gate)
            || checked[0] != continued
            || p.tail_break.is_some()
            || p.prefix
                .as_ref()
                .is_some_and(|prefix| !prefix.is_counter_composition())
        {
            return Err(invalid(s));
        }
        let mut distinct = Vec::new();
        for block in [
            entry, header, split, a, b, gate, yes, continued, checked[1], checked[2], finished,
            exit,
        ] {
            let id = self.block(block, s)?;
            if distinct.contains(&id) {
                return Err(invalid(s));
            }
            distinct.push(id);
        }
        if no != continued && distinct.contains(&self.block(no, s)?) {
            return Err(invalid(s));
        }
        self.links(f, gate, &[yes, no], s)?;
        self.links(f, yes, &[exit], s)?;
        self.incoming(f, yes, &[gate], s)?;
        self.incoming(f, no, &[gate], s)?;
        if no != continued {
            self.links(f, no, &[continued], s)?;
            self.incoming(f, continued, &[no], s)?;
            if !self.ids(f, no, s)?.is_empty() {
                return Err(invalid(s));
            }
        }
        let mut condition = p.clone();
        condition.used[1] = cb.used;
        condition.roles[2] = cb.role;
        condition.condition_spans[1] = cb.condition_span;
        self.condition(parameters, f, &condition, 1, s)?;
        let term = f
            .block(self.block(yes, s)?)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(s))?;
        if !matches!(term.kind, TerminatorKind::Branch(_))
            || term.origin != Origin::Source(cb.break_span)
            || !self.ids(f, yes, s)?.is_empty()
        {
            return Err(invalid(s));
        }
        // No exit may reenter the selected cycle, even through unrecorded intermediate blocks.
        let protected = [
            entry, header, split, a, b, gate, yes, no, continued, checked[1], checked[2], finished,
        ];
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
                if protected
                    .iter()
                    .any(|b| self.blocks.get(b) == Some(&target))
                {
                    return Err(invalid(s));
                }
                pending.push(target)
            }
        }
        Ok(())
    }
}
