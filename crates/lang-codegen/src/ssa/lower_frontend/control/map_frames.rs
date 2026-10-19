//! Path-local Map roles join ordinary CFG columns; cumulative exit evidence stays in Context.
use super::*;
use crate::ssa::model::{LoanId, LoanKind, SsaTypeId};
use lang_frontend::ownership_checking::IterationExitKind;
use std::collections::BTreeSet;

/// Preserve source identity without exposing the Map qualification implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum OwnerRoot {
    Parameter(SymbolId),
    Local {
        symbol: SymbolId,
        initializer: ExpressionId,
    },
    Formed {
        symbol: SymbolId,
        birth: crate::ssa::map_source::Birth<SymbolId>,
    },
    Temporary(ExpressionId),
}
impl OwnerRoot {
    fn current(
        self,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        temporaries: &BTreeMap<usize, ValueId>,
    ) -> Option<ValueId> {
        match self {
            Self::Parameter(symbol) | Self::Local { symbol, .. } | Self::Formed { symbol, .. } => {
                match bindings.get(&symbol) {
                    Some(LoweredValue::Value(owner)) => Some(*owner),
                    _ => None,
                }
            }
            Self::Temporary(expression) => temporaries.get(&expression.index()).copied(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(in super::super) struct Snapshot {
    pub(in super::super) statement: StatementId,
    pub(in super::super) source: Option<LoanId>,
    pub(in super::super) components: [(SymbolId, Option<LoanId>); 2],
    pub(in super::super) finished: bool,
    pub(in super::super) owner: Option<(OwnerRoot, ValueId)>,
    pub(in super::super) body: BlockId,
    pub(in super::super) cursor: ValueId,
    pub(in super::super) cursor_type: SsaTypeId,
    pub(in super::super) kind: IterationExitKind,
    pub(in super::super) cleaned: bool,
    pub(in super::super) next: Option<ValueId>,
    pub(in super::super) returned: Option<ValueId>,
    pub(in super::super) cleanup_start: Option<usize>,
}
pub(super) type Paths = BTreeMap<usize, Snapshot>;

impl Snapshot {
    fn live(
        self,
        block: BlockId,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        temporaries: &BTreeMap<usize, ValueId>,
        borrowed: &BTreeMap<SymbolId, LoanId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.body != block
            || self.cleaned
            || self.finished
            || self.source.is_none()
            || self.next.is_some()
            || self.returned.is_some()
            || self.cleanup_start.is_some()
            || self
                .components
                .iter()
                .any(|(symbol, loan)| loan.is_none() || borrowed.get(symbol).copied() != *loan)
            || self
                .owner
                .is_some_and(|(root, owner)| root.current(bindings, temporaries) != Some(owner))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }
    fn retired(self, span: Span) -> Result<(), LoweringError> {
        if !self.cleaned
            || !self.finished
            || self.source.is_some()
            || self.components.iter().any(|(_, loan)| loan.is_some())
            || !matches!(
                self.kind,
                IterationExitKind::Break(_) | IterationExitKind::Return(_)
            )
            || self.next.is_some()
            || self.cleanup_start.is_none()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }
    fn source(self, span: Span) -> Result<EntityId, LoweringError> {
        self.source
            .map(EntityId::Loan)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn map_path_snapshots(&self) -> Paths {
        self.loops
            .iter()
            .filter_map(|context| {
                context
                    .map_loop
                    .as_ref()
                    .and_then(|state| state.path_snapshot())
            })
            .map(|path| (path.statement.index(), path))
            .collect()
    }

    pub(in super::super) fn map_current_private_slot(
        &self,
        slot: &LinearBindingSlot,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let mut actual = None;
        for (cursor, keys) in [(false, &slot.map_sources), (true, &slot.map_cursors)].iter() {
            for statement in *keys {
                let state = self
                    .loops
                    .iter()
                    .filter_map(|context| context.map_loop.as_ref())
                    .find(|state| {
                        state
                            .path_snapshot()
                            .is_some_and(|path| path.statement.index() == *statement)
                    })
                    .ok_or_else(missing)?;
                let path = state.path_snapshot().ok_or_else(missing)?;
                let entity = if *cursor {
                    EntityId::Value(path.cursor)
                } else {
                    path.source(span)?
                };
                if actual
                    .replace(entity)
                    .is_some_and(|before| before != entity)
                    || self.function.entity(entity).map(|data| data.ty) != Some(slot.ty)
                {
                    return Err(missing());
                }
            }
        }
        actual.ok_or_else(missing)
    }

    pub(super) fn carry_map_paths(
        &self,
        paths: &Paths,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        slots: &mut Vec<LinearBindingSlot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for (&statement, &path) in paths {
            path.live(
                self.block,
                bindings,
                &self.temporaries,
                &self.borrow_bindings,
                span,
            )?;
            for (source, is_cursor) in [
                (path.source(span)?, false),
                (EntityId::Value(path.cursor), true),
            ] {
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                    .ty;
                if if is_cursor {
                    ty != EntityType::Value(path.cursor_type)
                } else {
                    !matches!(
                        ty,
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            ..
                        }
                    )
                } {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let index = if let Some(index) = slots.iter().position(|slot| slot.source == source)
                {
                    if slots[index].ty != ty {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    index
                } else {
                    slots.push(LinearBindingSlot::new(None, source, ty));
                    slots.len() - 1
                };
                if is_cursor {
                    slots[index].map_cursors.push(statement);
                } else {
                    slots[index].map_sources.push(statement);
                }
            }
        }
        Ok(())
    }

    /// Siblings restore entry path fields only after ordinary owner and Borrow slots.
    pub(super) fn rebind_map_paths(
        &mut self,
        paths: &Paths,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        slots: &[LinearBindingSlot],
        parameters: &[EntityId],
        block: BlockId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if paths.is_empty() {
            return self.restore_map_paths(paths, bindings, block, span);
        }
        if slots.len() > parameters.len() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        let mut mapping = BTreeMap::new();
        for (slot, &parameter) in slots.iter().zip(parameters) {
            let actual = self
                .function
                .entity(parameter)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
            if actual.ty != slot.ty
                || mapping
                    .insert(slot.source, parameter)
                    .is_some_and(|previous| previous != parameter)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        // Role tags must name one real original column; ordinary slots may
        // share that same column with another proven role.
        for (&statement, path) in paths {
            for (source, cursor) in [
                (path.source(span)?, false),
                (EntityId::Value(path.cursor), true),
            ] {
                let roles = slots
                    .iter()
                    .filter(|slot| {
                        if cursor {
                            slot.map_cursors.contains(&statement)
                        } else {
                            slot.map_sources.contains(&statement)
                        }
                    })
                    .collect::<Vec<_>>();
                if roles.len() != 1 || roles[0].source != source {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
        }
        let mut rebound = paths.clone();
        for path in rebound.values_mut() {
            let original = *path;
            let mapped = |entity| {
                mapping
                    .get(&entity)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
            };
            let EntityId::Loan(source) = mapped(original.source(span)?)? else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let EntityId::Value(cursor) = mapped(EntityId::Value(original.cursor))? else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            path.source = Some(source);
            path.cursor = cursor;
            path.body = block;
            for ((symbol, current), (_, before)) in
                path.components.iter_mut().zip(original.components)
            {
                let before = before.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let EntityId::Loan(loan) = mapped(EntityId::Loan(before))? else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                if self.borrow_bindings.get(symbol) != Some(&loan) {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                *current = Some(loan);
            }
            if let Some((root, old)) = original.owner {
                let current = root
                    .current(bindings, &self.temporaries)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if mapped(EntityId::Value(old))? != EntityId::Value(current) {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                path.owner = Some((root, current));
            }
        }
        self.restore_map_paths(&rebound, bindings, block, span)
    }

    pub(super) fn restore_map_paths(
        &mut self,
        paths: &Paths,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        block: BlockId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let active = self.map_path_snapshots();
        for (&key, path) in paths {
            if !active.contains_key(&key) {
                path.retired(span)?;
            }
        }
        if active.keys().any(|key| !paths.contains_key(key)) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        for context in &mut self.loops {
            let Some(state) = context.map_loop.as_mut() else {
                continue;
            };
            let Some(current) = state.path_snapshot() else {
                continue;
            };
            let path = *paths
                .get(&current.statement.index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            path.live(
                block,
                bindings,
                &self.temporaries,
                &self.borrow_bindings,
                span,
            )?;
            state.restore_path(path, span)?;
        }
        Ok(())
    }

    /// Every live edge supplies the same active Map set. Retired break snapshots
    /// may remain in LoopContext.breaks after that Map context was popped.
    pub(super) fn map_merge_paths(
        &self,
        exits: &[BranchExit],
        span: Span,
    ) -> Result<Paths, LoweringError> {
        let active = self.map_path_snapshots();
        let mut paths = Paths::new();
        for exit in exits {
            for (&key, path) in &exit.map_paths {
                if !active.contains_key(&key) {
                    path.retired(span)?;
                }
            }
            let keys = exit
                .map_paths
                .keys()
                .filter(|key| active.contains_key(key))
                .copied()
                .collect::<BTreeSet<_>>();
            if keys != active.keys().copied().collect() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            for &key in active.keys() {
                let path = exit.map_paths[&key];
                path.live(
                    exit.block,
                    &exit.bindings,
                    &exit.temporaries,
                    &exit.borrow_bindings,
                    span,
                )?;
                if let Some(first) = paths.get(&key) {
                    if first.statement != path.statement
                        || first.cursor_type != path.cursor_type
                        || first.kind != path.kind
                        || first.components.map(|(symbol, _)| symbol)
                            != path.components.map(|(symbol, _)| symbol)
                        || first.owner.map(|(root, _)| root) != path.owner.map(|(root, _)| root)
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                } else {
                    paths.insert(key, path);
                }
            }
        }
        Ok(paths)
    }

    pub(super) fn map_merge_slots(
        &self,
        exits: &[BranchExit],
        paths: &Paths,
        slots: &mut Vec<MergeSlot>,
        types: &mut Vec<EntityType>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for (&statement, path) in paths {
            for (slot, entity) in [
                (MergeSlot::MapSource(statement), path.source(span)?),
                (
                    MergeSlot::MapCursor(statement),
                    EntityId::Value(path.cursor),
                ),
            ] {
                let ty = self
                    .function
                    .entity(entity)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                    .ty;
                for exit in exits {
                    let candidate = map_slot(&exit.map_paths, slot, span)?;
                    if self.function.entity(candidate).map(|data| data.ty) != Some(ty) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                }
                slots.push(slot);
                types.push(ty);
            }
        }
        Ok(())
    }

    pub(super) fn finish_map_merge(
        &mut self,
        paths: &Paths,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut paths = paths.clone();
        for path in paths.values_mut() {
            path.body = self.block;
            for (symbol, loan) in &mut path.components {
                *loan = Some(
                    *self
                        .borrow_bindings
                        .get(symbol)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                );
            }
            if let Some((root, _)) = path.owner {
                path.owner = Some((
                    root,
                    root.current(&self.bindings, &self.temporaries)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                ));
            }
        }
        let bindings = self.bindings.clone();
        self.restore_map_paths(&paths, &bindings, self.block, span)
    }
}

pub(super) fn map_slot(
    paths: &Paths,
    slot: MergeSlot,
    span: Span,
) -> Result<EntityId, LoweringError> {
    match slot {
        MergeSlot::MapSource(statement) => paths
            .get(&statement)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .source(span),
        MergeSlot::MapCursor(statement) => paths
            .get(&statement)
            .map(|path| EntityId::Value(path.cursor))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span)),
        _ => Err(error(LoweringErrorKind::InvalidModel, span)),
    }
}
pub(super) fn rebind_merge_slot(
    paths: &mut Paths,
    slot: MergeSlot,
    parameter: EntityId,
    span: Span,
) -> Result<(), LoweringError> {
    match (slot, parameter) {
        (MergeSlot::MapSource(statement), EntityId::Loan(source)) => {
            paths
                .get_mut(&statement)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .source = Some(source)
        }
        (MergeSlot::MapCursor(statement), EntityId::Value(cursor)) => {
            paths
                .get_mut(&statement)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .cursor = cursor
        }
        _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
    }
    Ok(())
}
