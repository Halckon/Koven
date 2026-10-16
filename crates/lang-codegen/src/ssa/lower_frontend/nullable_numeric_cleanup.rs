//! A single readonly numeric-loop owner, proved by induction over the published
//! presence slots and separately by the actual SSA owner/view transport.
mod carrier;
mod source;

use carrier::{Pair, numeric_exit, transport};

use super::*;
use crate::ssa::model::SsaTypeKind;
use lang_frontend::{
    ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupOwnerValue,
        CleanupSelection, CleanupSelectorSource, DropFact, DropTarget, IterationCleanupAction,
        IterationClosurePhiBinding, IterationExitKind, IterationPhiBoundary,
        IterationPhiIncomingBinding, IterationPhiIncomingKind, IterationPhiPresenceSource,
        IterationSource,
    },
    type_checking::SequentialIterationBinding,
};

pub(super) struct Plan {
    fact: DropFact,
    root: SymbolId,
    storage: TypeId,
    statement: StatementId,
    outer: StatementId,
}

/// This admits no SSA and does not change the generic condition implication gate.
pub(super) fn candidate(
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
            .filter(|other| other.point() == fact.point() && other.target() == fact.target())
            .count()
            != 1
        || !owned.cleanup_steps().iter().any(|(point, action)| {
            *point == fact.point()
                && matches!(action, IterationCleanupAction::Drop(drop) if *drop == fact)
        })
        || descriptor.statement() != statement
        || !matches!(
            descriptor.provider(),
            lang_frontend::type_checking::IterationProvider::NumericRange(
                BuiltinType::Int | BuiltinType::Long
            )
        )
        || plan.source_kind() != &IterationSource::OwnedSnapshot(descriptor.source())
        || !plan.capture_graph().nodes().is_empty()
    {
        return None;
    }
    let [header, exit] = plan.closure_phis() else {
        return None;
    };
    if header.boundary() != IterationPhiBoundary::Header
        || exit.boundary() != IterationPhiBoundary::Exit
        || header.owner() == exit.owner()
        || header.availability_selector() == exit.availability_selector()
        || exit.owner() != fact.owner()?
        || fact.condition()? != exit.availability_condition()
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
    for phi in [header, exit] {
        if phi.symbol() != root
            || !phi.capture_layout().is_empty()
            || !phi.root_nodes().is_empty()
            || !phi.origins().is_empty()
            || !matches!(conditions.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi {
                statement: id, boundary, symbol, origin }) if *id == statement
                && *boundary == phi.boundary() && *symbol == root && *origin == span)
        {
            return None;
        }
        let selector = conditions.selector(phi.availability_selector())?;
        // There is exactly one binding at each boundary, hence its real slot is zero.
        if selector.source()
            != (CleanupSelectorSource::IterationPhi {
                statement,
                boundary: phi.boundary(),
                slot: 0,
            })
            || selector.selection() != CleanupSelection::IterationPresence
            || selector.branch_count() != 2
            || selector.origin() != span
            || !matches!(conditions.get(phi.availability_condition()), Some(CleanupCondition::Choice {
                selector, branches }) if *selector == phi.availability_selector() && branches.len() == 2
                && matches!(conditions.get(branches[0]), Some(CleanupCondition::Never))
                && always(conditions, branches[1]))
        {
            return None;
        }
    }
    for flow in [plan.closure_flow().header(), plan.closure_flow().exit()] {
        let [binding] = flow else {
            return None;
        };
        if binding.symbol() != root || !binding.origins().is_empty() {
            return None;
        }
    }
    let mut seed = None;
    let mut exhaustion = 0;
    let mut actual = Vec::new();
    for incoming in plan.closure_phi_incomings() {
        let [binding] = incoming.bindings() else {
            return None;
        };
        let [value] = binding.values() else {
            return None;
        };
        let target = match incoming.kind() {
            IterationPhiIncomingKind::Entry => {
                if seed.replace(value.source()).is_some()
                    || incoming.point() != DropPoint::AfterExpression(descriptor.source())
                    || !always(conditions, incoming.condition())
                    || !always(conditions, binding.available_when())
                {
                    return None;
                }
                header
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
                header
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
                header
            }
            IterationPhiIncomingKind::Exhaustion => {
                exhaustion += 1;
                actual.push((
                    IterationExitKind::Exhaustion,
                    incoming.point(),
                    incoming.condition(),
                ));
                if incoming.point() != DropPoint::LoopExit(statement)
                    || !always(conditions, incoming.condition())
                    || value.source() != header.owner()
                    || binding.available_when() != header.availability_condition()
                {
                    return None;
                }
                exit
            }
            IterationPhiIncomingKind::Break(_) => return None,
        };
        if incoming.boundary() != target.boundary()
            || !plain_binding(binding, target)
            || value.condition() != binding.available_when()
        {
            return None;
        }
    }
    let seed = seed?;
    let CleanupOwnerValue::Parameter {
        symbol: parameter,
        origin,
    } = conditions.owner_value(seed)?
    else {
        return None;
    };
    if typed.symbol_type(*parameter) != Some(storage)
        || names.symbols().get(parameter.index())?.span() != *origin
        || exhaustion != 1
        || actual.len() != plan.exits().len()
        || actual.iter().any(|item| {
            plan.exits()
                .iter()
                .filter(|exit| (exit.kind(), exit.point(), exit.condition()) == *item)
                .count()
                != 1
        })
        || actual.iter().enumerate().any(|(index, item)| {
            actual[..index]
                .iter()
                .any(|other| other.0 == item.0 || other.1 == item.1)
        })
    {
        return None;
    }
    for edge in plan.exits() {
        if !actual.contains(&(edge.kind(), edge.point(), edge.condition()))
            || edge.actions().iter().any(|action| match action {
                IterationCleanupAction::EndBinding {
                    statement: id,
                    symbol,
                } => {
                    *id != statement
                        || descriptor.binding() != &SequentialIterationBinding::Name(*symbol)
                }
                IterationCleanupAction::EndElement(id)
                | IterationCleanupAction::FinishProvider(id) => *id != statement,
                // In particular, no root Drop may be hidden in an iteration action.
                _ => true,
            })
        {
            return None;
        }
    }
    let outer = source::qualify(
        parsed, names, typed, statement, member, root, storage, *parameter,
    )?;
    let root_drops = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == DropTarget::Named(root))
        .collect::<Vec<_>>();
    if root_drops.len() != 2
        || root_drops.iter().any(|drop| {
            **drop != fact
                && !(drop.point() == DropPoint::LoopExit(outer)
                    && drop.owner() == Some(seed)
                    && drop.condition().is_none()
                    && drop.capture_slot().is_none()
                    && drop.instance_address().is_none())
        })
    {
        return None;
    }
    Some(Plan {
        fact,
        root,
        storage,
        statement,
        outer,
    })
}

