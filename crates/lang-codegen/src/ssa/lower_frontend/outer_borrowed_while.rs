use super::control::BranchExit;
use super::non_null_proof::outer_borrowed_provider as provider;
mod source;
pub(in crate::ssa::lower_frontend) use super::*;
use crate::ssa::model::SsaTypeKind;
use carrier::{Columns, instructions, transport};
use lang_frontend::{
    ownership_checking::{
        CleanupCondition, CleanupConditions, CleanupOwnerValue, CleanupSelection,
        CleanupSelectorSource, DropFact, DropTarget, IterationCleanupAction, IterationExitKind,
        IterationExitPlan, IterationPhiBoundary, IterationPhiIncomingKind,
        IterationPhiPresenceSource, IterationSource, OwnershipBindingKind,
    },
    type_checking::{
        IterationProvider, NullComparisonDescriptor, SequentialContainerKind,
        SequentialIterationBinding,
    },
};
pub(in crate::ssa::lower_frontend) use source::carrier;

/// Only complete published facts and the bounded source relation construct this token.
#[derive(Clone, PartialEq, Eq)]
pub(in crate::ssa::lower_frontend) struct Relation {
    pub(in crate::ssa::lower_frontend) outer: StatementId,
    pub(in crate::ssa::lower_frontend) inner: StatementId,
    pub(in crate::ssa::lower_frontend) breaking: ExpressionId,
    pub(in crate::ssa::lower_frontend) root: SymbolId,
    pub(in crate::ssa::lower_frontend) items: SymbolId,
    pub(in crate::ssa::lower_frontend) item: SymbolId,
    pub(in crate::ssa::lower_frontend) storage: TypeId,
    pub(in crate::ssa::lower_frontend) parameter_slot: usize,
    pub(in crate::ssa::lower_frontend) initializer: ExpressionId,
    pub(in crate::ssa::lower_frontend) drops: [DropFact; 2],
}

