//! Read-only projections of privately constructed bounded entry evidence.
use super::*;
impl EntryDescriptor {
    pub(in super::super) fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
    /// Original zero-parameter Unit declaration.
    pub fn entry(&self) -> DeclarationId {
        self.entry
    }
    /// Complete source body.
    pub fn body(&self) -> Span {
        self.body
    }
    /// Every source call occurrence, in the complete original body order.
    pub fn calls(&self) -> &[EntryCallDescriptor] {
        match &self.calls {
            EntryCalls::Single(call) => std::slice::from_ref(call.as_ref()),
            EntryCalls::Pair(calls)
            | EntryCalls::SharedNamedPair { calls, .. }
            | EntryCalls::LateTemporarySharedPair { calls, .. }
            | EntryCalls::DifferentLateTemporarySharedPair { calls, .. }
            | EntryCalls::EarlyTemporarySharedPair { calls, .. }
            | EntryCalls::DifferentSharedNamedPair { calls, .. }
            | EntryCalls::DifferentSharedMixedPair { calls, .. }
            | EntryCalls::StaggeredNamedPair { calls, .. }
            | EntryCalls::DifferentStaggeredNamedPair { calls, .. }
            | EntryCalls::JointFinalNamedPair { calls, .. }
            | EntryCalls::DifferentJointFinalNamedPair { calls, .. }
            | EntryCalls::LateInitializedPair(calls)
            | EntryCalls::DifferentLateInitializedPair(calls)
            | EntryCalls::DifferentTemporaryPair(calls) => calls.as_ref(),
        }
    }
    /// Separately proved distinct-callee joint final-use root.
    pub fn is_different_joint_final_named_pair(&self) -> bool {
        matches!(self.calls, EntryCalls::DifferentJointFinalNamedPair { .. })
    }
    /// Two original initializers and the independent original terminal cleanup sequence.
    pub fn joint_final_owners(&self) -> Option<(&[EntryOwnerDescriptor; 2], &[UnitDropFact; 2])> {
        if let EntryCalls::JointFinalNamedPair {
            owners,
            terminal_drops,
            ..
        }
        | EntryCalls::DifferentJointFinalNamedPair {
            owners,
            terminal_drops,
            ..
        } = &self.calls
        {
            Some((owners, terminal_drops))
        } else {
            None
        }
    }
    /// Two distinct root-owned initializers in source order and their separate final-use Drops.
    pub fn staggered_owners(&self) -> Option<&[EntryOwnerDescriptor; 2]> {
        let (EntryCalls::StaggeredNamedPair { owners, .. }
        | EntryCalls::DifferentStaggeredNamedPair { owners, .. }) = &self.calls
        else {
            return None;
        };
        Some(owners.as_ref())
    }
    /// Root-owned initialization and the unique original final-use Drop of a shared pair.
    pub fn shared_owner(
        &self,
    ) -> Option<(UnitSymbolId, UnitExpressionId, Span, &[u8], &UnitDropFact)> {
        let (EntryCalls::SharedNamedPair { owner, .. }
        | EntryCalls::DifferentSharedNamedPair { owner, .. }
        | EntryCalls::DifferentSharedMixedPair { owner, .. }
        | EntryCalls::LateTemporarySharedPair { owner, .. }
        | EntryCalls::DifferentLateTemporarySharedPair { owner, .. }
        | EntryCalls::EarlyTemporarySharedPair { owner, .. }) = &self.calls
        else {
            return None;
        };
        Some((
            owner.root,
            owner.initializer,
            owner.span,
            &owner.bytes,
            &owner.drop,
        ))
    }
    /// Separately proved distinct-callee shared-only then late temporary shape.
    pub fn is_different_late_temporary_shared_pair(&self) -> bool {
        matches!(
            self.calls,
            EntryCalls::DifferentLateTemporarySharedPair { .. }
        )
    }
    /// Exact first-call temporary followed by shared-only root use.
    pub fn is_early_temporary_shared_pair(&self) -> bool {
        matches!(self.calls, EntryCalls::EarlyTemporarySharedPair { .. })
    }
    /// Exact shared-only first call followed by one call-local temporary.
    pub fn is_late_temporary_shared_pair(&self) -> bool {
        matches!(self.calls, EntryCalls::LateTemporarySharedPair { .. })
    }
    /// Privately proven exact pair of fully temporary calls to two closed distinct instances.
    pub fn is_different_temporary_pair(&self) -> bool {
        matches!(self.calls, EntryCalls::DifferentTemporaryPair(_))
    }
    /// Privately proven single shared named owner across two closed distinct instances.
    pub fn is_different_shared_named_pair(&self) -> bool {
        matches!(self.calls, EntryCalls::DifferentSharedNamedPair { .. })
    }
    /// Privately proven shared root plus one temporary per distinct closed call.
    pub fn is_different_shared_mixed_pair(&self) -> bool {
        matches!(self.calls, EntryCalls::DifferentSharedMixedPair { .. })
    }
    /// Private two-root separate-last-use proof across two distinct closed instances.
    pub fn is_different_staggered_named_pair(&self) -> bool {
        matches!(self.calls, EntryCalls::DifferentStaggeredNamedPair { .. })
    }
    /// Privately proven late named formation after a distinct complete temporary call.
    pub fn is_different_late_initialized_pair(&self) -> bool {
        matches!(self.calls, EntryCalls::DifferentLateInitializedPair(_))
    }
    /// Returns a key only when every original occurrence shares the complete instance.
    pub fn common_instance(&self) -> Option<&UnitCallableInstanceKey> {
        let first = self.calls().first()?.instance();
        self.calls()
            .iter()
            .all(|call| call.instance() == first)
            .then_some(first)
    }
}
