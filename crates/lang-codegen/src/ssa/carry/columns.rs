//! Layout-local columns preserve aliases only when every incoming entity agrees.
use crate::ssa::{lowering_support::LoweringError, model::EntityId};

#[cfg(test)]
use crate::ssa::model::EntityType;

#[derive(Clone, Copy)]
pub(in crate::ssa) struct ColumnId(usize);

impl ColumnId {
    pub(super) fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub(in crate::ssa) fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy)]
pub(in crate::ssa) enum ColumnMatch {
    Added(ColumnId),
    Reused(ColumnId),
}

impl ColumnMatch {
    pub(in crate::ssa) fn column(self) -> ColumnId {
        match self {
            Self::Added(column) | Self::Reused(column) => column,
        }
    }
}

/// Read one already-selected role from one exit; no column or type searches.
pub(in crate::ssa) trait IncomingRole<E> {
    fn current(&self, exit: &E) -> Result<EntityId, LoweringError>;
}

pub(in crate::ssa) struct IncomingColumns<'e, E> {
    exits: &'e [E],
    columns: Vec<Vec<EntityId>>,
    role_columns: Vec<ColumnId>,
}

impl<'e, E> IncomingColumns<'e, E> {
    pub(in crate::ssa) fn new(exits: &'e [E]) -> Self {
        Self {
            exits,
            columns: Vec::new(),
            role_columns: Vec::new(),
        }
    }

    fn collect(&self, role: &impl IncomingRole<E>) -> Result<Vec<EntityId>, LoweringError> {
        self.exits.iter().map(|exit| role.current(exit)).collect()
    }

    fn append(&mut self, column: Vec<EntityId>) -> ColumnId {
        let id = ColumnId(self.columns.len());
        self.columns.push(column);
        id
    }

    /// Seeds remain separate even when all incoming entities are identical.
    pub(in crate::ssa) fn seed(
        &mut self,
        role: &impl IncomingRole<E>,
    ) -> Result<ColumnId, LoweringError> {
        let column = self.collect(role)?;
        let id = self.append(column);
        self.role_columns.push(id);
        Ok(id)
    }

    pub(in crate::ssa) fn associate(
        &mut self,
        role: &impl IncomingRole<E>,
    ) -> Result<ColumnMatch, LoweringError> {
        let column = self.collect(role)?;
        let choice = if let Some(index) = self.columns.iter().position(|other| *other == column) {
            ColumnMatch::Reused(ColumnId(index))
        } else {
            ColumnMatch::Added(self.append(column))
        };
        self.role_columns.push(choice.column());
        Ok(choice)
    }

    pub(in crate::ssa) fn columns(&self) -> &[Vec<EntityId>] {
        &self.columns
    }

    pub(in crate::ssa) fn role_columns(&self) -> &[ColumnId] {
        &self.role_columns
    }

    /// Exit count is independent of column count, including an empty-parameter merge.
    pub(in crate::ssa) fn arguments(&self) -> impl Iterator<Item = Vec<EntityId>> + '_ {
        (0..self.exits.len())
            .map(move |index| self.columns.iter().map(|column| column[index]).collect())
    }
}

/// Called immediately after each association at the existing single-entry type gate.
#[cfg(test)]
pub(in crate::ssa) fn record_type(
    types: &mut Vec<EntityType>,
    choice: ColumnMatch,
    ty: EntityType,
    mismatch: LoweringError,
) -> Result<(), LoweringError> {
    match choice {
        ColumnMatch::Added(_) => types.push(ty),
        ColumnMatch::Reused(column) if types[column.index()] != ty => return Err(mismatch),
        ColumnMatch::Reused(_) => {}
    }
    Ok(())
}

#[cfg(test)]
pub(in crate::ssa) mod nullable_while {
    //! Direct owned nullable guards create one real proof on each body entry.

