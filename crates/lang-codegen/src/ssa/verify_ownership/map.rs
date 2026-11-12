//! Map container operation ownership and reborrow parent-child lifetime verification.

use super::*;
use crate::ssa::model::Origin;

pub(in crate::ssa::verify_ownership) mod reborrow {
    use super::super::*;

    #[allow(clippy::too_many_arguments)]
    pub(in crate::ssa::verify_ownership) fn end_loan(
        function: &Function,
        loan: LoanId,
        aliases: &AliasRoots,
        closure_loans: &closure::ClosureLoans,
        reborrows: &ReborrowDependencies,
        state: &mut BlockState,
        location: VerifyLocation,
        origin: &crate::ssa::model::Origin,
        errors: &mut Vec<VerifyError>,
    ) -> bool {
        let kind = if let Some(owner) = range::live_descriptor(function, loan, state)
            .or_else(|| closure_loans.live_owner_holding(loan, state))
        {
            VerifyErrorKind::OwnerLoanConflict { value: owner }
        } else if let Some(dependent) = reborrows.active_descendant(loan, aliases, state) {
            VerifyErrorKind::LoanDependencyActive {
                parent: loan,
                dependent,
            }
        } else if !state.loans.remove(&loan) {
            VerifyErrorKind::LoanInactive { loan }
        } else {
            return true;
        };
        errors.push(error(kind, location, origin));
        false
    }

    impl ReborrowDependencies {
        pub(in crate::ssa::verify_ownership) fn compute(
            module: &Module,
            function: &Function,
        ) -> Self {
            let mut parent_by_child = BTreeMap::new();
            let mut must_end_children = BTreeSet::new();
            for instruction in &function.instructions {
                let (source, must_end) = match instruction.operation {
                    Operation::RangeConstruct { source, .. }
                    | Operation::RangeCall { source, .. } => {
                        let Some(EntityId::Loan(child)) = instruction.results.last() else {
                            continue;
                        };
                        must_end_children.insert(*child);
                        if range::source::is_view(module, function, source) {
                            let Some(root) = range::source::root(function, source) else {
                                continue;
                            };
                            if function.blocks[0]
                                .parameters
                                .contains(&EntityId::Loan(root))
                            {
                                parent_by_child.insert(*child, root);
                            }
                            continue;
                        }
                        (source, true)
                    }
                    Operation::SharedReborrow { source }
                    | Operation::CallableViewOf { source }
                    | Operation::SharedHeapFieldLoan { base: source, .. }
                    | Operation::BorrowCall { source, .. }
                    | Operation::MapIterationKeyLoan { source, .. }
                    | Operation::MapIterationValueLoan { source, .. }
                    | Operation::MapRequireValue { source, .. } => (source, true),
                    Operation::SharedFieldLoan { base: source, .. }
                    | Operation::SharedReferenceFollow { source } => (source, false),
                    Operation::BorrowBegin { place, .. } => {
                        let Some(entity) = function.entity(EntityId::Place(place)) else {
                            continue;
                        };
                        let Definition::InstructionResult { instruction, .. } = entity.definition
                        else {
                            continue;
                        };
                        let Some(instruction) = function.instruction(instruction) else {
                            continue;
                        };
                        let source = match instruction.operation {
                            Operation::ContainerElementPlace {
                                owner: EntityId::Loan(source),
                                ..
                            }
                            | Operation::RangeElementPlace { view: source, .. } => source,
                            _ => continue,
                        };
                        (source, true)
                    }
                    _ => continue,
                };
                let Some(EntityId::Loan(child)) = instruction
                    .results
                    .iter()
                    .find(|entity| matches!(entity, EntityId::Loan(_)))
                else {
                    continue;
                };
                parent_by_child.insert(*child, source);
                if must_end {
                    must_end_children.insert(*child);
                }
            }
            Self {
                parent_by_child,
                must_end_children,
                flows: LoanFlowAliases::compute(function),
            }
        }

        pub(in crate::ssa::verify_ownership) fn is_derived(
            &self,
            loan: LoanId,
            _aliases: &AliasRoots,
        ) -> bool {
            self.must_end_children
                .iter()
                .any(|child| self.flows.equivalent(loan, *child))
        }

        pub(in crate::ssa::verify_ownership) fn active_descendant(
            &self,
            parent: LoanId,
            _aliases: &AliasRoots,
            state: &BlockState,
        ) -> Option<LoanId> {
            self.active_descendant_among(parent, state.loans.iter().copied())
        }

