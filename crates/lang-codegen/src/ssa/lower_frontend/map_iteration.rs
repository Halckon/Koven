//! Bounded two-component Map provider; independent from Sequential snapshots.
mod carrier;
mod conditional;
mod exit_shape;
mod exits;
mod nested;
mod nested_events;
mod qualify;
mod source;
mod source_owner;
mod temporary;

use super::*;
use crate::ssa::model::{Definition, MapContainerKind, Module, SsaTypeKind};
use control::{LinearBindingSlot, LinearBindings};
use lang_frontend::ownership_checking::{
    CleanupCondition, IterationCleanupAction, IterationExitKind, IterationExitPlan,
    MapIterationOwnershipPlan, MapIterationSourceAccess,
};

pub(super) use exits::Context;
pub(super) use qualify::validate;
use source::SourceIdentity;
pub(super) fn supported_drop(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: lang_frontend::ownership_checking::DropFact,
) -> bool {
    source_owner::supported_drop(parsed, names, typed, owned, fact)
        || nested::tail::supported_drop(parsed, names, typed, owned, fact)
}

type Storage = BTreeMap<SsaTypeId, (MapContainerKind, SsaTypeId, SsaTypeId)>;

/// Read the actual Module only after ordinary type planning has finished.
pub(super) fn storage(module: &Module, types: &BTreeMap<TypeId, SsaTypeId>) -> Storage {
    types
        .values()
        .filter_map(|&ty| module.map_container(ty).map(|layout| (ty, layout)))
        .collect()
}

#[derive(Default)]
pub(super) struct State {
    entries: BTreeMap<SymbolId, EntityId>,
    storage: Storage,
}
impl State {
    pub(super) fn new(
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        loans: &BTreeMap<SymbolId, LoanId>,
        storage: &Storage,
    ) -> Self {
        Self {
            entries: bindings
                .iter()
                .filter_map(|(&symbol, binding)| match binding {
                    LoweredValue::Value(owner) => Some((symbol, EntityId::Value(*owner))),
                    _ => None,
                })
                .chain(
                    loans
                        .iter()
                        .map(|(&symbol, &loan)| (symbol, EntityId::Loan(loan))),
                )
                .collect(),
            storage: storage.clone(),
        }
    }
}

fn unique<T>(mut values: impl Iterator<Item = T>) -> Option<T> {
    let result = values.next()?;
    values.next().is_none().then_some(result)
}

/// One path-local provider frame; false is rebuilt from its independent snapshot.
#[derive(Clone, Copy)]
struct Frame {
    statement: StatementId,
    source: Option<LoanId>,
    components: [(SymbolId, Option<LoanId>); 2],
    finished: bool,
}

