//! One readonly nullable owner across one independently owned List provider.
//! Published presence and actual owner/view/provider columns are separate proofs.
pub(in crate::ssa) use crate::ssa::borrow_memory_test_support::borrowed_cleanup_carrier as carrier;
use carrier::source;
use super::loop_control::numeric_iteration::provider;

use super::*;
use lang_frontend::{
    ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupOwnerValue,
        CleanupSelection, CleanupSelectorSource, DropFact, DropTarget, IterationCleanupAction,
        IterationClosurePhiBinding, IterationExitKind, IterationPhiBoundary,
        IterationPhiIncomingBinding, IterationPhiIncomingKind, IterationPhiPresenceSource,
        IterationSource, OwnershipBindingKind,
    },
    type_checking::{IterationProvider, SequentialContainerKind, SequentialIterationBinding},
};

pub(in crate::ssa) struct Plan {
    fact: DropFact,
    root: SymbolId,
    storage: TypeId,
    pub(in crate::ssa) statement: StatementId,
    outer: StatementId,
    pub(in crate::ssa) source_root: SymbolId,
    pub(in crate::ssa) element: SymbolId,
    pub(in crate::ssa) source_drop: DropFact,
}

/// Admission never installs a proof, rewrites a condition or admits an SSA value.
pub(in crate::ssa) fn candidate(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> Option<Plan> {
    let DropTarget::Named(root) = fact.target() else {
        return None;
    };
    let DropPoint::AfterExpression(member) = fact.point() else {
        return None;
    };
    let conditions = owned.cleanup_conditions();
    let CleanupOwnerValue::IterationPhi {
        statement,
        boundary: IterationPhiBoundary::Exit,
        symbol,
        ..
    } = conditions.owner_value(fact.owner()?)?
    else {
        return None;
    };
    let statement = *statement;
    let plan = owned.iteration(statement)?;
    let descriptor = plan.descriptor();
    let storage = typed.symbol_type(root)?;
    let TypeKind::Nullable(inner) = typed.types().get(storage)? else {
        return None;
    };
    let TypeKind::Nominal { nominal, arguments } = typed.types().get(*inner)? else {
        return None;
    };
    let IterationSource::BorrowedSource(LoanTarget::Place(place)) = plan.source_kind() else {
        return None;
    };
    let source_root = place.root();
    let SequentialIterationBinding::Name(element) = descriptor.binding() else {
        return None;
    };
    let element = *element;
    if *symbol != root
        || !arguments.is_empty()
        || typed
            .nominals()
            .iter()
            .find(|item| item.id() == *nominal)?
            .kind()
            != NominalKind::Class
        || typed.copyability(storage) != Some(Copyability::MoveOnly)
        || typed.copyability(*inner) != Some(Copyability::MoveOnly)
        || fact.capture_slot().is_some()
        || fact.instance_address().is_some()
        || fact.value_origin() != names.symbols().get(root.index())?.span()
        || owned
            .drops()
            .iter()
            .filter(|drop| drop.point() == fact.point() && drop.target() == fact.target())
            .count()
            != 1
        || !owned.cleanup_steps().iter().any(|(point, action)| {
            *point == fact.point() && *action == IterationCleanupAction::Drop(fact)
        })
        || descriptor.statement() != statement
        || descriptor.provider() != IterationProvider::Sequential(SequentialContainerKind::List)
        || descriptor.delivery() != ParameterMode::Borrow
        || builtin_type(typed, descriptor.element_type()) != Some(BuiltinType::Int)
        || typed.symbol_type(element) != Some(descriptor.element_type())
        || !place.is_root()
        || source_root == root
        || element == root
        || element == source_root
        || typed.symbol_type(source_root) != Some(descriptor.source_type())
        || descriptor.source_type() == storage
        || typed.copyability(descriptor.source_type()) != Some(Copyability::MoveOnly)
        || !plan.capture_graph().nodes().is_empty()
        || plan.bindings().len() != 1
        || plan.bindings()[0].symbol() != element
        || plan.bindings()[0].kind() != OwnershipBindingKind::Shared
        || owned
            .borrow_results()
            .bindings()
            .iter()
            .any(|binding| [root, source_root].contains(&binding.binding()))
        || plan.closure_phis().len() != 4
        || plan.closure_phi_incomings().len() != 4
        || plan.exits().len() != 3
    {
        return None;
    }
    let node = parsed.ast().statements().get(statement).ok()?;
    let Statement::For { source, body, .. } = node.payload() else {
        return None;
    };
    if *source != descriptor.source() {
        return None;
    }
    let span = node.span();
    let phi = |symbol, boundary| {
        unique(
            plan.closure_phis()
                .iter()
                .filter(|phi| phi.symbol() == symbol && phi.boundary() == boundary),
        )
    };
    let header = phi(root, IterationPhiBoundary::Header)?;
    let exit = phi(root, IterationPhiBoundary::Exit)?;
    let source_header = phi(source_root, IterationPhiBoundary::Header)?;
    let source_exit = phi(source_root, IterationPhiBoundary::Exit)?;
    for (index, phi) in plan.closure_phis().iter().enumerate() {
        if !phi.capture_layout().is_empty()
            || !phi.root_nodes().is_empty()
            || !phi.origins().is_empty()
            || plan.closure_phis()[..index].iter().any(|other| {
                other.owner() == phi.owner()
                    || other.availability_selector() == phi.availability_selector()
            })
            || !matches!(conditions.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi {
                statement: id, boundary, symbol, origin }) if *id == statement && *boundary == phi.boundary()
                && *symbol == phi.symbol() && *origin == span)
        {
            return None;
        }
        let slot = plan.closure_phis()[..index]
            .iter()
            .filter(|other| other.boundary() == phi.boundary())
            .count();
        let selector = conditions.selector(phi.availability_selector())?;
        if selector.source()
            != (CleanupSelectorSource::IterationPhi {
                statement,
                boundary: phi.boundary(),
                slot,
            })
            || selector.selection() != CleanupSelection::IterationPresence
            || selector.branch_count() != 2
            || selector.origin() != span
            || !matches!(conditions.get(phi.availability_condition()), Some(CleanupCondition::Choice { selector, branches })
                if *selector == phi.availability_selector() && branches.len() == 2
                && never(conditions, branches[0]) && always(conditions, branches[1]))
        {
            return None;
        }
    }
    if exit.owner() != fact.owner()? || fact.condition()? != exit.availability_condition() {
        return None;
    }
    for flow in [plan.closure_flow().header(), plan.closure_flow().exit()] {
        if flow.len() != 2
            || [root, source_root].iter().any(|symbol| {
                unique(flow.iter().filter(|binding| binding.symbol() == *symbol))
                    .is_none_or(|binding| !binding.origins().is_empty())
            })
        {
            return None;
        }
    }
    let mut seeds = None;
    let mut actual = Vec::new();
    for incoming in plan.closure_phi_incomings() {
        if incoming.bindings().len() != 2 {
            return None;
        }
        let targets = match incoming.kind() {
            IterationPhiIncomingKind::Entry => {
                if incoming.point() != DropPoint::AfterExpression(descriptor.source())
                    || !always(conditions, incoming.condition())
                    || seeds.is_some()
                {
                    return None;
                }
                [header, source_header]
            }
            IterationPhiIncomingKind::Continue(expression) => {
                if incoming.point() != DropPoint::ControlTransfer(expression)
                    || !matches!(
                        source::expr(parsed, expression)?,
                        Expression::Continue { .. }
                    )
                {
                    return None;
                }
                actual.push((
                    IterationExitKind::Continue(expression),
                    incoming.point(),
                    incoming.condition(),
                ));
                [header, source_header]
            }
            IterationPhiIncomingKind::Fallthrough => {
                if incoming.point() != DropPoint::AfterStatement(*body) {
                    return None;
                }
                actual.push((
                    IterationExitKind::Fallthrough,
                    incoming.point(),
                    incoming.condition(),
                ));
                [header, source_header]
            }
            IterationPhiIncomingKind::Exhaustion => {
                if incoming.point() != DropPoint::LoopExit(statement)
                    || !always(conditions, incoming.condition())
                {
                    return None;
                }
                actual.push((
                    IterationExitKind::Exhaustion,
                    incoming.point(),
                    incoming.condition(),
                ));
                [exit, source_exit]
            }
            IterationPhiIncomingKind::Break(_) => return None,
        };
        let mut entry = Vec::new();
        for (target, header) in targets.into_iter().zip([header, source_header]) {
            let binding = unique(
                incoming
                    .bindings()
                    .iter()
                    .filter(|binding| binding.target() == target.owner()),
            )?;
            if incoming.boundary() != target.boundary() || !plain_binding(binding, target) {
                return None;
            }
            if incoming.kind() == IterationPhiIncomingKind::Exhaustion
                && target.symbol() == source_root
            {
                if !binding.values().is_empty() || !never(conditions, binding.available_when()) {
                    return None;
                }
                continue;
            }
            let [value] = binding.values() else {
                return None;
            };
            if value.condition() != binding.available_when() {
                return None;
            }
            match incoming.kind() {
                IterationPhiIncomingKind::Entry => {
                    if !always(conditions, binding.available_when()) {
                        return None;
                    }
                    entry.push(value.source());
                }
                IterationPhiIncomingKind::Exhaustion => {
                    if value.source() != header.owner()
                        || binding.available_when() != header.availability_condition()
                    {
                        return None;
                    }
                }
                _ => {
                    if value.source() != header.owner()
                        || !header_conjunction(
                            conditions,
                            header,
                            binding.available_when(),
                            incoming.condition(),
                        )
                    {
                        return None;
                    }
                }
            }
        }
        if incoming.kind() == IterationPhiIncomingKind::Entry {
            let [current, source] = entry.as_slice() else {
                return None;
            };
            seeds = Some((*current, *source));
        }
    }
    let (seed, source_seed) = seeds?;
    let CleanupOwnerValue::Parameter {
        symbol: parameter,
        origin,
    } = conditions.owner_value(seed)?
    else {
        return None;
    };
    let CleanupOwnerValue::Expression {
        expression: source_seed_expression,
        origin: source_seed_origin,
    } = conditions.owner_value(source_seed)?
    else {
        return None;
    };
    if parsed
        .ast()
        .expressions()
        .get(*source_seed_expression)
        .ok()?
        .span()
        != *source_seed_origin
        || unique(
            owned
                .bindings()
                .iter()
                .filter(|binding| binding.symbol() == *parameter),
        )
        .is_none_or(|binding| binding.kind() != OwnershipBindingKind::Owned)
        || typed.symbol_type(*parameter) != Some(storage)
        || names.symbols().get(parameter.index())?.span() != *origin
        || seed == source_seed
        || actual.len() != 3
        || actual.iter().enumerate().any(|(index, item)| {
            actual[..index]
                .iter()
                .any(|other| other.0 == item.0 || other.1 == item.1)
        })
        || actual.iter().any(|item| {
            plan.exits()
                .iter()
                .filter(|exit| (exit.kind(), exit.point(), exit.condition()) == *item)
                .count()
                != 1
        })
    {
        return None;
    }
    let source_drop = *unique(owned.drops().iter().filter(|drop| {
        drop.target() == DropTarget::Named(source_root)
            && drop.owner() == Some(source_header.owner())
    }))?;
    if source_drop.point() != DropPoint::LoopExit(statement)
        || source_drop.condition() != Some(source_header.availability_condition())
        || source_drop.value_origin() != names.symbols().get(source_root.index())?.span()
        || !super::constant_presence::constant_drop(parsed, names, typed, owned, source_drop)
    {
        return None;
    }
    for edge in plan.exits() {
        let expected = match edge.kind() {
            IterationExitKind::Continue(_) | IterationExitKind::Fallthrough => vec![
                IterationCleanupAction::EndBinding {
                    statement,
                    symbol: element,
                },
                IterationCleanupAction::EndElement(statement),
            ],
            IterationExitKind::Exhaustion => vec![
                IterationCleanupAction::FinishProvider(statement),
                IterationCleanupAction::EndSource(statement),
                IterationCleanupAction::Drop(source_drop),
            ],
            _ => return None,
        };
        if edge.actions() != expected.as_slice() {
            return None;
        }
    }
    let outer = source::qualify(
        parsed,
        names,
        typed,
        statement,
        member,
        root,
        *parameter,
        source_root,
        element,
        *source_seed_expression,
    )?;
    for (symbol, seed, final_drop) in [(root, seed, fact), (source_root, source_seed, source_drop)]
    {
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == DropTarget::Named(symbol))
            .collect::<Vec<_>>();
        if drops.len() != 2
            || drops.iter().any(|drop| {
                **drop != final_drop
                    && !(drop.point() == DropPoint::LoopExit(outer)
                        && drop.owner() == Some(seed)
                        && drop.condition().is_none()
                        && drop.capture_slot().is_none()
                        && drop.instance_address().is_none())
            })
        {
            return None;
        }
    }
    Some(Plan {
        fact,
        root,
        storage,
        statement,
        outer,
        source_root,
        element,
        source_drop,
    })
}

