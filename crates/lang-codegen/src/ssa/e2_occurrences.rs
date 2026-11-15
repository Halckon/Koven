//! Bounded source occurrences; definition-set deduplication never erases call multiplicity.
use super::model::FunctionId;
use lang_frontend::{
    ownership_checking::two_source_experiment::{EntryCallDescriptor, EntryDescriptor},
    type_checking::{UnitCallableInstanceKey, ValidatedCompilationUnitTypes},
};
#[derive(Clone)]
pub(super) enum Occurrences<T> {
    Single(T),
    Pair([T; 2]),
}
impl<T> Occurrences<T> {
    pub(super) fn from_vec(values: Vec<T>) -> Option<Self> {
        match values.len() {
            1 => Some(Self::Single(values.into_iter().next()?)),
            2 => Some(Self::Pair(values.try_into().ok()?)),
            _ => None,
        }
    }
    pub(super) fn as_slice(&self) -> &[T] {
        match self {
            Self::Single(value) => std::slice::from_ref(value),
            Self::Pair(values) => values,
        }
    }
}
pub(super) type EntryKeys = Occurrences<UnitCallableInstanceKey>;
impl EntryKeys {
    pub(super) fn checked(
        d: &EntryDescriptor,
        typed: &ValidatedCompilationUnitTypes,
    ) -> Option<Self> {
        // This refactor deliberately retains the previous same-instance admission boundary.
        let common = d.common_instance()?;
        let keys = d
            .calls()
            .iter()
            .map(|call| {
                let actual = typed.types().call(call.contracts()[0].call())?.instance();
                (actual == call.instance() && actual == common).then(|| actual.clone())
            })
            .collect::<Option<Vec<_>>>()?;
        Self::from_vec(keys)
    }
    pub(super) fn resolve(
        &self,
        d: &EntryDescriptor,
        typed: &ValidatedCompilationUnitTypes,
        mut lookup: impl FnMut(&UnitCallableInstanceKey) -> Option<FunctionId>,
    ) -> Option<Bindings> {
        let original = Self::checked(d, typed)?;
        if self.as_slice() != original.as_slice() {
            return None;
        }
        let values = self
            .as_slice()
            .iter()
            .map(|key| Some((key.clone(), lookup(key)?)))
            .collect::<Option<Vec<_>>>()?;
        // Both directions: repeated key must reuse its function; distinct keys cannot alias it.
        if values.iter().any(|(key, id)| {
            values
                .iter()
                .any(|(other, other_id)| (key == other) != (id == other_id))
        }) {
            return None;
        }
        Some(Bindings(Occurrences::from_vec(values)?))
    }
}
pub(super) struct Bindings(Occurrences<(UnitCallableInstanceKey, FunctionId)>);
impl Bindings {
    /// Legacy one-caller routes reject any absent common key, never choose the first call.
    #[cfg(all(test, feature = "e2-two-source-test-support"))]
    pub(super) fn uniform(d: &EntryDescriptor, caller: FunctionId) -> Option<Self> {
        let common = d.common_instance()?;
        Some(Self(Occurrences::from_vec(
            d.calls().iter().map(|_| (common.clone(), caller)).collect(),
        )?))
    }
    pub(super) fn matches(&self, d: &EntryDescriptor) -> bool {
        self.0.as_slice().len() == d.calls().len()
            && self
                .0
                .as_slice()
                .iter()
                .zip(d.calls())
                .all(|((key, _), call)| key == call.instance())
    }
    pub(super) fn caller(&self, index: usize, call: &EntryCallDescriptor) -> Option<FunctionId> {
        let (key, id) = self.0.as_slice().get(index)?;
        (key == call.instance()).then_some(*id)
    }
}
