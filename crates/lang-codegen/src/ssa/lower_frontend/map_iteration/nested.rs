//! Exact transport for one qualified two-level pair, never a generic loop stack.
mod evidence;
mod evidence_tail;
pub(super) mod qualify;
pub(super) mod tail;
mod transport_tail;

use super::*;
use transport_tail::PathCheckpoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pair {
    pub(super) outer: StatementId,
    pub(super) inner: StatementId,
    pub(super) tail: Option<tail::Shape>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Roles {
    pub(super) caller: LoanId,
    pub(super) source: LoanId,
    pub(super) cursor: ValueId,
    pub(super) key: LoanId,
    pub(super) value: LoanId,
    pub(super) guard: Option<ValueId>,
}
impl Roles {
    fn base_entities(self) -> [EntityId; 5] {
        [
            EntityId::Loan(self.caller),
            EntityId::Loan(self.source),
            EntityId::Value(self.cursor),
            EntityId::Loan(self.key),
            EntityId::Loan(self.value),
        ]
    }

    fn entities(self) -> Vec<EntityId> {
        self.base_entities()
            .into_iter()
            .chain(self.guard.map(EntityId::Value))
            .collect()
    }
}

#[derive(Clone, Copy)]
pub(super) struct Checkpoint {
    statement: StatementId,
    before: Roles,
    columns: [usize; 5],
    guard_column: Option<usize>,
    guard_anchor: Option<ValueId>,
    path: PathCheckpoint,
}

#[derive(Clone, Copy)]
pub(super) struct Parent {
    checkpoint: Checkpoint,
    pub(super) blocks: [BlockId; 4],
    pub(super) header: Roles,
    pub(super) body: Roles,
    pub(super) exhausted: Roles,
    pub(super) continuation: BlockId,
    pub(super) resumed_roles: Roles,
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

pub(super) fn current_roles(
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
        guard: transport_tail::current_guard(lowerer, state, span)?,
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
        let before = current_roles(self, state, span)?;
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
        for (role, entity) in before.base_entities().into_iter().enumerate() {
            columns[role] = unique(
                carried
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(index, slot)| (slot.source == entity).then_some(index)),
            )
            .ok_or_else(missing)?;
        }
        let guard_column = before
            .guard
            .map(|guard| {
                unique(
                    carried
                        .slots
                        .iter()
                        .enumerate()
                        .filter_map(|(index, slot)| {
                            (slot.source == EntityId::Value(guard)).then_some(index)
                        }),
                )
                .ok_or_else(missing)
            })
            .transpose()?;
        if let (Some(shape), Some(column)) = (pair.tail, guard_column) {
            if carried.slots[column].symbol != Some(shape.outer_guard.symbol) {
                return Err(missing());
            }
        }
        let guard_anchor = pair
            .tail
            .map(|shape| transport_tail::constructor_anchor(self, shape.outer_guard, span))
            .transpose()?;
        if before.guard != guard_anchor || guard_column.is_some() != guard_anchor.is_some() {
            return Err(missing());
        }
        Ok(Some(Checkpoint {
            statement: pair.outer,
            before,
            columns,
            guard_column,
            guard_anchor,
            path: PathCheckpoint::capture(state),
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
            checkpoint.guard_column,
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
        if current_roles(self, state, span)? != roles {
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
                guard: None,
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
        if self.loops.len() != 1 || self.block != parent.continuation {
            return Err(missing());
        }
        self.install_map_parent(parent.checkpoint, parent.resumed_roles, span)?;
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
        let roles = current_roles(self, outer, span)?;
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
                continuation: blocks[3],
                resumed_roles: exhausted.ok_or_else(missing)?,
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
        Some(TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        }) => vec![when_null, when_non_null],
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
    guard_column: Option<usize>,
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
    if guard_column.is_some() != before.guard.is_some() {
        return Err(missing());
    }
    let columns = columns.into_iter().chain(guard_column).collect::<Vec<_>>();
    if columns
        .iter()
        .enumerate()
        .any(|(index, column)| columns[..index].contains(column))
    {
        return Err(missing());
    }
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
        let actual_data = function.entity(actual).ok_or_else(missing)?;
        let Definition::BlockParameter { block, index } = actual_data.definition else {
            return Err(missing());
        };
        if actual_data.ty != function.entity(entity).ok_or_else(missing)?.ty
            || index != column
            || function
                .block(block)
                .map(|block| block.parameters.as_slice())
                != Some(parameters)
        {
            return Err(missing());
        }
        result.push(actual);
    }
    let [
        EntityId::Loan(caller),
        EntityId::Loan(source),
        EntityId::Value(cursor),
        EntityId::Loan(key),
        EntityId::Loan(value),
        rest @ ..,
    ] = result.as_slice()
    else {
        return Err(missing());
    };
    let guard = match rest {
        [] if before.guard.is_none() => None,
        [EntityId::Value(guard)] if before.guard.is_some() => Some(*guard),
        _ => return Err(missing()),
    };
    Ok(Roles {
        caller: *caller,
        source: *source,
        cursor: *cursor,
        key: *key,
        value: *value,
        guard,
    })
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
