//! Shared H transport proof; consumers provide their own endpoint authorization.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn validate_header_transport(&self, span: Span) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let p = self.header_return_plan.as_ref().ok_or_else(fail)?;
        let [entry, header, body, finished, exit] = p.geometry.ok_or_else(fail)?;
        let slot = p.root.ok_or_else(fail)?;
        let old = p.old.ok_or_else(fail)?;
        let detached = p.detached.ok_or_else(fail)?;
        let initial = p.produced[0].ok_or_else(fail)?;
        let replacement = p.produced[1].ok_or_else(fail)?;
        let incoming = |target| {
            self.body
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(i, b)| {
                    b.terminator
                        .as_ref()
                        .filter(|(t, _)| t.successors().contains(&target))
                        .map(|_| BlockRef(i))
                })
                .collect::<Vec<_>>()
        };
        let backedge = match (p.boundary.increment(), p.increment) {
            (None, None) => body,
            (Some(_), Some(([origin, success, abort], [result, failed, left, right], index))) => {
                let instruction = self.body.blocks[body.0]
                    .statements
                    .get(index)
                    .ok_or_else(fail)?;
                let definition_count = |slot| {
                    self.body
                        .blocks
                        .iter()
                        .flat_map(|b| &b.statements)
                        .filter(|s| s.results.contains(&slot))
                        .count()
                };
                if instruction.results != [result, failed]
                    || !matches!(instruction.op, ROp::CheckedArithmetic {operator: crate::ssa::model::CheckedArithmeticOperator::Add, left: l, right: r} if l == left && r == right)
                    || definition_count(result) != 1
                    || definition_count(failed) != 1
                    || self.body.blocks[success.0].statements.iter().any(|s| {
                        s.op.operands()
                            .iter()
                            .chain(s.results.iter())
                            .any(|v| [slot, old, detached, initial, replacement].contains(v))
                    })
                    || origin != body
                    || incoming(success) != [body]
                    || incoming(abort) != [body]
                    || !self.body.blocks[abort.0].statements.is_empty()
                    || !matches!(self.body.blocks[abort.0].terminator, Some((RTerm::Abort(crate::ssa::refine::body::RAbort::CheckedArithmetic {operator: crate::ssa::model::CheckedArithmeticOperator::Add, divisor}),_)) if divisor == right)
                    || !matches!(self.body.blocks[body.0].terminator, Some((RTerm::Branch {condition,when_true,when_false},_)) if condition == failed && when_true == abort && when_false == success)
                {
                    return Err(fail());
                }
                success
            }
            _ => return Err(fail()),
        };
        if matches!(p.boundary, Boundary::Mixed(_)) {
            self.mixed_header_validate(backedge, span)?;
        } else if matches!(p.boundary, Boundary::ConditionalBreak(_)) {
            self.cb_validate(backedge, span)?;
        } else if matches!(p.boundary, Boundary::ConditionalReturn(_)) {
            self.cr_validate(backedge, span)?;
        } else {
            match (p.boundary.jump(), p.continued) {
                (None, None) => {}
                (Some(jump), Some(receipt))
                    if receipt.expression == jump
                        && receipt.source == backedge
                        && receipt.target == header
                        && self.body.blocks[backedge.0].statements.len() == receipt.statements => {}
                _ => return Err(fail()),
            }
            // This finite CFG equation closes under any number of traversals: each body
            // replaces the current slot with fresh Q, drops its old value once, and goes
            // back to the same header. SSA subsequently versions that slot into phi inputs.
            if incoming(header) != [entry, backedge]
                || incoming(body) != [header]
                || incoming(finished) != [header]
                || incoming(exit) != [finished]
                || !matches!(self.body.blocks[entry.0].terminator, Some((RTerm::Goto(b),_)) if b == header)
                || !matches!(self.body.blocks[backedge.0].terminator, Some((RTerm::Goto(b),_)) if b == header)
                || !matches!(self.body.blocks[finished.0].terminator, Some((RTerm::Goto(b),_)) if b == exit)
                || !matches!(self.body.blocks[header.0].terminator, Some((RTerm::Branch {when_true,when_false,..},_)) if when_true == body && when_false == finished)
            {
                return Err(fail());
            }
        }
        let entry_statements = &self.body.blocks[entry.0].statements;
        let statements = &self.body.blocks[body.0].statements;
        if !entry_statements.iter().any(|s| {
            s.results == [slot] && matches!(s.op, ROp::Alias {source} if source == initial)
        }) {
            return Err(fail());
        }
        let replaces = statements
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s.op, ROp::RootReplace { .. }))
            .collect::<Vec<_>>();
        let [(replace_index, replace)] = replaces.as_slice() else {
            return Err(fail());
        };
        let aliases = statements
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.results == [detached] && matches!(s.op, ROp::Alias {source} if source == old)
            })
            .collect::<Vec<_>>();
        let [(alias_index, _)] = aliases.as_slice() else {
            return Err(fail());
        };
        let drops = statements
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s.op, ROp::Drop { .. }))
            .collect::<Vec<_>>();
        let [(drop_index, drop)] = drops.as_slice() else {
            return Err(fail());
        };
        let valid_replace_drop = replace.results == [slot, old]
            && matches!(replace.op, ROp::RootReplace {owner,replacement: q,..} if owner == slot && q == replacement)
            && matches!(drop.op, ROp::Drop {owner} if owner == detached)
            && replace_index < alias_index
            && alias_index < drop_index;
        let outside_drop = self.body.blocks.iter().enumerate().filter(|(i,_)| *i != body.0)
            .any(|(_,b)| b.statements.iter().any(|s| matches!(s.op, ROp::Drop {owner} if owner == slot || owner == old || owner == detached)));
        if !valid_replace_drop || detached == slot || outside_drop {
            return Err(fail());
        }
        // Exhaustive definition census: extra aliases/redefinitions cannot silently
        // change H, P, Q or the detached value between the observed hooks.
        let definitions = |target| {
            self.body
                .blocks
                .iter()
                .enumerate()
                .flat_map(|(block, b)| {
                    b.statements
                        .iter()
                        .enumerate()
                        .filter_map(move |(index, s)| {
                            s.results
                                .contains(&target)
                                .then_some((BlockRef(block), index))
                        })
                })
                .collect::<Vec<_>>()
        };
        let initial_defs = definitions(initial);
        let q_defs = definitions(replacement);
        let root_defs = definitions(slot);
        let [(q_block, q_index)] = q_defs.as_slice() else {
            return Err(fail());
        };
        let entry_alias = entry_statements
            .iter()
            .position(|s| {
                s.results == [slot] && matches!(s.op, ROp::Alias {source} if source == initial)
            })
            .ok_or_else(fail)?;
        let valid_initial = if let Some((parameter, index)) = p.parameter {
            p.entry_read
                && initial_defs.is_empty()
                && self.body.parameters.get(index) == Some(&initial)
                && self.bindings.get(&parameter) == Some(&initial)
                && self.dead.contains(&parameter)
                && self
                    .body
                    .parameters
                    .iter()
                    .filter(|s| **s == initial)
                    .count()
                    == 1
        } else {
            matches!(initial_defs.as_slice(), [(block,index)] if *block == entry && *index < entry_alias
                && matches!(entry_statements[*index].op, ROp::StringLiteral { .. }))
        };
        if !valid_initial
            || *q_block != body
            || q_index >= replace_index
            || !matches!(statements[*q_index].op, ROp::StringLiteral { .. })
            || root_defs != [(entry, entry_alias), (body, *replace_index)]
            || definitions(old) != [(body, *replace_index)]
            || definitions(detached) != [(body, *alias_index)]
            || [slot, initial, replacement, old, detached]
                .iter()
                .enumerate()
                .any(|(i, value)| [slot, initial, replacement, old, detached][..i].contains(value))
        {
            return Err(fail());
        }
        Ok(())
    }
}
