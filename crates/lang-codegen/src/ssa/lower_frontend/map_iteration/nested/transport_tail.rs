//! Path-local restoration and the exact two-edge Break join for InnerTail.
use super::super::super::control::BranchExit;
use super::*;

/// Only mutable execution state belongs here. In particular, cumulative cleanup
/// events and nested transport evidence are never restored from this checkpoint.
#[derive(Clone, Copy)]
pub(super) struct PathCheckpoint {
    frame: Frame,
    body: BlockId,
    kind: IterationExitKind,
    cursor: ValueId,
    cleaned: bool,
    next: Option<ValueId>,
    returned: Option<ValueId>,
    cleanup_start: Option<usize>,
}

impl PathCheckpoint {
    pub(super) fn capture(state: &Context) -> Self {
        Self {
            frame: state.frame,
            body: state.body,
            kind: state.kind,
            cursor: state.cursor,
            cleaned: state.cleaned,
            next: state.next,
            returned: state.returned,
            cleanup_start: state.cleanup_start,
        }
    }

    fn restore(self, state: &mut Context) {
        state.frame = self.frame;
        state.body = self.body;
        state.kind = self.kind;
        state.cursor = self.cursor;
        state.cleaned = self.cleaned;
        state.next = self.next;
        state.returned = self.returned;
        state.cleanup_start = self.cleanup_start;
    }
}

pub(super) fn current_guard(
    lowerer: &ExpressionLowerer<'_>,
    state: &Context,
    span: Span,
) -> Result<Option<ValueId>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let nested = state.nested.as_ref().ok_or_else(missing)?;
    let Some(shape) = nested.pair.tail else {
        return Ok(None);
    };
    // Roles describe this provider. Only the outer provider has the transported
    // Guard role; the inner body's Guard is consumed before any outgoing edge.
    if state.frame.statement != nested.pair.outer {
        return Ok(None);
    }
    let Some(LoweredValue::Value(current)) = lowerer.bindings.get(&shape.outer_guard.symbol) else {
        return Err(missing());
    };
    let anchor = constructor_anchor(lowerer, shape.outer_guard, span)?;
    transport(
        lowerer.function,
        EntityId::Value(*current),
        EntityId::Value(anchor),
        span,
    )?;
    Ok(Some(*current))
}

/// Find and validate the actual zero-field class construction result. The
/// initializer's source identity only locates it; operation/result/payload/type
/// identities form the anchor used by every later edge proof.
pub(super) fn constructor_anchor(
    lowerer: &ExpressionLowerer<'_>,
    local: tail::Local,
    span: Span,
) -> Result<ValueId, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let ty = lowerer.expression_ssa_type(local.initializer, span)?;
    let resolved = lowerer.resolve_type(local.ty, span)?;
    if lowerer.type_ids.get(&resolved) != Some(&ty) {
        return Err(missing());
    }
    let instruction = unique(lowerer.function.instructions.iter().filter(|instruction| {
        instruction.origin == Origin::Source(local.initializer_span)
            && matches!(instruction.operation, Operation::HeapAllocate { owner, .. } if owner == ty)
    }))
    .ok_or_else(missing)?;
    let [EntityId::Value(owner)] = instruction.results.as_slice() else {
        return Err(missing());
    };
    let Operation::HeapAllocate { payload, .. } = instruction.operation else {
        return Err(missing());
    };
    let payload =
        carrier::definition(lowerer.function, EntityId::Value(payload)).ok_or_else(missing)?;
    let expected_payload = lowerer.heap_payloads.get(&ty).ok_or_else(missing)?;
    if payload.block != instruction.block
        || payload.origin != instruction.origin
        || !matches!(&payload.operation, Operation::AggregateConstruct { aggregate, fields }
            if aggregate == expected_payload && fields.is_empty())
        || carrier::definition(lowerer.function, EntityId::Value(*owner)).map(|i| i.id)
            != Some(instruction.id)
    {
        return Err(missing());
    }
    Ok(*owner)
}