pub(in crate::ssa) fn unique<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}
fn always(table: &CleanupConditions, id: CleanupConditionId) -> bool {
    matches!(table.get(id), Some(CleanupCondition::Always))
}
fn never(table: &CleanupConditions, id: CleanupConditionId) -> bool {
    matches!(table.get(id), Some(CleanupCondition::Never))
}
fn header_conjunction(
    table: &CleanupConditions,
    header: &IterationClosurePhiBinding,
    condition: CleanupConditionId,
    edge: CleanupConditionId,
) -> bool {
    matches!(table.get(condition), Some(CleanupCondition::Choice { selector, branches })
        if *selector == header.availability_selector() && branches.len() == 2
        && never(table, branches[0]) && branches[1] == edge)
}
fn plain_binding(
    binding: &IterationPhiIncomingBinding,
    target: &IterationClosurePhiBinding,
) -> bool {
    binding.target() == target.owner()
        && binding.availability_selector() == target.availability_selector()
        && binding.presence_source() == IterationPhiPresenceSource::StaticConditions
        && binding.capture_slots_to_clear().is_empty()
        && binding.root_sources().is_empty()
        && binding.selector_writes().is_empty()
        && binding.origins().is_empty()
}

pub(in crate::ssa) mod runtime {
    //! Local provider admission; final cleanup rechecks the fully emitted CFG.
    use super::carrier::{Birth, Columns, birth, parameter, transport};
    use super::*;
    use crate::ssa::model::SsaTypeKind;
    use lang_frontend::ownership_checking::IterationExitPlan;

