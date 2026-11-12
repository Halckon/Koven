//! CFG rebinding, including one exactly typed nullable proof suffix.
use super::*;
use crate::ssa::{
    carry::{AliasSlot, IncomingRole, SourceSlot, Temporary},
    model::{LoanKind, SsaTypeId},
};

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn rebind_carried_control(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        bindings: &[CarriedBinding],
        loans: &[CarriedAccess],
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        self.rebind_carried_control_suffix(baseline, block, bindings, loans, 0, span)
    }

    pub(in super::super) fn rebind_carried_loans(
        &mut self,
        block: BlockId,
        binding_count: usize,
        loans: &[CarriedAccess],
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoanId>, LoweringError> {
        self.rebind_carried_loans_suffix(block, binding_count, loans, 0, span)
    }

    pub(in super::super) fn add_map_borrow_block(
        &mut self,
        bindings: &[CarriedBinding],
        loans: &[CarriedAccess],
        inner: SsaTypeId,
        span: Span,
    ) -> Result<(BlockId, LoanId), LoweringError> {
        let mut types = bindings
            .iter()
            .map(|slot| slot.ty)
            .chain(loans.iter().map(|slot| slot.ty))
            .collect::<Vec<_>>();
        types.push(EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        });
        let block = self
            .function
            .add_block(types, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let Some(EntityId::Loan(proof)) = self
            .function
            .block(block)
            .and_then(|block| block.parameters.last())
            .copied()
        else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok((block, proof))
    }

    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn rebind_map_borrow_control(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        bindings: &[CarriedBinding],
        loans: &[CarriedAccess],
        proof: LoanId,
        inner: SsaTypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() != bindings.len() + loans.len() + 1
            || parameters.last() != Some(&EntityId::Loan(proof))
            || self
                .function
                .entity(EntityId::Loan(proof))
                .map(|data| data.ty)
                != Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                })
        {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        self.bindings =
            self.rebind_carried_control_suffix(baseline, block, bindings, loans, 1, span)?;
        self.borrow_bindings =
            self.rebind_carried_loans_suffix(block, bindings.len(), loans, 1, span)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn rebind_carried_control_suffix(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        bindings: &[CarriedBinding],
        loans: &[CarriedAccess],
        suffix: usize,
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        let rebound = self.rebind_carried_prefix(
            baseline,
            block,
            bindings,
            bindings.len() + loans.len() + suffix,
            span,
        )?;
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        let mut pending = BTreeMap::new();
        for (indices, parameter) in bindings
            .iter()
            .map(|slot| &slot.pending)
            .chain(loans.iter().map(|slot| &slot.pending))
            .zip(parameters)
        {
            for index in indices {
                pending.insert(*index, *parameter);
            }
        }
        if pending.keys().copied().ne(0..pending.len()) {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        self.pending_operands = pending.into_values().collect();
        self.temporaries.clear();
        for (slot, parameter) in bindings.iter().zip(parameters) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            carry::apply_alias_column(&slot.temporaries, *value, &mut self.temporaries);
        }
        self.rebind_carried_nullable_proofs(block, &rebound, bindings, loans, span)?;
        Ok(rebound)
    }

    fn rebind_carried_prefix(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        carried: &[CarriedBinding],
        expected_parameter_count: usize,
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() != expected_parameter_count {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        let mut rebound = baseline.clone();
        for (slot, parameter) in carried.iter().zip(parameters) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            if let Some(symbol) = slot.symbol {
                rebound.insert(symbol, LoweredValue::Value(*value));
            } else if let Some(mut receiver) = slot.receiver {
                receiver.entity = EntityId::Value(*value);
                self.current_receiver = Some(receiver);
                self.consumed_receiver = None;
            }
        }
        Ok(rebound)
    }

    fn rebind_carried_loans_suffix(
        &mut self,
        block: BlockId,
        binding_count: usize,
        loans: &[CarriedAccess],
        suffix: usize,
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoanId>, LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() != binding_count + loans.len() + suffix {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        let mut rebound = BTreeMap::new();
        for (slot, parameter) in loans.iter().zip(parameters.iter().skip(binding_count)) {
            if slot.symbol.is_none() && slot.receiver.is_none() {
                continue;
            }
            let EntityId::Loan(loan) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            if let Some(symbol) = slot.symbol {
                rebound.insert(symbol, *loan);
            } else if let Some(mut receiver) = slot.receiver {
                receiver.entity = EntityId::Loan(*loan);
                self.current_receiver = Some(receiver);
                self.consumed_receiver = None;
            }
        }
        Ok(rebound)
    }
}

impl SourceSlot for CarriedBinding {
    type Source = ValueId;

    fn source(&self) -> ValueId {
        self.source
    }

    fn entity(source: ValueId) -> EntityId {
        EntityId::Value(source)
    }

    fn ty(&self) -> EntityType {
        self.ty
    }

    fn empty(source: ValueId, ty: EntityType) -> Self {
        Self {
            symbol: None,
            receiver: None,
            source,
            ty,
            pending: Vec::new(),
            temporaries: Vec::new(),
        }
    }
}

impl SourceSlot for CarriedAccess {
    type Source = EntityId;

    fn source(&self) -> EntityId {
        self.source
    }

    fn entity(source: EntityId) -> EntityId {
        source
    }

    fn ty(&self) -> EntityType {
        self.ty
    }

    fn empty(source: EntityId, ty: EntityType) -> Self {
        Self {
            nullable_proofs: Vec::new(),
            symbol: None,
            receiver: None,
            source,
            ty,
            pending: Vec::new(),
        }
    }
}

impl AliasSlot<Temporary, UnitExpressionId> for CarriedBinding {
    fn alias_source(source: ValueId) -> ValueId {
        source
    }

    fn keys_mut(&mut self) -> &mut Vec<UnitExpressionId> {
        &mut self.temporaries
    }
}

#[derive(Clone, Copy)]
pub(super) enum MergeRole {
    Result,
    Receiver,
    NamedValue(UnitSymbolId),
    NamedBorrow(UnitSymbolId),
    Pending(usize),
    Temporary(UnitExpressionId),
    NullableProof(UnitSymbolId),
}

pub(super) struct MergeProjection {
    pub(super) role: MergeRole,
    pub(super) span: Span,
}

impl IncomingRole<BranchExit> for MergeProjection {
    fn current(&self, exit: &BranchExit) -> Result<EntityId, LoweringError> {
        let missing = || lowering_error(LoweringErrorKind::MissingFact, self.span);
        let entity = match self.role {
            MergeRole::Result => match exit.result {
                LoweredValue::Value(value) => EntityId::Value(value),
                LoweredValue::Unit | LoweredValue::Diverged => return Err(missing()),
            },
            MergeRole::Receiver => exit.receiver.ok_or_else(missing)?.entity,
            MergeRole::NamedValue(symbol) => match exit.bindings[&symbol] {
                LoweredValue::Value(value) => EntityId::Value(value),
                LoweredValue::Unit | LoweredValue::Diverged => return Err(missing()),
            },
            MergeRole::NamedBorrow(symbol) => EntityId::Loan(exit.borrow_bindings[&symbol]),
            MergeRole::Pending(index) => exit.pending_operands[index],
            MergeRole::Temporary(key) => EntityId::Value(exit.temporaries[&key]),
            MergeRole::NullableProof(symbol) => EntityId::Loan(exit.nullable_proofs[&symbol].loan),
        };
        Ok(entity)
    }
}

pub(in crate::ssa::unit_lower) mod nullable {
    //! Nullable proof roles share access slots, while their owner uses the value prefix.
    use super::super::super::nullable_proof::{NullableProof, NullableProofs};
    use super::super::*;

    #[derive(Clone)]
    pub(in crate::ssa::unit_lower) struct CarriedNullableProof {
        pub(in crate::ssa::unit_lower) root: UnitSymbolId,
        pub(in crate::ssa::unit_lower) proof: NullableProof,
        pub(in crate::ssa::unit_lower) owner_column: Option<usize>,
    }

    impl UnitExpressionLowerer<'_> {
        pub(in crate::ssa::unit_lower) fn nullable_owner_column(
            &self,
            bindings: &[CarriedBinding],
            root: UnitSymbolId,
            owner: ValueId,
            span: Span,
        ) -> Result<usize, LoweringError> {
            bindings
                .iter()
                .position(|slot| slot.symbol == Some(root) && slot.source == owner)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }

        /// Locate the real temporary alias column, never a synthetic root binding.
        pub(in crate::ssa::unit_lower) fn nullable_temporary_owner_column(
            &self,
            bindings: &[CarriedBinding],
            subject: UnitExpressionId,
            owner: ValueId,
            span: Span,
        ) -> Result<usize, LoweringError> {
            let mut matches = bindings
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.source == owner && slot.temporaries.contains(&subject));
            let (column, slot) = matches
                .next()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if matches.next().is_some()
                || slot.symbol.is_some()
                || self.temporaries.get(&subject) != Some(&owner)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            Ok(column)
        }

        /// Finalize roles from the real state after all value-prefix slots are present.
        /// Raw source edges can subsequently use this same checked layout unchanged.
        pub(in crate::ssa::unit_lower) fn bind_nullable_columns(
            &self,
            bindings: &[CarriedBinding],
            loans: &mut [CarriedAccess],
            span: Span,
        ) -> Result<(), LoweringError> {
            self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
            let roots = loans
                .iter()
                .flat_map(|slot| &slot.nullable_proofs)
                .map(|role| role.root)
                .collect::<Vec<_>>();
            if roots.len() != self.nullable_proofs.len()
                || roots.iter().copied().collect::<BTreeSet<_>>()
                    != self.nullable_proofs.keys().copied().collect()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            for slot in loans.iter_mut() {
                for role in &mut slot.nullable_proofs {
                    if self.nullable_proofs.get(&role.root) != Some(&role.proof) {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                    role.owner_column = Some(self.nullable_owner_column(
                        bindings,
                        role.root,
                        role.proof.owner,
                        span,
                    )?);
                }
            }
            self.validate_nullable_layout(bindings, loans, span)
        }

        fn validate_nullable_layout(
            &self,
            bindings: &[CarriedBinding],
            loans: &[CarriedAccess],
            span: Span,
        ) -> Result<(), LoweringError> {
            for slot in loans {
                for role in &slot.nullable_proofs {
                    let owner = role.owner_column.and_then(|index| bindings.get(index));
                    if slot.source != EntityId::Loan(role.proof.loan)
                        || owner.is_none_or(|owner| {
                            owner.symbol != Some(role.root) || owner.source != role.proof.owner
                        })
                    {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                }
            }
            Ok(())
        }

        /// Backedges and other state-derived edges check the actual pair at both columns.
        pub(in crate::ssa::unit_lower) fn validate_nullable_edge_columns(
            &self,
            bindings: &[CarriedBinding],
            loans: &[CarriedAccess],
            state: &BranchExit,
            arguments: &[EntityId],
            span: Span,
        ) -> Result<(), LoweringError> {
            self.validate_nullable_layout(bindings, loans, span)?;
            for (index, slot) in loans.iter().enumerate() {
                for role in &slot.nullable_proofs {
                    let actual = state
                        .nullable_proofs
                        .get(&role.root)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    if role.owner_column.and_then(|column| arguments.get(column))
                        != Some(&EntityId::Value(actual.owner))
                        || arguments.get(bindings.len() + index)
                            != Some(&EntityId::Loan(actual.loan))
                    {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                }
            }
            Ok(())
        }

        pub(in crate::ssa::unit_lower) fn carry_nullable_proofs(
            &self,
            carried: &mut Vec<CarriedAccess>,
            span: Span,
        ) -> Result<(), LoweringError> {
            self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
            for (&root, &proof) in &self.nullable_proofs {
                let source = EntityId::Loan(proof.loan);
                let choice = carry::associate_source(
                    self.function,
                    carried,
                    source,
                    TypeAtMiss::Known(EntityType::Loan {
                        kind: crate::ssa::model::LoanKind::Shared,
                        target: proof.inner,
                    }),
                    lowering_error(LoweringErrorKind::MissingFact, span),
                )?;
                // Share the physical entity slot, never collapse distinct root/scope roles.
                carried[choice.column().index()]
                    .nullable_proofs
                    .push(CarriedNullableProof {
                        root,
                        proof,
                        owner_column: None,
                    });
            }
            Ok(())
        }

        /// The complete owner prefix must be rebound before reconstructing proof records.
        pub(in crate::ssa::unit_lower) fn rebind_carried_nullable_proofs(
            &mut self,
            block: BlockId,
            rebound: &BTreeMap<UnitSymbolId, LoweredValue>,
            bindings: &[CarriedBinding],
            loans: &[CarriedAccess],
            span: Span,
        ) -> Result<(), LoweringError> {
            if loans.iter().all(|slot| slot.nullable_proofs.is_empty()) {
                self.nullable_proofs.clear();
                return Ok(());
            }
            let parameters = &self
                .function
                .block(block)
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
                .parameters;
            let mut proofs = NullableProofs::new();
            self.validate_nullable_layout(bindings, loans, span)?;
            for (slot, parameter) in loans.iter().zip(parameters.iter().skip(bindings.len())) {
                for role in &slot.nullable_proofs {
                    let owner_column = role
                        .owner_column
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    let (Some(EntityId::Value(owner)), EntityId::Loan(loan)) =
                        (parameters.get(owner_column), parameter)
                    else {
                        return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                    };
                    if proofs
                        .insert(
                            role.root,
                            NullableProof {
                                owner: *owner,
                                loan: *loan,
                                ..role.proof
                            },
                        )
                        .is_some()
                    {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                }
            }
            self.validate_nullable_proofs(rebound, &proofs, span)?;
            self.nullable_proofs = proofs;
            Ok(())
        }

        pub(in crate::ssa::unit_lower) fn validate_carried_nullable_edge(
            &self,
            loans: &[CarriedAccess],
            state: &BranchExit,
            span: Span,
        ) -> Result<(), LoweringError> {
            self.validate_nullable_proofs(&state.bindings, &state.nullable_proofs, span)?;
            let roles = loans
                .iter()
                .flat_map(|slot| &slot.nullable_proofs)
                .collect::<Vec<_>>();
            if roles.len() != state.nullable_proofs.len()
                || roles
                    .iter()
                    .map(|role| role.root)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != roles.len()
                || roles.iter().any(|role| {
                    state.nullable_proofs.get(&role.root).is_none_or(|actual| {
                        actual.origin != role.proof.origin || actual.inner != role.proof.inner
                    })
                })
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            Ok(())
        }

        pub(in crate::ssa::unit_lower) fn validate_nullable_exit_proofs(
            &self,
            exits: &[BranchExit],
            span: Span,
        ) -> Result<(), LoweringError> {
            let Some(first) = exits.first() else {
                return Ok(());
            };
            for exit in exits {
                self.validate_nullable_proofs(&exit.bindings, &exit.nullable_proofs, span)?;
                if exit.nullable_proofs.keys().ne(first.nullable_proofs.keys())
                    || first.nullable_proofs.iter().any(|(root, expected)| {
                        exit.nullable_proofs.get(root).is_none_or(|actual| {
                            actual.origin != expected.origin || actual.inner != expected.inner
                        })
                    })
                {
                    // A missing side or a different birth needs a separate proof-intersection contract.
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            Ok(())
        }

        /// Prove both merge columns against each real exit, after all-input alias dedup.
        pub(in crate::ssa::unit_lower) fn validate_nullable_merge_columns(
            &self,
            exits: &[BranchExit],
            arguments: &[Vec<EntityId>],
            owners: &[UnitSymbolId],
            owner_offset: usize,
            proof_columns: &[usize],
            span: Span,
        ) -> Result<(), LoweringError> {
            for (exit, arguments) in exits.iter().zip(arguments) {
                if exit.nullable_proofs.len() != proof_columns.len() {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                for ((root, proof), column) in exit.nullable_proofs.iter().zip(proof_columns) {
                    let owner = owners
                        .iter()
                        .position(|candidate| candidate == root)
                        .and_then(|index| arguments.get(owner_offset + index));
                    if owner != Some(&EntityId::Value(proof.owner))
                        || arguments.get(*column) != Some(&EntityId::Loan(proof.loan))
                    {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                }
            }
            Ok(())
        }

        pub(in crate::ssa::unit_lower) fn rebind_merged_nullable_proofs(
            &self,
            first: &BranchExit,
            bindings: &BTreeMap<UnitSymbolId, LoweredValue>,
            mut parameters: impl Iterator<Item = EntityId>,
            span: Span,
        ) -> Result<NullableProofs, LoweringError> {
            let mut proofs = NullableProofs::new();
            for (&root, &proof) in &first.nullable_proofs {
                let (Some(LoweredValue::Value(owner)), Some(EntityId::Loan(loan))) =
                    (bindings.get(&root), parameters.next())
                else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                proofs.insert(
                    root,
                    NullableProof {
                        owner: *owner,
                        loan,
                        ..proof
                    },
                );
            }
            if parameters.next().is_some() {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            }
            self.validate_nullable_proofs(bindings, &proofs, span)?;
            Ok(proofs)
        }
    }
}
