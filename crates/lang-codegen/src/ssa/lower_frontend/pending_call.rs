//! Pending call loans keep their optional, immutable nullable-field provenance.
use std::collections::BTreeMap;
use std::ops::Index;

use super::non_null_proof::ProofOrigin;
use super::*;

type Key = (usize, usize);

/// This identity is minted only at a validated SharedHeapFieldLoan instruction.
/// In particular, it does not contain a child LoanId from its birth block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PendingFieldChild {
    call: ExpressionId,
    argument: ExpressionId,
    root: SymbolId,
    parent: ProofOrigin,
    field: usize,
    target: SsaTypeId,
}

#[derive(Clone, Copy)]
struct PendingCallLoan {
    loan: Option<LoanId>,
    child: Option<PendingFieldChild>,
}

/// Snapshotting the existing pending carrier also snapshots its provenance.
#[derive(Clone, Default)]
pub(super) struct PendingCallLoans(BTreeMap<Key, PendingCallLoan>);

impl PendingCallLoans {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }
    pub(super) fn contains_key(&self, key: &Key) -> bool {
        self.0.contains_key(key)
    }
    pub(super) fn keys(&self) -> impl Iterator<Item = &Key> {
        self.0.keys()
    }
    pub(super) fn get(&self, key: &Key) -> Option<&Option<LoanId>> {
        self.0.get(key).map(|entry| &entry.loan)
    }
    pub(super) fn insert(&mut self, key: Key, loan: Option<LoanId>) -> Option<Option<LoanId>> {
        self.0
            .insert(key, PendingCallLoan { loan, child: None })
            .map(|entry| entry.loan)
    }
    pub(super) fn remove(&mut self, key: &Key) -> Option<Option<LoanId>> {
        self.0.remove(key).map(|entry| entry.loan)
    }
    pub(super) fn retain(&mut self, mut predicate: impl FnMut(&Key, &Option<LoanId>) -> bool) {
        self.0.retain(|key, entry| predicate(key, &entry.loan));
    }
    pub(super) fn iter(&self) -> impl Iterator<Item = (&Key, &Option<LoanId>)> {
        self.0.iter().map(|(key, entry)| (key, &entry.loan))
    }
    pub(super) fn child(&self, key: &Key) -> Option<PendingFieldChild> {
        self.0.get(key).and_then(|entry| entry.child)
    }
    pub(super) fn rebind(&mut self, key: Key, loan: LoanId, child: Option<PendingFieldChild>) {
        self.0.insert(
            key,
            PendingCallLoan {
                loan: Some(loan),
                child,
            },
        );
    }
}

impl FromIterator<(Key, Option<LoanId>)> for PendingCallLoans {
    fn from_iter<T: IntoIterator<Item = (Key, Option<LoanId>)>>(values: T) -> Self {
        Self(
            values
                .into_iter()
                .map(|(key, loan)| (key, PendingCallLoan { loan, child: None }))
                .collect(),
        )
    }
}

impl Index<&Key> for PendingCallLoans {
    type Output = Option<LoanId>;
    fn index(&self, key: &Key) -> &Self::Output {
        &self.0[key].loan
    }
}

