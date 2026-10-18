//! Exact transport for one qualified two-level normal pair, never a generic loop stack.
mod evidence;
pub(super) mod qualify;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pair {
    pub(super) outer: StatementId,
    pub(super) inner: StatementId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Roles {
    pub(super) caller: LoanId,
    pub(super) source: LoanId,
    pub(super) cursor: ValueId,
    pub(super) key: LoanId,
    pub(super) value: LoanId,
}
impl Roles {
    fn entities(self) -> [EntityId; 5] {
        [
            EntityId::Loan(self.caller),
            EntityId::Loan(self.source),
            EntityId::Value(self.cursor),
            EntityId::Loan(self.key),
            EntityId::Loan(self.value),
        ]
    }
}

#[derive(Clone, Copy)]
pub(super) struct Checkpoint {
    statement: StatementId,
    before: Roles,
    columns: [usize; 5],
}

#[derive(Clone, Copy)]
pub(super) struct Parent {
    checkpoint: Checkpoint,
    pub(super) blocks: [BlockId; 4],
    pub(super) header: Roles,
    pub(super) body: Roles,
    pub(super) exhausted: Roles,
    resume_start: Option<usize>,
}

pub(super) struct State {
    pub(super) pair: Pair,
    pub(super) caller: SymbolId,
    pub(super) anchor: LoanId,
    pub(super) birth: Roles,
    pub(super) parent: Option<Parent>,
    pub(super) resumed: Option<Parent>,
}

fn frame_roles(
    lowerer: &ExpressionLowerer<'_>,
    state: &Context,
    span: Span,
) -> Result<Roles, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let nested = state.nested.as_ref().ok_or_else(missing)?;
    let [(key_symbol, Some(key)), (value_symbol, Some(value))] = state.frame.components else {
        return Err(missing());
    };
    if state.body != lowerer.block
        || state.cleaned
        || state.frame.finished
        || state.owner.is_some()
        || state.conditional.is_some()
        || lowerer.borrow_bindings.get(&key_symbol) != Some(&key)
        || lowerer.borrow_bindings.get(&value_symbol) != Some(&value)
    {
        return Err(missing());
    }
    Ok(Roles {
        caller: *lowerer
            .borrow_bindings
            .get(&nested.caller)
            .ok_or_else(missing)?,
        source: state.frame.source.ok_or_else(missing)?,
        cursor: state.cursor,
        key,
        value,
    })
}

