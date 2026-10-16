//! Nullable proof roles share access slots, while their owner uses the value prefix.
use super::super::nullable_proof::{NullableProof, NullableProofs};
use super::*;

impl UnitExpressionLowerer<'_> {
    pub(super) fn carry_nullable_proofs(
        &self,
        carried: &mut Vec<CarriedAccess>,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        for (&root, &proof) in &self.nullable_proofs {
            let source = EntityId::Loan(proof.loan);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                // Share the physical entity slot, never collapse distinct root/scope roles.
                slot.nullable_proofs.push((root, proof));
            } else {
                carried.push(CarriedAccess {
                    nullable_proofs: vec![(root, proof)],
                    range_sources: Vec::new(),
                    captures: Vec::new(),
                    symbol: None,
                    receiver: None,
                    source,
                    ty: EntityType::Loan {
                        kind: crate::ssa::model::LoanKind::Shared,
                        target: proof.inner,
                    },
                    pending: Vec::new(),
                });
            }
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
        let incoming = self
            .function
            .blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .flat_map(|terminator| transport_edges(&terminator.kind))
            .filter(|edge| edge.target == block)
            .collect::<Vec<_>>();
        for (index, (slot, parameter)) in loans
            .iter()
            .zip(parameters.iter().skip(bindings.len()))
            .enumerate()
        {
            for &(root, proof) in &slot.nullable_proofs {
                let owner_slot = bindings
                    .iter()
                    .position(|binding| {
                        binding.symbol == Some(root) && binding.source == proof.owner
                    })
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let (Some(EntityId::Value(owner)), EntityId::Loan(loan)) =
                    (parameters.get(owner_slot), parameter)
                else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                if incoming.is_empty()
                    || slot.source != EntityId::Loan(proof.loan)
                    || proofs
                        .insert(
                            root,
                            NullableProof {
                                owner: *owner,
                                loan: *loan,
                                ..proof
                            },
                        )
                        .is_some()
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                // Loop templates retain preheader IDs; the actual predecessor edge carries
                // the current owner/proof pair. Validate that pair at the same parameter slots.
                for edge in &incoming {
                    let (Some(EntityId::Value(owner)), Some(EntityId::Loan(loan))) = (
                        edge.arguments.get(owner_slot),
                        edge.arguments.get(bindings.len() + index),
                    ) else {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    };
                    self.validate_nullable_proofs(
                        &BTreeMap::from([(root, LoweredValue::Value(*owner))]),
                        &BTreeMap::from([(
                            root,
                            NullableProof {
                                owner: *owner,
                                loan: *loan,
                                ..proof
                            },
                        )]),
                        span,
                    )?;
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
                .map(|(root, _)| *root)
                .collect::<BTreeSet<_>>()
                .len()
                != roles.len()
            || roles.iter().any(|(root, expected)| {
                state.nullable_proofs.get(root).is_none_or(|actual| {
                    actual.scope != expected.scope || actual.inner != expected.inner
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
                        actual.scope != expected.scope || actual.inner != expected.inner
                    })
                })
            {
                // Proof retirement/join intersection is outside this transport-only slice.
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
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

fn transport_edges(terminator: &TerminatorKind) -> Vec<&Edge> {
    match terminator {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        TerminatorKind::Return { .. }
        | TerminatorKind::BorrowReturn { .. }
        | TerminatorKind::RangeReturn { .. }
        | TerminatorKind::Abort => Vec::new(),
    }
}
