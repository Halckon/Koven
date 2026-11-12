//! Detached validation of a dynamic direct-loop sequence, without Plans or slot maps.
use super::emitted::LoopReceipt;
use super::*;
use crate::ssa::model::{
    BlockId, Edge, EntityId, EntityType, Function, Instruction, InstructionId, LoanKind, Operation,
    PlaceAccess, TerminatorKind,
};

impl Receipt {
    pub(crate) fn validate(&self, function: &Function) -> Result<(), LoweringError> {
        let span = self.span;
        let resolved = self.proofs(function)?;
        let first = self.loops.first().ok_or_else(|| invalid(span))?;
        let entry = resolved[0][0];
        let covered = self.blocks.values().copied().collect::<BTreeSet<_>>();
        let mut ids = BTreeSet::new();
        let mut expected_instructions = BTreeMap::<BlockId, Vec<InstructionId>>::new();
        for (local, blocks) in self.loops.iter().zip(&resolved) {
            let mut tags = vec![Tag::Condition, Tag::Result];
            tags.extend((0..local.proof.callees.len()).map(Tag::Consumer));
            tags.push(Tag::ResultEnd);
            if local.instructions.len() != tags.len() {
                return Err(invalid(span));
            }
            let mut instructions = Vec::new();
            for tag in tags {
                let instruction = local.instruction(function, tag, span)?;
                if !ids.insert(instruction.id) {
                    return Err(invalid(span));
                }
                instructions.push(instruction.id);
            }
            if expected_instructions
                .insert(blocks[1], vec![instructions[0]])
                .is_some()
                || expected_instructions
                    .insert(blocks[2], instructions[1..].to_vec())
                    .is_some()
            {
                return Err(invalid(span));
            }
        }
        for block in &function.blocks {
            if block.instructions.as_slice()
                != expected_instructions
                    .get(&block.id)
                    .map_or(&[][..], Vec::as_slice)
            {
                return Err(invalid(span));
            }
        }
        super::whole_function::check(function, &covered, &ids, span)?;
        let entry_block = function.block(entry).ok_or_else(|| invalid(span))?;
        if entry_block.parameters.len() != self.abi_types.len() {
            return Err(invalid(span));
        }
        for (entity, semantic) in entry_block.parameters.iter().zip(&self.abi_types) {
            if !matches!(entity, EntityId::Loan(_))
                || function.entity(*entity).map(|d| d.ty) != Some(shared(*semantic))
            {
                return Err(invalid(span));
            }
        }
        let source_role = first.proof.source_parameter;
        let mut live = BTreeMap::new();
        let all = (0..self.abi_types.len()).collect::<BTreeSet<_>>();
        insert_live(&mut live, entry, all, span)?;
        for (index, blocks) in resolved.iter().enumerate() {
            let required = self.loops[index..]
                .iter()
                .map(|peer| peer.proof.flag_parameter)
                .chain(std::iter::once(source_role))
                .collect::<BTreeSet<_>>();
            for block in [blocks[0], blocks[1], blocks[2]] {
                insert_live(&mut live, block, required.clone(), span)?;
            }
            let remaining = if index + 1 < self.loops.len() {
                self.loops[index + 1..]
                    .iter()
                    .map(|peer| peer.proof.flag_parameter)
                    .chain(std::iter::once(source_role))
                    .collect::<BTreeSet<_>>()
            } else {
                BTreeSet::new()
            };
            for block in [blocks[3], blocks[4]] {
                insert_live(&mut live, block, remaining.clone(), span)?;
            }
        }
        let roles = transport(function, entry, &live, span)?;
        let mut terms = BTreeMap::new();
        let mut results = BTreeSet::new();
        for (index, (local, blocks)) in self.loops.iter().zip(&resolved).enumerate() {
            let [entry, header, body, finished, exit] = *blocks;
            let (condition, result) = local.operations(function, header, body, &roles, span)?;
            if !results.insert(result) || roles.contains_key(&result) {
                return Err(invalid(span));
            }
            insert_term(&mut terms, entry, ExpectedTerm::Branch(header), span)?;
            insert_term(&mut terms, body, ExpectedTerm::Branch(header), span)?;
            insert_term(
                &mut terms,
                header,
                ExpectedTerm::Conditional(condition, body, finished),
                span,
            )?;
            if finished != exit {
                insert_term(&mut terms, finished, ExpectedTerm::Branch(exit), span)?;
            }
            if index + 1 == resolved.len() {
                insert_term(&mut terms, exit, ExpectedTerm::Return, span)?;
            }
        }
        if terms.keys().copied().collect::<BTreeSet<_>>() != covered {
            return Err(invalid(span));
        }
        let mut edges = Vec::new();
        for block in &function.blocks {
            let term = &block.terminator.as_ref().ok_or_else(|| invalid(span))?.kind;
            let actual = match (terms.get(&block.id), term) {
                (Some(ExpectedTerm::Branch(target)), TerminatorKind::Branch(edge))
                    if edge.target == *target =>
                {
                    vec![edge.clone()]
                }
                (
                    Some(ExpectedTerm::Conditional(value, body, finished)),
                    TerminatorKind::Conditional {
                        condition,
                        when_true,
                        when_false,
                    },
                ) if EntityId::Value(*condition) == *value
                    && when_true.target == *body
                    && when_false.target == *finished =>
                {
                    vec![when_true.clone(), when_false.clone()]
                }
                (Some(ExpectedTerm::Return), TerminatorKind::Return { values })
                    if values.is_empty() =>
                {
                    Vec::new()
                }
                _ => return Err(invalid(span)),
            };
            if actual
                .iter()
                .any(|edge| edge.arguments.iter().any(|entity| results.contains(entity)))
            {
                return Err(invalid(span));
            }
            edges.extend(actual.into_iter().map(|edge| (block.id, edge)));
        }
        if edges.len() != self.edges.len()
            || edges
                .iter()
                .any(|edge| self.edges.iter().filter(|actual| *actual == edge).count() != 1)
        {
            return Err(invalid(span));
        }
        Ok(())
    }