        pub(in crate::ssa::verify_ownership) fn active_descendant_among(
            &self,
            parent: LoanId,
            active_loans: impl Iterator<Item = LoanId> + Clone,
        ) -> Option<LoanId> {
            for child in self.parent_by_child.keys() {
                let mut cursor = *child;
                let mut visited = BTreeSet::new();
                while visited.insert(cursor) {
                    let Some(next) = self.parent_by_child.get(&cursor).copied() else {
                        break;
                    };
                    if self.flows.equivalent(parent, next) {
                        if let Some(active) = active_loans
                            .clone()
                            .find(|active| self.flows.equivalent(*active, *child))
                        {
                            return Some(active);
                        }
                        break;
                    }
                    cursor = next;
                }
            }
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_map_ownership(
    module: &Module,
    function: &Function,
    operation: &Operation,
    aliases: &AliasRoots,
    reborrows: &ReborrowDependencies,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    match operation {
        Operation::MapIterationFirst { source } => {
            check_map_read_owner(
                module,
                function,
                EntityId::Loan(*source),
                state,
                location,
                origin,
                errors,
            );
        }
        Operation::MapIterationHasNext { source, cursor }
        | Operation::MapIterationNext { source, cursor }
        | Operation::MapIterationKeyLoan { source, cursor }
        | Operation::MapIterationValueLoan { source, cursor } => {
            check_map_read_owner(
                module,
                function,
                EntityId::Loan(*source),
                state,
                location.clone(),
                origin,
                errors,
            );
            require_value(
                module,
                function,
                *cursor,
                state,
                location.clone(),
                origin,
                errors,
            );
            if matches!(operation, Operation::MapIterationNext { .. })
                && let Some(dependent) = reborrows.active_descendant(*source, aliases, state)
            {
                errors.push(error(
                    VerifyErrorKind::LoanDependencyActive {
                        parent: *source,
                        dependent,
                    },
                    location,
                    origin,
                ));
            }
        }
        Operation::MapConsume { owner, .. } => {
            check_and_consume_container_owner(
                module, function, *owner, aliases, state, location, origin, errors,
            );
        }
        Operation::MapConstruct { .. } => {}
        Operation::MapResultUnwrap { result } => {
            require_value(module, function, *result, state, location, origin, errors);
        }
        Operation::MapSize { owner } => {
            check_map_read_owner(module, function, *owner, state, location, origin, errors);
        }
        Operation::MapContains { owner, key } | Operation::MapGet { owner, key } => {
            check_map_read_owner(
                module,
                function,
                *owner,
                state,
                location.clone(),
                origin,
                errors,
            );
            check_map_read_owner(module, function, *key, state, location, origin, errors);
        }
        Operation::MapRequireValue { source, key } => {
            check_map_read_owner(
                module,
                function,
                EntityId::Loan(*source),
                state,
                location.clone(),
                origin,
                errors,
            );
            check_map_read_owner(module, function, *key, state, location, origin, errors);
        }
        Operation::MapWithValue {
            source,
            key,
            action,
        } => {
            check_map_read_owner(
                module,
                function,
                EntityId::Loan(*source),
                state,
                location.clone(),
                origin,
                errors,
            );
            check_map_read_owner(
                module,
                function,
                *key,
                state,
                location.clone(),
                origin,
                errors,
            );
            check_map_read_owner(
                module,
                function,
                EntityId::Loan(*action),
                state,
                location,
                origin,
                errors,
            );
        }
        Operation::MapPut { owner, key, value } => {
            check_and_consume_container_owner(
                module,
                function,
                *owner,
                aliases,
                state,
                location.clone(),
                origin,
                errors,
            );
            consume_value(
                module,
                function,
                *key,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location.clone(),
                origin,
                errors,
            );
            consume_value(
                module,
                function,
                *value,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location,
                origin,
                errors,
            );
        }
        Operation::MapRemove { owner, key } => {
            check_and_consume_container_owner(
                module,
                function,
                *owner,
                aliases,
                state,
                location.clone(),
                origin,
                errors,
            );
            check_map_read_owner(module, function, *key, state, location, origin, errors);
        }
        _ => {}
    }
}

fn check_map_read_owner(
    module: &Module,
    function: &Function,
    owner: EntityId,
    state: &BlockState,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    match owner {
        EntityId::Value(owner) => {
            require_value(module, function, owner, state, location, origin, errors);
        }
        EntityId::Loan(loan) => {
            if !state.loans.contains(&loan) {
                errors.push(error(
                    VerifyErrorKind::LoanInactive { loan },
                    location,
                    origin,
                ));
            }
        }
        EntityId::Place(_) => unreachable!("operation contract rejects a place owner"),
    }
}
