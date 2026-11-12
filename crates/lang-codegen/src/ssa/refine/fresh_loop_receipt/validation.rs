//! Detached validation of complete fresh-body SSA, without Plans, slots or current maps.
use super::*;
use crate::ssa::model::{
    BlockId, Edge, EntityId, EntityType, Function, Instruction, LoanKind, Operation, PlaceAccess,
    TerminatorKind,
};
impl Receipt {
    pub(crate) fn validate(&self, function: &Function) -> Result<(), LoweringError> {
        let span = self.span;
        let p = &self.proof;
        let b = p.blocks;
        if !self.fact.same_identity(&p.original_fact)
            || self.fact != p.original_fact
            || self.caller != function.id()
            || function.receiver.is_some()
            || function.never_returns
            || function.borrow_return.is_some()
            || function.carrier_return.is_some()
            || !function.return_types.is_empty()
            || p.callees.is_empty()
            || self.fact.consumers().len() != p.callees.len()
            || p.consumer_spans.len() != p.callees.len()
            || p.source_type == p.flag_type
            || p.source_parameter > 1
            || p.flag_parameter > 1
            || p.source_parameter == p.flag_parameter
        {
            return Err(invalid(span));
        }
        let original = [b.entry, b.header, b.body, b.finished, b.exit];
        let wanted_edges = vec![
            (b.entry, b.header),
            (b.header, b.body),
            (b.header, b.finished),
            (b.body, b.header),
            (b.finished, b.exit),
        ];
        let mut actual_original = p.edges.clone();
        let mut wanted_original = wanted_edges.clone();
        actual_original.sort();
        wanted_original.sort();
        if original.into_iter().collect::<BTreeSet<_>>().len() != 5
            || actual_original != wanted_original
            || p.returns != [b.exit]
            || !(p.folds.is_empty() || p.folds.as_slice() == [(b.finished, b.exit)])
        {
            return Err(invalid(span));
        }
        let canonical = original
            .into_iter()
            .map(|b| p.canonical(b))
            .collect::<BTreeSet<_>>();
        if self.blocks.keys().copied().collect::<BTreeSet<_>>() != canonical {
            return Err(invalid(span));
        }
        let resolve = |raw| {
            self.blocks
                .get(&p.canonical(raw))
                .copied()
                .ok_or_else(|| invalid(span))
        };
        let entry = resolve(b.entry)?;
        let header = resolve(b.header)?;
        let body = resolve(b.body)?;
        let finished = resolve(b.finished)?;
        let exit = resolve(b.exit)?;
        if function.entry_block() != Some(entry) {
            return Err(invalid(span));
        }
        let covered = self.blocks.values().copied().collect::<BTreeSet<_>>();
        if covered.len() != canonical.len() {
            return Err(invalid(span));
        }
        let mut tags = vec![Tag::Condition, Tag::Result];
        tags.extend((0..p.callees.len()).map(Tag::Consumer));
        tags.push(Tag::ResultEnd);
        let ids = self
            .instructions
            .iter()
            .map(|(_, id)| *id)
            .collect::<BTreeSet<_>>();
        if self.instructions.len() != tags.len()
            || ids.len() != tags.len()
            || tags.iter().any(|t| {
                self.instructions
                    .iter()
                    .filter(|(actual, _)| actual == t)
                    .count()
                    != 1
            })
        {
            return Err(invalid(span));
        }
        super::whole_function::check(function, &covered, &ids, span)?;
        for block in &function.blocks {
            let local = if block.id == header {
                vec![Tag::Condition]
            } else if block.id == body {
                tags.iter()
                    .copied()
                    .filter(|t| *t != Tag::Condition)
                    .collect()
            } else {
                Vec::new()
            };
            let expected = local
                .iter()
                .map(|t| self.instruction(function, *t).map(|i| i.id))
                .collect::<Result<Vec<_>, _>>()?;
            if block.instructions != expected {
                return Err(invalid(span));
            }
        }
        let entry_block = function.block(entry).ok_or_else(|| invalid(span))?;
        if entry_block.parameters.len() != 2 {
            return Err(invalid(span));
        }
        let source = entry_block.parameters[p.source_parameter];
        let flag = entry_block.parameters[p.flag_parameter];
        let shared = |semantic| EntityType::Loan {
            kind: LoanKind::Shared,
            target: semantic,
        };
        if !matches!(source, EntityId::Loan(_))
            || !matches!(flag, EntityId::Loan(_))
            || function.entity(source).map(|d| d.ty) != Some(shared(p.source_type))
            || function.entity(flag).map(|d| d.ty) != Some(shared(p.flag_type))
        {
            return Err(invalid(span));
        }
        let roles = transport(
            function,
            [entry, header, body, finished, exit],
            source,
            flag,
            span,
        )?;
        let local = |block: BlockId, role: usize| {
            function
                .block(block)
                .and_then(|b| {
                    let found = b
                        .parameters
                        .iter()
                        .filter(|e| roles.get(e) == Some(&role))
                        .copied()
                        .collect::<Vec<_>>();
                    (found.len() == 1).then(|| found[0])
                })
                .ok_or_else(|| invalid(span))
        };
        let condition = self.instruction(function, Tag::Condition)?;
        let condition_value = single(condition, span)?;
        if !matches!(condition.operation,Operation::Read{source:PlaceAccess::Loan(loan)} if EntityId::Loan(loan)==local(header,1)?)
            || condition.origin.span() != p.condition_span
            || function.entity(condition_value).map(|e| e.ty)
                != Some(EntityType::Value(p.flag_type))
        {
            return Err(invalid(span));
        }
        let acquire = self.instruction(function, Tag::Result)?;
        let result = single(acquire, span)?;
        let body_source = local(body, 0)?;
        if !matches!(result, EntityId::Loan(_))
            || function.entity(result).map(|e| e.ty) != Some(shared(p.source_type))
            || acquire.origin.span() != p.source_span
            || !matches!(&acquire.operation,Operation::BorrowCall{callee,source,arguments}if *callee==p.source_callee&&EntityId::Loan(*source)==body_source&&arguments.as_slice()==[body_source])
        {
            return Err(invalid(span));
        }
        for (index, callee) in p.callees.iter().enumerate() {
            let call = self.instruction(function, Tag::Consumer(index))?;
            if !call.results.is_empty()
                || call.origin.span() != p.consumer_spans[index]
                || !matches!(&call.operation,Operation::DirectCall{callee:actual,receiver:None,arguments}if actual==callee&&arguments.as_slice()==[result])
            {
                return Err(invalid(span));
            }
        }
        let end = self.instruction(function, Tag::ResultEnd)?;
        if !end.results.is_empty()
            || end.origin.span() != p.end_span
            || !matches!(end.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==result)
        {
            return Err(invalid(span));
        }
        let mut edges = Vec::new();
        for block in &function.blocks {
            let term = &block.terminator.as_ref().ok_or_else(|| invalid(span))?.kind;
            let actual = if block.id == entry || block.id == body {
                branch(term, header, span)?
            } else if block.id == header {
                let TerminatorKind::Conditional {
                    condition,
                    when_true,
                    when_false,
                } = term
                else {
                    return Err(invalid(span));
                };
                if EntityId::Value(*condition) != condition_value
                    || when_true.target != body
                    || when_false.target != finished
                {
                    return Err(invalid(span));
                }
                vec![when_true.clone(), when_false.clone()]
            } else if block.id == exit {
                if !matches!(term,TerminatorKind::Return{values}if values.is_empty()) {
                    return Err(invalid(span));
                }
                Vec::new()
            } else if block.id == finished {
                branch(term, exit, span)?
            } else {
                return Err(invalid(span));
            };
            if actual.iter().any(|e| e.arguments.contains(&result)) {
                return Err(invalid(span));
            }
            edges.extend(actual.into_iter().map(|edge| (block.id, edge)));
        }
        if edges.len() != self.edges.len()
            || edges
                .iter()
                .any(|edge| self.edges.iter().filter(|e| *e == edge).count() != 1)
        {
            return Err(invalid(span));
        }
        Ok(())
    }
    fn instruction<'a>(
        &self,
        function: &'a Function,
        tag: Tag,
    ) -> Result<&'a Instruction, LoweringError> {
        let mut found = self.instructions.iter().filter(|(t, _)| *t == tag);
        let (_, id) = found.next().ok_or_else(|| invalid(self.span))?;
        if found.next().is_some() {
            return Err(invalid(self.span));
        }
        function.instruction(*id).ok_or_else(|| invalid(self.span))
    }
}
fn single(i: &Instruction, span: Span) -> Result<EntityId, LoweringError> {
    match i.results.as_slice() {
        [id] => Ok(*id),
        _ => Err(invalid(span)),
    }
}
fn branch(term: &TerminatorKind, target: BlockId, span: Span) -> Result<Vec<Edge>, LoweringError> {
    match term {
        TerminatorKind::Branch(e) if e.target == target => Ok(vec![e.clone()]),
        _ => Err(invalid(span)),
    }
}