impl ExpressionLowerer<'_> {
    pub(super) fn prepare_map_nested(
        &self,
        pair: Option<Pair>,
        statement: StatementId,
        identity: SourceIdentity,
        carried: &mut LinearBindings,
        span: Span,
    ) -> Result<Option<Checkpoint>, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let Some(pair) = pair else {
            if !self.loops.is_empty() {
                return Err(missing());
            }
            return Ok(None);
        };
        let SourceIdentity::Parameter(symbol) = identity else {
            return Err(missing());
        };
        let caller = EntityId::Loan(*self.borrow_bindings.get(&symbol).ok_or_else(missing)?);
        if let Some(slot) = carried.slots.iter_mut().find(|slot| slot.source == caller) {
            if slot.borrow_symbols.as_slice() != [symbol] {
                return Err(missing());
            }
        } else {
            let ty = self.function.entity(caller).ok_or_else(missing)?.ty;
            let mut slot = LinearBindingSlot::new(None, caller, ty);
            slot.borrow_symbols.push(symbol);
            carried.slots.push(slot);
        }
        if statement == pair.outer {
            if !self.loops.is_empty() {
                return Err(missing());
            }
            return Ok(None);
        }
        if statement != pair.inner || self.loops.len() != 1 {
            return Err(missing());
        }
        let context = self.loops.first().ok_or_else(missing)?;
        let state = context.map_loop.as_ref().ok_or_else(missing)?;
        let nested = state.nested.as_ref().ok_or_else(missing)?;
        if state.frame.statement != pair.outer
            || nested.pair != pair
            || nested.resumed.is_some()
            || nested.parent.is_some()
            || state.kind != IterationExitKind::Fallthrough
            || !context.continues.is_empty()
            || !context.breaks.is_empty()
        {
            return Err(missing());
        }
        let before = frame_roles(self, state, span)?;
        if EntityId::Loan(before.caller) != caller {
            return Err(missing());
        }
        for entity in [
            EntityId::Loan(before.source),
            EntityId::Value(before.cursor),
        ] {
            if carried.slots.iter().any(|slot| slot.source == entity) {
                return Err(missing());
            }
            let ty = self.function.entity(entity).ok_or_else(missing)?.ty;
            carried.slots.push(LinearBindingSlot::new(None, entity, ty));
        }
        let mut columns = [0; 5];
        for (role, entity) in before.entities().into_iter().enumerate() {
            columns[role] = unique(
                carried
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(index, slot)| (slot.source == entity).then_some(index)),
            )
            .ok_or_else(missing)?;
        }
        Ok(Some(Checkpoint {
            statement: pair.outer,
            before,
            columns,
        }))
    }

    pub(super) fn rebind_map_parent(
        &mut self,
        checkpoint: Option<Checkpoint>,
        before: Option<Roles>,
        from: BlockId,
        to: BlockId,
        span: Span,
    ) -> Result<Option<Roles>, LoweringError> {
        let Some(checkpoint) = checkpoint else {
            return Ok(None);
        };
        let before = before.unwrap_or(checkpoint.before);
        let edge = edge_to(self.function, from, to, span)?;
        let parameters = self.map_iteration_parameters(to, span)?;
        let after = forwarded(
            self.function,
            checkpoint.columns,
            before,
            &edge.arguments,
            &parameters,
            span,
        )?;
        self.install_map_parent(checkpoint, after, span)?;
        Ok(Some(after))
    }

    fn install_map_parent(
        &mut self,
        checkpoint: Checkpoint,
        roles: Roles,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let state = self
            .loops
            .first_mut()
            .and_then(|context| context.map_loop.as_mut())
            .ok_or_else(missing)?;
        if state.frame.statement != checkpoint.statement || state.cleaned || state.frame.finished {
            return Err(missing());
        }
        state.body = self.block;
        state.frame.source = Some(roles.source);
        state.frame.components[0].1 = Some(roles.key);
        state.frame.components[1].1 = Some(roles.value);
        state.cursor = roles.cursor;
        let state = self
            .loops
            .first()
            .and_then(|context| context.map_loop.as_ref())
            .ok_or_else(missing)?;
        if frame_roles(self, state, span)? != roles {
            return Err(missing());
        }
        Ok(())
    }

    pub(super) fn map_nested_state(
        &self,
        pair: Option<Pair>,
        identity: SourceIdentity,
        birth: (LoanId, ValueId, [LoanId; 2]),
        parent: Option<Parent>,
        span: Span,
    ) -> Result<Option<State>, LoweringError> {
        let Some(pair) = pair else {
            return Ok(None);
        };
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let SourceIdentity::Parameter(caller) = identity else {
            return Err(missing());
        };
        let Some(EntityId::Loan(anchor)) = self.map_iteration_state.entries.get(&caller) else {
            return Err(missing());
        };
        let current = *self.borrow_bindings.get(&caller).ok_or_else(missing)?;
        let (source, cursor, [key, value]) = birth;
        transport(
            self.function,
            EntityId::Loan(current),
            EntityId::Loan(*anchor),
            span,
        )?;
        Ok(Some(State {
            pair,
            caller,
            anchor: *anchor,
            birth: Roles {
                caller: current,
                source,
                cursor,
                key,
                value,
            },
            parent,
            resumed: None,
        }))
    }

    pub(super) fn restore_map_parent(
        &mut self,
        parent: Option<Parent>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(mut parent) = parent else {
            return Ok(());
        };
        let missing = || error(LoweringErrorKind::MissingFact, span);
        if self.loops.len() != 1 || self.block != parent.blocks[3] {
            return Err(missing());
        }
        self.install_map_parent(parent.checkpoint, parent.exhausted, span)?;
        parent.resume_start = Some(
            self.function
                .block(self.block)
                .ok_or_else(missing)?
                .instructions
                .len(),
        );
        let nested = self
            .loops
            .first_mut()
            .and_then(|context| context.map_loop.as_mut())
            .and_then(|state| state.nested.as_mut())
            .ok_or_else(missing)?;
        if nested.resumed.replace(parent).is_some() {
            return Err(missing());
        }
        Ok(())
    }

    pub(super) fn map_nested_private_argument(
        &self,
        column: usize,
        span: Span,
    ) -> Result<Option<EntityId>, LoweringError> {
        let Some(parent) = self
            .loops
            .last()
            .and_then(|context| context.map_loop.as_ref())
            .and_then(|state| state.nested.as_ref())
            .and_then(|nested| nested.parent)
        else {
            return Ok(None);
        };
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let outer = self
            .loops
            .first()
            .and_then(|context| context.map_loop.as_ref())
            .ok_or_else(missing)?;
        let roles = frame_roles(self, outer, span)?;
        if column == parent.checkpoint.columns[1] {
            return Ok(Some(EntityId::Loan(roles.source)));
        }
        if column == parent.checkpoint.columns[2] {
            return Ok(Some(EntityId::Value(roles.cursor)));
        }
        Ok(None)
    }
}

