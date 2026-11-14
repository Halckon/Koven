//! Exact source storage identity, shared by the bounded provider and its carrier.
use crate::ssa::lower_frontend::map_iteration::*;

use lang_frontend::type_checking::CallableTarget;

pub(in crate::ssa) mod borrowed {
    //! A stable shared binding is distinct from the canonical parameter it borrows.
    use crate::ssa::lower_frontend::map_iteration::*;

    use lang_frontend::name_resolution::{Namespace, ReferenceTarget};

    use lang_frontend::ownership_checking::{BorrowBindingStorage, DropTarget};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::ssa) struct Proof {
        pub(in crate::ssa) binding: SymbolId,
        pub(in crate::ssa) source: SymbolId,
        pub(in crate::ssa) declaration: Span,
    }

    pub(in crate::ssa) fn prove(
        frontend: (
            &ParsedFile,
            &NameResolution,
            &TypedFile,
            &OwnershipCheckedFile,
        ),
        plan: &MapIterationOwnershipPlan,
        binding: SymbolId,
        span: Span,
    ) -> Result<Proof, LoweringError> {
        let (parsed, names, typed, owned) = frontend;
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let fact = unique(
            owned
                .borrow_results()
                .bindings()
                .iter()
                .filter(|f| f.binding() == binding),
        )
        .ok_or_else(bad)?;
        let Some(LoanTarget::Place(origin)) = plan.source() else {
            return Err(bad());
        };
        let symbol = names.symbols().get(binding.index()).ok_or_else(bad)?;
        if fact.storage() != BorrowBindingStorage::BorrowedStorage
            || fact.parent().is_some()
            || fact.source_loan().is_some()
            || !origin.is_root()
            || plan.source_access() != MapIterationSourceAccess::Shared
        {
            return Err(unsupported());
        }
        if symbol.kind() != SymbolKind::Variable
            || symbol.is_synthetic()
            || fact.origin() != plan.source().ok_or_else(bad)?
            || binding == origin.root()
            || plan.descriptor().bindings().contains(&binding)
            || typed.symbol_type(binding) != Some(plan.descriptor().source_type())
            || typed.expression_type(fact.initializer()) != Some(plan.descriptor().source_type())
            || typed.expression_category(fact.initializer())
                != Some(lang_frontend::type_checking::ExpressionCategory::Place)
            || owned.formed_owned_locals().contains(&binding)
            || owned
                .drops()
                .iter()
                .any(|f| f.target() == DropTarget::Named(binding))
        {
            return Err(bad());
        }
        let initializer =
            crate::ssa::map_source::transparent_expression(parsed, fact.initializer(), |id| {
                typed.expression_type(id).zip(typed.expression_category(id))
            })?;
        let node = parsed
            .ast()
            .expressions()
            .get(initializer)
            .map_err(|_| bad())?;
        let reference = unique(
            names
                .references()
                .iter()
                .filter(|r| r.namespace() == Namespace::Value && r.span() == node.span()),
        )
        .ok_or_else(bad)?;
        if node.payload() != &Expression::Name
            || reference.target() != &ReferenceTarget::Symbol(origin.root())
        {
            return Err(bad());
        }
        let end = unique(
            owned
                .borrow_results()
                .ends()
                .iter()
                .filter(|f| f.binding() == binding),
        )
        .ok_or_else(bad)?;
        let DropPoint::AfterStatement(scope) = end.point() else {
            return Err(unsupported());
        };
        let declaration = crate::ssa::borrow_result_support::stable_map::declaration(
            parsed,
            symbol.span(),
            fact.marker_span(),
            fact.initializer(),
            scope,
            plan.descriptor().statement(),
        )?;
        Ok(Proof {
            binding,
            source: origin.root(),
            declaration,
        })
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn borrowed_map_seed(
            &self,
            proof: Proof,
            span: Span,
        ) -> Result<EntityId, LoweringError> {
            let bad = || error(LoweringErrorKind::MissingFact, span);
            let current =
                EntityId::Loan(*self.borrow_bindings.get(&proof.binding).ok_or_else(bad)?);
            let parent = *self
                .map_iteration_state
                .entries
                .get(&proof.source)
                .ok_or_else(bad)?;
            if self.bindings.contains_key(&proof.binding)
                || self
                    .result_source_loans
                    .get(&proof.binding)
                    .is_none_or(|loans| !loans.is_empty())
            {
                return Err(bad());
            }
            crate::ssa::borrow_result_support::stable_map::seed(
                self.function,
                current,
                parent,
                proof.declaration,
                span,
                nested::transport,
            )?;
            Ok(current)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa) enum SourceIdentity {
    Parameter(SymbolId),
    BorrowedBinding(borrowed::Proof),
    Local {
        symbol: SymbolId,
        initializer: ExpressionId,
    },
    /// Current owner follows an original Call through normal binding/CFG transport.
    Formed {
        symbol: SymbolId,
        birth: crate::ssa::map_source::Birth<SymbolId>,
    },
    Temporary(ExpressionId),
}

impl SourceIdentity {
    pub(in crate::ssa) fn matches_slot(self, slot: &LinearBindingSlot) -> bool {
        match self {
            Self::BorrowedBinding(_) => false,
            Self::Parameter(symbol) | Self::Local { symbol, .. } | Self::Formed { symbol, .. } => {
                slot.symbol == Some(symbol)
            }
            Self::Temporary(expression) => {
                slot.symbol.is_none() && slot.temporaries.as_slice() == [expression.index()]
            }
        }
    }

    pub(in crate::ssa) fn current_owner(self, lowerer: &ExpressionLowerer<'_>) -> Option<ValueId> {
        match self {
            Self::BorrowedBinding(_) => None,
            Self::Parameter(symbol) | Self::Local { symbol, .. } | Self::Formed { symbol, .. } => {
                match lowerer.bindings.get(&symbol) {
                    Some(LoweredValue::Value(owner)) => Some(*owner),
                    _ => None,
                }
            }
            Self::Temporary(expression) => lowerer.temporaries.get(&expression.index()).copied(),
        }
    }

    pub(in crate::ssa) fn take_owner(
        self,
        lowerer: &mut ExpressionLowerer<'_>,
        owner: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.current_owner(lowerer) != Some(owner) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        match self {
            Self::BorrowedBinding(_) => {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            Self::Parameter(symbol) | Self::Local { symbol, .. } | Self::Formed { symbol, .. } => {
                lowerer.bindings.remove(&symbol);
            }
            Self::Temporary(expression) => {
                // The admitted source is a direct Call, so there are no transparent
                // aliases to erase and no unrelated outer owner may be removed.
                if lowerer
                    .temporaries
                    .iter()
                    .any(|(&key, &value)| key != expression.index() && value == owner)
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                lowerer.temporaries.remove(&expression.index());
            }
        }
        Ok(())
    }
}

impl ExpressionLowerer<'_> {
    pub(in crate::ssa) fn map_iteration_seed(
        &mut self,
        source: SourceIdentity,
        container: SsaTypeId,
        origin: Span,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        match source {
            SourceIdentity::BorrowedBinding(proof) => self.borrowed_map_seed(proof, span),
            SourceIdentity::Parameter(symbol) => {
                let seed = *self
                    .map_iteration_state
                    .entries
                    .get(&symbol)
                    .ok_or_else(missing)?;
                let entry = self.function.entry_block().ok_or_else(missing)?;
                if !matches!(self.function.entity(seed).map(|entity| entity.definition),
                        Some(Definition::BlockParameter { block, .. }) if block == entry)
                {
                    return Err(missing());
                }
                let current = match seed {
                    EntityId::Value(_) => match self.bindings.get(&symbol) {
                        Some(LoweredValue::Value(owner)) => EntityId::Value(*owner),
                        _ => return Err(missing()),
                    },
                    EntityId::Loan(_) => {
                        EntityId::Loan(*self.borrow_bindings.get(&symbol).ok_or_else(missing)?)
                    }
                    _ => return Err(missing()),
                };
                nested::transport(self.function, current, seed, span)?;
                Ok(current)
            }
            SourceIdentity::Local {
                symbol,
                initializer,
            } => self.local_map_seed(symbol, initializer, container, span),
            SourceIdentity::Formed { symbol, birth } => {
                let formation = formed::formation(
                    self.parsed,
                    self.names,
                    self.typed,
                    self.owned,
                    symbol,
                    self.typed.symbol_type(symbol).ok_or_else(missing)?,
                    span,
                )?;
                if formation.birth != birth {
                    return Err(missing());
                }
                self.formed_map_anchor(&formation, None, span)?;
                let owner = source.current_owner(self).ok_or_else(missing)?;
                if self.function.entity(EntityId::Value(owner)).map(|e| e.ty)
                    != Some(EntityType::Value(container))
                {
                    return Err(missing());
                }
                let owner = source.current_owner(self).ok_or_else(missing)?;
                Ok(EntityId::Value(owner))
            }
            SourceIdentity::Temporary(expression) => {
                let LoweredValue::Value(owner) = self.lower(expression)? else {
                    return Err(missing());
                };
                // Ordinary argument lowering owns its CFG and loan cleanup. The
                // provider starts only after that call has delivered this owner.
                let preheader = self.block;
                let seed = EntityId::Value(owner);
                let instruction = carrier::definition(self.function, seed).ok_or_else(missing)?;
                if instruction.block != preheader
                    || instruction.origin != Origin::Source(origin)
                    || self.map_call_birth(expression, span)? != owner
                    || self.function.entity(seed).map(|entity| entity.ty)
                        != Some(EntityType::Value(container))
                    || source.current_owner(self) != Some(owner)
                    || self.temporaries.iter().any(|(&key, &value)| {
                        key != expression.index() && !self.active_map_temporary(key, value)
                    })
                    || !self.pending_call_loans.is_empty()
                    || !self.capture_loans.is_empty()
                    || !self.result_source_loans.is_empty()
                {
                    return Err(missing());
                }
                Ok(seed)
            }
        }
    }
}

impl ExpressionLowerer<'_> {
    /// Anchor the result already emitted by ordinary argument lowering. This
    /// consumes no operands and does not reconstruct argument/loan delivery.
    pub(in crate::ssa) fn map_call_birth(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let origin = self.expression_span(expression)?;
        let ty = self.expression_ssa_type(expression, span)?;
        let instruction = unique(self.function.instructions.iter().filter(|instruction| {
            instruction.origin == Origin::Source(origin)
                && matches!(
                    instruction.operation,
                    Operation::DirectCall { .. }
                        | Operation::MapConstruct { .. }
                        | Operation::MapConsume { .. }
                )
        }))
        .ok_or_else(missing)?;
        let [EntityId::Value(owner)] = instruction.results.as_slice() else {
            return Err(missing());
        };
        if self
            .function
            .entity(EntityId::Value(*owner))
            .map(|data| (data.ty, data.definition))
            != Some((
                EntityType::Value(ty),
                Definition::InstructionResult {
                    instruction: instruction.id,
                    index: 0,
                },
            ))
        {
            return Err(missing());
        }
        if let Some(construction) = self.typed.map_construction(expression) {
            if construction.expression() != expression
                || self.typed.expression_type(expression) != Some(construction.map_type())
                || instruction.operation != (Operation::MapConstruct { map_type: ty })
            {
                return Err(missing());
            }
        } else if let Some(consume) = self.typed.map_consume(expression) {
            let Operation::MapConsume {
                owner: input,
                map_type,
            } = instruction.operation
            else {
                return Err(missing());
            };
            if consume.expression() != expression
                || map_type != ty
                || self.typed.expression_type(expression) != Some(consume.result_type())
                || self
                    .function
                    .entity(EntityId::Value(input))
                    .map(|data| data.ty)
                    != Some(EntityType::Value(
                        self.expression_ssa_type(consume.receiver(), span)?,
                    ))
            {
                return Err(missing());
            }
        } else {
            let call = self.typed.call(expression).ok_or_else(missing)?;
            let CallableTarget::Source(target) = call.target() else {
                return Err(missing());
            };
            let instance = if let Some(source) = self.source_token {
                self.instance_plan
                    .call_site(source, expression)
                    .cloned()
                    .ok_or_else(missing)?
            } else {
                let type_arguments = call
                    .instance()
                    .type_arguments()
                    .iter()
                    .map(|ty| self.resolve_type(*ty, span))
                    .collect::<Result<Vec<_>, _>>()?;
                FunctionInstanceKey::new(target, type_arguments)
            };
            let expected = *self.function_ids.get(&instance).ok_or_else(missing)?;
            if instance.symbol() != target
                || call.expression() != expression
                || call.result_source() != lang_frontend::type_checking::CallableResultSource::Owned
                || self.typed.expression_type(expression) != Some(call.return_type())
                || !matches!(&instruction.operation, Operation::DirectCall {
                        callee, receiver: None, arguments,
                    } if *callee == expected && arguments.len() == call.arguments().len())
            {
                return Err(missing());
            }
        }
        Ok(*owner)
    }

    pub(in crate::ssa) fn validate_pure_map_parameter_drop(
        &self,
        fact: lang_frontend::ownership_checking::DropFact,
        owner: ValueId,
    ) -> Result<bool, LoweringError> {
        use lang_frontend::ownership_checking::DropTarget;
        let DropTarget::Named(root) = fact.target() else {
            return Ok(false);
        };
        let Some(&anchor) = self.map_iteration_state.entries.get(&root) else {
            return Ok(false);
        };
        let candidate = self.typed.map_iterations().iter().any(|descriptor| {
            self.typed.is_resource_type(descriptor.source_type()) == Some(false)
                && self
                    .owned
                    .map_iteration(descriptor.statement())
                    .is_some_and(|plan| {
                        matches!(plan.source(), Some(LoanTarget::Place(place))
                            if place.is_root() && place.root() == root)
                    })
        });
        if !candidate
            || !source_owner::supported_drop(self.parsed, self.names, self.typed, self.owned, fact)
        {
            return Ok(false);
        }
        let bad = || error(LoweringErrorKind::MissingFact, fact.value_origin());
        if !matches!(anchor, EntityId::Value(_))
            || self.borrow_bindings.contains_key(&root)
            || self.temporaries.values().any(|&v| v == owner)
            || self
                .bindings
                .values()
                .any(|v| *v == LoweredValue::Value(owner))
        {
            return Err(bad());
        }
        nested::transport(
            self.function,
            EntityId::Value(owner),
            anchor,
            fact.value_origin(),
        )?;
        Ok(true)
    }
}

