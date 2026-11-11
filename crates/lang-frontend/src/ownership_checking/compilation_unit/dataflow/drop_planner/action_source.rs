//! The cleanup projections and acquisition observation share one append-only source.
pub(super) mod binding_child_links;
mod binding_dependency_drops;
mod binding_links;
mod binding_regions;
mod child_regions;
pub(super) use binding_dependency_drops::BindingDropCause;
mod consumer_links;
mod forward_links;
use super::PlannerDropPoint;
use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::{
    BorrowBindingEndFact, UnitBorrowCallHandoffFact, UnitDropPoint, UnitIterationCleanupAction,
    UnitLoanFact,
};
use std::sync::Arc;
type BindingEnd = BorrowBindingEndFact<UnitSymbolId, UnitDropPoint>;

#[derive(Clone, Debug)]
pub(super) struct ActionHandle {
    identity: Arc<()>,
    index: usize,
}
impl ActionHandle {
    pub(super) fn same_record(&self, other: &Self) -> bool {
        self.index == other.index && Arc::ptr_eq(&self.identity, &other.identity)
    }
}

pub(super) struct ActionSource {
    identity: Arc<()>,
    records: Vec<Record>,
    cleanups: usize,
    regions: Vec<crate::ownership_checking::compilation_unit::loan_region_validation::Candidate>,
    binding_regions:
        Vec<crate::ownership_checking::compilation_unit::loan_binding_region_validation::Candidate>,
    active_binding_region: Option<usize>,
    child_regions:
        Vec<crate::ownership_checking::compilation_unit::loan_child_region_validation::Candidate>,
    active_child_region: Option<usize>,
    dependency_drops: Vec<binding_dependency_drops::AppendedDrop>,
    pub(super) enabled: bool,
}

enum Record {
    Begin(UnitLoanFact),
    ReceiverBegin(crate::ownership_checking::UnitReceiverOwnershipFact),
    ReceiverEnd(crate::ownership_checking::UnitReceiverCallReturnEndFact),
    BorrowHandoffCommitted(UnitBorrowCallHandoffFact),
    BorrowBindingEnd(BindingEnd),
    BorrowBindingUseReturned(binding_links::UseReturn),
    BorrowBindingSourceRetired(binding_links::SourceRetirement),
    BorrowBindingChildCommitted(binding_child_links::Commit),
    BorrowBindingChildUseReturned(binding_child_links::UseReturn),
    BorrowBindingChildRetired(binding_child_links::Retirement),
    BorrowConsumerTransfer {
        source: UnitLoanFact,
        acquisition: Option<ActionHandle>,
        consumer: crate::type_checking::UnitExpressionId,
        argument: crate::type_checking::UnitExpressionId,
    },
    BorrowConsumerRetired {
        transfer: Option<ActionHandle>,
        acquisition: Option<ActionHandle>,
        transaction: crate::ownership_checking::UnitBorrowCallTransportFact,
    },
    BorrowForwardHandoff {
        transaction: crate::ownership_checking::UnitBorrowForwardTransportFact,
        cause: Option<super::borrow_forward_transport::ForwardCause>,
        completed: Option<super::borrow_forward_transport::ForwardScope>,
    },
    Cleanup((PlannerDropPoint, UnitIterationCleanupAction)),
}

impl Default for ActionSource {
    fn default() -> Self {
        Self {
            identity: Arc::new(()),
            records: Vec::new(),
            cleanups: 0,
            regions: Vec::new(),
            binding_regions: Vec::new(),
            active_binding_region: None,
            child_regions: Vec::new(),
            active_child_region: None,
            dependency_drops: Vec::new(),
            enabled: true,
        }
    }
}

impl ActionSource {
    pub(super) fn enter_region(
        &mut self,
        mut candidate: crate::ownership_checking::compilation_unit::loan_region_validation::Candidate,
    ) -> usize {
        let index = self.regions.len();
        // Only actual planner entry/completion owns these cursors, never admission.
        candidate.start = self.records.len();
        candidate.end = None;
        self.regions.push(candidate);
        index
    }
    pub(super) fn complete_region(&mut self, index: usize) {
        self.regions[index].end = Some(self.records.len());
    }

    pub(super) fn push(&mut self, cleanup: (PlannerDropPoint, UnitIterationCleanupAction)) {
        self.push_with_binding_drop(cleanup, None);
    }