    use crate::ssa::lower_frontend::{NonNullProof, non_null_proof::ProofScope, span_key};
    use crate::ssa::lower_frontend::loop_control::*;
    use crate::ssa::lower_frontend::*;
    use crate::ssa::model::SsaTypeKind;
    use entry::DirectNullableWhileEntry;
    pub(in crate::ssa) use entry::NullableWhileEntry;
    use lang_frontend::{
        ownership_checking::DropPoint,
        parser::{BinaryOperator, Expression, LiteralKind},
        type_checking::{Copyability, ExpressionCategory, NullComparisonDescriptor, TypeKind},
    };

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn direct_nullable_while_condition(
            &self,
            condition: ExpressionId,
            span: Span,
        ) -> Result<Option<(NullComparisonDescriptor, ExpressionId)>, LoweringError> {
            let expression = self.field_replace_ungroup(condition, span)?;
            let Some(descriptor) = self.typed.null_comparison(expression) else {
                return Ok(None);
            };
            if !descriptor.non_null_when_true()
                || self.borrow_bindings.contains_key(&descriptor.symbol())
                || self.typed.copyability(descriptor.nullable_type()) == Some(Copyability::Copyable)
            {
                return Ok(None);
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Binary {
                left,
                operator: BinaryOperator::NotEqual,
                right,
                ..
            } = *node.payload()
            else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let is_null = |operand| {
                self.parsed
                    .ast()
                    .expressions()
                    .get(operand)
                    .is_ok_and(|node| matches!(node.payload(), Expression::Literal(LiteralKind::Null)))
            };
            let subject = match (is_null(left), is_null(right)) {
                (true, false) => right,
                (false, true) => left,
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            let leaf = self.field_replace_ungroup(subject, span)?;
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(leaf)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(node.payload(), Expression::Name) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            if descriptor.expression() != expression
                || self.references.get(&span_key(node.span())) != Some(&descriptor.symbol())
                || self.typed.symbol_type(descriptor.symbol()) != Some(descriptor.nullable_type())
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            // This proof-producing path supports exclusive heap owners only.
            // Shared owners retain the existing ordinary while lowering.
            let nullable = self.resolve_type(descriptor.nullable_type(), span)?;
            let nullable = *self
                .type_ids
                .get(&nullable)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let kind = self
                .ssa_types
                .get(nullable.index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let SsaTypeKind::NullableHandle { inner } = kind else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            match self
                .ssa_types
                .get(inner.index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            {
                SsaTypeKind::HeapOwner { .. } => {}
                SsaTypeKind::SharedOwner { .. } => return Ok(None),
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
            Ok(Some((descriptor, subject)))
        }

        /// Real exits must retain the inherited proof. A killed or replaced proof
        /// cannot be reconstructed from the loop's entry record.
        pub(in crate::ssa) fn validate_readonly_while_exit(
            &self,
            root: SymbolId,
            inherited: NonNullProof,
            exit: &BranchExit,
            span: Span,
        ) -> Result<(), LoweringError> {
            let Some(proof) = exit.views.get(&root).copied() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            if exit.views.len() != 1
                || proof.origin != inherited.origin
                || proof.inner != inherited.inner
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            self.validate_non_null_proof(root, proof, &exit.bindings, span)
        }

        fn direct_nullable_while_entry(
            &self,
            descriptor: NullComparisonDescriptor,
            provider: Option<&crate::ssa::lower_frontend::outer_borrowed_while::Relation>,
            span: Span,
        ) -> Result<(SsaTypeId, SsaTypeId), LoweringError> {
            if let Some(provider) = provider {
                self.validate_outer_borrowed_entry(provider, descriptor, span)?;
            } else {
                self.ensure_no_unresolved_while_proof_carriers(span)?;
            }
            let symbol = descriptor.symbol();
            let nullable = self.resolve_type(descriptor.nullable_type(), span)?;
            let nullable = *self
                .type_ids
                .get(&nullable)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let Some(SsaTypeKind::NullableHandle { inner }) = self.ssa_types.get(nullable.index())
            else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let inner = *inner;
            if !matches!(
                self.ssa_types.get(inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            ) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let Some(LoweredValue::Value(entry_owner)) = self.bindings.get(&symbol).copied() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            if self
                .function
                .entity(EntityId::Value(entry_owner))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(nullable))
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            Ok((nullable, inner))
        }

        pub(in crate::ssa) fn lower_direct_nullable_while(
            &mut self,
            statement: StatementId,
            condition: ExpressionId,
            body: StatementId,
            entry: DirectNullableWhileEntry,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let provider = entry.provider().cloned();
            let (descriptor, inherited) = entry.into_parts(statement, span)?;
            if let Some((root, proof)) = inherited {
                self.validate_readonly_while_exit(root, proof, &self.loop_exit(), span)?;
            } else if !self.non_null_bindings.is_empty() {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let (nullable, inner) =
                self.direct_nullable_while_entry(descriptor, provider.as_ref(), span)?;
            let symbol = descriptor.symbol();
            let baseline = self.bindings.clone();
            let mut context = self.create_loop_header(&baseline, span)?;
            context.outer_borrowed_while = provider.clone();
            if let Some((root, proof)) = inherited {
                self.validate_readonly_while_exit(root, proof, &self.loop_exit(), span)?;
            }
            let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            if self
                .function
                .entity(EntityId::Value(owner))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(nullable))
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            // Both successors start from the complete header state, before the new birth.
            let condition_state = self.loop_exit();
            let carried = self.linear_binding_slots(&condition_state.bindings, span)?;
            let owner_slot = carried
                .slots
                .iter()
                .position(|slot| slot.symbol == Some(symbol) && slot.source == EntityId::Value(owner))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let mut body_types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
            let false_block = self
                .function
                .add_block(body_types.clone(), Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            body_types.push(EntityType::Loan {
                kind: LoanKind::Shared,
                target: inner,
            });
            let body_block = self
                .function
                .add_block(body_types, Origin::Source(span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            let parameters = &self
                .function
                .block(body_block)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                .parameters;
            let (Some(EntityId::Value(body_owner)), Some(EntityId::Loan(view))) =
                (parameters.get(owner_slot), parameters.last())
            else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let (body_owner, view) = (*body_owner, *view);
            self.function
                .set_terminator(
                    self.block,
                    TerminatorKind::NullableBranch {
                        owner,
                        when_null: loop_edge(false_block, &carried),
                        when_non_null: loop_edge(body_block, &carried),
                        view,
                    },
                    Origin::Source(span),
                )
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

            self.restore_direct_while_condition(&condition_state, provider.as_ref(), span)?;
            self.block = false_block;
            self.bindings =
                self.rebind_linear_bindings(&condition_state.bindings, false_block, &carried, span)?;
            if let Some((root, proof)) = inherited {
                self.validate_readonly_while_exit(root, proof, &self.loop_exit(), span)?;
            }
            self.emit_direct_while_condition_drops(condition, span)?;
            let false_exit = self.loop_exit();

            self.restore_direct_while_condition(&condition_state, provider.as_ref(), span)?;
            self.block = body_block;
            self.bindings =
                self.rebind_linear_bindings(&condition_state.bindings, body_block, &carried, span)?;
            if let Some((root, inherited_proof)) = inherited {
                self.validate_readonly_while_exit(root, inherited_proof, &self.loop_exit(), span)?;
                let outer = self
                    .non_null_bindings
                    .get(&root)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if outer.owner == body_owner || outer.loan == view {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
            let proof = NonNullProof::introduced(view, body_owner, inner, ProofScope::While(statement));
            self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
            self.non_null_bindings.insert(symbol, proof);
            self.loops.push(context);
            self.emit_direct_while_condition_drops(condition, span)?;
            if self.lower_statement(body)? != LoweredValue::Diverged {
                // The statement already consumed its exact cleanup facts.
                self.record_continue(span)?;
            }
            let context = self
                .loops
                .pop()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if let Some((root, proof)) = inherited {
                for jump in &context.continues {
                    self.validate_readonly_while_exit(root, proof, &jump.exit, jump.span)?;
                }
            }
            self.finish_loop_continues(&context)?;
            let mut exits = context.breaks;
            exits.push(false_exit);
            let result = self.finish_loop_exits(
                statement,
                exits,
                &baseline,
                inherited,
                provider.as_ref(),
                span,
            )?;
            if let Some((root, proof)) = inherited {
                self.validate_readonly_while_exit(root, proof, &self.loop_exit(), span)?;
            }
            Ok(result)
        }

        fn restore_direct_while_condition(
            &mut self,
            state: &BranchExit,
            provider: Option<&crate::ssa::lower_frontend::outer_borrowed_while::Relation>,
            span: Span,
        ) -> Result<(), LoweringError> {
            if let Some(provider) = provider {
                self.restore_outer_borrowed_state(provider, state, span)?;
            } else if !state.for_sources.is_empty() || !state.for_elements.is_empty() {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            self.block = state.block;
            self.bindings.clone_from(&state.bindings);
            self.temporaries.clone_from(&state.temporaries);
            self.pending_call_loans.clone_from(&state.loans);
            self.capture_loans.clone_from(&state.capture_loans);
            self.borrow_bindings.clone_from(&state.borrow_bindings);
            self.result_source_loans
                .clone_from(&state.result_source_loans);
            self.non_null_bindings.clone_from(&state.views);
            Ok(())
        }

        fn emit_direct_while_condition_drops(
            &mut self,
            condition: ExpressionId,
            span: Span,
        ) -> Result<(), LoweringError> {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(condition)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            match *node.payload() {
                Expression::Group { expression } => {
                    self.emit_direct_while_condition_drops(expression, span)?
                }
                Expression::Binary { left, right, .. } => {
                    self.emit_null_condition_drops(left)?;
                    self.emit_null_condition_drops(right)?;
                    self.emit_drops(DropPoint::AfterBinaryOperands(condition))?;
                }
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            }
            self.emit_drops(DropPoint::AfterExpression(condition))
        }
    }

    mod entry {
        //! Qualification is the only way to retain an outer proof across a new direct guard.
        use crate::ssa::lower_frontend::loop_control::*;
    use crate::ssa::lower_frontend::*;

        pub(in crate::ssa) enum NullableWhileEntry {
            Readonly(SymbolId, NonNullProof),
            Direct(DirectNullableWhileEntry),
        }

        /// The fields stay private: a caller cannot supply an arbitrary inherited pair.
        pub(in crate::ssa) struct DirectNullableWhileEntry {
            statement: StatementId,
            descriptor: NullComparisonDescriptor,
            inherited: Option<(SymbolId, NonNullProof)>,
            provider: Option<Box<crate::ssa::lower_frontend::outer_borrowed_while::Relation>>,
        }

        impl DirectNullableWhileEntry {
            pub(in crate::ssa) fn provider(
                &self,
            ) -> Option<&crate::ssa::lower_frontend::outer_borrowed_while::Relation> {
                self.provider.as_deref()
            }
            pub(in crate::ssa) fn into_parts(
                self,
                statement: StatementId,
                span: Span,
            ) -> Result<(NullComparisonDescriptor, Option<(SymbolId, NonNullProof)>), LoweringError>
            {
                if self.statement != statement {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                Ok((self.descriptor, self.inherited))
            }
        }

        impl ExpressionLowerer<'_> {
            /// The real typed operand distinguishes an inherited read from a head kill.
            /// Qualify every transparent layer before retiring the current same-root view.
            fn prepare_nested_nullable_while(
                &mut self,
                descriptor: NullComparisonDescriptor,
                mut operand: ExpressionId,
                span: Span,
            ) -> Result<Option<(SymbolId, NonNullProof)>, LoweringError> {
                let root = descriptor.symbol();
                let Some(proof) = self.non_null_bindings.get(&root).copied() else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                };
                if self.non_null_bindings.len() != 1 || !proof.is_while_origin() {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                self.validate_non_null_proof(root, proof, &self.bindings, span)?;
                let declared = self.resolve_type(descriptor.nullable_type(), span)?;
                let nullable = self
                    .type_ids
                    .get(&declared)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if self.typed.copyability(declared) != Some(Copyability::MoveOnly)
                    || self.borrow_bindings.contains_key(&root)
                    || !matches!(
                        self.ssa_types.get(proof.inner.index()),
                        Some(SsaTypeKind::HeapOwner { .. })
                    )
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                if self.ssa_types.get(nullable.index())
                    != Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
                    || self
                        .function
                        .entity(EntityId::Value(proof.owner))
                        .map(|data| data.ty)
                        != Some(EntityType::Value(nullable))
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let storage = descriptor.nullable_type();
                let Some(TypeKind::Nullable(narrowed)) = self.typed.types().get(storage) else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let narrowed = *narrowed;
                let concrete_inner = self.resolve_type(narrowed, span)?;
                if self.type_ids.get(&concrete_inner) != Some(&proof.inner) {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                if self.typed.copyability(concrete_inner) != Some(Copyability::MoveOnly) {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let operand_type = self
                    .typed
                    .expression_type(operand)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if operand_type != narrowed && operand_type != storage {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let inherited = operand_type == narrowed;
                loop {
                    let node = self
                        .parsed
                        .ast()
                        .expressions()
                        .get(operand)
                        .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                    if self.typed.expression_type(operand) != Some(operand_type)
                        || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    match (inherited, self.typed.non_null_use(operand)) {
                        (true, Some(fact))
                            if fact.expression() == operand
                                && fact.symbol() == root
                                && fact.declared_type() == storage
                                && fact.narrowed_type() == narrowed => {}
                        (false, None) => {}
                        _ => return Err(error(LoweringErrorKind::MissingFact, span)),
                    }
                    match node.payload() {
                        Expression::Group { expression } => operand = *expression,
                        Expression::Name => {
                            if self.references.get(&span_key(node.span())) != Some(&root) {
                                return Err(error(LoweringErrorKind::MissingFact, span));
                            }
                            break;
                        }
                        _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
                    }
                }
                if inherited {
                    self.ensure_readonly_while_field_carriers(root, span)?;
                    return Ok(Some((root, proof)));
                }
                // Retire only the old view, before the inner baseline/header can capture it.
                // The nullable owner remains intact for the direct guard's independent birth.
                self.direct_nullable_while_entry(descriptor, None, span)?;
                self.end_non_null_proof(root, span)?;
                Ok(None)
            }

            pub(in crate::ssa) fn qualify_nullable_while_entry(
                &mut self,
                statement: StatementId,
                descriptor: NullComparisonDescriptor,
                operand: ExpressionId,
                span: Span,
            ) -> Result<NullableWhileEntry, LoweringError> {
                if let Some(provider) =
                    self.qualify_outer_borrowed_entry(statement, descriptor, operand, span)?
                {
                    return Ok(NullableWhileEntry::Direct(DirectNullableWhileEntry {
                        statement,
                        descriptor,
                        inherited: None,
                        provider: Some(Box::new(provider)),
                    }));
                }
                let inherited = if self.non_null_bindings.is_empty() {
                    None
                } else if self.non_null_bindings.contains_key(&descriptor.symbol()) {
                    if let Some((root, proof)) =
                        self.prepare_nested_nullable_while(descriptor, operand, span)?
                    {
                        return Ok(NullableWhileEntry::Readonly(root, proof));
                    }
                    None
                } else {
                    Some(self.qualify_distinct_while_outer(statement, descriptor, operand, span)?)
                };
                Ok(NullableWhileEntry::Direct(DirectNullableWhileEntry {
                    statement,
                    descriptor,
                    inherited,
                    provider: None,
                }))
            }

            fn qualify_distinct_while_outer(
                &self,
                statement: StatementId,
                descriptor: NullComparisonDescriptor,
                mut operand: ExpressionId,
                span: Span,
            ) -> Result<(SymbolId, NonNullProof), LoweringError> {
                if self.non_null_bindings.len() != 1 {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let (&outer, &proof) = self
                    .non_null_bindings
                    .first_key_value()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let Some((scope, birth)) = proof.while_origin() else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                };
                let root = descriptor.symbol();
                if root == outer
                    || statement == scope
                    || self.borrow_bindings.contains_key(&outer)
                    || self.borrow_bindings.contains_key(&root)
                    || !self.loops.iter().any(|context| {
                        matches!(
                            self.function.block(context.header)
                                .and_then(|block| block.terminator.as_ref())
                                .map(|terminator| &terminator.kind),
                            Some(TerminatorKind::NullableBranch { view, .. }) if *view == birth
                        )
                    })
                    || !matches!(
                        self.ssa_types.get(proof.inner.index()),
                        Some(SsaTypeKind::HeapOwner { .. })
                    )
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                self.validate_non_null_proof(outer, proof, &self.bindings, span)?;
                let outer_storage = self
                    .typed
                    .symbol_type(outer)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let outer_type = self.resolve_type(outer_storage, span)?;
                let outer_nullable = self
                    .type_ids
                    .get(&outer_type)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if self.typed.copyability(outer_type) != Some(Copyability::MoveOnly)
                    || self.ssa_types.get(outer_nullable.index())
                        != Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
                    || self
                        .function
                        .entity(EntityId::Value(proof.owner))
                        .map(|data| data.ty)
                        != Some(EntityType::Value(outer_nullable))
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                // The new root qualifies independently; its payload need not match the outer's.
                let (_, inner) = self.direct_nullable_while_entry(descriptor, None, span)?;
                let Some(LoweredValue::Value(owner)) = self.bindings.get(&root) else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                if *owner == proof.owner {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let storage = descriptor.nullable_type();
                let declared = self.resolve_type(storage, span)?;
                let Some(TypeKind::Nullable(narrowed)) = self.typed.types().get(storage) else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let narrowed = self.resolve_type(*narrowed, span)?;
                if self.typed.copyability(declared) != Some(Copyability::MoveOnly)
                    || self.typed.copyability(narrowed) != Some(Copyability::MoveOnly)
                    || self.type_ids.get(&narrowed) != Some(&inner)
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                loop {
                    let node = self
                        .parsed
                        .ast()
                        .expressions()
                        .get(operand)
                        .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                    if self.typed.expression_type(operand) != Some(storage)
                        || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
                        || self.typed.non_null_use(operand).is_some()
                    {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    match node.payload() {
                        Expression::Group { expression } => operand = *expression,
                        Expression::Name => {
                            if self.references.get(&span_key(node.span())) != Some(&root) {
                                return Err(error(LoweringErrorKind::MissingFact, span));
                            }
                            return Ok((outer, proof));
                        }
                        _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
                    }
                }
            }
        }
    }
}