    /// Recheck original producer topology and identities before resolving any folded block.
    fn proofs(&self, function: &Function) -> Result<Vec<[BlockId; 5]>, LoweringError> {
        let span = self.span;
        let first = self.loops.first().ok_or_else(|| invalid(span))?;
        if self.abi_types.len() < 2
            || self.caller != function.id()
            || function.receiver.is_some()
            || function.never_returns
            || function.borrow_return.is_some()
            || function.carrier_return.is_some()
            || !function.return_types.is_empty()
        {
            return Err(invalid(span));
        }
        let mut original = BTreeSet::new();
        let mut bindings = BTreeSet::new();
        let mut statements = BTreeSet::new();
        let mut bodies = BTreeSet::new();
        let mut initializers = BTreeSet::new();
        let mut allowed_folds = BTreeSet::new();
        let mut previous_exit = None;
        for (index, local) in self.loops.iter().enumerate() {
            let p = &local.proof;
            let b = p.blocks;
            if !local.fact.same_identity(&p.original_fact)
                || local.fact != p.original_fact
                || local.fact.source() != first.fact.source()
                || local.fact.callable() != first.fact.callable()
                || !bindings.insert(local.fact.binding())
                || !statements.insert(local.fact.loop_statement())
                || !bodies.insert(local.fact.body())
                || !initializers.insert(local.fact.initializer())
                || p.callees.is_empty()
                || local.fact.consumers().len() != p.callees.len()
                || p.consumer_spans.len() != p.callees.len()
                || p.source_type == p.flag_type
                || self.abi_types.get(p.source_parameter) != Some(&p.source_type)
                || self.abi_types.get(p.flag_parameter) != Some(&p.flag_type)
                || p.source_parameter == p.flag_parameter
                || p.source_parameter != first.proof.source_parameter
                || p.source_type != first.proof.source_type
                || p.flag_type != first.proof.flag_type
                || [b.entry, b.header, b.body, b.finished, b.exit]
                    .into_iter()
                    .collect::<BTreeSet<_>>()
                    .len()
                    != 5
            {
                return Err(invalid(span));
            }
            if let Some(previous) = previous_exit {
                if b.entry != previous {
                    return Err(invalid(span));
                }
            } else if !original.insert(b.entry) {
                return Err(invalid(span));
            }
            for block in [b.header, b.body, b.finished, b.exit] {
                if !original.insert(block) {
                    return Err(invalid(span));
                }
            }
            previous_exit = Some(b.exit);
            let mut wanted = vec![
                (b.entry, b.header),
                (b.header, b.body),
                (b.header, b.finished),
                (b.body, b.header),
                (b.finished, b.exit),
            ];
            let mut actual = p.edges.clone();
            wanted.sort();
            actual.sort();
            let expected_returns = if index + 1 == self.loops.len() {
                vec![b.exit]
            } else {
                Vec::new()
            };
            if actual != wanted || p.returns != expected_returns {
                return Err(invalid(span));
            }
            allowed_folds.insert((b.finished, b.exit));
        }
        let mut used = self
            .loops
            .iter()
            .map(|peer| peer.proof.flag_parameter)
            .collect::<BTreeSet<_>>();
        used.insert(first.proof.source_parameter);
        if used != (0..self.abi_types.len()).collect::<BTreeSet<_>>()
            || self.abi_types.iter().enumerate().any(|(index, ty)| {
                *ty != if index == first.proof.source_parameter {
                    first.proof.source_type
                } else {
                    first.proof.flag_type
                }
            })
        {
            return Err(invalid(span));
        }
        let mut seen = BTreeSet::new();
        let mut active = original.clone();
        let mut remaining = self
            .loops
            .iter()
            .flat_map(|local| local.proof.edges.iter().copied())
            .collect::<Vec<_>>();
        for &(from, to) in &self.folds {
            if !allowed_folds.contains(&(from, to))
                || !seen.insert((from, to))
                || !active.contains(&from)
                || !active.remove(&to)
                || remaining.iter().filter(|(_, target)| *target == to).count() != 1
                || remaining
                    .iter()
                    .filter(|(source, _)| *source == from)
                    .count()
                    != 1
                || remaining.iter().filter(|edge| **edge == (from, to)).count() != 1
            {
                return Err(invalid(span));
            }
            // Consume the real single-predecessor Goto before rewriting the survivor.
            remaining.retain(|edge| *edge != (from, to));
            for (source, target) in &mut remaining {
                if *source == to {
                    *source = from;
                }
                if *target == to {
                    *target = from;
                }
            }
        }
        let canonical = original
            .into_iter()
            .map(|b| self.canonical(b))
            .collect::<BTreeSet<_>>();
        if active != canonical
            || remaining.iter().any(|(from, to)| from == to)
            || self.blocks.keys().copied().collect::<BTreeSet<_>>() != canonical
            || self.blocks.values().copied().collect::<BTreeSet<_>>().len() != canonical.len()
        {
            return Err(invalid(span));
        }
        let resolve = |raw| {
            self.blocks
                .get(&self.canonical(raw))
                .copied()
                .ok_or_else(|| invalid(span))
        };
        let mut expected_edges = remaining
            .iter()
            .map(|(from, to)| Ok((resolve(*from)?, resolve(*to)?)))
            .collect::<Result<Vec<_>, LoweringError>>()?;
        let mut actual_edges = Vec::new();
        for block in &function.blocks {
            match &block.terminator.as_ref().ok_or_else(|| invalid(span))?.kind {
                TerminatorKind::Branch(edge) => actual_edges.push((block.id, edge.target)),
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => {
                    actual_edges.push((block.id, when_true.target));
                    actual_edges.push((block.id, when_false.target));
                }
                TerminatorKind::Return { values } if values.is_empty() => {}
                _ => return Err(invalid(span)),
            }
        }
        expected_edges.sort();
        actual_edges.sort();
        if actual_edges != expected_edges {
            return Err(invalid(span));
        }
        let mut resolved = Vec::new();
        for local in &self.loops {
            let b = local.proof.blocks;
            resolved.push([
                resolve(b.entry)?,
                resolve(b.header)?,
                resolve(b.body)?,
                resolve(b.finished)?,
                resolve(b.exit)?,
            ]);
        }
        if function.entry_block() != Some(resolved[0][0]) {
            return Err(invalid(span));
        }
        Ok(resolved)
    }
}