impl ExpressionLowerer<'_> {
    /// The sibling false path is licensed by its own header edge and the exact
    /// checkpoint frozen before inner body execution, never by terminal body state.
    pub(in super::super) fn restore_map_parent_false(
        &mut self,
        parent: Option<Parent>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(parent) = parent else {
            return Ok(());
        };
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let [preheader, header, body, exhausted] = parent.blocks;
        if self.loops.len() != 1 || self.block != exhausted {
            return Err(missing());
        }
        let state = self
            .loops
            .first()
            .and_then(|context| context.map_loop.as_ref())
            .ok_or_else(missing)?;
        let nested = state.nested.as_ref().ok_or_else(missing)?;
        let Some(shape) = nested.pair.tail else {
            return Ok(());
        };
        let saved = parent.checkpoint.path;
        if nested.pair.outer != parent.checkpoint.statement
            || state.frame.statement != nested.pair.outer
            || nested.resumed.is_some()
            || saved.frame.statement != nested.pair.outer
            || saved.body != preheader
            || saved.kind != IterationExitKind::Fallthrough
            || saved.cleaned
            || saved.frame.finished
            || saved.next.is_some()
            || saved.returned.is_some()
            || saved.cleanup_start.is_some()
            || saved.frame.source != Some(parent.checkpoint.before.source)
            || saved.frame.components[0].1 != Some(parent.checkpoint.before.key)
            || saved.frame.components[1].1 != Some(parent.checkpoint.before.value)
            || saved.cursor != parent.checkpoint.before.cursor
            || parent.resume_start.is_some()
            || parent.continuation != exhausted
        {
            return Err(missing());
        }
        let edge = edge_to(self.function, header, exhausted, span)?;
        let parameters = self.map_iteration_parameters(exhausted, span)?;
        if forwarded(
            self.function,
            parent.checkpoint.columns,
            parent.checkpoint.guard_column,
            parent.header,
            &edge.arguments,
            &parameters,
            span,
        )? != parent.exhausted
        {
            return Err(missing());
        }
        if matches!(shape.kind, IterationExitKind::Return(_)) {
            let terminal = unique(state.nested_events.iter().filter(|event| {
                event.kind == shape.kind
                    && event.statement == nested.pair.outer
                    && event.block == body
                    && event.returned.is_some()
                    && event.source == parent.body.source
                    && event.cursor == parent.body.cursor
                    && event.key == parent.body.key
                    && event.value == parent.body.value
                    && Some(event.guard) == parent.body.guard
            }))
            .ok_or_else(missing)?;
            let returned = terminal.returned.ok_or_else(missing)?;
            if !matches!(carrier::terminator(self.function, body),
                Some(TerminatorKind::Return { values }) if values.as_slice() == [returned])
                || !state.cleaned
                || !state.frame.finished
                || state.frame.source.is_some()
                || state
                    .frame
                    .components
                    .iter()
                    .any(|(_, loan)| loan.is_some())
                || state.body != body
                || state.next.is_some()
            {
                return Err(missing());
            }
        } else if state.cleaned
            || state.frame.finished
            || state.frame.source != Some(parent.body.source)
            || state.frame.components[0].1 != Some(parent.body.key)
            || state.frame.components[1].1 != Some(parent.body.value)
            || state.cursor != parent.body.cursor
            || state.body != body
            || state.kind != IterationExitKind::Fallthrough
            || state.returned.is_some()
            || state.next.is_some()
            || state.cleanup_start.is_some()
        {
            return Err(missing());
        }
        let state = self
            .loops
            .first_mut()
            .and_then(|context| context.map_loop.as_mut())
            .ok_or_else(missing)?;
        saved.restore(state);
        self.install_map_parent(parent.checkpoint, parent.exhausted, span)
    }

    /// Both predecessors have already consumed their own inner source. The join
    /// carries exactly the original inner carried set, which contains all outer
    /// roles and deliberately contains neither inner source nor inner cursor.
    pub(in super::super) fn merge_map_nested_break(
        &mut self,
        mut parent: Parent,
        break_exit: BranchExit,
        false_exit: BranchExit,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        carried: &LinearBindings,
        span: Span,
    ) -> Result<Parent, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let state = self
            .loops
            .first()
            .and_then(|context| context.map_loop.as_ref())
            .ok_or_else(missing)?;
        let pair = state.nested.as_ref().ok_or_else(missing)?.pair;
        if self.loops.len() != 1
            || !pair
                .tail
                .is_some_and(|shape| matches!(shape.kind, IterationExitKind::Break(_)))
            || parent.checkpoint.statement != pair.outer
            || break_exit.block != parent.blocks[2]
            || false_exit.block != parent.blocks[3]
            || parent.continuation != parent.blocks[3]
            || self.block != false_exit.block
            || parent.resume_start.is_some()
            || carrier::terminator(self.function, break_exit.block).is_some()
            || carrier::terminator(self.function, false_exit.block).is_some()
        {
            return Err(missing());
        }
        let entry = edge_to(self.function, parent.blocks[0], parent.blocks[1], span)?;
        if entry.arguments.len() != carried.slots.len() + 2
            || entry.arguments[..carried.slots.len()]
                != carried.slots.iter().map(|s| s.source).collect::<Vec<_>>()
        {
            return Err(missing());
        }
        let mut branches = Vec::new();
        for (exit, roles) in [(&break_exit, parent.body), (&false_exit, parent.exhausted)] {
            if exit.result != LoweredValue::Unit
                || !exit.temporaries.is_empty()
                || !exit.loans.is_empty()
                || !exit.capture_loans.is_empty()
                || !exit.views.is_empty()
                || !exit.result_source_loans.is_empty()
                || !exit.for_sources.is_empty()
                || !exit.for_elements.is_empty()
            {
                return Err(missing());
            }
            let arguments = carried
                .slots
                .iter()
                .enumerate()
                .map(|(column, slot)| {
                    let entity = if column == parent.checkpoint.columns[1] {
                        EntityId::Loan(roles.source)
                    } else if column == parent.checkpoint.columns[2] {
                        EntityId::Value(roles.cursor)
                    } else if let Some(symbol) = slot.symbol {
                        match exit.bindings.get(&symbol) {
                            Some(LoweredValue::Value(owner)) => EntityId::Value(*owner),
                            _ => return Err(missing()),
                        }
                    } else {
                        let symbol =
                            unique(slot.borrow_symbols.iter().copied()).ok_or_else(missing)?;
                        EntityId::Loan(*exit.borrow_bindings.get(&symbol).ok_or_else(missing)?)
                    };
                    if self.function.entity(entity).map(|e| e.ty) != Some(slot.ty)
                        || slot.borrow_symbols.iter().any(|symbol| {
                            exit.borrow_bindings
                                .get(symbol)
                                .copied()
                                .map(EntityId::Loan)
                                != Some(entity)
                        })
                    {
                        return Err(missing());
                    }
                    Ok(entity)
                })
                .collect::<Result<Vec<_>, _>>()?;
            branches.push((exit.block, roles, arguments));
        }
        let join = self.add_linear_binding_block(carried, span)?;
        let parameters = self.map_iteration_parameters(join, span)?;
        let mut joined = None;
        for (block, roles, arguments) in branches {
            let actual = forwarded(
                self.function,
                parent.checkpoint.columns,
                parent.checkpoint.guard_column,
                roles,
                &arguments,
                &parameters,
                span,
            )?;
            if joined
                .replace(actual)
                .is_some_and(|previous| previous != actual)
            {
                return Err(missing());
            }
            self.function
                .set_terminator(
                    block,
                    TerminatorKind::Branch(Edge {
                        target: join,
                        arguments,
                    }),
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        }
        self.block = join;
        self.bindings = self.rebind_linear_bindings(baseline, join, carried, span)?;
        parent.continuation = join;
        parent.resumed_roles = joined.ok_or_else(missing)?;
        self.install_map_parent(parent.checkpoint, parent.resumed_roles, span)?;
        Ok(parent)
    }
}