fn always(conditions: &CleanupConditions, condition: CleanupConditionId) -> bool {
    matches!(conditions.get(condition), Some(CleanupCondition::Always))
}

fn header_conjunction(
    conditions: &CleanupConditions,
    header: &IterationClosurePhiBinding,
    condition: CleanupConditionId,
    edge: CleanupConditionId,
) -> bool {
    matches!(conditions.get(condition), Some(CleanupCondition::Choice { selector, branches })
        if *selector == header.availability_selector() && branches.len() == 2
        && matches!(conditions.get(branches[0]), Some(CleanupCondition::Never)) && branches[1] == edge)
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

impl ExpressionLowerer<'_> {
    /// Presence admits only this checked readonly path. No proof is installed here.
    pub(super) fn qualify_numeric_while_entry(
        &self,
        statement: StatementId,
        span: Span,
    ) -> Result<Option<(SymbolId, NonNullProof)>, LoweringError> {
        let Some((&root, &proof)) = self
            .non_null_bindings
            .iter()
            .find(|(_, proof)| proof.is_while_origin())
        else {
            return Ok(None);
        };
        let mut candidates = self
            .owned
            .drops()
            .iter()
            .copied()
            .filter_map(|fact| candidate(self.parsed, self.names, self.typed, self.owned, fact))
            .filter(|plan| plan.statement == statement && plan.root == root);
        let plan = candidates
            .next()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if candidates.next().is_some()
            || proof.while_origin().map(|(scope, _)| scope) != Some(plan.outer)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_numeric_while_pair(Some((root, proof)), span)?;
        self.numeric_while_birth(root, proof)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        Ok(Some((root, proof)))
    }

    /// Revalidate the actual state at every edge; the inherited value supplies only
    /// immutable provenance, never an owner or loan to put back into the state.
    pub(super) fn validate_numeric_while_pair(
        &self,
        inherited: Option<(SymbolId, NonNullProof)>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some((root, inherited)) = inherited else {
            return Ok(());
        };
        self.validate_readonly_while_exit(
            root,
            inherited,
            &self.branch_exit(LoweredValue::Unit),
            span,
        )?;
        self.ensure_no_unresolved_while_proof_carriers(span)?;
        let proof = self.non_null_bindings[&root];
        let storage = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let storage = self.resolve_type(storage, span)?;
        let Some(TypeKind::Nullable(inner)) = self.typed.types().get(storage) else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if !self.temporaries.is_empty()
            || self.borrow_bindings.contains_key(&root)
            || self.typed.copyability(storage) != Some(Copyability::MoveOnly)
            || self.typed.copyability(*inner) != Some(Copyability::MoveOnly)
            || self.type_ids.get(inner) != Some(&proof.inner)
            || !matches!(
                self.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
            || self
                .type_ids
                .get(&storage)
                .and_then(|id| self.ssa_types.get(id.index()))
                != Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
            || self.numeric_while_birth(root, proof).is_none()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    /// Read-only before end_result_bindings can retire a child or alter a pair.
    pub(super) fn preflight_nullable_numeric_cleanup(
        &self,
        facts: &[DropFact],
    ) -> Result<(), LoweringError> {
        for &fact in facts {
            if let Some(plan) = candidate(self.parsed, self.names, self.typed, self.owned, fact) {
                self.checked_nullable_numeric_owner(&plan)?;
            }
        }
        Ok(())
    }

    pub(super) fn emit_nullable_numeric_cleanup(
        &mut self,
        fact: DropFact,
    ) -> Result<bool, LoweringError> {
        let Some(plan) = candidate(self.parsed, self.names, self.typed, self.owned, fact) else {
            return Ok(false);
        };
        let owner = self.checked_nullable_numeric_owner(&plan)?;
        self.end_non_null_proof(plan.root, fact.value_origin())?;
        if self.bindings.remove(&plan.root) != Some(LoweredValue::Value(owner)) {
            return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
        }
        self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
        self.release_owner_capture_loans(owner, fact.value_origin())?;
        Ok(true)
    }

    fn checked_nullable_numeric_owner(&self, plan: &Plan) -> Result<ValueId, LoweringError> {
        let span = plan.fact.value_origin();
        let proof = self
            .non_null_bindings
            .get(&plan.root)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        self.validate_numeric_while_pair(Some((plan.root, proof)), span)?;
        let anchor = self
            .numeric_while_birth(plan.root, proof)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let current = Pair {
            block: self.block,
            owner: proof.owner,
            loan: proof.loan,
        };
        if self.loops.len() != 1
            || self.thunk_expression.is_some()
            || proof.while_origin().map(|(scope, _)| scope) != Some(plan.outer)
            || self.typed.symbol_type(plan.root) != Some(plan.storage)
            || self
                .function
                .block(self.block)
                .is_none_or(|block| block.terminator.is_some())
            || !numeric_exit(self, plan, current, anchor)
            || !transport(self.function, current, anchor, proof.inner, self.ssa_types)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(proof.owner)
    }
}
