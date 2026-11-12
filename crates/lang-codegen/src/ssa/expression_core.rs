//! Shared expression algorithms; entry adapters retain identity and stateful submission.
pub(in crate::ssa) mod checked;

use super::model::ValueId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa) enum LoweredValue {
    Unit,
    Value(ValueId),
    Diverged,
}

#[cfg(test)]
pub(in crate::ssa) mod pending_call {
    //! Pending call loans keep their optional, immutable nullable-field provenance.
    use std::collections::BTreeMap;
    use std::ops::Index;

    use crate::ssa::lower_frontend::non_null_proof::ProofOrigin;
    use crate::ssa::lower_frontend::*;

    type Key = (usize, usize);

    /// This identity is minted only at a validated SharedHeapFieldLoan instruction.
    /// In particular, it does not contain a child LoanId from its birth block.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::ssa) struct PendingFieldChild {
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
    pub(in crate::ssa) struct PendingCallLoans(BTreeMap<Key, PendingCallLoan>);

    impl PendingCallLoans {
        pub(in crate::ssa) fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        pub(in crate::ssa) fn len(&self) -> usize {
            self.0.len()
        }
        pub(in crate::ssa) fn contains_key(&self, key: &Key) -> bool {
            self.0.contains_key(key)
        }
        pub(in crate::ssa) fn keys(&self) -> impl Iterator<Item = &Key> {
            self.0.keys()
        }
        pub(in crate::ssa) fn get(&self, key: &Key) -> Option<&Option<LoanId>> {
            self.0.get(key).map(|entry| &entry.loan)
        }
        pub(in crate::ssa) fn insert(
            &mut self,
            key: Key,
            loan: Option<LoanId>,
        ) -> Option<Option<LoanId>> {
            self.0
                .insert(key, PendingCallLoan { loan, child: None })
                .map(|entry| entry.loan)
        }
        pub(in crate::ssa) fn remove(&mut self, key: &Key) -> Option<Option<LoanId>> {
            self.0.remove(key).map(|entry| entry.loan)
        }
        pub(in crate::ssa) fn retain(
            &mut self,
            mut predicate: impl FnMut(&Key, &Option<LoanId>) -> bool,
        ) {
            self.0.retain(|key, entry| predicate(key, &entry.loan));
        }
        pub(in crate::ssa) fn iter(&self) -> impl Iterator<Item = (&Key, &Option<LoanId>)> {
            self.0.iter().map(|(key, entry)| (key, &entry.loan))
        }
        pub(in crate::ssa) fn child(&self, key: &Key) -> Option<PendingFieldChild> {
            self.0.get(key).and_then(|entry| entry.child)
        }
        pub(in crate::ssa) fn rebind(
            &mut self,
            key: Key,
            loan: LoanId,
            child: Option<PendingFieldChild>,
        ) {
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
        pub(in crate::ssa) fn register_pending_field_child(
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
        pub(in crate::ssa) fn insert_pending_source_argument(
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

        pub(in crate::ssa) fn validate_pending_field_children(
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
        pub(in crate::ssa) fn ensure_readonly_while_field_carriers(
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

        pub(in crate::ssa) fn validate_pending_field_exits(
            &self,
            exits: &[crate::ssa::lower_frontend::control::BranchExit],
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
                self.validate_pending_field_children(
                    &exit.bindings,
                    &exit.views,
                    &exit.loans,
                    span,
                )?;
            }
            Ok(())
        }
    }

    /// Existing owner, parent and pending-child columns, with no extra SSA parameter.
    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct PendingFieldCarry {
        child: PendingFieldChild,
        owner_column: usize,
        parent_column: usize,
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn bind_pending_field_columns(
            &self,
            bindings: &BTreeMap<SymbolId, LoweredValue>,
            slots: &mut [crate::ssa::lower_frontend::control::LinearBindingSlot],
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
                        slot.symbol == Some(child.root)
                            && slot.source == EntityId::Value(proof.owner)
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

        pub(in crate::ssa) fn restore_pending_field_columns(
            &mut self,
            bindings: &BTreeMap<SymbolId, LoweredValue>,
            slots: &[crate::ssa::lower_frontend::control::LinearBindingSlot],
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

        pub(in crate::ssa) fn validate_pending_field_edge(
            &self,
            carried: &crate::ssa::lower_frontend::control::LinearBindings,
            exit: &crate::ssa::lower_frontend::control::BranchExit,
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

        pub(in crate::ssa) fn validate_pending_field_loop_exit(
            &self,
            carried: &crate::ssa::lower_frontend::control::LinearBindings,
            exit: &crate::ssa::lower_frontend::control::BranchExit,
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

        pub(in crate::ssa) fn validate_pending_field_merge_columns(
            &self,
            exits: &[crate::ssa::lower_frontend::control::BranchExit],
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
                if indexes[0] == indexes[1] || indexes[1] == indexes[2] || indexes[0] == indexes[2]
                {
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

        pub(in crate::ssa) fn validate_pending_source_arguments(
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
        pub(in crate::ssa) fn end_pending_source_loan(
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

}

#[cfg(test)]
mod non_null_proof_guard_tests {
    //! CI-only private guards start with the ordinary pending-call source and real SSA.
    use crate::ssa::lower_frontend::non_null_proof::*;
    use crate::ssa::lower_frontend::*;
    use crate::ssa::{
        lower_frontend::cleanup_guard_harness::Fixture,
        model::{Function, TerminatorKind},
        while_flow_tests::proof_cases::pending_child_source,
    };
    use lang_frontend::parser::Statement;

    fn missing<T>(result: Result<T, LoweringError>, span: Span) {
        let Err(failure) = result else {
            panic!("guard accepted corrupted proof")
        };
        assert_eq!(failure.kind, LoweringErrorKind::MissingFact);
        assert_eq!(failure.span, Some(span));
    }

    fn proof_at(function: &Function, block: crate::ssa::model::BlockId, seed: NonNullProof) -> NonNullProof {
        let parameters = &function.block(block).unwrap().parameters;
        let owner_type = function.entity(EntityId::Value(seed.owner)).unwrap().ty;
        let loan_type = function.entity(EntityId::Loan(seed.loan)).unwrap().ty;
        let owners = parameters
            .iter()
            .filter(|entity| function.entity(**entity).unwrap().ty == owner_type)
            .copied()
            .collect::<Vec<_>>();
        let loans = parameters
            .iter()
            .filter(|entity| function.entity(**entity).unwrap().ty == loan_type)
            .copied()
            .collect::<Vec<_>>();
        let [EntityId::Value(owner)] = owners.as_slice() else {
            panic!("one current owner column")
        };
        let [EntityId::Loan(loan)] = loans.as_slice() else {
            panic!("one current proof column")
        };
        NonNullProof {
            owner: *owner,
            loan: *loan,
            ..seed
        }
    }

    #[test]
    fn single_non_null_proof_guards_preserve_binding_and_merge_origin() {
        let fixture = Fixture::analyze(&pending_child_source());
        let baseline = fixture.entry();
        let root = fixture.symbol("current");
        let whiles = fixture
            .parsed
            .ast()
            .statements()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), Statement::While { .. }).then_some((id, node.span()))
            })
            .collect::<Vec<_>>();
        assert_eq!(whiles.len(), 2);
        let header = baseline
            .blocks
            .iter()
            .find(|block| {
                matches!(
                    block.terminator.as_ref().map(|term| &term.kind),
                    Some(TerminatorKind::NullableBranch { .. })
                )
            })
            .unwrap();
        let term = header.terminator.as_ref().unwrap();
        let (scope, span) = *whiles
            .iter()
            .find(|(_, span)| term.origin == crate::ssa::model::Origin::Source(*span))
            .unwrap();
        let TerminatorKind::NullableBranch {
            owner,
            view,
            when_non_null,
            ..
        } = &term.kind
        else {
            unreachable!()
        };
        let column = when_non_null
            .arguments
            .iter()
            .position(|entity| *entity == EntityId::Value(*owner))
            .unwrap();
        let EntityId::Value(body_owner) =
            baseline.block(when_non_null.target).unwrap().parameters[column]
        else {
            unreachable!()
        };
        let EntityType::Loan { target: inner, .. } = baseline.entity(EntityId::Loan(*view)).unwrap().ty
        else {
            unreachable!()
        };
        let seed = NonNullProof::introduced(*view, body_owner, inner, ProofScope::While(scope));
        let call = baseline
            .instructions
            .iter()
            .find(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
            .unwrap();
        let current = proof_at(baseline, call.block, seed);
        assert_ne!(current.owner, seed.owner);
        assert_ne!(current.loan, seed.loan);
        let incoming = baseline
            .blocks
            .iter()
            .filter_map(
                |block| match block.terminator.as_ref().map(|term| &term.kind) {
                    Some(TerminatorKind::Branch(edge)) if edge.target == call.block => {
                        Some((block.id, edge.clone()))
                    }
                    _ => None,
                },
            )
            .collect::<Vec<_>>();
        assert_eq!(
            incoming.len(),
            2,
            "the real argument-if has two merge predecessors"
        );

        let exits_at = |lowerer: &ExpressionLowerer<'_>| {
            incoming
                .iter()
                .map(|(block, edge)| {
                    let parameters = &lowerer.function.block(call.block).unwrap().parameters;
                    let operand = |entity| {
                        edge.arguments[parameters
                            .iter()
                            .position(|parameter| *parameter == entity)
                            .unwrap()]
                    };
                    let EntityId::Value(owner) = operand(EntityId::Value(current.owner)) else {
                        unreachable!()
                    };
                    let EntityId::Loan(loan) = operand(EntityId::Loan(current.loan)) else {
                        unreachable!()
                    };
                    let mut exit = lowerer.branch_exit(LoweredValue::Unit);
                    exit.block = *block;
                    exit.bindings.insert(root, LoweredValue::Value(owner));
                    exit.views.insert(
                        root,
                        NonNullProof {
                            owner,
                            loan,
                            ..seed
                        },
                    );
                    exit
                })
                .collect::<Vec<_>>()
        };

        let mut function = baseline.clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            lowerer.block = call.block;
            lowerer
                .bindings
                .insert(root, LoweredValue::Value(current.owner));
            lowerer.non_null_bindings.insert(root, current);
            assert_eq!(
                lowerer.checked_non_null_proof(root, inner, span).unwrap(),
                Some(current)
            );
            let exits = exits_at(lowerer);
            lowerer.validate_non_null_exit_proofs(&exits, span).unwrap();
            let merged = lowerer
                .merged_non_null_proofs(
                    &lowerer.bindings,
                    vec![(root, current.loan, exits[0].views[&root])],
                    span,
                )
                .unwrap();
            assert_eq!(merged[&root], current);
        });

        for mutation in 0..5 {
            let mut function = baseline.clone();
            fixture.with_lowerer(&mut function, |lowerer| {
                let mut bindings = BTreeMap::from([(root, LoweredValue::Value(current.owner))]);
                let mut proof = current;
                match mutation {
                    0 => {
                        bindings.insert(root, LoweredValue::Value(seed.owner));
                    }
                    1 => {
                        bindings.remove(&root);
                    }
                    2 => {
                        proof.owner = seed.owner;
                    }
                    3 => {
                        proof.inner = lowerer
                            .function
                            .entity(EntityId::Value(current.owner))
                            .unwrap()
                            .ty
                            .semantic_type();
                    }
                    4 => {
                        proof.loan = lowerer.function.blocks[0]
                            .parameters
                            .iter()
                            .find_map(|entity| match entity {
                                EntityId::Loan(loan) if *loan != current.loan => Some(*loan),
                                _ => None,
                            })
                            .unwrap();
                    }
                    _ => unreachable!(),
                }
                missing(
                    lowerer.validate_non_null_proof(root, proof, &bindings, span),
                    span,
                );
            });
        }
        for change_birth in [false, true] {
            let mut function = baseline.clone();
            fixture.with_lowerer(&mut function, |lowerer| {
                lowerer.block = call.block;
                lowerer
                    .bindings
                    .insert(root, LoweredValue::Value(current.owner));
                lowerer.non_null_bindings.insert(root, current);
                let mut exits = exits_at(lowerer);
                let changed = exits[1].views.get_mut(&root).unwrap();
                if change_birth {
                    changed.origin.birth = current.loan;
                } else {
                    changed.origin.introduced_by =
                        ProofScope::While(whiles.iter().find(|(id, _)| *id != scope).unwrap().0);
                }
                let changed = *changed;
                // Basic binding/type validation deliberately does not claim birth validation.
                lowerer
                    .validate_non_null_proof(root, changed, &exits[1].bindings, span)
                    .unwrap();
                missing(lowerer.validate_non_null_exit_proofs(&exits, span), span);
            });
        }
    }
}