pub(super) fn candidate(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> Option<Relation> {
    let DropPoint::LoopExit(outer) = fact.point() else {
        return None;
    };
    let plan = owned.iteration(outer)?;
    let descriptor = plan.descriptor();
    let IterationSource::BorrowedSource(LoanTarget::Place(place)) = plan.source_kind() else {
        return None;
    };
    let items = place.root();
    let SequentialIterationBinding::Name(item) = descriptor.binding() else {
        return None;
    };
    let item = *item;
    let conditions = owned.cleanup_conditions();
    let exhausted = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
    )?;
    let [
        IterationCleanupAction::FinishProvider(finish),
        IterationCleanupAction::EndSource(end),
        IterationCleanupAction::Drop(items_drop),
        IterationCleanupAction::Drop(current_drop),
    ] = exhausted.actions()
    else {
        return None;
    };
    let DropTarget::Named(root) = current_drop.target() else {
        return None;
    };
    let storage = typed.symbol_type(root)?;
    let TypeKind::Nullable(inner) = typed.types().get(storage)? else {
        return None;
    };
    let TypeKind::Nominal { nominal, arguments } = typed.types().get(*inner)? else {
        return None;
    };
    if ![current_drop, items_drop].contains(&&fact)
        || *finish != outer
        || *end != outer
        || exhausted.point() != fact.point()
        || descriptor.statement() != outer
        || descriptor.provider() != IterationProvider::Sequential(SequentialContainerKind::List)
        || descriptor.delivery() != ParameterMode::Borrow
        || !place.is_root()
        || root == items
        || item == root
        || item == items
        || typed.symbol_type(items) != Some(descriptor.source_type())
        || typed.symbol_type(item) != Some(descriptor.element_type())
        || builtin_type(typed, descriptor.element_type()) != Some(BuiltinType::Int)
        || descriptor.source_type() == storage
        || !arguments.is_empty()
        || typed
            .nominals()
            .iter()
            .find(|entry| entry.id() == *nominal)?
            .kind()
            != NominalKind::Class
        || [storage, *inner, descriptor.source_type()]
            .iter()
            .any(|ty| typed.copyability(*ty) != Some(Copyability::MoveOnly))
        || !plan.capture_graph().nodes().is_empty()
        || plan.bindings().len() != 1
        || plan.bindings()[0].symbol() != item
        || plan.bindings()[0].kind() != OwnershipBindingKind::Shared
        || owned
            .borrow_results()
            .bindings()
            .iter()
            .any(|binding| [root, items].contains(&binding.binding()))
        || plan.closure_phis().len() != 4
        || plan.closure_phi_incomings().len() != 3
        || plan.exits().len() != 2
    {
        return None;
    }
    let node = parsed.ast().statements().get(outer).ok()?;
    let Statement::For { source, body, .. } = node.payload() else {
        return None;
    };
    if *source != descriptor.source() {
        return None;
    }
    let span = node.span();
    let mut seeds = Vec::new();
    for (symbol, drop) in [(root, current_drop), (items, items_drop)] {
        let header = unique(plan.closure_phis().iter().filter(|phi| {
            phi.symbol() == symbol && phi.boundary() == IterationPhiBoundary::Header
        }))?;
        let exit =
            unique(plan.closure_phis().iter().filter(|phi| {
                phi.symbol() == symbol && phi.boundary() == IterationPhiBoundary::Exit
            }))?;
        if drop.target() != DropTarget::Named(symbol)
            || drop.owner() != Some(header.owner())
            || drop.condition() != Some(header.availability_condition())
            || drop.point() != fact.point()
            || drop.capture_slot().is_some()
            || drop.instance_address().is_some()
            || drop.value_origin() != names.symbols().get(symbol.index())?.span()
            || unique(
                owned
                    .drops()
                    .iter()
                    .filter(|candidate| candidate.target() == DropTarget::Named(symbol)),
            ) != Some(drop)
            || !super::constant_presence::prove_graph(
                owned,
                header.owner(),
                symbol,
                drop.condition(),
            )
        {
            return None;
        }
        for phi in [header, exit] {
            if !phi.root_nodes().is_empty()
                || !phi.capture_layout().is_empty()
                || !phi.origins().is_empty()
                || !matches!(conditions.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi { statement, boundary, symbol: identity, origin })
                    if *statement == outer && *boundary == phi.boundary() && *identity == symbol && *origin == span)
            {
                return None;
            }
            let slot = plan
                .closure_phis()
                .iter()
                .filter(|other| other.boundary() == phi.boundary())
                .position(|other| other.owner() == phi.owner())?;
            let selector = conditions.selector(phi.availability_selector())?;
            if selector.source()
                != (CleanupSelectorSource::IterationPhi {
                    statement: outer,
                    boundary: phi.boundary(),
                    slot,
                })
                || selector.selection() != CleanupSelection::IterationPresence
                || selector.branch_count() != 2
                || selector.origin() != span
                || !matches!(conditions.get(phi.availability_condition()), Some(CleanupCondition::Choice { selector, branches })
                    if *selector == phi.availability_selector() && branches.len() == 2 && never(conditions, branches[0]) && always(conditions, branches[1]))
            {
                return None;
            }
        }
        for kind in [
            IterationPhiIncomingKind::Entry,
            IterationPhiIncomingKind::Fallthrough,
            IterationPhiIncomingKind::Exhaustion,
        ] {
            let edge = unique(
                plan.closure_phi_incomings()
                    .iter()
                    .filter(|edge| edge.kind() == kind),
            )?;
            let target = if kind == IterationPhiIncomingKind::Exhaustion {
                exit
            } else {
                header
            };
            let point = match kind {
                IterationPhiIncomingKind::Entry => DropPoint::AfterExpression(*source),
                IterationPhiIncomingKind::Fallthrough => DropPoint::AfterStatement(*body),
                _ => fact.point(),
            };
            let binding = unique(
                edge.bindings()
                    .iter()
                    .filter(|binding| binding.target() == target.owner()),
            )?;
            if edge.bindings().len() != 2
                || edge.boundary() != target.boundary()
                || edge.point() != point
                || !always(conditions, edge.condition())
                || binding.availability_selector() != target.availability_selector()
                || binding.presence_source() != IterationPhiPresenceSource::StaticConditions
                || !binding.capture_slots_to_clear().is_empty()
                || !binding.root_sources().is_empty()
                || !binding.selector_writes().is_empty()
                || !binding.origins().is_empty()
            {
                return None;
            }
            if kind == IterationPhiIncomingKind::Exhaustion {
                if !binding.values().is_empty() || !never(conditions, binding.available_when()) {
                    return None;
                }
            } else {
                let [value] = binding.values() else {
                    return None;
                };
                if value.condition() != binding.available_when() {
                    return None;
                }
                if kind == IterationPhiIncomingKind::Entry {
                    if !always(conditions, binding.available_when()) {
                        return None;
                    }
                    seeds.push(value.source());
                } else if value.source() != header.owner()
                    || binding.available_when() != header.availability_condition()
                {
                    return None;
                }
            }
        }
    }
    for (index, phi) in plan.closure_phis().iter().enumerate() {
        if plan.closure_phis()[..index].iter().any(|other| {
            other.owner() == phi.owner()
                || other.availability_selector() == phi.availability_selector()
        }) {
            return None;
        }
    }
    for flow in [plan.closure_flow().header(), plan.closure_flow().exit()] {
        if flow.len() != 2
            || [root, items].iter().any(|symbol| {
                unique(flow.iter().filter(|binding| binding.symbol() == *symbol))
                    .is_none_or(|binding| !binding.origins().is_empty())
            })
        {
            return None;
        }
    }
    let fallthrough = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.kind() == IterationExitKind::Fallthrough),
    )?;
    if fallthrough.point() != DropPoint::AfterStatement(*body)
        || !always(conditions, fallthrough.condition())
        || !always(conditions, exhausted.condition())
        || fallthrough.actions()
            != [
                IterationCleanupAction::EndBinding {
                    statement: outer,
                    symbol: item,
                },
                IterationCleanupAction::EndElement(outer),
            ]
    {
        return None;
    }
    let [seed, items_seed] = seeds.as_slice() else {
        return None;
    };
    let CleanupOwnerValue::Parameter {
        symbol: parameter,
        origin,
    } = conditions.owner_value(*seed)?
    else {
        return None;
    };
    let CleanupOwnerValue::Expression {
        expression: initializer,
        origin: source_origin,
    } = conditions.owner_value(*items_seed)?
    else {
        return None;
    };
    if seed == items_seed
        || typed.symbol_type(*parameter) != Some(storage)
        || names.symbols().get(parameter.index())?.span() != *origin
        || parsed.ast().expressions().get(*initializer).ok()?.span() != *source_origin
        || unique(
            owned
                .bindings()
                .iter()
                .filter(|binding| binding.symbol() == *parameter),
        )
        .is_none_or(|binding| binding.kind() != OwnershipBindingKind::Owned)
    {
        return None;
    }
    let (inner, breaking, parameter_slot) = source::qualify(
        parsed,
        names,
        typed,
        outer,
        [root, items, item, *parameter],
        *initializer,
    )?;
    Some(Relation {
        outer,
        inner,
        breaking,
        root,
        items,
        item,
        storage,
        parameter_slot,
        initializer: *initializer,
        drops: [*items_drop, *current_drop],
    })
}
pub(in crate::ssa::lower_frontend) fn unique<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}
fn always(
    table: &CleanupConditions,
    id: lang_frontend::ownership_checking::CleanupConditionId,
) -> bool {
    matches!(table.get(id), Some(CleanupCondition::Always))
}
fn never(
    table: &CleanupConditions,
    id: lang_frontend::ownership_checking::CleanupConditionId,
) -> bool {
    matches!(table.get(id), Some(CleanupCondition::Never))
}

