//! Inout instance receivers reserve before arguments and activate only at call entry.
use super::loan::{ActiveLoan, ActiveLoanOwner, ActiveLoanTarget};
use super::{AccessKind, Checker, Flows, OwnershipCheckingError, State};
use crate::{
    ast::ExpressionId,
    name_resolution::SymbolKind,
    ownership_checking::{LoanFact, LoanKind, LoanTarget},
    source::Span,
    type_checking::{NominalId, ParameterMode, TypeId, TypeKind},
};

impl Checker<'_> {
    pub(super) fn receiver_nominal(&self, mut ty: TypeId) -> Option<NominalId> {
        loop {
            match self.typed.types().get(ty)? {
                TypeKind::Nominal { nominal, .. } => return Some(*nominal),
                TypeKind::Nullable(inner) | TypeKind::StaticSelf(inner) => ty = *inner,
                _ => return None,
            }
        }
    }

    pub(super) fn access_this(
        &mut self,
        access: AccessKind,
        primary: Span,
        state: &State,
    ) -> Result<bool, OwnershipCheckingError> {
        if let Some(conflict) = state.loans.iter().find(|loan| {
            self.receiver_targets_overlap(&ActiveLoanTarget::This, &loan.target)
                && !(matches!(access, AccessKind::Read | AccessKind::SharedLoan)
                    && (loan.kind == LoanKind::Shared
                        || matches!(loan.owner, ActiveLoanOwner::ReservedReceiver(_))))
        }) {
            self.emit_loan_conflict(
                primary,
                conflict.origin,
                "this access conflicts with a live loan",
            )?;
            return Ok(false);
        }
        if access == AccessKind::Move
            && matches!(
                self.current_receiver_mode,
                Some(ParameterMode::Borrow | ParameterMode::Inout)
            )
        {
            self.diagnostics.push(crate::diagnostic::Diagnostic::new(
                self.sources,
                crate::diagnostic::Severity::Error,
                self.borrowed_move_code,
                "cannot move a non-Copyable this receiver from a non-owning body",
                primary,
            )?);
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn apply_this_contract(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        nominal: NominalId,
        mode: ParameterMode,
        is_receiver: bool,
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(state) = flows.next.as_mut() else {
            return Ok(());
        };
        let primary = self.parsed.ast().expressions().get(argument)?.span();
        let kind = match mode {
            ParameterMode::Value => return Ok(()),
            ParameterMode::Borrow => LoanKind::Shared,
            ParameterMode::Inout => LoanKind::Exclusive,
        };
        if mode == ParameterMode::Inout && self.current_receiver_mode != Some(ParameterMode::Inout)
        {
            self.diagnostics.push(crate::diagnostic::Diagnostic::new(
                self.sources,
                crate::diagnostic::Severity::Error,
                self.immutable_inout_code,
                "current this cannot supply an inout receiver",
                primary,
            )?);
            return Ok(());
        }
        let access = if kind == LoanKind::Shared {
            AccessKind::SharedLoan
        } else {
            AccessKind::ExclusiveLoan
        };
        if !self.access_this(access, primary, state)? {
            return Ok(());
        }
        let reserved = is_receiver && kind == LoanKind::Exclusive;
        let fact = LoanFact::new(
            call,
            argument,
            LoanTarget::This(nominal),
            kind,
            primary,
            self.parsed.ast().expressions().get(call)?.span(),
        );
        self.loans.push(if reserved {
            fact.reserve_receiver()
        } else {
            fact
        });
        state.loans.push(ActiveLoan {
            owner: if reserved {
                ActiveLoanOwner::ReservedReceiver(call)
            } else {
                ActiveLoanOwner::Call(call)
            },
            target: ActiveLoanTarget::This,
            kind,
            origin: primary,
        });
        Ok(())
    }

    pub(super) fn receiver_targets_overlap(
        &self,
        left: &ActiveLoanTarget,
        right: &ActiveLoanTarget,
    ) -> bool {
        match (left, right) {
            (ActiveLoanTarget::Place(left), ActiveLoanTarget::Place(right)) => left.overlaps(right),
            (ActiveLoanTarget::This, ActiveLoanTarget::This) => true,
            (ActiveLoanTarget::Place(place), ActiveLoanTarget::This)
            | (ActiveLoanTarget::This, ActiveLoanTarget::Place(place)) => self
                .names
                .symbols()
                .get(place.root().index())
                .is_some_and(|symbol| symbol.kind() == SymbolKind::Field),
        }
    }

    pub(super) fn activate_receiver(
        &mut self,
        call: ExpressionId,
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(state) = flows.next.as_mut() else {
            return Ok(());
        };
        if let Some(index) = state
            .loans
            .iter()
            .position(|loan| loan.owner == ActiveLoanOwner::ReservedReceiver(call))
        {
            let reservation = &state.loans[index];
            if let Some(conflict) = state.loans.iter().enumerate().find_map(|(other, loan)| {
                (other != index && self.receiver_targets_overlap(&reservation.target, &loan.target))
                    .then_some(loan)
            }) {
                self.emit_loan_conflict(
                    conflict.origin,
                    reservation.origin,
                    "receiver activation conflicts with a live argument loan",
                )?;
                return Ok(());
            }
            state.loans[index].owner = ActiveLoanOwner::Call(call);
        }
        for fact in &mut self.loans {
            if fact.call() == call && fact.is_receiver_reservation() {
                fact.activate_receiver();
            }
        }
        Ok(())
    }
}
