//! Independent local dependency-to-Drop evidence. No Complete-region candidate is consulted.
use super::super::OwnedValue;
use super::{
    ActionHandle, ActionSource, Record, binding_child_links, binding_links,
    binding_regions::RegionDropCause,
};
use crate::{
    name_resolution::SourceUnitId,
    ownership_checking::{
        UnitDropTarget, UnitIterationCleanupAction, UnitLoanBindingDependencyDrop, UnitLoanTarget,
    },
    source::SourceId,
};
use std::{collections::BTreeMap, sync::Arc};

pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) enum BindingDropCause {
    UseSet(super::binding_use_sets::UseSetDropCause),
    MultiAuxiliary(super::binding_use_sets::MultiAuxDropCause),
    Region(RegionDropCause),
    LaterAuxiliaryRegion(super::later_auxiliary_binding_regions::LaterAuxiliaryRegionDropCause),
    LaterAuxiliaryReturn(super::later_auxiliary_binding_regions::LaterAuxiliaryReturnDropCause),
    AuxiliaryRegion(super::auxiliary_binding_regions::AuxiliaryRegionDropCause),
    DistinctAuxiliaryRegion(
        super::distinct_auxiliary_binding_regions::DistinctAuxiliaryRegionDropCause,
    ),
    Dependency(Box<DependencyDropCause>),
}

pub(super) struct ParentRetirement {
    child: ActionHandle,
    owner: OwnedValue,
}

pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct DependencyDropCause
{
    child: ActionHandle,
    parent: ActionHandle,
    expected_owner: OwnedValue,
    removed_owner: Option<OwnedValue>,
}