    pub(super) fn push_with_binding_drop(
        &mut self,
        cleanup: (PlannerDropPoint, UnitIterationCleanupAction),
        cause: Option<BindingDropCause>,
    ) {
        let handle = self.next_handle();
        self.records.push(Record::Cleanup(cleanup));
        self.cleanups += 1;
        if let Some(cause) = cause {
            self.record_binding_drop(cause, handle);
        }
    }

    pub(super) fn begin(&mut self, loan: UnitLoanFact) -> Option<ActionHandle> {
        if !self.enabled {
            return None;
        }
        let handle = ActionHandle {
            identity: self.identity.clone(),
            index: self.records.len(),
        };
        self.records.push(Record::Begin(loan));
        Some(handle)
    }

    pub(super) fn begin_receiver(
        &mut self,
        receiver: crate::ownership_checking::UnitReceiverOwnershipFact,
    ) {
        if self.enabled
            && receiver.kind() == crate::ownership_checking::UnitReceiverOwnershipKind::SharedLoan
        {
            self.records.push(Record::ReceiverBegin(receiver));
        }
    }

    pub(super) fn end_receiver(
        &mut self,
        end: crate::ownership_checking::UnitReceiverCallReturnEndFact,
    ) {
        // Kept with observation disabled: the legacy table is a projection of this source.
        self.records.push(Record::ReceiverEnd(end));
    }

    pub(super) fn receiver_return_ends(
        &self,
    ) -> Vec<crate::ownership_checking::UnitReceiverCallReturnEndFact> {
        self.records
            .iter()
            .filter_map(|record| match record {
                Record::ReceiverEnd(end) => Some(end.clone()),
                _ => None,
            })
            .collect()
    }

    pub(super) fn commit_borrow_handoff(
        &mut self,
        fact: UnitBorrowCallHandoffFact,
    ) -> ActionHandle {
        // Even observer-off retains the source used by the old public projection.
        let handle = self.next_handle();
        self.records.push(Record::BorrowHandoffCommitted(fact));
        handle
    }

    pub(super) fn end_borrow_binding(&mut self, fact: BindingEnd) -> ActionHandle {
        // Each actual retirement has its own record. Only the old projection deduplicates.
        let handle = self.next_handle();
        self.records.push(Record::BorrowBindingEnd(fact));
        handle
    }

    fn next_handle(&self) -> ActionHandle {
        ActionHandle {
            identity: self.identity.clone(),
            index: self.records.len(),
        }
    }

    pub(super) fn return_binding_use(
        &mut self,
        binding: UnitSymbolId,
        loan: UnitLoanFact,
        source_begin: Option<ActionHandle>,
        handoff: Option<ActionHandle>,
        begin: Option<ActionHandle>,
    ) -> Option<ActionHandle> {
        if !self.enabled {
            return None;
        }
        let handle = self.next_handle();
        self.records
            .push(Record::BorrowBindingUseReturned(binding_links::UseReturn {
                binding,
                loan,
                source_begin,
                handoff,
                begin,
            }));
        Some(handle)
    }

    pub(super) fn retire_binding_source(
        &mut self,
        transaction: crate::ownership_checking::UnitBorrowBindingTransportFact,
        source_begin: Option<ActionHandle>,
        handoff: Option<ActionHandle>,
        use_return: Option<ActionHandle>,
        binding_end: ActionHandle,
    ) -> Option<ActionHandle> {
        if self.enabled {
            let handle = self.next_handle();
            self.records.push(Record::BorrowBindingSourceRetired(
                binding_links::SourceRetirement {
                    transaction,
                    source_begin,
                    handoff,
                    use_return,
                    binding_end,
                    dependency: None,
                    dependency_seen: false,
                },
            ));
            Some(handle)
        } else {
            None
        }
    }

    pub(super) fn borrow_handoffs(&self) -> Vec<UnitBorrowCallHandoffFact> {
        self.records
            .iter()
            .filter_map(|record| match record {
                Record::BorrowHandoffCommitted(fact) => Some(fact.clone()),
                _ => None,
            })
            .collect()
    }

    pub(super) fn borrow_binding_ends(&self) -> Vec<BindingEnd> {
        let mut ends = Vec::new();
        for record in &self.records {
            if let Record::BorrowBindingEnd(fact) = record
                && !ends.contains(fact)
            {
                ends.push(*fact);
            }
        }
        ends
    }

