pub(in crate::ssa) mod carrier;

use carrier::{Pair, numeric_exit, source, transport};

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

pub(in crate::ssa) struct Plan {
    fact: DropFact,
    root: SymbolId,
    storage: TypeId,
    statement: StatementId,
    outer: StatementId,
}

/// This admits no SSA and does not change the generic condition implication gate.
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
    let outer = source::qualify(parsed, names, typed, statement, member, root, *parameter)?;
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
    pub(in crate::ssa) fn qualify_numeric_while_entry(
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
    pub(in crate::ssa) fn validate_numeric_while_pair(
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
    pub(in crate::ssa) fn preflight_nullable_numeric_cleanup(
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

    pub(in crate::ssa) fn emit_nullable_numeric_cleanup(
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

pub(in crate::ssa) mod formation {
    //! Resolve actual producer epochs through ordinary formation, replacement and local Move names.
    use crate::ssa::map_source::*;
    use lang_frontend::{
        ast::ExpressionId,
        parser::{AssignmentOperator, Expression, FunctionBody, FunctionForm, Item, ParsedFile, Statement},
        source::Span,
    };

    #[derive(Clone, Copy, Debug)]
    pub(in crate::ssa) struct BindingEpoch {
        /// The original outer RHS ID, including any validated transparent Groups.
        pub(in crate::ssa) rhs: ExpressionId,
        /// Declaration marker for the first event, full assignment span thereafter.
        pub(in crate::ssa) origin: Span,
    }

    /// A real ownership birth, before aliases and mutation produce the current owner.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::ssa) enum Birth<R> {
        Call(ExpressionId),
        /// The original expression's owned result, independently witnessed by its consumer.
        Expression(ExpressionId),
        EntryParameter {
            root: R,
            reference: ExpressionId,
            origin: Span,
        },
    }

    #[derive(Clone, Copy, Debug)]
    pub(in crate::ssa) enum BindingDeclaration {
        Local {
            marker: Span,
            initializer: ExpressionId,
        },
        EntryParameter {
            marker: Span,
        },
    }

    /// Both IDs come from one actual typed Map mutation descriptor.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::ssa) struct OwnerUpdate {
        pub(in crate::ssa) expression: ExpressionId,
        pub(in crate::ssa) receiver: ExpressionId,
    }

    #[derive(Clone, Debug)]
    pub(in crate::ssa) struct Formation<R> {
        pub(in crate::ssa) birth: Birth<R>,
        pub(in crate::ssa) updates: Vec<OwnerUpdate>,
        /// The current root first, followed by roots consumed by its initializer chain.
        pub(in crate::ssa) roots: Vec<R>,
        /// Original first-root declaration marker; single-file Drop provenance uses it.
        pub(in crate::ssa) declaration: Span,
        /// Complete linear binding events of the first root, in actual source order.
        pub(in crate::ssa) history: Vec<BindingEpoch>,
        /// Each original RHS is resolved by the consumer at its own Move point.
        pub(in crate::ssa) births: Vec<(ExpressionId, R, Birth<R>)>,
        /// Completion boundary of the current root's actual initializer/replacement RHS.
        pub(in crate::ssa) binding: Span,
        /// Current root declaration marker, or the complete assignment of its new epoch.
        pub(in crate::ssa) origin: Span,
        /// First real replacement of this current root after the provider statement.
        pub(in crate::ssa) replacement: Option<ExpressionId>,
    }

    impl<R: Eq> Formation<R> {
        /// Later mutations may change the SSA owner, but not its binding epoch.
        pub(in crate::ssa) fn same_epoch(&self, other: &Self) -> bool {
            self.birth == other.birth
                && self.roots == other.roots
                && self.declaration == other.declaration
                && self.binding == other.binding
                && self.origin == other.origin
                && self
                    .history
                    .iter()
                    .map(|e| (e.rhs, e.origin))
                    .eq(other.history.iter().map(|e| (e.rhs, e.origin)))
        }
    }

    #[derive(Clone, Copy)]
    struct Write {
        span: Span,
        target: ExpressionId,
        value: ExpressionId,
        operator: AssignmentOperator,
    }

    /// Resolve formation only. The caller still proves the complete FE current-owner
    /// graph and the actual SSA Call/transport identity before publishing capability.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::ssa) fn named_formation<R: Copy + Eq, T: Copy + Eq>(
        parsed: &ParsedFile,
        statement: Span,
        root: R,
        mut declaration: impl FnMut(R) -> Result<BindingDeclaration, LoweringError>,
        mut reference: impl FnMut(Span) -> Result<R, LoweringError>,
        mut expression_type: impl FnMut(ExpressionId) -> Option<T>,
        mut birth: impl FnMut(ExpressionId) -> Result<Birth<R>, LoweringError>,
        updates: impl Iterator<Item = OwnerUpdate>,
    ) -> Result<Formation<R>, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, statement);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, statement);
        let callable_bodies = parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| {
                let Item::Function { form, .. } = node.payload() else {
                    return None;
                };
                let body = match form {
                    FunctionForm::ImplicitUnitBlock(body)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    } => *body,
                    _ => return None,
                };
                parsed.ast().statements().get(body).ok().map(|n| n.span())
            })
            .collect::<Vec<_>>();
        let scope = callable_bodies
            .iter()
            .copied()
            .filter(|&body| contains(body, statement))
            .min_by_key(|span| span.end() - span.start())
            .ok_or_else(unsupported)?;
        let same_callable = |span| {
            contains(scope, span)
                && callable_bodies
                    .iter()
                    .filter(|&&body| contains(body, span))
                    .min_by_key(|body| body.end() - body.start())
                    == Some(&scope)
                && !parsed.ast().expressions().iter().any(|(_, node)| {
                    matches!(node.payload(), Expression::Lambda { .. })
                        && contains(node.span(), span)
                })
        };
        let writes = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, node)| {
                let Expression::Assignment {
                    target,
                    value,
                    operator,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                same_callable(node.span()).then_some(Write {
                    span: node.span(),
                    target: *target,
                    value: *value,
                    operator: *operator,
                })
            })
            .collect::<Vec<_>>();
        let updates = updates.collect::<Vec<_>>();
        let mut roots = Vec::new();
        let mut current = root;
        let mut cutoff = statement;
        let mut binding = None;
        let mut origin = None;
        let mut replacement = None;
        let mut root_declaration = None;
        let mut history = Vec::new();
        let mut selected_updates = Vec::new();
        let mut last_reference = None;
        loop {
            if roots.contains(&current) {
                return Err(unsupported());
            }
            roots.push(current);
            let declared_binding = declaration(current)?;
            if let BindingDeclaration::EntryParameter { marker } = declared_binding {
                let parameter_reference = last_reference.ok_or_else(unsupported)?;
                collect_updates(
                    parsed,
                    &updates,
                    &mut selected_updates,
                    (marker.end(), cutoff, current, None),
                    &same_callable,
                    &mut expression_type,
                    &mut reference,
                )?;
                return Ok(Formation {
                    birth: Birth::EntryParameter {
                        root: current,
                        reference: parameter_reference,
                        origin: marker,
                    },
                    updates: sorted_updates(parsed, selected_updates)?,
                    roots,
                    declaration: root_declaration.ok_or_else(missing)?,
                    history,
                    births: Vec::new(),
                    binding: binding.ok_or_else(missing)?,
                    origin: origin.ok_or_else(missing)?,
                    replacement,
                });
            }
            let BindingDeclaration::Local {
                marker,
                initializer: declared,
            } = declared_binding
            else {
                return Err(missing());
            };
            let declared_node = parsed
                .ast()
                .expressions()
                .get(declared)
                .map_err(|_| missing())?;
            if !same_callable(marker)
                || !same_callable(declared_node.span())
                || marker.end() > declared_node.span().start()
                || declared_node.span().end() >= cutoff.start()
            {
                return Err(unsupported());
            }
            let first_root = roots.len() == 1;
            let mut matching = Vec::new();
            for &write in &writes {
                if write.span.start() <= declared_node.span().end()
                    || (!first_root && write.span.end() >= cutoff.start())
                {
                    continue;
                }
                let target = transparent_expression(parsed, write.target, &mut expression_type)?;
                let target = parsed
                    .ast()
                    .expressions()
                    .get(target)
                    .map_err(|_| missing())?;
                if target.payload() != &Expression::Name || reference(target.span())? != current {
                    continue;
                }
                if write.operator != AssignmentOperator::Assign
                    || conditional_write(parsed, write.span)
                {
                    return Err(unsupported());
                }
                matching.push(write);
            }
            matching.sort_by_key(|write| (write.span.start(), write.span.end()));
            if matching.iter().enumerate().any(|(index, write)| {
                matching[..index].iter().any(|previous| {
                    contains(previous.span, write.span) || contains(write.span, previous.span)
                })
            }) {
                return Err(unsupported());
            }
            let prior = matching
                .iter()
                .copied()
                .filter(|write| write.span.end() < cutoff.start())
                .max_by_key(|write| write.span.end());
            let next = if first_root {
                matching
                    .iter()
                    .copied()
                    .find(|write| write.span.start() > statement.end())
            } else {
                None
            };
            let initializer = prior.map_or(declared, |write| write.value);
            let outer = parsed
                .ast()
                .expressions()
                .get(initializer)
                .map_err(|_| missing())?;
            if !same_callable(outer.span()) || outer.span().end() >= cutoff.start() {
                return Err(unsupported());
            }
            if first_root {
                root_declaration = Some(marker);
                binding = Some(outer.span());
                origin = Some(prior.map_or(marker, |write| write.span));
                replacement = next.map(|write| write.value);
                history.push(BindingEpoch {
                    rhs: declared,
                    origin: marker,
                });
                history.extend(matching.iter().map(|write| BindingEpoch {
                    rhs: write.value,
                    origin: write.span,
                }));
                for epoch in &history {
                    let canonical =
                        transparent_expression(parsed, epoch.rhs, &mut expression_type)?;
                    let node = parsed
                        .ast()
                        .expressions()
                        .get(canonical)
                        .map_err(|_| missing())?;
                    if node.payload() != &Expression::Name {
                        birth(canonical)?;
                    }
                }
            }
            collect_updates(
                parsed,
                &updates,
                &mut selected_updates,
                (outer.span().end(), cutoff, current, None),
                &same_callable,
                &mut expression_type,
                &mut reference,
            )?;
            let initializer = transparent_expression(parsed, initializer, &mut expression_type)?;
            let node = parsed
                .ast()
                .expressions()
                .get(initializer)
                .map_err(|_| missing())?;
            match node.payload() {
                Expression::Name => {
                    cutoff = node.span();
                    last_reference = Some(initializer);
                    current = reference(cutoff)?;
                }
                _ => {
                    return Ok(Formation {
                        birth: birth(initializer)?,
                        updates: sorted_updates(parsed, selected_updates)?,
                        roots,
                        declaration: root_declaration.ok_or_else(missing)?,
                        history,
                        births: Vec::new(),
                        binding: binding.ok_or_else(missing)?,
                        origin: origin.ok_or_else(missing)?,
                        replacement,
                    });
                }
            }
        }
    }

    pub(in crate::ssa) fn continuation<R: Copy + Eq, T: Copy + Eq>(
        parsed: &ParsedFile,
        formation: &Formation<R>,
        event: DropEvent,
        acquisition: Span,
        mut expression_type: impl FnMut(ExpressionId) -> Option<T>,
        mut reference: impl FnMut(Span) -> Result<R, LoweringError>,
        updates: impl Iterator<Item = OwnerUpdate>,
    ) -> Result<Formation<R>, LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, event.span);
        if acquisition.source_id() != event.span.source_id()
            || !in_formation(parsed, formation, event)?
        {
            return Err(bad());
        }
        let mut terminal = formation.clone();
        if event.span.end() > acquisition.end() {
            let root = *formation.roots.first().ok_or_else(bad)?;
            let updates = updates.collect::<Vec<_>>();
            collect_updates(
                parsed,
                &updates,
                &mut terminal.updates,
                (acquisition.end(), event.span, root, Some(event.span.end())),
                &|at| handoff::same_callable(parsed, acquisition, at),
                &mut expression_type,
                &mut reference,
            )?;
            terminal.updates = sorted_updates(parsed, terminal.updates)?;
        }
        Ok(terminal)
    }

    fn collect_updates<R: Copy + Eq, T: Copy + Eq>(
        parsed: &ParsedFile,
        updates: &[OwnerUpdate],
        selected: &mut Vec<OwnerUpdate>,
        window: (usize, Span, R, Option<usize>),
        same_callable: &impl Fn(Span) -> bool,
        typed: &mut impl FnMut(ExpressionId) -> Option<T>,
        reference: &mut impl FnMut(Span) -> Result<R, LoweringError>,
    ) -> Result<(), LoweringError> {
        let (start, cutoff, root, completed_end) = window;
        for &update in updates {
            let operation = parsed
                .ast()
                .expressions()
                .get(update.expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, cutoff))?;
            let at = operation.span();
            let completed =
                completed_end.map_or_else(|| at.end() < cutoff.start(), |end| at.end() <= end);
            if !same_callable(at) || start >= at.start() || !completed {
                continue;
            }
            let receiver = transparent_expression(parsed, update.receiver, &mut *typed)?;
            let node = parsed
                .ast()
                .expressions()
                .get(receiver)
                .map_err(|_| error(LoweringErrorKind::MissingFact, cutoff))?;
            if node.payload() == &Expression::Name && reference(node.span())? == root {
                selected.push(update);
            }
        }
        Ok(())
    }

    fn sorted_updates(
        parsed: &ParsedFile,
        mut updates: Vec<OwnerUpdate>,
    ) -> Result<Vec<OwnerUpdate>, LoweringError> {
        updates.sort_by_key(|u| {
            parsed
                .ast()
                .expressions()
                .get(u.expression)
                .ok()
                .map(|n| n.span().start())
        });
        for pair in updates.windows(2) {
            let a = parsed
                .ast()
                .expressions()
                .get(pair[0].expression)
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?
                .span();
            let b = parsed
                .ast()
                .expressions()
                .get(pair[1].expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, a))?
                .span();
            if a.end() >= b.start() {
                return Err(error(LoweringErrorKind::UnsupportedNode, b));
            }
        }
        Ok(updates)
    }

    fn contains(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() <= inner.start()
            && inner.end() <= outer.end()
    }

    fn conditional_write(parsed: &ParsedFile, span: Span) -> bool {
        parsed.ast().statements().iter().any(|(_, node)| {
            contains(node.span(), span)
                && matches!(
                    node.payload(),
                    Statement::For { .. } | Statement::While { .. } | Statement::Loop { .. }
                )
        }) || parsed.ast().expressions().iter().any(|(_, node)| {
            contains(node.span(), span)
                && matches!(
                    node.payload(),
                    Expression::If { .. } | Expression::When { .. }
                )
        })
    }

    fn error(kind: LoweringErrorKind, span: Span) -> LoweringError {
        LoweringError {
            kind,
            span: Some(span),
        }
    }
}
