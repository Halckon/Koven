//! Path-local Map roles join ordinary CFG columns; cumulative exit evidence stays in Context.
use super::*;
use crate::ssa::model::{LoanId, LoanKind, SsaTypeId};
use lang_frontend::ownership_checking::IterationExitKind;
use std::collections::BTreeSet;

/// Preserve source identity without exposing the Map qualification implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa) enum OwnerRoot {
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
pub(in crate::ssa) struct Snapshot {
    pub(in crate::ssa) statement: StatementId,
    pub(in crate::ssa) source: Option<LoanId>,
    pub(in crate::ssa) components: [(SymbolId, Option<LoanId>); 2],
    pub(in crate::ssa) finished: bool,
    pub(in crate::ssa) owner: Option<(OwnerRoot, ValueId)>,
    pub(in crate::ssa) body: BlockId,
    pub(in crate::ssa) cursor: ValueId,
    pub(in crate::ssa) cursor_type: SsaTypeId,
    pub(in crate::ssa) kind: IterationExitKind,
    pub(in crate::ssa) cleaned: bool,
    pub(in crate::ssa) next: Option<ValueId>,
    pub(in crate::ssa) returned: Option<ValueId>,
    pub(in crate::ssa) cleanup_start: Option<usize>,
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

    pub(in crate::ssa) fn map_current_private_slot(
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
                let choice = carry::associate_source(
                    self.function,
                    slots,
                    source,
                    carry::TypeAtMiss::Known(ty),
                    error(LoweringErrorKind::MissingFact, span),
                )?;
                let column = choice.column();
                if let carry::ColumnMatch::Reused(_) = choice {
                    carry::require_column_type(
                        slots,
                        column,
                        ty,
                        error(LoweringErrorKind::MissingFact, span),
                    )?;
                }
                let index = column.index();
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

pub(in crate::ssa) mod carrier {
    //! Verify the actual seed, source lease, ordered cursor, and owner edge chain.
    use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::model::Instruction;

    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct Child {
        pub(in crate::ssa) statement: StatementId,
        pub(in crate::ssa) blocks: [BlockId; 4],
        pub(in crate::ssa) source: LoanId,
        pub(in crate::ssa) first: ValueId,
        pub(in crate::ssa) source_slot: usize,
        pub(in crate::ssa) cursor_slot: usize,
    }

    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct Evidence<'a> {
        pub(in crate::ssa) parsed: &'a ParsedFile,
        pub(in crate::ssa) names: &'a NameResolution,
        pub(in crate::ssa) typed: &'a TypedFile,
        pub(in crate::ssa) owned: &'a OwnershipCheckedFile,
        pub(in crate::ssa) function: &'a Function,
        pub(in crate::ssa) exhaustion_point: DropPoint,
        pub(in crate::ssa) conditional: Option<&'a conditional::State>,
        pub(in crate::ssa) children: &'a [Child],
        pub(in crate::ssa) cleanup_body: BlockId,
        pub(in crate::ssa) seed: EntityId,
        pub(in crate::ssa) source: LoanId,
        pub(in crate::ssa) first: ValueId,
        pub(in crate::ssa) next: Option<ValueId>,
        pub(in crate::ssa) body_exit: IterationExitKind,
        pub(in crate::ssa) body_owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
        pub(in crate::ssa) resumed: Option<checked::Roles>,
        pub(in crate::ssa) returned: Option<ValueId>,
        pub(in crate::ssa) cleanup_start: Option<usize>,
        pub(in crate::ssa) continuation: BlockId,
        pub(in crate::ssa) key: LoanId,
        pub(in crate::ssa) value: LoanId,
        pub(in crate::ssa) blocks: [BlockId; 4],
        pub(in crate::ssa) carried: &'a LinearBindings,
        pub(in crate::ssa) source_slot: usize,
        pub(in crate::ssa) cursor_slot: usize,
        pub(in crate::ssa) owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
        pub(in crate::ssa) source_origin: Span,
        pub(in crate::ssa) span: Span,
    }

    impl Evidence<'_> {
        pub(in crate::ssa) fn validate(self) -> Result<(), LoweringError> {
            let Self {
                parsed: _,
                names: _,
                typed: _,
                owned,
                function,
                exhaustion_point,
                conditional,
                children: _,
                cleanup_body,
                seed,
                source,
                first,
                next,
                body_exit,
                body_owner,
                resumed,
                returned,
                cleanup_start,
                continuation,
                key,
                value,
                blocks,
                carried,
                source_slot,
                cursor_slot,
                owner,
                source_origin,
                span,
            } = self;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let [preheader, header, body, exhausted] = blocks;
            if let Some((_, SourceIdentity::Temporary(_), _)) = owner {
                let call = definition(function, seed).ok_or_else(missing)?;
                if call.block != preheader
                    || call.origin != Origin::Source(source_origin)
                    || !matches!(
                        &call.operation,
                        Operation::DirectCall { receiver: None, .. }
                            | Operation::MapConstruct { .. }
                            | Operation::MapConsume { .. }
                    )
                    || unique(function.instructions.iter().filter(|instruction| {
                        instruction.origin == Origin::Source(source_origin)
                            && matches!(
                                instruction.operation,
                                Operation::DirectCall { .. }
                                    | Operation::MapConstruct { .. }
                                    | Operation::MapConsume { .. }
                            )
                    }))
                    .map(|instruction| instruction.id)
                        != Some(call.id)
                {
                    return Err(missing());
                }
            }
            let parameters = |block| {
                function
                    .block(block)
                    .map(|block| block.parameters.as_slice())
                    .ok_or_else(missing)
            };
            let header_parameters = parameters(header)?;
            let body_parameters = parameters(body)?;
            let exhausted_parameters = parameters(exhausted)?;
            let (header_source, header_cursor) =
                roles(header_parameters, source_slot, cursor_slot, span)?;
            let (body_source, body_cursor) =
                roles(body_parameters, source_slot, cursor_slot, span)?;
            let (exhausted_source, _) =
                roles(exhausted_parameters, source_slot, cursor_slot, span)?;
            let source_instruction =
                definition(function, EntityId::Loan(source)).ok_or_else(missing)?;
            if source_instruction.block != preheader {
                return Err(missing());
            }
            match (seed, &source_instruction.operation) {
                (
                    EntityId::Value(seed),
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                ) => {
                    let root = definition(function, EntityId::Place(*place)).ok_or_else(missing)?;
                    if root.block != preheader
                        || root.operation != (Operation::RootPlace { owner: seed })
                    {
                        return Err(missing());
                    }
                }
                (EntityId::Loan(seed), Operation::SharedReborrow { source }) if seed == *source => {
                }
                _ => return Err(missing()),
            }
            let first_instruction =
                definition(function, EntityId::Value(first)).ok_or_else(missing)?;
            if first_instruction.block != preheader
                || first_instruction.operation != (Operation::MapIterationFirst { source })
            {
                return Err(missing());
            }
            let Some(TerminatorKind::Branch(entry)) = terminator(function, preheader) else {
                return Err(missing());
            };
            if entry.target != header
                || entry.arguments.get(source_slot) != Some(&EntityId::Loan(source))
                || entry.arguments.get(cursor_slot) != Some(&EntityId::Value(first))
            {
                return Err(missing());
            }
            let Some(TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            }) = terminator(function, header)
            else {
                return Err(missing());
            };
            let guard = definition(function, EntityId::Value(*condition)).ok_or_else(missing)?;
            if guard.block != header
                || guard.operation
                    != (Operation::MapIterationHasNext {
                        source: header_source,
                        cursor: header_cursor,
                    })
                || when_true.target != body
                || when_false.target != exhausted
                || when_true.arguments != header_parameters
                || when_false.arguments != header_parameters
            {
                return Err(missing());
            }
            for (loan, operation) in [
                (
                    key,
                    Operation::MapIterationKeyLoan {
                        source: body_source,
                        cursor: body_cursor,
                    },
                ),
                (
                    value,
                    Operation::MapIterationValueLoan {
                        source: body_source,
                        cursor: body_cursor,
                    },
                ),
            ] {
                let projected = definition(function, EntityId::Loan(loan)).ok_or_else(missing)?;
                if projected.block != body
                    || projected.operation != operation
                    || projected.results != [EntityId::Loan(loan)]
                {
                    return Err(missing());
                }
            }
            if key == value {
                return Err(missing());
            }
            if let Some(state) = conditional {
                return conditional::validate(self, state);
            }
            let (body, body_source, body_cursor, key, value) = if let Some(current) = resumed {
                for (actual, original) in [
                    (EntityId::Loan(current.source), EntityId::Loan(body_source)),
                    (
                        EntityId::Value(current.cursor),
                        EntityId::Value(body_cursor),
                    ),
                    (EntityId::Loan(current.key), EntityId::Loan(key)),
                    (EntityId::Loan(current.value), EntityId::Loan(value)),
                ] {
                    nested::transport(function, actual, original, span)?;
                }
                (
                    cleanup_body,
                    current.source,
                    current.cursor,
                    current.key,
                    current.value,
                )
            } else {
                if cleanup_body != body {
                    return Err(missing());
                }
                (body, body_source, body_cursor, key, value)
            };
            let body_instructions = instructions(function, body).ok_or_else(missing)?;
            let end_position = |loan| {
                unique(body_instructions.iter().enumerate().filter_map(
                    |(position, instruction)| {
                        (instruction.operation == Operation::BorrowEnd { loan }).then_some(position)
                    },
                ))
                .ok_or_else(missing)
            };
            let value_end = end_position(value)?;
            let key_end = end_position(key)?;
            if value_end >= key_end {
                return Err(missing());
            }
            let backedge = match body_exit {
                IterationExitKind::Fallthrough | IterationExitKind::Continue(_) => {
                    let next = next.ok_or_else(missing)?;
                    let instruction =
                        definition(function, EntityId::Value(next)).ok_or_else(missing)?;
                    let position = body_instructions
                        .iter()
                        .position(|i| i.id == instruction.id)
                        .ok_or_else(missing)?;
                    if instruction.block != body
                        || instruction.operation
                            != (Operation::MapIterationNext {
                                source: body_source,
                                cursor: body_cursor,
                            })
                        || key_end >= position
                        || returned.is_some()
                    {
                        return Err(missing());
                    }
                    let Some(TerminatorKind::Branch(edge)) = terminator(function, body) else {
                        return Err(missing());
                    };
                    if edge.target != header
                        || edge.arguments.get(source_slot) != Some(&EntityId::Loan(body_source))
                        || edge.arguments.get(cursor_slot) != Some(&EntityId::Value(next))
                    {
                        return Err(missing());
                    }
                    Some(edge)
                }
                IterationExitKind::Break(_) | IterationExitKind::Return(_) => {
                    if next.is_some()
                        || body_instructions
                            .iter()
                            .any(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
                        || end_position(body_source)? <= key_end
                    {
                        return Err(missing());
                    }
                    match body_exit {
                        IterationExitKind::Break(_) => {
                            let Some(TerminatorKind::Branch(body_edge)) =
                                terminator(function, body)
                            else {
                                return Err(missing());
                            };
                            let Some(TerminatorKind::Branch(false_edge)) =
                                terminator(function, exhausted)
                            else {
                                return Err(missing());
                            };
                            if returned.is_some()
                                || body_edge.target != continuation
                                || false_edge.target != continuation
                                || continuation == exhausted
                            {
                                return Err(missing());
                            }
                        }
                        IterationExitKind::Return(_) => {
                            let Some(TerminatorKind::Return { values }) =
                                terminator(function, body)
                            else {
                                return Err(missing());
                            };
                            if values.as_slice() != returned.as_slice()
                                || cleanup_start.ok_or_else(missing)? > value_end
                                || continuation != exhausted
                            {
                                return Err(missing());
                            }
                        }
                        _ => return Err(missing()),
                    }
                    None
                }
                IterationExitKind::Exhaustion => return Err(missing()),
            };
            let exhausted_instructions = instructions(function, exhausted).ok_or_else(missing)?;
            if exhausted_instructions.iter().any(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::MapIterationKeyLoan { .. }
                        | Operation::MapIterationValueLoan { .. }
                        | Operation::MapIterationNext { .. }
                )
            }) {
                return Err(missing());
            }
            let source_end = unique(exhausted_instructions.iter().enumerate().filter_map(
                |(position, instruction)| {
                    (instruction.operation
                        == Operation::BorrowEnd {
                            loan: exhausted_source,
                        })
                    .then_some(position)
                },
            ))
            .ok_or_else(missing)?;
            if let Some((proof, symbol, dropped)) = owner {
                let (slot, binding) = unique(
                    carried
                        .slots
                        .iter()
                        .enumerate()
                        .filter(|(_, slot)| symbol.matches_slot(slot)),
                )
                .ok_or_else(missing)?;
                if binding.source != seed
                    || entry.arguments.get(slot) != Some(&seed)
                    || backedge
                        .is_some_and(|edge| edge.arguments.get(slot) != body_parameters.get(slot))
                    || exhausted_parameters.get(slot) != Some(&EntityId::Value(dropped))
                    || proof.origin() != source_origin
                {
                    return Err(missing());
                }
                let drops = exhausted_instructions
                    .iter()
                    .enumerate()
                    .filter(|(_, instruction)| {
                        instruction.operation == Operation::Drop { owner: dropped }
                    })
                    .collect::<Vec<_>>();
                let live = proof.at_checked(exhaustion_point, owned)?.is_none();
                if live {
                    if !drops.is_empty() {
                        return Err(missing());
                    }
                } else {
                    let [(drop_position, instruction)] = drops.as_slice() else {
                        return Err(missing());
                    };
                    if *drop_position <= source_end
                        || instruction.origin != Origin::Source(source_origin)
                    {
                        return Err(missing());
                    }
                }
                let Some((body_proof, body_symbol, body_value)) = body_owner else {
                    return Err(missing());
                };
                if body_symbol != symbol {
                    return Err(missing());
                }
                nested::transport(
                    function,
                    EntityId::Value(body_value),
                    *body_parameters.get(slot).ok_or_else(missing)?,
                    span,
                )?;
                let body_drops = body_instructions
                    .iter()
                    .enumerate()
                    .filter(|(_, instruction)| {
                        instruction.operation == Operation::Drop { owner: body_value }
                    })
                    .collect::<Vec<_>>();
                if exit_shape::terminal(body_exit) {
                    let point = match body_exit {
                        IterationExitKind::Break(expression)
                        | IterationExitKind::Return(expression) => {
                            DropPoint::ControlTransfer(expression)
                        }
                        _ => return Err(missing()),
                    };
                    if let Some(fact) = body_proof.at_checked(point, owned)? {
                        let [(position, instruction)] = body_drops.as_slice() else {
                            return Err(missing());
                        };
                        if fact.value_origin() != source_origin
                            || *position <= end_position(body_source)?
                            || instruction.origin != Origin::Source(source_origin)
                        {
                            return Err(missing());
                        }
                    } else if !matches!(body_exit, IterationExitKind::Break(_))
                        || !body_drops.is_empty()
                    {
                        return Err(missing());
                    }
                } else if !body_drops.is_empty() {
                    return Err(missing());
                }
            } else if !matches!(seed, EntityId::Loan(_)) {
                return Err(missing());
            }
            if let EntityId::Loan(entry_loan) = seed
                && (body_owner.is_some()
                    || body_instructions
                        .iter()
                        .chain(exhausted_instructions.iter())
                        .any(|i| i.operation == Operation::BorrowEnd { loan: entry_loan }))
            {
                return Err(missing());
            }
            Ok(())
        }
    }

    pub(in crate::ssa) fn definition(
        function: &Function,
        entity: EntityId,
    ) -> Option<&Instruction> {
        let Definition::InstructionResult {
            instruction,
            index: 0,
        } = function.entity(entity)?.definition
        else {
            return None;
        };
        let instruction = function.instruction(instruction)?;
        (instruction.results.as_slice() == [entity]).then_some(instruction)
    }
    pub(in crate::ssa) fn terminator(
        function: &Function,
        block: BlockId,
    ) -> Option<&TerminatorKind> {
        function
            .block(block)?
            .terminator
            .as_ref()
            .map(|terminator| &terminator.kind)
    }
    pub(in crate::ssa) fn instructions(
        function: &Function,
        block: BlockId,
    ) -> Option<Vec<&Instruction>> {
        function
            .block(block)?
            .instructions
            .iter()
            .map(|id| function.instruction(*id))
            .collect()
    }
}