impl ExpressionLowerer<'_> {
    /// Called only after the field producer emitted and checked its real child.
    /// Builtins (notably immediate println) never acquire pending provenance.
    pub(super) fn register_pending_field_child(
        &mut self,
        fact: &lang_frontend::ownership_checking::LoanFact,
        proof: NonNullProof,
        field: usize,
        target: SsaTypeId,
        loan: LoanId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let call = fact.call();
        let argument = fact.argument();
        let LoanTarget::Place(path) = fact.target() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let root = path.root();
        let descriptor = self
            .typed
            .call(call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(descriptor.target(), CallableTarget::Source(_))
            || descriptor.receiver().is_some()
            || descriptor.aborts()
            || descriptor.result_source()
                != lang_frontend::type_checking::CallableResultSource::Owned
        {
            return Ok(());
        }
        let key = (call.index(), argument.index());
        if self.pending_call_loans.contains_key(&key) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let child = PendingFieldChild {
            call,
            argument,
            root,
            parent: proof.origin,
            field,
            target,
        };
        self.pending_call_loans.rebind(key, loan, Some(child));
        self.validate_pending_field_children(
            &self.bindings,
            &self.non_null_bindings,
            &self.pending_call_loans,
            span,
        )
    }

    /// Only this ordinary source-call insertion can retain a producer's identity.
    pub(super) fn insert_pending_source_argument(
        &mut self,
        key: Key,
        loan: Option<LoanId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.pending_call_loans.child(&key).is_some() {
            if self.pending_call_loans.get(&key) != Some(&loan) || loan.is_none() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.validate_pending_field_children(
                &self.bindings,
                &self.non_null_bindings,
                &self.pending_call_loans,
                span,
            )?;
        } else {
            self.pending_call_loans.insert(key, loan);
        }
        Ok(())
    }

    pub(super) fn validate_pending_field_children(
        &self,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        views: &BTreeMap<SymbolId, NonNullProof>,
        pending: &PendingCallLoans,
        span: Span,
    ) -> Result<(), LoweringError> {
        for (&key, entry) in &pending.0 {
            let Some(child) = entry.child else { continue };
            let proof = views
                .get(&child.root)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let loan = entry
                .loan
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            self.validate_non_null_proof(child.root, proof, bindings, span)?;
            if key != (child.call.index(), child.argument.index())
                || proof.origin != child.parent
                || !proof.is_while_origin()
                || loan == proof.loan
                || self
                    .function
                    .entity(EntityId::Loan(loan))
                    .map(|data| data.ty)
                    != Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: child.target,
                    })
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let Some(crate::ssa::model::SsaTypeKind::HeapOwner {
                payload: Some(payload),
                ..
            }) = self.ssa_types.get(proof.inner.index())
            else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            if self.heap_payloads.get(&proof.inner) != Some(payload)
                || !matches!(self.ssa_types.get(payload.index()),
                    Some(crate::ssa::model::SsaTypeKind::Aggregate { fields, .. })
                        if fields.get(child.field) == Some(&child.target))
                || self.ssa_types.get(child.target.index())
                    != Some(&crate::ssa::model::SsaTypeKind::StringOwner)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }

    /// The sole relaxation is an already qualified readonly use of the same root.
    pub(super) fn ensure_readonly_while_field_carriers(
        &self,
        root: SymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.pending_call_loans.is_empty() {
            return self.ensure_no_unresolved_while_proof_carriers(span);
        }
        self.ensure_no_other_while_proof_carriers(span)?;
        if self.non_null_bindings.len() != 1
            || self.pending_call_loans.len() != 1
            || !self.temporaries.is_empty()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some((_, entry)) = self.pending_call_loans.0.first_key_value() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if entry.child.is_none_or(|child| child.root != root) || entry.loan.is_none() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let child = entry
            .child
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.pending_field_parameter(child, span)?.0 != 0 {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_pending_field_children(
            &self.bindings,
            &self.non_null_bindings,
            &self.pending_call_loans,
            span,
        )
    }

    pub(super) fn validate_pending_field_exits(
        &self,
        exits: &[super::control::BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(first) = exits.first() else {
            return Ok(());
        };
        let identities = |pending: &PendingCallLoans| {
            pending
                .0
                .iter()
                .filter_map(|(&key, entry)| entry.child.map(|child| (key, child)))
                .collect::<Vec<_>>()
        };
        let expected = identities(&first.loans);
        for exit in exits {
            if identities(&exit.loans) != expected {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.validate_pending_field_children(&exit.bindings, &exit.views, &exit.loans, span)?;
        }
        Ok(())
    }
}

/// Existing owner, parent and pending-child columns, with no extra SSA parameter.
#[derive(Clone, Copy)]
pub(super) struct PendingFieldCarry {
    child: PendingFieldChild,
    owner_column: usize,
    parent_column: usize,
}

impl ExpressionLowerer<'_> {
    pub(super) fn bind_pending_field_columns(
        &self,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        slots: &mut [super::control::LinearBindingSlot],
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_pending_field_children(
            bindings,
            &self.non_null_bindings,
            &self.pending_call_loans,
            span,
        )?;
        for (&key, entry) in &self.pending_call_loans.0 {
            let Some(child) = entry.child else { continue };
            self.ensure_readonly_while_field_carriers(child.root, span)?;
            let proof = self.non_null_bindings[&child.root];
            let owner_column = slots
                .iter()
                .position(|slot| {
                    slot.symbol == Some(child.root) && slot.source == EntityId::Value(proof.owner)
                })
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let parent_column = slots
                .iter()
                .position(|slot| {
                    slot.source == EntityId::Loan(proof.loan)
                        && slot.views.as_slice() == [(child.root, proof)]
                })
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let child_column = slots
                .iter()
                .position(|slot| {
                    slot.loans.as_slice() == [key]
                        && Some(slot.source) == entry.loan.map(EntityId::Loan)
                })
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if owner_column == parent_column
                || parent_column == child_column
                || owner_column == child_column
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            slots[child_column].field_child = Some(PendingFieldCarry {
                child,
                owner_column,
                parent_column,
            });
        }
        Ok(())
    }

    pub(super) fn restore_pending_field_columns(
        &mut self,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        slots: &[super::control::LinearBindingSlot],
        parameters: &[EntityId],
        span: Span,
    ) -> Result<(), LoweringError> {
        for (index, slot) in slots.iter().enumerate() {
            let Some(carried) = slot.field_child else {
                continue;
            };
            let child = carried.child;
            let key = (child.call.index(), child.argument.index());
            let proof = self
                .non_null_bindings
                .get(&child.root)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let Some(EntityId::Loan(loan)) = parameters.get(index).copied() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            if slot.loans.as_slice() != [key]
                || parameters.get(carried.owner_column) != Some(&EntityId::Value(proof.owner))
                || parameters.get(carried.parent_column) != Some(&EntityId::Loan(proof.loan))
                || self.pending_call_loans.get(&key) != Some(&Some(loan))
                || proof.origin != child.parent
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.pending_call_loans.rebind(key, loan, Some(child));
        }
        self.validate_pending_field_children(
            bindings,
            &self.non_null_bindings,
            &self.pending_call_loans,
            span,
        )
    }

    pub(super) fn validate_pending_field_edge(
        &self,
        carried: &super::control::LinearBindings,
        exit: &super::control::BranchExit,
        arguments: &[EntityId],
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_pending_field_loop_exit(carried, exit, span)?;
        for (index, slot) in carried.slots.iter().enumerate() {
            let Some(carried) = slot.field_child else {
                continue;
            };
            let child = carried.child;
            let key = (child.call.index(), child.argument.index());
            let proof = exit
                .views
                .get(&child.root)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if exit.loans.child(&key) != Some(child)
                || arguments.get(carried.owner_column) != Some(&EntityId::Value(proof.owner))
                || arguments.get(carried.parent_column) != Some(&EntityId::Loan(proof.loan))
                || arguments.get(index).copied()
                    != exit.loans.get(&key).copied().flatten().map(EntityId::Loan)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }

    pub(super) fn validate_pending_field_loop_exit(
        &self,
        carried: &super::control::LinearBindings,
        exit: &super::control::BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_pending_field_children(&exit.bindings, &exit.views, &exit.loans, span)?;
        let expected = carried
            .slots
            .iter()
            .filter_map(|slot| slot.field_child.map(|carry| carry.child))
            .collect::<Vec<_>>();
        let actual = exit
            .loans
            .0
            .values()
            .filter_map(|entry| entry.child)
            .collect::<Vec<_>>();
        if expected != actual {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn validate_pending_field_merge_columns(
        &self,
        exits: &[super::control::BranchExit],
        columns: &[Vec<EntityId>],
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_pending_field_exits(exits, span)?;
        let Some(first) = exits.first() else {
            return Ok(());
        };
        for (&key, entry) in &first.loans.0 {
            let Some(child) = entry.child else { continue };
            let mut identities = [Vec::new(), Vec::new(), Vec::new()];
            for exit in exits {
                let proof = exit
                    .views
                    .get(&child.root)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let loan = exit
                    .loans
                    .get(&key)
                    .copied()
                    .flatten()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                identities[0].push(EntityId::Value(proof.owner));
                identities[1].push(EntityId::Loan(proof.loan));
                identities[2].push(EntityId::Loan(loan));
            }
            // Each real incoming triple must be present after column deduplication.
            // The independent owner and both distinct loans cannot collapse together.
            let indexes = identities
                .iter()
                .map(|identity| columns.iter().position(|column| column == identity))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if indexes[0] == indexes[1] || indexes[1] == indexes[2] || indexes[0] == indexes[2] {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }
}

impl ExpressionLowerer<'_> {
    fn pending_field_parameter(
        &self,
        child: PendingFieldChild,
        span: Span,
    ) -> Result<(usize, usize), LoweringError> {
        let descriptor = self
            .typed
            .call(child.call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(child.call)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call { arguments, .. } = node.payload() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if !matches!(descriptor.target(), CallableTarget::Source(_))
            || descriptor.receiver().is_some()
            || descriptor.aborts()
            || descriptor.result_source()
                != lang_frontend::type_checking::CallableResultSource::Owned
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let argument_index = arguments
            .iter()
            .position(|argument| argument.value == child.argument)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mapping = descriptor
            .arguments()
            .iter()
            .find(|mapping| mapping.argument_index() == argument_index)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if mapping.mode() != ParameterMode::Borrow {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok((argument_index, mapping.parameter_index()))
    }

    pub(super) fn validate_pending_source_arguments(
        &self,
        call: ExpressionId,
        arguments: &[EntityId],
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_pending_field_children(
            &self.bindings,
            &self.non_null_bindings,
            &self.pending_call_loans,
            span,
        )?;
        for (&key, entry) in &self.pending_call_loans.0 {
            let Some(child) = entry.child.filter(|child| child.call == call) else {
                continue;
            };
            let (_, parameter) = self.pending_field_parameter(child, span)?;
            if arguments.get(parameter).copied()
                != self
                    .pending_call_loans
                    .get(&key)
                    .copied()
                    .flatten()
                    .map(EntityId::Loan)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }

    /// A caller must supply the exact key from its original frontend loan-end fact.
    pub(super) fn end_pending_source_loan(
        &mut self,
        key: Key,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_pending_field_children(
            &self.bindings,
            &self.non_null_bindings,
            &self.pending_call_loans,
            span,
        )?;
        let loan = self
            .pending_call_loans
            .get(&key)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if let Some(loan) = loan {
            self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.pending_call_loans.remove(&key);
        Ok(())
    }
}

#[cfg(test)]
mod guard_tests;