/// Establish role identity from the two actual ABI parameters; do not use captured slot maps.
fn transport(
    function: &Function,
    blocks: [BlockId; 5],
    source: EntityId,
    flag: EntityId,
    span: Span,
) -> Result<BTreeMap<EntityId, usize>, LoweringError> {
    let [entry, header, body, _, _] = blocks;
    let mut roles = BTreeMap::from([(source, 0), (flag, 1)]);
    let mut incoming = BTreeMap::<BlockId, Vec<(BlockId, &Edge)>>::new();
    for block in &function.blocks {
        let term = &block.terminator.as_ref().ok_or_else(|| invalid(span))?.kind;
        let edges = match term {
            TerminatorKind::Branch(e) => vec![e],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            TerminatorKind::Return { values } if values.is_empty() => Vec::new(),
            _ => return Err(invalid(span)),
        };
        for edge in edges {
            let target = function.block(edge.target).ok_or_else(|| invalid(span))?;
            if target.parameters.len() != edge.arguments.len() {
                return Err(invalid(span));
            }
            incoming
                .entry(edge.target)
                .or_default()
                .push((block.id, edge));
        }
    }
    if incoming.contains_key(&entry) {
        return Err(invalid(span));
    }
    for (target, expected) in [(header, vec![entry, body]), (body, vec![header])] {
        let actual = incoming.get(&target).ok_or_else(|| invalid(span))?;
        if actual.len() != expected.len()
            || expected
                .iter()
                .any(|wanted| actual.iter().filter(|(from, _)| from == wanted).count() != 1)
        {
            return Err(invalid(span));
        }
    }
    loop {
        let mut changed = false;
        for (target, edges) in &incoming {
            let params = &function
                .block(*target)
                .ok_or_else(|| invalid(span))?
                .parameters;
            for (_, edge) in edges {
                for (argument, param) in edge.arguments.iter().zip(params) {
                    if let Some(role) = roles.get(argument).copied() {
                        match roles.get(param) {
                            Some(existing) if *existing != role => return Err(invalid(span)),
                            Some(_) => {}
                            None => {
                                roles.insert(*param, role);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    for block in &function.blocks {
        let live = block.id == entry || block.id == header || block.id == body;
        if block.parameters.len() != if live { 2 } else { 0 } {
            return Err(invalid(span));
        }
        if live
            && (0..2).any(|role| {
                block
                    .parameters
                    .iter()
                    .filter(|id| roles.get(id) == Some(&role))
                    .count()
                    != 1
            })
        {
            return Err(invalid(span));
        }
    }
    for (target, edges) in incoming {
        let params = &function
            .block(target)
            .ok_or_else(|| invalid(span))?
            .parameters;
        for (from, edge) in edges {
            let predecessor = function.block(from).ok_or_else(|| invalid(span))?;
            for (argument, param) in edge.arguments.iter().zip(params) {
                let role = roles.get(param).ok_or_else(|| invalid(span))?;
                if roles.get(argument) != Some(role)
                    || !predecessor.parameters.contains(argument)
                    || predecessor
                        .parameters
                        .iter()
                        .filter(|id| roles.get(id) == Some(role))
                        .count()
                        != 1
                {
                    return Err(invalid(span));
                }
            }
        }
    }
    Ok(roles)
}