pub(super) fn parent(
    checkpoint: Option<Checkpoint>,
    blocks: [BlockId; 4],
    header: Option<Roles>,
    body: Option<Roles>,
    exhausted: Option<Roles>,
    span: Span,
) -> Result<Option<Parent>, LoweringError> {
    checkpoint
        .map(|checkpoint| {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            Ok(Parent {
                checkpoint,
                blocks,
                header: header.ok_or_else(missing)?,
                body: body.ok_or_else(missing)?,
                exhausted: exhausted.ok_or_else(missing)?,
                resume_start: None,
            })
        })
        .transpose()
}

fn edge_to(
    function: &Function,
    from: BlockId,
    to: BlockId,
    span: Span,
) -> Result<&Edge, LoweringError> {
    unique(
        edges(function, from)
            .into_iter()
            .filter(|edge| edge.target == to),
    )
    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
}
fn edges(function: &Function, from: BlockId) -> Vec<&Edge> {
    match carrier::terminator(function, from) {
        Some(TerminatorKind::Branch(edge)) => vec![edge],
        Some(TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        }) => vec![when_true, when_false],
        _ => Vec::new(),
    }
}
fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            edges(function, block.id)
                .into_iter()
                .filter(move |edge| edge.target == target)
                .map(move |edge| (block.id, edge))
        })
        .collect()
}

fn forwarded(
    function: &Function,
    columns: [usize; 5],
    before: Roles,
    arguments: &[EntityId],
    parameters: &[EntityId],
    span: Span,
) -> Result<Roles, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    if arguments.len() != parameters.len() {
        return Err(missing());
    }
    let mut result = Vec::new();
    for (column, entity) in columns.into_iter().zip(before.entities()) {
        if arguments.get(column) != Some(&entity)
            || arguments
                .iter()
                .filter(|&&argument| argument == entity)
                .count()
                != 1
        {
            return Err(missing());
        }
        let actual = *parameters.get(column).ok_or_else(missing)?;
        if function.entity(actual).map(|entity| entity.ty)
            != function.entity(entity).map(|entity| entity.ty)
        {
            return Err(missing());
        }
        result.push(actual);
    }
    match result.as_slice() {
        [
            EntityId::Loan(caller),
            EntityId::Loan(source),
            EntityId::Value(cursor),
            EntityId::Loan(key),
            EntityId::Loan(value),
        ] => Ok(Roles {
            caller: *caller,
            source: *source,
            cursor: *cursor,
            key: *key,
            value: *value,
        }),
        _ => Err(missing()),
    }
}

/// Only exact edge-column transport is accepted. Every predecessor is visited,
/// including backedges; every cycle must have a real path to this immutable anchor.
pub(super) fn transport(
    function: &Function,
    current: EntityId,
    anchor: EntityId,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let ty = function.entity(anchor).ok_or_else(missing)?.ty;
    let mut pending = vec![current];
    let mut dependencies = BTreeMap::<EntityId, Vec<EntityId>>::new();
    while let Some(entity) = pending.pop() {
        if entity == anchor || dependencies.contains_key(&entity) {
            continue;
        }
        let data = function.entity(entity).ok_or_else(missing)?;
        if data.ty != ty {
            return Err(missing());
        }
        let Definition::BlockParameter { block, index } = data.definition else {
            return Err(missing());
        };
        let predecessors = incoming(function, block);
        if predecessors.is_empty() {
            return Err(missing());
        }
        let mut sources = Vec::new();
        for (_, edge) in predecessors {
            let parameters = &function.block(block).ok_or_else(missing)?.parameters;
            if edge.arguments.len() != parameters.len() || parameters.get(index) != Some(&entity) {
                return Err(missing());
            }
            let source = *edge.arguments.get(index).ok_or_else(missing)?;
            if function.entity(source).map(|data| data.ty) != Some(ty) {
                return Err(missing());
            }
            sources.push(source);
            pending.push(source);
        }
        dependencies.insert(entity, sources);
    }
    let mut anchored = std::collections::BTreeSet::from([anchor]);
    loop {
        let previous = anchored.len();
        for (&entity, sources) in &dependencies {
            if sources.iter().any(|source| anchored.contains(source)) {
                anchored.insert(entity);
            }
        }
        if previous == anchored.len() {
            break;
        }
    }
    if dependencies.keys().any(|entity| !anchored.contains(entity)) {
        return Err(missing());
    }
    Ok(())
}