impl LoopReceipt {
    fn instruction<'a>(
        &self,
        function: &'a Function,
        tag: Tag,
        span: Span,
    ) -> Result<&'a Instruction, LoweringError> {
        let mut found = self.instructions.iter().filter(|(t, _)| *t == tag);
        let (_, id) = found.next().ok_or_else(|| invalid(span))?;
        if found.next().is_some() {
            return Err(invalid(span));
        }
        function.instruction(*id).ok_or_else(|| invalid(span))
    }

    fn operations(
        &self,
        function: &Function,
        header: BlockId,
        body: BlockId,
        roles: &BTreeMap<EntityId, usize>,
        span: Span,
    ) -> Result<(EntityId, EntityId), LoweringError> {
        let p = &self.proof;
        let condition = self.instruction(function, Tag::Condition, span)?;
        let value = single(condition, span)?;
        if condition.block != header
            || !matches!(condition.operation, Operation::Read { source: PlaceAccess::Loan(loan) } if EntityId::Loan(loan) == local(function, roles, header, p.flag_parameter, span)?)
            || condition.origin.span() != p.condition_span
            || function.entity(value).map(|e| e.ty) != Some(EntityType::Value(p.flag_type))
        {
            return Err(invalid(span));
        }
        let acquire = self.instruction(function, Tag::Result, span)?;
        let result = single(acquire, span)?;
        let source = local(function, roles, body, p.source_parameter, span)?;
        if acquire.block != body
            || !matches!(result, EntityId::Loan(_))
            || function.entity(result).map(|e| e.ty) != Some(shared(p.source_type))
            || acquire.origin.span() != p.source_span
            || !matches!(&acquire.operation, Operation::BorrowCall { callee, source: actual, arguments } if *callee == p.source_callee && EntityId::Loan(*actual) == source && arguments.as_slice() == [source])
        {
            return Err(invalid(span));
        }
        for (index, callee) in p.callees.iter().enumerate() {
            let call = self.instruction(function, Tag::Consumer(index), span)?;
            if call.block != body
                || !call.results.is_empty()
                || call.origin.span() != p.consumer_spans[index]
                || !matches!(&call.operation, Operation::DirectCall { callee: actual, receiver: None, arguments } if actual == callee && arguments.as_slice() == [result])
            {
                return Err(invalid(span));
            }
        }
        let end = self.instruction(function, Tag::ResultEnd, span)?;
        if end.block != body
            || !end.results.is_empty()
            || end.origin.span() != p.end_span
            || !matches!(end.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == result)
        {
            return Err(invalid(span));
        }
        Ok((value, result))
    }
}