    pub(super) fn transfer_consumer(
        &mut self,
        source: &UnitLoanFact,
        consumer: crate::type_checking::UnitExpressionId,
        argument: crate::type_checking::UnitExpressionId,
        acquisition: Option<ActionHandle>,
    ) -> ActionHandle {
        let handle = ActionHandle {
            identity: self.identity.clone(),
            index: self.records.len(),
        };
        self.records.push(Record::BorrowConsumerTransfer {
            source: source.clone(),
            acquisition,
            consumer,
            argument,
        });
        handle
    }

    pub(super) fn retire_consumer(
        &mut self,
        transfer: Option<ActionHandle>,
        transaction: crate::ownership_checking::UnitBorrowCallTransportFact,
        acquisition: Option<ActionHandle>,
    ) {
        self.records.push(Record::BorrowConsumerRetired {
            acquisition,
            transfer,
            transaction,
        });
    }

    pub(super) fn consumer_transports(
        &self,
    ) -> Vec<crate::ownership_checking::UnitBorrowCallTransportFact> {
        self.records
            .iter()
            .filter_map(|record| match record {
                Record::BorrowConsumerRetired { transaction, .. } => Some(transaction.clone()),
                _ => None,
            })
            .collect()
    }

    pub(super) fn forward_handoff(
        &mut self,
        transaction: crate::ownership_checking::UnitBorrowForwardTransportFact,
        cause: Option<super::borrow_forward_transport::ForwardCause>,
        completed: Option<super::borrow_forward_transport::ForwardScope>,
    ) {
        // This is the unique old-table source, even with observation disabled.
        self.records.push(Record::BorrowForwardHandoff {
            transaction,
            cause,
            completed,
        });
    }

    pub(super) fn forward_transports(
        &self,
    ) -> Vec<crate::ownership_checking::UnitBorrowForwardTransportFact> {
        self.records
            .iter()
            .filter_map(|record| match record {
                Record::BorrowForwardHandoff { transaction, .. } => Some(transaction.clone()),
                _ => None,
            })
            .collect()
    }

    pub(super) fn len(&self) -> usize {
        self.cleanups
    }

    pub(super) fn iter(
        &self,
    ) -> impl Iterator<Item = &(PlannerDropPoint, UnitIterationCleanupAction)> {
        self.records.iter().filter_map(|record| match record {
            Record::Cleanup(cleanup) => Some(cleanup),
            Record::Begin(_)
            | Record::ReceiverBegin(_)
            | Record::ReceiverEnd(_)
            | Record::BorrowHandoffCommitted(_)
            | Record::BorrowBindingEnd(_)
            | Record::BorrowBindingUseReturned(_)
            | Record::BorrowBindingSourceRetired(_)
            | Record::BorrowBindingChildCommitted(_)
            | Record::BorrowBindingChildUseReturned(_)
            | Record::BorrowBindingChildRetired(_)
            | Record::BorrowConsumerTransfer { .. }
            | Record::BorrowConsumerRetired { .. }
            | Record::BorrowForwardHandoff { .. } => None,
        })
    }