impl ExpressionLowerer<'_> {
    fn outer_borrowed_relation(&self, outer: StatementId) -> Option<Relation> {
        let mut relations = self
            .owned
            .drops()
            .iter()
            .copied()
            .filter_map(|fact| candidate(self.parsed, self.names, self.typed, self.owned, fact))
            .filter(|relation| relation.outer == outer);
        let relation = relations.next()?;
        relations.all(|other| other == relation).then_some(relation)
    }

    pub(super) fn qualify_outer_borrowed_entry(
        &self,
        inner: StatementId,
        descriptor: NullComparisonDescriptor,
        operand: ExpressionId,
        span: Span,
    ) -> Result<Option<Relation>, LoweringError> {
        if !self.loops.iter().any(|context| context.for_loop.is_some()) {
            return Ok(None);
        }
        let outer = self
            .loops
            .first()
            .and_then(|context| context.for_loop.as_ref())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?
            .statement;
        let relation = self
            .outer_borrowed_relation(outer)
            .filter(|relation| relation.inner == inner)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if self.loops.len() != 1
            || self.typed.non_null_use(operand).is_some()
            || self.typed.expression_type(operand) != Some(relation.storage)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_outer_borrowed_entry(&relation, descriptor, span)?;
        Ok(Some(relation))
    }

    pub(super) fn validate_outer_borrowed_entry(
        &self,
        relation: &Relation,
        descriptor: NullComparisonDescriptor,
        span: Span,
    ) -> Result<(), LoweringError> {
        if descriptor.symbol() != relation.root
            || descriptor.nullable_type() != relation.storage
            || !descriptor.non_null_when_true()
            || !self.non_null_bindings.is_empty()
            || self.resolve_type(relation.storage, span)? != relation.storage
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_active_outer_borrowed_source(relation, span)?;
        self.validate_outer_borrowed_state(
            relation,
            &self.branch_exit(LoweredValue::Unit),
            false,
            span,
        )
    }

    fn validate_active_outer_borrowed_source(
        &self,
        relation: &Relation,
        span: Span,
    ) -> Result<(), LoweringError> {
        let data = provider::context(self, relation)
            .and_then(|context| context.for_loop.as_ref())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if data.active_source_loan != Some(data.body_source)
            || self.borrow_bindings.get(&relation.item) != Some(&data.guarded_element.loan())
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    fn validate_outer_borrowed_state(
        &self,
        relation: &Relation,
        state: &BranchExit,
        allow_view: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let provider = provider::shape(self, relation)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let guard = provider::guard(self, relation, &provider);
        let source = *state
            .for_sources
            .get(&relation.outer.index())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let element = *state
            .for_elements
            .get(&relation.outer.index())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let owner = |symbol| match state.bindings.get(&symbol) {
            Some(LoweredValue::Value(value)) => Some(*value),
            _ => None,
        };
        let current =
            owner(relation.root).ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let items =
            owner(relation.items).ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let mut columns = Columns {
            block: state.block,
            entities: vec![
                EntityId::Value(current),
                EntityId::Value(items),
                EntityId::Loan(source),
                EntityId::Loan(element),
            ],
        };
        if state.for_sources.len() != 1 || state.for_elements.len() != 1
            || state.borrow_bindings.get(&relation.item) != Some(&element)
            || !state.temporaries.is_empty() || !state.loans.is_empty() || !state.capture_loans.is_empty()
            || !state.result_source_loans.is_empty() || self.readonly_receiver.is_some() || self.thunk_expression.is_some()
            || state.borrow_bindings.iter().any(|(&symbol, &loan)| symbol != relation.item
                && (!self.is_entry_loan(loan) || !matches!(self.function.entity(EntityId::Loan(loan)).map(|entity| entity.ty),
                    Some(EntityType::Loan { kind: LoanKind::Shared, target }) if self.ssa_types.get(target.index()) == Some(&SsaTypeKind::Boolean))))
            || !transport(self.function, columns.clone(), &provider.body, None, guard.as_ref())
        { return Err(error(LoweringErrorKind::UnsupportedNode, span)); }
        if allow_view {
            let guard = guard.ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let proof = state
                .views
                .get(&relation.root)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            self.validate_non_null_proof(relation.root, *proof, &state.bindings, span)?;
            columns.entities.push(EntityId::Loan(proof.loan));
            if state.views.len() != 1
                || proof.while_origin() != Some((relation.inner, guard.view))
                || !transport(self.function, columns, &guard.non_null, None, None)
                || self
                    .function
                    .block(state.block)
                    .is_none_or(|block| block.terminator.is_some())
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        } else if !state.views.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    /// Validate the snapshot before touching ambient sibling state. The exact
    /// proven source is live; rebind_source cannot revive an already ended loan.
    pub(super) fn restore_outer_borrowed_state(
        &mut self,
        relation: &Relation,
        state: &BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_outer_borrowed_state(relation, state, false, span)?;
        let source = state.for_sources[&relation.outer.index()];
        let element = state.for_elements[&relation.outer.index()];
        let data = self
            .loops
            .first_mut()
            .and_then(|context| context.for_loop.as_mut())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if data.statement != relation.outer
            || data.new_source_loan.is_none()
            || data.is_carried_source
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        data.rebind_source(source);
        data.guarded_element.set_loan(element);
        Ok(())
    }

    pub(super) fn validate_outer_borrowed_exit_states(
        &self,
        relation: &Relation,
        states: &[BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        for state in states {
            self.validate_outer_borrowed_state(relation, state, false, span)?;
        }
        Ok(())
    }

    pub(super) fn retire_outer_borrowed_break(
        &mut self,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let Some(relation) = self
            .loops
            .last()
            .and_then(|context| context.outer_borrowed_while.clone())
        else {
            return Ok(false);
        };
        if self.loops.len() != 2
            || self
                .parsed
                .ast()
                .expressions()
                .get(relation.breaking)
                .map(|node| node.span())
                .ok()
                != Some(span)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_active_outer_borrowed_source(&relation, span)?;
        self.validate_outer_borrowed_state(
            &relation,
            &self.branch_exit(LoweredValue::Unit),
            true,
            span,
        )?;
        let proof = self.non_null_bindings[&relation.root];
        self.append(Operation::BorrowEnd { loan: proof.loan }, Vec::new(), span)?;
        self.non_null_bindings.remove(&relation.root);
        Ok(true)
    }

    pub(super) fn preflight_outer_borrowed_iteration(
        &self,
        exit: &IterationExitPlan,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(data) = self
            .loops
            .last()
            .and_then(|context| context.for_loop.as_ref())
        else {
            return Ok(());
        };
        let Some(relation) = self.outer_borrowed_relation(data.statement) else {
            return Ok(());
        };
        if self.loops.len() != 1 || !data.plan.exits().contains(exit) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if exit.kind() == IterationExitKind::Fallthrough {
            self.validate_active_outer_borrowed_source(&relation, span)?;
            return self.validate_outer_borrowed_state(
                &relation,
                &self.branch_exit(LoweredValue::Unit),
                false,
                span,
            );
        }
        if exit.kind() != IterationExitKind::Exhaustion {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let provider = self.checked_outer_borrowed_exhaustion(&relation, span)?;
        if data.active_source_loan != provider::loan(provider.exhausted.entities[2])
            || !instructions(self.function, self.block).is_some_and(|ops| ops.is_empty())
            || self.bindings.get(&relation.items)
                != provider::value(provider.exhausted.entities[1])
                    .as_ref()
                    .map(|owner| LoweredValue::Value(*owner))
                    .as_ref()
            || self.bindings.get(&relation.root)
                != provider::value(provider.exhausted.entities[0])
                    .as_ref()
                    .map(|owner| LoweredValue::Value(*owner))
                    .as_ref()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    fn checked_outer_borrowed_exhaustion(
        &self,
        relation: &Relation,
        span: Span,
    ) -> Result<provider::Provider, LoweringError> {
        let provider = provider::shape(self, relation)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if self.loops.len() != 1 || self.block != provider.exhausted.block
            || !self.non_null_bindings.is_empty() || !self.temporaries.is_empty() || !self.pending_call_loans.is_empty()
            || !self.capture_loans.is_empty() || !self.result_source_loans.is_empty() || self.readonly_receiver.is_some() || self.thunk_expression.is_some()
            || self.borrow_bindings.contains_key(&relation.item)
            || self.borrow_bindings.values().any(|&loan| !self.is_entry_loan(loan) || !matches!(self.function.entity(EntityId::Loan(loan)).map(|entity| entity.ty),
                Some(EntityType::Loan { kind: LoanKind::Shared, target }) if self.ssa_types.get(target.index()) == Some(&SsaTypeKind::Boolean)))
            || self.function.block(self.block).is_none_or(|block| block.terminator.is_some())
            || provider::completed_backedge(self, relation, &provider).is_none()
        { return Err(error(LoweringErrorKind::UnsupportedNode, span)); }
        Ok(provider)
    }

    pub(super) fn emit_outer_borrowed_drop(
        &mut self,
        fact: DropFact,
    ) -> Result<bool, LoweringError> {
        let Some(relation) = candidate(self.parsed, self.names, self.typed, self.owned, fact)
        else {
            return Ok(false);
        };
        let span = fact.value_origin();
        let provider = self.checked_outer_borrowed_exhaustion(&relation, span)?;
        let source = provider::loan(provider.exhausted.entities[2])
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let items = provider::value(provider.exhausted.entities[1])
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let current = provider::value(provider.exhausted.entities[0])
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let outer_span = self
            .parsed
            .ast()
            .statements()
            .get(relation.outer)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
            .span();
        let ops = instructions(self.function, self.block)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let expected = if fact == relation.drops[0] {
            vec![Operation::BorrowEnd { loan: source }]
        } else {
            vec![
                Operation::BorrowEnd { loan: source },
                Operation::Drop { owner: items },
            ]
        };
        if self.loops[0]
            .for_loop
            .as_ref()
            .is_none_or(|data| data.active_source_loan.is_some())
            || ops.iter().map(|op| &op.operation).ne(expected.iter())
            || ops
                .first()
                .is_none_or(|op| op.origin != Origin::Source(outer_span))
            || (fact == relation.drops[1]
                && (self.bindings.contains_key(&relation.items)
                    || ops[1].origin != Origin::Source(relation.drops[0].value_origin())))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let (symbol, owner) = if fact == relation.drops[0] {
            (relation.items, items)
        } else {
            (relation.root, current)
        };
        if self.bindings.get(&symbol) != Some(&LoweredValue::Value(owner)) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.bindings.remove(&symbol);
        self.append(Operation::Drop { owner }, Vec::new(), span)?;
        self.release_owner_capture_loans(owner, span)?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "outer_borrowed_while/runtime/guard_tests.rs"]
mod guard_tests;