fn shared(semantic: SsaTypeId) -> EntityType {
    EntityType::Loan {
        kind: LoanKind::Shared,
        target: semantic,
    }
}
fn single(instruction: &Instruction, span: Span) -> Result<EntityId, LoweringError> {
    match instruction.results.as_slice() {
        [id] => Ok(*id),
        _ => Err(invalid(span)),
    }
}
fn local(
    function: &Function,
    roles: &BTreeMap<EntityId, usize>,
    block: BlockId,
    role: usize,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let parameters = &function
        .block(block)
        .ok_or_else(|| invalid(span))?
        .parameters;
    let mut found = parameters.iter().filter(|id| roles.get(id) == Some(&role));
    let value = found.next().copied().ok_or_else(|| invalid(span))?;
    if found.next().is_some() {
        return Err(invalid(span));
    }
    Ok(value)
}
enum ExpectedTerm {
    Branch(BlockId),
    Conditional(EntityId, BlockId, BlockId),
    Return,
}
fn insert_term(
    terms: &mut BTreeMap<BlockId, ExpectedTerm>,
    block: BlockId,
    term: ExpectedTerm,
    span: Span,
) -> Result<(), LoweringError> {
    if terms.insert(block, term).is_some() {
        return Err(invalid(span));
    }
    Ok(())
}

fn insert_live(
    live: &mut BTreeMap<BlockId, BTreeSet<usize>>,
    block: BlockId,
    roles: BTreeSet<usize>,
    span: Span,
) -> Result<(), LoweringError> {
    if let Some(previous) = live.insert(block, roles.clone())
        && previous != roles
    {
        return Err(invalid(span));
    }
    Ok(())
}

/// Start from actual ABI entities, then recheck every incoming edge and predecessor-local role.
fn transport(
    function: &Function,
    entry: BlockId,
    live: &BTreeMap<BlockId, BTreeSet<usize>>,
    span: Span,
) -> Result<BTreeMap<EntityId, usize>, LoweringError> {
    let entry_parameters = &function
        .block(entry)
        .ok_or_else(|| invalid(span))?
        .parameters;
    let mut roles = entry_parameters
        .iter()
        .copied()
        .enumerate()
        .map(|(index, entity)| (entity, index))
        .collect::<BTreeMap<_, _>>();
    if roles.len() != entry_parameters.len() {
        return Err(invalid(span));
    }
    let mut incoming = BTreeMap::<BlockId, Vec<(BlockId, &Edge)>>::new();
    for block in &function.blocks {
        let term = &block.terminator.as_ref().ok_or_else(|| invalid(span))?.kind;
        let edges = match term {
            TerminatorKind::Branch(edge) => vec![edge],
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
    if live.keys().copied().collect::<BTreeSet<_>>()
        != function
            .blocks
            .iter()
            .map(|block| block.id)
            .collect::<BTreeSet<_>>()
    {
        return Err(invalid(span));
    }
    for block in &function.blocks {
        let required = live.get(&block.id).ok_or_else(|| invalid(span))?;
        if block.parameters.len() != required.len() {
            return Err(invalid(span));
        }
        for role in required {
            local(function, &roles, block.id, *role, span)?;
        }
    }
    for (target, edges) in incoming {
        let params = &function
            .block(target)
            .ok_or_else(|| invalid(span))?
            .parameters;
        for (from, edge) in edges {
            for (argument, param) in edge.arguments.iter().zip(params) {
                let role = *roles.get(param).ok_or_else(|| invalid(span))?;
                if local(function, &roles, from, role, span)? != *argument {
                    return Err(invalid(span));
                }
            }
        }
    }
    Ok(roles)
}