    impl ExpressionLowerer<'_> {
        fn borrowed_while_plan(
            &self,
            statement: StatementId,
            span: Span,
        ) -> Result<Plan, LoweringError> {
            unique(
                self.owned
                    .drops()
                    .iter()
                    .copied()
                    .filter_map(|fact| {
                        candidate(self.parsed, self.names, self.typed, self.owned, fact)
                    })
                    .filter(|plan| plan.statement == statement),
            )
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))
        }

        /// The initial gate is strict: this provider has not created any loans yet.
        pub(in super::super) fn qualify_borrowed_while_entry(
            &self,
            statement: StatementId,
            span: Span,
        ) -> Result<(), LoweringError> {
            if !self
                .non_null_bindings
                .values()
                .any(|proof| proof.is_while_origin())
            {
                return Ok(());
            }
            let plan = self.borrowed_while_plan(statement, span)?;
            self.ensure_no_unresolved_while_proof_carriers(span)?;
            let (proof, birth) = self.borrowed_while_pair(&plan, span)?;
            if self.loops.len() != 1
                || self.block != birth.pair.block
                || birth.pair.entities != [EntityId::Value(proof.owner), EntityId::Loan(proof.loan)]
                || self.bindings.get(&plan.source_root)
                    != Some(&LoweredValue::Value(birth.source_owner))
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            Ok(())
        }

        fn borrowed_while_pair(
            &self,
            plan: &Plan,
            span: Span,
        ) -> Result<(NonNullProof, Birth), LoweringError> {
            let proof = self
                .non_null_bindings
                .get(&plan.root)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            self.validate_readonly_while_exit(
                plan.root,
                proof,
                &self.branch_exit(LoweredValue::Unit),
                span,
            )?;
            let birth = birth(self, plan, proof)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            if !self.temporaries.is_empty()
                || !self.pending_call_loans.is_empty()
                || !self.capture_loans.is_empty()
                || !self.result_source_loans.is_empty()
                || self.readonly_receiver.is_some()
                || self.thunk_expression.is_some()
                || self.typed.symbol_type(plan.root) != Some(plan.storage)
                || self.resolve_type(plan.storage, span)? != plan.storage
                || self
                    .function
                    .block(self.block)
                    .is_none_or(|block| block.terminator.is_some())
                || !transport(
                    self.function,
                    Columns {
                        block: self.block,
                        entities: vec![EntityId::Value(proof.owner), EntityId::Loan(proof.loan)],
                    },
                    &birth.pair,
                    None,
                )
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            Ok((proof, birth))
        }

        /// Do not relax the generic no-carrier gate. Only this exact active provider
        /// may keep its independently rooted source and its own element alive.
        pub(in super::super) fn preflight_borrowed_while_exit(
            &self,
            exit: &IterationExitPlan,
            span: Span,
        ) -> Result<(), LoweringError> {
            let Some(context) = self.loops.last() else {
                return Ok(());
            };
            let Some(data) = &context.for_loop else {
                return Ok(());
            };
            if !context
                .entry_views
                .values()
                .any(|proof| proof.is_while_origin())
            {
                return Ok(());
            }
            let plan = self.borrowed_while_plan(data.statement, span)?;
            let (proof, birth) = self.borrowed_while_pair(&plan, span)?;
            let provider = provider::shape(self, &plan, &birth)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let inherited = context
                .entry_views
                .get(&plan.root)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let header_source = provider::loan(provider.header.entities[3])
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let header_column = |entity| parameter(self.function, entity).map(|(_, index)| index);
            if self.loops.len() != 2
                || !context.breaks.is_empty()
                || !context.continues.is_empty()
                || context.entry_views.len() != 1
                || proof.origin != inherited.origin
                || proof.inner != inherited.inner
                || !data.plan.exits().contains(exit)
                || data.statement != plan.statement
                || context.header != provider.header.block
                || data.new_source_loan != Some(provider.source)
                || data.is_carried_source
                || data.provider_header.block != provider.header.block
                || data.provider_header.source != header_source
                || data.provider_header.length != provider.length
                || data.provider_header.cursor != provider.cursor
                || data.provider_header.step != provider.step
                || header_column(EntityId::Loan(header_source)) != Some(data.source_slot)
                || header_column(EntityId::Value(provider.length)) != Some(data.length_slot)
                || header_column(EntityId::Value(provider.cursor)) != Some(data.cursor_slot)
                || data.provider_header.source_slot != data.source_slot
                || data.provider_header.length_slot != data.length_slot
                || data.provider_header.cursor_slot != data.cursor_slot
                || data.body_cursor != provider.body_cursor
                || data.guarded_element.cursor() != provider.body_cursor
                || data.guarded_element.body() != provider.body.block
                || data.guarded_element.header() != provider.header.block
                || Some(data.guarded_element.source()) != provider::loan(provider.body.entities[3])
                || data.guarded_element.step() != provider.step
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let exhausted = exit.kind() == IterationExitKind::Exhaustion;
            let element = data.guarded_element.loan();
            for (&symbol, &loan) in &self.borrow_bindings {
                if !exhausted && symbol == plan.element && loan == element {
                    continue;
                }
                if !self.is_entry_loan(loan)
                    || !matches!(self.function.entity(EntityId::Loan(loan)).map(|entity| entity.ty),
                    Some(EntityType::Loan { kind: LoanKind::Shared, target })
                        if matches!(self.ssa_types.get(target.index()), Some(SsaTypeKind::Boolean)))
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            let Some(LoweredValue::Value(owner)) = self.bindings.get(&plan.source_root).copied()
            else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            if exhausted {
                let ended = provider::backedges(self, &plan, &birth, &provider)
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                if self.block != provider.exhausted.block
                    || provider.exhausted.entities[..3]
                        != [
                            EntityId::Value(proof.owner),
                            EntityId::Loan(proof.loan),
                            EntityId::Value(owner),
                        ]
                    || data.active_source_loan != provider::loan(provider.exhausted.entities[3])
                    || self.borrow_bindings.contains_key(&plan.element)
                    || !ended.contains(&(data.body_source, element))
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            } else {
                if !matches!(
                    exit.kind(),
                    IterationExitKind::Continue(_) | IterationExitKind::Fallthrough
                ) || data.active_source_loan != Some(data.body_source)
                    || self.borrow_bindings.get(&plan.element) != Some(&element)
                    || !transport(
                        self.function,
                        Columns {
                            block: self.block,
                            entities: vec![
                                EntityId::Value(proof.owner),
                                EntityId::Loan(proof.loan),
                                EntityId::Value(owner),
                                EntityId::Loan(data.body_source),
                                EntityId::Loan(element),
                            ],
                        },
                        &provider.body,
                        None,
                    )
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            Ok(())
        }

        pub(in super::super) fn preflight_nullable_borrowed_cleanup(
            &self,
            facts: &[DropFact],
        ) -> Result<(), LoweringError> {
            for &fact in facts {
                if let Some(plan) = candidate(self.parsed, self.names, self.typed, self.owned, fact)
                {
                    self.checked_nullable_borrowed_owner(&plan)?;
                }
            }
            Ok(())
        }

        pub(in super::super) fn emit_nullable_borrowed_cleanup(
            &mut self,
            fact: DropFact,
        ) -> Result<bool, LoweringError> {
            let Some(plan) = candidate(self.parsed, self.names, self.typed, self.owned, fact)
            else {
                return Ok(false);
            };
            let owner = self.checked_nullable_borrowed_owner(&plan)?;
            self.end_non_null_proof(plan.root, fact.value_origin())?;
            if self.bindings.remove(&plan.root) != Some(LoweredValue::Value(owner)) {
                return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
            }
            self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
            self.release_owner_capture_loans(owner, fact.value_origin())?;
            Ok(true)
        }

        pub(in crate::ssa) fn checked_nullable_borrowed_owner(&self, plan: &Plan) -> Result<ValueId, LoweringError> {
            let span = plan.fact.value_origin();
            self.ensure_no_unresolved_while_proof_carriers(span)?;
            let (proof, birth) = self.borrowed_while_pair(plan, span)?;
            let provider = provider::shape(self, plan, &birth)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            if self.loops.len() != 1
                || provider.exhausted.entities[..2]
                    != [EntityId::Value(proof.owner), EntityId::Loan(proof.loan)]
                || provider::complete(self, plan, &birth, &provider).is_none()
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            Ok(proof.owner)
        }
    }

}