pub(super) struct AppendedDrop {
    cause: DependencyDropCause,
    cleanup: ActionHandle,
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn attach_binding_dependency(
        &mut self,
        retirement: &ActionHandle,
        child: ActionHandle,
        owner: OwnedValue,
    ) {
        if !Arc::ptr_eq(&retirement.identity, &self.identity) {
            return;
        }
        let Some(Record::BorrowBindingSourceRetired(parent)) =
            self.records.get_mut(retirement.index)
        else {
            return;
        };
        // The real publish consumes one transport. Repeated attachment permanently
        // loses evidence; None must never become a slot a later call can refill.
        if parent.dependency_seen {
            parent.dependency = None;
            return;
        }
        parent.dependency_seen = true;
        parent.dependency = Some(ParentRetirement { child, owner });
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn dependency_drop_cause(
        &self,
        child: ActionHandle,
        parent: ActionHandle,
    ) -> Option<BindingDropCause> {
        if !self.enabled
            || !Arc::ptr_eq(&parent.identity, &self.identity)
            || !Arc::ptr_eq(&child.identity, &self.identity)
            || child.index >= parent.index
        {
            return None;
        }
        let Record::BorrowBindingSourceRetired(retired) = self.records.get(parent.index)? else {
            return None;
        };
        let evidence = retired.dependency.as_ref()?;
        if !evidence.child.same_record(&child) || evidence.owner.owner.is_none() {
            return None;
        }
        Some(BindingDropCause::Dependency(Box::new(
            DependencyDropCause {
                child,
                parent,
                expected_owner: evidence.owner.clone(),
                removed_owner: None,
            },
        )))
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn verify_removed_binding_owner(
        &mut self,
        cause: BindingDropCause,
        value: &OwnedValue,
    ) -> Option<BindingDropCause> {
        match cause {
            BindingDropCause::MultiAuxiliary(cause) => self
                .verify_removed_multi_auxiliary(cause, value)
                .map(BindingDropCause::MultiAuxiliary),
            BindingDropCause::UseSet(cause) => self
                .verify_removed_use_set_owner(cause, value)
                .map(BindingDropCause::UseSet),
            BindingDropCause::LaterAuxiliaryRegion(cause) => self
                .verify_removed_later_auxiliary_region_owner(cause, value)
                .map(BindingDropCause::LaterAuxiliaryRegion),
            BindingDropCause::LaterAuxiliaryReturn(cause) => self
                .verify_removed_later_auxiliary_return_owner(cause, value)
                .map(BindingDropCause::LaterAuxiliaryReturn),
            BindingDropCause::DistinctAuxiliaryRegion(cause) => self
                .verify_removed_distinct_auxiliary_region_owner(cause, value)
                .map(BindingDropCause::DistinctAuxiliaryRegion),
            BindingDropCause::AuxiliaryRegion(cause) => self
                .verify_removed_auxiliary_region_owner(cause, value)
                .map(BindingDropCause::AuxiliaryRegion),
            BindingDropCause::Region(cause) => self
                .verify_removed_region_owner(cause, value)
                .map(BindingDropCause::Region),
            BindingDropCause::Dependency(mut cause) => {
                // OwnedValue equality includes UnitOwnerId's catalog identity, callable and index.
                if cause.removed_owner.is_some()
                    || value.owner.is_none()
                    || cause.expected_owner != *value
                {
                    return None;
                }
                cause.removed_owner = Some(value.clone());
                Some(BindingDropCause::Dependency(cause))
            }
        }
    }

    pub(super) fn record_binding_drop(&mut self, cause: BindingDropCause, cleanup: ActionHandle) {
        match cause {
            BindingDropCause::MultiAuxiliary(cause) => {
                self.record_multi_auxiliary_drop(cause, cleanup)
            }
            BindingDropCause::UseSet(cause) => self.record_use_set_source_drop(cause, cleanup),
            BindingDropCause::LaterAuxiliaryRegion(cause) => {
                self.record_later_auxiliary_region_drop(cause, cleanup)
            }
            BindingDropCause::LaterAuxiliaryReturn(cause) => {
                self.record_later_auxiliary_return_drop(cause, cleanup)
            }
            BindingDropCause::Region(cause) => self.record_region_drop(cause, cleanup),
            BindingDropCause::DistinctAuxiliaryRegion(cause) => {
                self.record_distinct_auxiliary_region_drop(cause, cleanup)
            }
            BindingDropCause::AuxiliaryRegion(cause) => {
                self.record_auxiliary_region_drop(cause, cleanup)
            }
            BindingDropCause::Dependency(cause) => self.dependency_drops.push(AppendedDrop {
                cause: *cause,
                cleanup,
            }),
        }
    }
}

fn earlier(handle: &ActionHandle, before: usize, identity: &Arc<()>) -> Option<usize> {
    (Arc::ptr_eq(&handle.identity, identity) && handle.index < before).then_some(handle.index)
}

fn validate_one(
    records: &[Record],
    identity: &Arc<()>,
    appended: &AppendedDrop,
    children: &BTreeMap<usize, binding_child_links::Links>,
    parents: &BTreeMap<usize, binding_links::Links>,
    source: SourceUnitId,
    file: SourceId,
) -> Option<UnitLoanBindingDependencyDrop> {
    let cause = &appended.cause;
    let cleanup = earlier(&appended.cleanup, records.len(), identity)?;
    let parent_index = earlier(&cause.parent, cleanup, identity)?;
    let child_index = earlier(&cause.child, parent_index, identity)?;
    let Record::BorrowBindingSourceRetired(parent) = records.get(parent_index)? else {
        return None;
    };
    let Record::BorrowBindingChildRetired(child) = records.get(child_index)? else {
        return None;
    };
    let Record::Cleanup((point, UnitIterationCleanupAction::Drop(drop))) = records.get(cleanup)?
    else {
        return None;
    };
    let evidence = parent.dependency.as_ref()?;
    let owner = cause.removed_owner.as_ref()?;
    let p = parents.get(&parent_index)?;
    let c = children.get(&child_index)?;
    let source_begin = p.source_begin?;
    let parent_handoff = p.handoff?;
    let child_commit = c.commit?;
    let child_use_return = c.use_return?;
    let parent_end = p.binding_end?;
    let child_end = c.binding_end?;
    let root = owner.symbol;
    if owner.owner.is_none()
        || *owner != cause.expected_owner
        || *owner != evidence.owner
        || owner.scope_depth != 1
        || owner.origin != owner.declaration
        || root.source_unit() != source
        || owner.origin.source_id() != file
        || !evidence.child.same_record(&cause.child)
        || child.parent.binding != parent.transaction.binding()
        || child.binding == parent.transaction.binding()
        || child.binding == root
        || parent.transaction.binding() == root
        || c.source_begin != Some(source_begin)
        || c.parent_handoff != Some(parent_handoff)
        || !matches!(parent.transaction.source_loan().target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == root)
        || parent.transaction.source_loan() != &child.parent.source
        || !matches!(drop.target(), UnitDropTarget::Named(symbol) if symbol == root)
        || drop.value_origin() != owner.origin
        || point.into_unit(source) != drop.point()
        || drop.point() != parent.transaction.point()
        || child.point != drop.point()
        || !matches!(
            drop.point(),
            crate::ownership_checking::UnitDropPoint::AfterStatement(_)
        )
        || !(source_begin < parent_handoff
            && parent_handoff < child_commit
            && child_commit < child_use_return
            && child_use_return < child_end)
        || child_end.checked_add(1) != Some(child_index)
        || child_index.checked_add(1) != Some(parent_end)
        || parent_end.checked_add(1) != Some(parent_index)
        || parent_index.checked_add(1) != Some(cleanup)
        || parent.use_return.is_some()
    {
        return None;
    }
    Some(UnitLoanBindingDependencyDrop {
        identity: identity.clone(),
        root,
        parent: parent.transaction.binding(),
        child: child.binding,
        source_begin,
        parent_handoff,
        child_commit,
        child_use_return,
        child_retirement: child_index,
        parent_retirement: parent_index,
        cleanup,
    })
}

pub(super) fn validate(
    records: &[Record],
    identity: &Arc<()>,
    appended: &[AppendedDrop],
    children: &BTreeMap<usize, binding_child_links::Links>,
    parents: &BTreeMap<usize, binding_links::Links>,
    source: SourceUnitId,
    file: SourceId,
) -> Vec<UnitLoanBindingDependencyDrop> {
    appended
        .iter()
        .filter(|entry| {
            // A duplicate cause/append is not resolved by picking the first or last witness.
            appended
                .iter()
                .filter(|other| {
                    other.cleanup.same_record(&entry.cleanup)
                        || other.cause.parent.same_record(&entry.cause.parent)
                        || other.cause.child.same_record(&entry.cause.child)
                })
                .count()
                == 1
        })
        .filter_map(|entry| validate_one(records, identity, entry, children, parents, source, file))
        .collect()
}

// Read only the owner removed by the same actual append represented by the validated relation.
pub(super) fn removed_owner(
    appended: &[AppendedDrop],
    dependency: &UnitLoanBindingDependencyDrop,
) -> Option<
    crate::ownership_checking::compilation_unit::loan_binding_region_validation::OwnerSnapshot,
> {
    let id = dependency.cleanup();
    let mut matching = appended.iter().filter(|entry| {
        Arc::ptr_eq(&entry.cleanup.identity, &id.identity) && entry.cleanup.index == id.index
    });
    let owner = matching.next()?.cause.removed_owner.as_ref()?;
    if matching.next().is_some() {
        return None;
    }
    Some(crate::ownership_checking::compilation_unit::loan_binding_region_validation::OwnerSnapshot {
        owner: owner.owner.clone(), symbol: owner.symbol, origin: owner.origin,
        declaration: owner.declaration, scope_depth: owner.scope_depth,
    })
}

#[cfg(test)]
mod tests;