    pub(super) fn finish(
        self,
        source: crate::name_resolution::SourceUnitId,
        file: crate::source::SourceId,
    ) -> Vec<crate::ownership_checking::UnitLoanActionObservation> {
        use crate::ownership_checking::UnitLoanAction;
        if !self.enabled {
            return Vec::new();
        }
        let child_links =
            binding_child_links::validate(&self.records, &self.identity, source, file);
        let binding_links = binding_links::validate(&self.records, &self.identity, source, file);
        let binding_dependency_drops = binding_dependency_drops::validate(
            &self.records,
            &self.identity,
            &self.dependency_drops,
            &child_links,
            &binding_links,
            source,
            file,
        );
        let consumer_links = consumer_links::validate(&self.records, &self.identity, source, file);
        let acquisitions =
            consumer_links::acquisitions(&self.records, &self.identity, &consumer_links);
        let forward_links = forward_links::validate(&self.records, &self.identity, source, file);
        let identity = self.identity;
        let actions: Vec<_> = self
            .records
            .into_iter()
            .enumerate()
            .map(|(index, record)| match record {
                Record::Begin(loan) => UnitLoanAction::Begin(loan),
                Record::ReceiverBegin(receiver) => UnitLoanAction::ReceiverBegin(receiver),
                Record::ReceiverEnd(end) => UnitLoanAction::ReceiverEnd(end),
                Record::BorrowHandoffCommitted(fact) => {
                    UnitLoanAction::BorrowHandoffCommitted(fact)
                }
                Record::BorrowBindingEnd(fact) => UnitLoanAction::BorrowBindingEnd(fact),
                Record::BorrowBindingUseReturned(fact) => {
                    let links = binding_links.get(&index).copied().unwrap_or_default();
                    UnitLoanAction::BorrowBindingUseReturned(
                        crate::ownership_checking::UnitLoanBindingUseReturn {
                            identity: identity.clone(),
                            binding: fact.binding,
                            loan: fact.loan,
                            source_begin: links.source_begin,
                            handoff: links.handoff,
                            begin: links.begin,
                        },
                    )
                }
                Record::BorrowBindingSourceRetired(fact) => {
                    let links = binding_links.get(&index).copied().unwrap_or_default();
                    UnitLoanAction::BorrowBindingSourceRetired(
                        crate::ownership_checking::UnitLoanBindingSourceRetirement {
                            identity: identity.clone(),
                            transaction: fact.transaction,
                            source_begin: links.source_begin,
                            handoff: links.handoff,
                            use_return: links.use_return,
                            binding_end: links.binding_end,
                        },
                    )
                }
                Record::BorrowBindingChildCommitted(fact) => fact.into_action(
                    identity.clone(),
                    child_links.get(&index).copied().unwrap_or_default(),
                ),
                Record::BorrowBindingChildUseReturned(fact) => fact.into_action(
                    identity.clone(),
                    child_links.get(&index).copied().unwrap_or_default(),
                ),
                Record::BorrowBindingChildRetired(fact) => fact.into_action(
                    identity.clone(),
                    child_links.get(&index).copied().unwrap_or_default(),
                ),
                Record::BorrowConsumerTransfer {
                    source,
                    consumer,
                    argument,
                    ..
                } => UnitLoanAction::BorrowConsumerTransfer {
                    source,
                    consumer,
                    argument,
                },
                Record::BorrowConsumerRetired { transaction, .. } => {
                    UnitLoanAction::BorrowConsumerRetired(
                        crate::ownership_checking::UnitLoanConsumerRetirement {
                            identity: identity.clone(),
                            transfer: consumer_links.get(&index).copied(),
                            source_begin: acquisitions.get(&index).map(|&(source, _)| source),
                            consumer_begin: acquisitions.get(&index).map(|&(_, consumer)| consumer),
                            transaction,
                        },
                    )
                }
                Record::BorrowForwardHandoff { transaction, .. } => {
                    let link = forward_links.get(&index);
                    UnitLoanAction::BorrowForwardHandoff(
                        crate::ownership_checking::UnitLoanForwardHandoff {
                            identity: identity.clone(),
                            source_begin: link.map(|&(begin, _)| begin),
                            callable: link.map(|&(_, callable)| callable),
                            transaction,
                        },
                    )
                }
                Record::Cleanup((point, action)) => UnitLoanAction::Cleanup {
                    point: point.into_unit(source),
                    action,
                },
            })
            .collect();
        let regions = self
            .regions
            .into_iter()
            .map(|region| region.finish(identity.clone(), &actions))
            .collect();
        let binding_regions = self
            .binding_regions
            .into_iter()
            .map(|region| region.finish(identity.clone(), &actions))
            .collect();
        let child_regions = self
            .child_regions
            .into_iter()
            .map(|region| {
                let mut matching = binding_dependency_drops.iter().filter(|dependency| {
                    dependency.root() == region.scope.root
                        && dependency.parent() == region.scope.parent
                        && dependency.child() == region.scope.child
                });
                let dependency = matching.next().filter(|_| matching.next().is_none());
                let removed = dependency.and_then(|dependency| {
                    binding_dependency_drops::removed_owner(&self.dependency_drops, dependency)
                });
                region.finish(identity.clone(), &actions, dependency, removed)
            })
            .collect();
        vec![crate::ownership_checking::UnitLoanActionObservation {
            identity,
            source,
            actions,
            regions,
            binding_regions,
            child_regions,
            forward_regions: Vec::new(),
            binding_dependency_drops,
        }]
    }
}

#[cfg(test)]
mod tests;