pub(in crate::ssa) mod topology {
    //! Complete two-/four-column normal local proof, including absent old columns.
    use crate::ssa::lower_frontend::map_iteration::local::*;
    use crate::ssa::lower_frontend::map_iteration::*;

    use lang_frontend::ownership_checking::{
        CleanupConditions, CleanupOwnerValue, CleanupSelectorSource, IterationClosurePhiBinding,
        IterationPhiBoundary, IterationPhiIncoming, IterationPhiIncomingBinding,
        IterationPhiIncomingKind, IterationPhiPresenceSource,
    };
    use lang_frontend::ownership_checking::{DropFact, DropTarget};

    pub(in crate::ssa) fn prove(
        parsed: &ParsedFile,
        owned: &OwnershipCheckedFile,
        site: &identity::Site<'_>,
        previous: Option<(&identity::Site<'_>, &Proof)>,
    ) -> Result<Proof, LoweringError> {
        let span = site.span;
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let plan = site.plan;
        let descriptor = plan.descriptor();
        let conditions = owned.cleanup_conditions();
        if !plan.capture_graph().nodes().is_empty() {
            return Err(unsupported());
        }
        let mut roots = previous
            .map(|(site, _)| vec![site.root])
            .unwrap_or_default();
        roots.push(site.root);
        roots.sort();
        if plan
            .closure_phis()
            .iter()
            .any(|phi| !roots.contains(&phi.symbol()))
        {
            return Err(unsupported());
        }
        if plan.closure_phis().len() != roots.len() * 2 || plan.closure_phi_incomings().len() != 3 {
            return Err(missing());
        }
        let headers = phis(conditions, plan, &roots, IterationPhiBoundary::Header, span)?;
        let exits = phis(conditions, plan, &roots, IterationPhiBoundary::Exit, span)?;
        let all = headers.iter().chain(&exits).copied().collect::<Vec<_>>();
        if all.iter().enumerate().any(|(index, phi)| {
            all[..index].iter().any(|prior| {
                prior.owner() == phi.owner()
                    || prior.availability_selector() == phi.availability_selector()
                    || prior.availability_condition() == phi.availability_condition()
            })
        }) {
            return Err(missing());
        }
        let entry = incoming(
            plan,
            IterationPhiIncomingKind::Entry,
            IterationPhiBoundary::Header,
            DropPoint::AfterExpression(descriptor.source()),
            &headers,
            span,
        )?;
        let back = incoming(
            plan,
            IterationPhiIncomingKind::Fallthrough,
            IterationPhiBoundary::Header,
            DropPoint::AfterStatement(site.body),
            &headers,
            span,
        )?;
        let exhausted = incoming(
            plan,
            IterationPhiIncomingKind::Exhaustion,
            IterationPhiBoundary::Exit,
            DropPoint::LoopExit(descriptor.statement()),
            &exits,
            span,
        )?;
        let path = conditions::path(parsed, conditions, entry.condition(), previous, span)?;
        for incoming in [entry, back, exhausted] {
            conditions::check(
                conditions,
                incoming.condition(),
                path,
                None,
                conditions::Expected::Path,
                span,
            )?;
        }
        let current = roots
            .iter()
            .position(|root| *root == site.root)
            .ok_or_else(missing)?;
        let header = headers[current];
        let exit = exits[current];
        let entry_binding = &entry.bindings()[current];
        let [seed] = entry_binding.values() else {
            return Err(missing());
        };
        if seed.condition() != entry_binding.available_when()
            || all.iter().any(|phi| phi.owner() == seed.source())
            || previous.is_some_and(|(_, proof)| seed.source() == proof.birth)
        {
            return Err(missing());
        }
        conditions::check(
            conditions,
            seed.condition(),
            path,
            None,
            conditions::Expected::Path,
            span,
        )?;
        let initializer = parsed
            .ast()
            .expressions()
            .get(site.initializer)
            .map_err(|_| missing())?;
        if !matches!(conditions.owner_value(seed.source()), Some(CleanupOwnerValue::Expression { expression, origin })
                if *expression == site.initializer && *origin == initializer.span())
        {
            // Once the ordinary local Call is admitted, Parameter/other owner kinds
            // are corrupt anchors, rather than alternate local production paths.
            return Err(missing());
        }
        self_edge(conditions, &back.bindings()[current], header, path, span)?;
        let late_size = site.guard.map(|guard| guard.size);
        let drop_point = late_size.map_or(
            DropPoint::LoopExit(descriptor.statement()),
            DropPoint::CallReturn,
        );
        let drop_owner = if late_size.is_some() { exit } else { header };
        let drop = drop_fact(
            owned,
            site.root,
            site.origin,
            drop_point,
            drop_owner,
            path,
            span,
        )?;
        if late_size.is_some() {
            let binding = &exhausted.bindings()[current];
            let [value] = binding.values() else {
                return Err(missing());
            };
            if value.source() != header.owner() || value.condition() != binding.available_when() {
                return Err(missing());
            }
            // The whole exhaustion incoming is guarded by P. FE publishes the
            // transferred H presence itself here, distinct from P-and-H on backedge.
            conditions::check(
                conditions,
                binding.available_when(),
                path,
                Some(header.availability_selector()),
                conditions::Expected::Presence,
                span,
            )?;
        } else {
            absent(conditions, &exhausted.bindings()[current], span)?;
        }
        let mut dead = Vec::new();
        if let Some((prior, proof)) = previous {
            let index = roots
                .iter()
                .position(|root| *root == prior.root)
                .ok_or_else(missing)?;
            let header = headers[index];
            let exit = exits[index];
            // Absence is anchored at Entry; the sole backedge can preserve only this
            // same absent H. There is no incoming source from which an owner arises.
            absent(conditions, &entry.bindings()[index], span)?;
            self_edge(conditions, &back.bindings()[index], header, path, span)?;
            absent(conditions, &exhausted.bindings()[index], span)?;
            let dead_drop = drop_fact(
                owned,
                prior.root,
                prior.origin,
                DropPoint::LoopExit(descriptor.statement()),
                header,
                path,
                span,
            )?;
            dead.push(DeadColumn {
                root: prior.root,
                initializer: prior.initializer,
                previous: prior.plan.descriptor().statement(),
                consumed: proof.drop,
                drop: dead_drop,
                header: header.owner(),
                exit: exit.owner(),
            });
        }
        cleanup(
            owned,
            site,
            path,
            late_size.is_none().then_some(drop),
            &dead,
        )?;
        Ok(Proof {
            root: site.root,
            initializer: site.initializer,
            consumer: site.consumer,
            birth: seed.source(),
            header: header.owner(),
            exit: exit.owner(),
            drop,
            late_size,
            dead,
            path,
        })
    }

    fn phis<'a>(
        conditions: &CleanupConditions,
        plan: &'a MapIterationOwnershipPlan,
        roots: &[SymbolId],
        boundary: IterationPhiBoundary,
        span: Span,
    ) -> Result<Vec<&'a IterationClosurePhiBinding>, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let flow = match boundary {
            IterationPhiBoundary::Header => plan.closure_flow().header(),
            IterationPhiBoundary::Exit => plan.closure_flow().exit(),
        };
        if flow.len() != roots.len()
            || flow
                .iter()
                .zip(roots)
                .any(|(binding, root)| binding.symbol() != *root || !binding.origins().is_empty())
        {
            return Err(missing());
        }
        let result = roots
            .iter()
            .map(|root| source_owner::phi(conditions, plan, *root, boundary, span))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = plan
            .closure_phis()
            .iter()
            .filter(|phi| phi.boundary() == boundary)
            .collect::<Vec<_>>();
        if actual != result {
            return Err(missing());
        }
        for (slot, phi) in result.iter().enumerate() {
            let selector = conditions
                .selector(phi.availability_selector())
                .ok_or_else(missing)?;
            if selector.source()
                != (CleanupSelectorSource::IterationPhi {
                    statement: plan.descriptor().statement(),
                    boundary,
                    slot,
                })
            {
                return Err(missing());
            }
        }
        Ok(result)
    }

    fn incoming<'a>(
        plan: &'a MapIterationOwnershipPlan,
        kind: IterationPhiIncomingKind,
        boundary: IterationPhiBoundary,
        point: DropPoint,
        phis: &[&IterationClosurePhiBinding],
        span: Span,
    ) -> Result<&'a IterationPhiIncoming, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let incoming = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|incoming| incoming.kind() == kind),
        )
        .ok_or_else(missing)?;
        if incoming.boundary() != boundary
            || incoming.point() != point
            || incoming.bindings().len() != phis.len()
        {
            return Err(missing());
        }
        for (binding, phi) in incoming.bindings().iter().zip(phis) {
            if binding.target() != phi.owner()
                || binding.availability_selector() != phi.availability_selector()
                || binding.presence_source() != IterationPhiPresenceSource::StaticConditions
                || !binding.capture_slots_to_clear().is_empty()
                || !binding.root_sources().is_empty()
                || !binding.selector_writes().is_empty()
                || !binding.origins().is_empty()
            {
                return Err(missing());
            }
        }
        Ok(incoming)
    }

    fn absent(
        conditions: &CleanupConditions,
        binding: &IterationPhiIncomingBinding,
        span: Span,
    ) -> Result<(), LoweringError> {
        if conditions.get(binding.available_when()) != Some(&CleanupCondition::Never)
            || !binding.values().is_empty()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    fn self_edge(
        conditions: &CleanupConditions,
        binding: &IterationPhiIncomingBinding,
        header: &IterationClosurePhiBinding,
        path: Option<Path>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let [value] = binding.values() else {
            return Err(missing());
        };
        if value.source() != header.owner() || value.condition() != binding.available_when() {
            return Err(missing());
        }
        conditions::check(
            conditions,
            binding.available_when(),
            path,
            Some(header.availability_selector()),
            conditions::Expected::Conjunction,
            span,
        )
    }

    fn drop_fact(
        owned: &OwnershipCheckedFile,
        root: SymbolId,
        origin: Span,
        point: DropPoint,
        phi: &IterationClosurePhiBinding,
        path: Option<Path>,
        span: Span,
    ) -> Result<DropFact, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let fact =
            unique(
                owned.drops().iter().copied().filter(|fact| {
                    fact.target() == DropTarget::Named(root) && fact.point() == point
                }),
            )
            .ok_or_else(missing)?;
        if fact.owner() != Some(phi.owner())
            || fact.value_origin() != origin
            || fact.capture_slot().is_some()
            || fact.instance_address().is_some()
        {
            return Err(missing());
        }
        conditions::check(
            owned.cleanup_conditions(),
            fact.condition().ok_or_else(missing)?,
            path,
            Some(phi.availability_selector()),
            conditions::Expected::Conjunction,
            span,
        )?;
        Ok(fact)
    }

    fn cleanup(
        owned: &OwnershipCheckedFile,
        site: &identity::Site<'_>,
        path: Option<Path>,
        current_drop: Option<DropFact>,
        dead: &[DeadColumn],
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, site.span);
        let plan = site.plan;
        let descriptor = plan.descriptor();
        let statement = descriptor.statement();
        if plan.exits().len() != 2 {
            return Err(missing());
        }
        for kind in [
            IterationExitKind::Fallthrough,
            IterationExitKind::Exhaustion,
        ] {
            let exit = unique(plan.exits().iter().filter(|exit| exit.kind() == kind))
                .ok_or_else(missing)?;
            let point = exit_shape::point(kind, statement, site.body);
            conditions::check(
                owned.cleanup_conditions(),
                exit.condition(),
                path,
                None,
                conditions::Expected::Path,
                site.span,
            )?;
            let expected = if kind == IterationExitKind::Fallthrough {
                vec![
                    IterationCleanupAction::EndBinding {
                        statement,
                        symbol: descriptor.value_binding(),
                    },
                    IterationCleanupAction::EndBinding {
                        statement,
                        symbol: descriptor.key_binding(),
                    },
                ]
            } else {
                [
                    IterationCleanupAction::FinishProvider(statement),
                    IterationCleanupAction::EndSource(statement),
                ]
                .into_iter()
                .chain(current_drop.into_iter().map(IterationCleanupAction::Drop))
                .chain(
                    dead.iter()
                        .map(|dead| IterationCleanupAction::Drop(dead.drop)),
                )
                .collect()
            };
            let actual = owned
                .cleanup_steps()
                .iter()
                .filter_map(|(actual, action)| (*actual == point).then_some(*action))
                .collect::<Vec<_>>();
            if exit.point() != point || exit.actions() != expected || actual != expected {
                return Err(missing());
            }
        }
        Ok(())
    }
}
