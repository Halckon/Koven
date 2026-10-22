//! Nullable proof roles share access slots, while their owner uses the value prefix.
use super::super::nullable_proof::{NullableProof, NullableProofs};
use super::*;

#[derive(Clone)]
pub(super) struct CarriedNullableProof {
    pub(super) root: UnitSymbolId,
    pub(super) proof: NullableProof,
    pub(super) owner_column: Option<usize>,
}

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn nullable_owner_column(
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
    pub(in super::super) fn nullable_temporary_owner_column(
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
    pub(super) fn bind_nullable_columns(
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
    pub(super) fn validate_nullable_edge_columns(
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
                    || arguments.get(bindings.len() + index) != Some(&EntityId::Loan(actual.loan))
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
        }
        Ok(())
    }

    pub(super) fn carry_nullable_proofs(
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
    pub(super) fn rebind_carried_nullable_proofs(
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

    pub(super) fn validate_carried_nullable_edge(
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

    pub(super) fn validate_nullable_exit_proofs(
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
    pub(super) fn validate_nullable_merge_columns(
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

    pub(super) fn rebind_merged_nullable_proofs(
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