impl ExpressionLowerer<'_> {
    pub(super) fn lower_map_iteration(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = *self
            .typed
            .map_iteration(statement)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let qualified =
            qualify::qualify(self.parsed, self.names, self.typed, self.owned, descriptor)?;
        if descriptor.source() != source || qualified.body != body {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if (!self.loops.is_empty() && qualified.nested.is_none())
            || self.thunk_expression.is_some()
            || self.readonly_receiver.is_some()
            || !self.pending_call_loans.is_empty()
            || !self.temporaries.is_empty()
            || !self.non_null_bindings.is_empty()
            || !self.capture_loans.is_empty()
            || !self.result_source_loans.is_empty()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let plan = qualified.plan.clone();
        let body_exit = qualified.body_exit;
        let source_identity = qualified.source;
        let owner_proof = qualified.owner;
        let source_origin = qualified.origin;
        let mapped = |ty| {
            self.resolve_type(ty, span).and_then(|ty| {
                self.type_ids
                    .get(&ty)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
            })
        };
        let container = mapped(descriptor.source_type())?;
        let key_type = mapped(descriptor.key_type())?;
        let value_type = mapped(descriptor.value_type())?;
        let expected_kind = match descriptor.kind() {
            lang_frontend::type_checking::MapIterationKind::Map => MapContainerKind::Map,
            lang_frontend::type_checking::MapIterationKind::MutableMap => {
                MapContainerKind::MutableMap
            }
        };
        if self.map_iteration_state.storage.get(&container)
            != Some(&(expected_kind, key_type, value_type))
            || self.ssa_types.get(container.index())
                != Some(&SsaTypeKind::MapContainer {
                    kind: expected_kind,
                    key: key_type,
                    value: value_type,
                })
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let cursor_type = self.ssa_builtin(BuiltinType::ULong, span)?;
        let bool_type = self.ssa_builtin(BuiltinType::Boolean, span)?;
        if self.ssa_types.get(cursor_type.index())
            != Some(&SsaTypeKind::Integer {
                bits: 64,
                signed: false,
            })
            || self.ssa_types.get(bool_type.index()) != Some(&SsaTypeKind::Boolean)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let seed = self.map_iteration_seed(
            source_identity,
            container,
            source_origin,
            qualified.nested,
            span,
        )?;
        let source_loan = match (plan.source_access(), seed) {
            (
                MapIterationSourceAccess::Owned | MapIterationSourceAccess::Temporary,
                EntityId::Value(owner),
            ) => {
                if self.function.entity(seed).map(|entity| entity.ty)
                    != Some(EntityType::Value(container))
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                if source_identity.current_owner(self) != Some(owner) {
                    let kind = match source_identity {
                        SourceIdentity::Parameter(_) => LoweringErrorKind::UnsupportedNode,
                        SourceIdentity::Temporary(_) => LoweringErrorKind::MissingFact,
                    };
                    return Err(error(kind, span));
                }
                let (_, places) = self.append(
                    Operation::RootPlace { owner },
                    vec![EntityType::Place(container)],
                    span,
                )?;
                let [EntityId::Place(place)] = places.as_slice() else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                self.map_iteration_loan(
                    Operation::BorrowBegin {
                        place: *place,
                        kind: LoanKind::Shared,
                    },
                    container,
                    span,
                )?
            }
            (MapIterationSourceAccess::Shared, EntityId::Loan(loan)) => {
                let SourceIdentity::Parameter(symbol) = source_identity else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                if self.borrow_bindings.get(&symbol) != Some(&loan)
                    || self.function.entity(seed).map(|entity| entity.ty)
                        != Some(EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: container,
                        })
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                self.map_iteration_loan(
                    Operation::SharedReborrow { source: loan },
                    container,
                    span,
                )?
            }
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let first = self.map_iteration_value(
            Operation::MapIterationFirst {
                source: source_loan,
            },
            cursor_type,
            span,
        )?;
        let baseline = self.bindings.clone();
        let mut carried = self.linear_binding_slots(&baseline, span)?;
        for (&binding, &lowered) in &baseline {
            let LoweredValue::Value(owner) = lowered else {
                continue;
            };
            if carried
                .slots
                .iter()
                .any(|slot| slot.symbol == Some(binding))
            {
                continue;
            }
            let entity = EntityId::Value(owner);
            let ty = self
                .function
                .entity(entity)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .ty;
            carried
                .slots
                .push(LinearBindingSlot::new(Some(binding), entity, ty));
        }
        let nested_checkpoint = self.prepare_map_nested(
            qualified.nested,
            statement,
            source_identity,
            &mut carried,
            span,
        )?;
        let source_slot = carried.slots.len();
        let cursor_slot = source_slot + 1;
        let mut types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
        types.extend([
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: container,
            },
            EntityType::Value(cursor_type),
        ]);
        let preheader = self.block;
        let header = self
            .function
            .add_block(types.clone(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let body_block = self
            .function
            .add_block(types.clone(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let exhausted = self
            .function
            .add_block(types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let mut entry_args = carried
            .slots
            .iter()
            .map(|slot| slot.source)
            .collect::<Vec<_>>();
        entry_args.extend([EntityId::Loan(source_loan), EntityId::Value(first)]);
        self.function
            .set_terminator(
                preheader,
                TerminatorKind::Branch(Edge {
                    target: header,
                    arguments: entry_args,
                }),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.block = header;
        self.bindings = self.rebind_linear_bindings(&baseline, header, &carried, span)?;
        let parent_header =
            self.rebind_map_parent(nested_checkpoint, None, preheader, header, span)?;
        let header_parameters = self.map_iteration_parameters(header, span)?;
        let (header_source, header_cursor) =
            roles(&header_parameters, source_slot, cursor_slot, span)?;
        let condition = self.map_iteration_value(
            Operation::MapIterationHasNext {
                source: header_source,
                cursor: header_cursor,
            },
            bool_type,
            span,
        )?;
        self.function
            .set_terminator(
                header,
                TerminatorKind::Conditional {
                    condition,
                    when_true: Edge {
                        target: body_block,
                        arguments: header_parameters.clone(),
                    },
                    when_false: Edge {
                        target: exhausted,
                        arguments: header_parameters,
                    },
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.block = exhausted;
        self.bindings = self.rebind_linear_bindings(&baseline, exhausted, &carried, span)?;
        let parent_exhausted =
            self.rebind_map_parent(nested_checkpoint, parent_header, header, exhausted, span)?;
        let false_exit = self.branch_exit(LoweredValue::Unit);
        self.block = body_block;
        self.bindings = self.rebind_linear_bindings(&baseline, body_block, &carried, span)?;
        let parent_body =
            self.rebind_map_parent(nested_checkpoint, parent_header, header, body_block, span)?;
        let body_parameters = self.map_iteration_parameters(body_block, span)?;
        let (body_source, body_cursor) = roles(&body_parameters, source_slot, cursor_slot, span)?;
        let key = self.map_iteration_loan(
            Operation::MapIterationKeyLoan {
                source: body_source,
                cursor: body_cursor,
            },
            key_type,
            span,
        )?;
        let value = self.map_iteration_loan(
            Operation::MapIterationValueLoan {
                source: body_source,
                cursor: body_cursor,
            },
            value_type,
            span,
        )?;
        if key == value
            || self
                .borrow_bindings
                .insert(descriptor.key_binding(), key)
                .is_some()
            || self
                .borrow_bindings
                .insert(descriptor.value_binding(), value)
                .is_some()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let body_owner = self.map_iteration_owner(
            owner_proof,
            source_identity,
            &carried,
            &body_parameters,
            span,
        )?;
        let parent = nested::parent(
            nested_checkpoint,
            [preheader, header, body_block, exhausted],
            parent_header,
            parent_body,
            parent_exhausted,
            span,
        )?;
        let nested = self.map_nested_state(
            qualified.nested,
            source_identity,
            (body_source, body_cursor, [key, value]),
            parent,
            span,
        )?;
        self.loops.push(loop_control::LoopContext {
            entry_views: self.non_null_bindings.clone(),
            outer_borrowed_while: None,
            header,
            carried: carried.clone(),
            continues: Vec::new(),
            breaks: Vec::new(),
            for_loop: None,
            numeric_loop: None,
            map_loop: Some(Context {
                plan: plan.clone(),
                kind: body_exit,
                body: body_block,
                frame: Frame {
                    statement,
                    source: Some(body_source),
                    components: [
                        (descriptor.key_binding(), Some(key)),
                        (descriptor.value_binding(), Some(value)),
                    ],
                    finished: false,
                },
                owner: body_owner,
                cursor: body_cursor,
                cursor_type,
                next: None,
                cleaned: false,
                returned: None,
                cleanup_start: None,
                conditional: qualified.conditional.map(conditional::State::new),
                nested,
                nested_events: Vec::new(),
            }),
        });
        // The exact selected exit owns the body cleanup point. A real transfer
        // is already Diverged and must not acquire a synthetic fallthrough/Next.
        let result = self.lower_statement_inner(body)?;
        if (qualified.conditional.is_none()
            && qualified.nested.is_none()
            && self.block != body_block)
            || (result == LoweredValue::Diverged) == (body_exit == IterationExitKind::Fallthrough)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if result != LoweredValue::Diverged {
            self.clean_map_iteration_body(IterationExitKind::Fallthrough, span)?;
            self.advance_map_iteration_body(span)?;
        }
        let mut context = self
            .loops
            .pop()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mut state = context
            .map_loop
            .take()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if !state.cleaned
            || !context.continues.is_empty()
            || context.breaks.len()
                != usize::from(
                    qualified.conditional.is_some()
                        || matches!(body_exit, IterationExitKind::Break(_)),
                )
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // Restore every ordinary owner/loan slot from the independent false edge.
        if self.merge_exits(vec![false_exit], &baseline, span)? != LoweredValue::Unit {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.restore_map_parent_false(
            state.nested.as_ref().and_then(|nested| nested.parent),
            span,
        )?;
        let exhausted_parameters = self.map_iteration_parameters(exhausted, span)?;
        let (exhausted_source, _) = roles(&exhausted_parameters, source_slot, cursor_slot, span)?;
        let mut frame = Frame {
            statement,
            source: Some(exhausted_source),
            components: [
                (descriptor.key_binding(), None),
                (descriptor.value_binding(), None),
            ],
            finished: false,
        };
        let owner = self.map_iteration_owner(
            owner_proof,
            source_identity,
            &carried,
            &exhausted_parameters,
            span,
        )?;
        let exhaustion = unique(
            plan.exits()
                .iter()
                .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
        )
        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.emit_map_iteration_cleanup(exhaustion, &mut frame, owner, span)?;
        if !frame.finished
            || frame.source.is_some()
            || owner.is_some_and(|(_, source, _)| source.current_owner(self).is_some())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let parent = state.nested.as_ref().and_then(|nested| nested.parent);
        if let Some(parent) = parent.filter(|_| {
            qualified.nested.is_some_and(|pair| {
                pair.tail
                    .is_some_and(|shape| matches!(shape.kind, IterationExitKind::Break(_)))
                    && statement == pair.inner
            })
        }) {
            let [broken] = <[control::BranchExit; 1]>::try_from(context.breaks)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let false_exit = self.branch_exit(LoweredValue::Unit);
            let parent =
                self.merge_map_nested_break(parent, broken, false_exit, &baseline, &carried, span)?;
            state
                .nested
                .as_mut()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .parent = Some(parent);
            self.restore_map_parent(Some(parent), span)?;
        } else {
            self.restore_map_parent(parent, span)?;
            if qualified.nested.is_some() && !context.breaks.is_empty() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            context.breaks.push(self.branch_exit(LoweredValue::Unit));
            self.merge_exits(context.breaks, &baseline, span)?;
        }
        carrier::Evidence {
            function: self.function,
            conditional: state.conditional.as_ref(),
            nested: state.nested.as_ref(),
            events: &state.nested_events,
            cleanup_body: state.body,
            seed,
            source: source_loan,
            first,
            next: state.next,
            body_exit,
            body_owner,
            returned: state.returned,
            cleanup_start: state.cleanup_start,
            continuation: self.block,
            key,
            value,
            blocks: [preheader, header, body_block, exhausted],
            carried: &carried,
            source_slot,
            cursor_slot,
            owner,
            source_origin,
            span,
        }
        .validate()?;
        Ok(LoweredValue::Unit)
    }

    fn map_iteration_owner(
        &self,
        proof: Option<source_owner::Proof>,
        source: SourceIdentity,
        carried: &LinearBindings,
        parameters: &[EntityId],
        span: Span,
    ) -> Result<Option<(source_owner::Proof, SourceIdentity, ValueId)>, LoweringError> {
        proof
            .map(|proof| {
                let slot = unique(
                    carried
                        .slots
                        .iter()
                        .enumerate()
                        .filter(|(_, slot)| source.matches_slot(slot)),
                )
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .0;
                let Some(EntityId::Value(value)) = parameters.get(slot) else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                if source.current_owner(self) != Some(*value) {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                Ok((proof, source, *value))
            })
            .transpose()
    }

    fn map_iteration_value(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
        match results.as_slice() {
            [EntityId::Value(value)] => Ok(*value),
            _ => Err(error(LoweringErrorKind::InvalidModel, span)),
        }
    }
    fn map_iteration_loan(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        let (_, results) = self.append(
            operation,
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            }],
            span,
        )?;
        match results.as_slice() {
            [EntityId::Loan(loan)] => Ok(*loan),
            _ => Err(error(LoweringErrorKind::InvalidModel, span)),
        }
    }
    fn map_iteration_parameters(
        &self,
        block: BlockId,
        span: Span,
    ) -> Result<Vec<EntityId>, LoweringError> {
        self.function
            .block(block)
            .map(|block| block.parameters.clone())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }
    fn map_iteration_carried_arguments(
        &self,
        carried: &LinearBindings,
        span: Span,
    ) -> Result<Vec<EntityId>, LoweringError> {
        carried
            .slots
            .iter()
            .enumerate()
            .map(|(column, slot)| {
                if let Some(entity) = self.map_nested_private_argument(column, span)? {
                    return Ok(entity);
                }
                if let Some(symbol) = slot.symbol {
                    return match self.bindings.get(&symbol) {
                        Some(LoweredValue::Value(owner)) => Ok(EntityId::Value(*owner)),
                        _ => Err(error(LoweringErrorKind::MissingFact, span)),
                    };
                }
                if let [key] = slot.temporaries.as_slice() {
                    return self
                        .temporaries
                        .get(key)
                        .copied()
                        .map(EntityId::Value)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span));
                }
                // Only original non-entry borrow bindings or this qualified source's
                // exact temporary owner can be present; other state is rejected at entry.
                let loans = slot
                    .borrow_symbols
                    .iter()
                    .filter_map(|symbol| self.borrow_bindings.get(symbol))
                    .copied()
                    .collect::<Vec<_>>();
                match loans.as_slice() {
                    [loan] => Ok(EntityId::Loan(*loan)),
                    _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
                }
            })
            .collect()
    }

    fn emit_map_iteration_cleanup(
        &mut self,
        exit: &IterationExitPlan,
        frame: &mut Frame,
        owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for action in exit.actions() {
            match action {
                IterationCleanupAction::EndBinding { statement, symbol }
                    if *statement == frame.statement =>
                {
                    let index = frame
                        .components
                        .iter()
                        .position(|(binding, _)| binding == symbol)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    if index == 0 && frame.components[1].1.is_some() {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    let loan = frame.components[index]
                        .1
                        .take()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    if self.borrow_bindings.remove(symbol) != Some(loan) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                }
                IterationCleanupAction::FinishProvider(statement)
                    if *statement == frame.statement =>
                {
                    if frame.finished
                        || frame.source.is_none()
                        || frame.components.iter().any(|(_, loan)| loan.is_some())
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    frame.finished = true;
                }
                IterationCleanupAction::EndSource(statement) if *statement == frame.statement => {
                    if !frame.finished {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    let loan = frame
                        .source
                        .take()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                }
                IterationCleanupAction::Drop(fact)
                    if owner.is_some_and(|(proof, _, _)| proof.at(exit.point()) == Some(*fact)) =>
                {
                    let (_, source, value) =
                        owner.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    if frame.source.is_some()
                        || !frame.finished
                        || source.current_owner(self) != Some(value)
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    source.take_owner(self, value, span)?;
                    self.append(
                        Operation::Drop { owner: value },
                        Vec::new(),
                        fact.value_origin(),
                    )?;
                }
                IterationCleanupAction::Drop(_) => {
                    self.emit_iteration_cleanup_action(action, span)?
                }
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            }
        }
        Ok(())
    }
}

fn roles(
    parameters: &[EntityId],
    source: usize,
    cursor: usize,
    span: Span,
) -> Result<(LoanId, ValueId), LoweringError> {
    match (parameters.get(source), parameters.get(cursor)) {
        (Some(EntityId::Loan(source)), Some(EntityId::Value(cursor))) => Ok((*source, *cursor)),
        _ => Err(error(LoweringErrorKind::MissingFact, span)),
    }
}
