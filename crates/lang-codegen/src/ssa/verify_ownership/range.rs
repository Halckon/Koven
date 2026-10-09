//! Descriptor metadata and protecting source loan have a single explicit lifetime.
use super::*;

mod pairs;
pub(super) mod source;
fn pair(function: &Function, view: ValueId) -> Option<LoanId> {
    pairs::compute(function)
        .into_iter()
        .find_map(|(value, loan)| (value == view).then_some(loan))
}

pub(super) fn live_descriptor(
    function: &Function,
    source: LoanId,
    state: &BlockState,
) -> Option<ValueId> {
    state
        .values
        .iter()
        .copied()
        .find(|view| pair(function, *view) == Some(source))
}

fn require_source(
    source: LoanId,
    state: &BlockState,
    location: VerifyLocation,
    origin: &super::super::model::Origin,
    errors: &mut Vec<VerifyError>,
) -> bool {
    if state.loans.contains(&source) {
        true
    } else {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan: source },
            location,
            origin,
        ));
        false
    }
}

fn require_root(
    function: &Function,
    loan: LoanId,
    state: &BlockState,
    location: VerifyLocation,
    origin: &super::super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    let flows = LoanFlowAliases::compute(function);
    if source::capability(function, loan).is_none_or(|root| {
        !state
            .loans
            .iter()
            .any(|active| flows.equivalent(*active, root))
    }) {
        errors.push(error(
            VerifyErrorKind::OperationContract {
                reason: "range source has no live exact root capability",
            },
            location,
            origin,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply(
    module: &Module,
    function: &Function,
    instruction: &super::super::model::Instruction,
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    dependencies: &ReborrowDependencies,
    state: &mut BlockState,
    errors: &mut Vec<VerifyError>,
) {
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    match &instruction.operation {
        Operation::RangeElementPlace { view, index } => {
            require_source(*view, state, location.clone(), origin, errors);
            require_value(module, function, *index, state, location, origin, errors);
        }
        Operation::RangeConstruct { source, .. } => {
            require_source(*source, state, location.clone(), origin, errors);
            require_root(function, *source, state, location, origin, errors);
        }
        Operation::RangeCall {
            source, arguments, ..
        } => {
            require_source(*source, state, location.clone(), origin, errors);
            require_root(function, *source, state, location.clone(), origin, errors);
            for argument in arguments {
                match argument {
                    EntityId::Loan(loan) => {
                        require_source(*loan, state, location.clone(), origin, errors);
                    }
                    EntityId::Value(value) => {
                        consume_value(
                            module,
                            function,
                            *value,
                            aliases,
                            state,
                            &BTreeSet::new(),
                            &BTreeSet::new(),
                            location.clone(),
                            origin,
                            errors,
                        );
                    }
                    EntityId::Place(place) => {
                        require_place(*place, state, location.clone(), origin, errors);
                    }
                }
            }
        }
        Operation::RangeLength {
            view: EntityId::Value(view),
        } => {
            require_value(
                module,
                function,
                *view,
                state,
                location.clone(),
                origin,
                errors,
            );
            if let Some(source) = pair(function, *view) {
                require_source(source, state, location, origin, errors);
            } else {
                errors.push(error(
                    VerifyErrorKind::ReturnType { index: 0 },
                    location,
                    origin,
                ));
            }
        }
        Operation::RangeLength {
            view: EntityId::Loan(loan),
        } => {
            require_source(*loan, state, location, origin, errors);
        }
        Operation::RangeEnd { view, source } => {
            if pair(function, *view) != Some(*source) {
                errors.push(error(
                    VerifyErrorKind::ReturnType { index: 0 },
                    location,
                    origin,
                ));
                return;
            }
            if consume_value(
                module,
                function,
                *view,
                aliases,
                state,
                &BTreeSet::new(),
                &BTreeSet::new(),
                location.clone(),
                origin,
                errors,
            ) {
                end_source(
                    function,
                    *source,
                    aliases,
                    closure_loans,
                    dependencies,
                    state,
                    location,
                    origin,
                    errors,
                );
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn end_source(
    function: &Function,
    source: LoanId,
    aliases: &AliasRoots,
    closure_loans: &closure::ClosureLoans,
    dependencies: &ReborrowDependencies,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &super::super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    if let Some(owner) = live_descriptor(function, source, state)
        .or_else(|| closure_loans.live_owner_holding(source, state))
    {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value: owner },
            location,
            origin,
        ));
    } else if let Some(dependent) = dependencies.active_descendant(source, aliases, state) {
        errors.push(error(
            VerifyErrorKind::LoanDependencyActive {
                parent: source,
                dependent,
            },
            location,
            origin,
        ));
    } else if !state.loans.remove(&source) {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan: source },
            location,
            origin,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_return(
    module: &Module,
    function: &Function,
    view: ValueId,
    source: LoanId,
    aliases: &AliasRoots,
    dependencies: &ReborrowDependencies,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &super::super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    let Some((parameter, _, _)) = super::super::verify_operation::range_parameter(module, function)
    else {
        return;
    };
    if pair(function, view) != Some(source)
        || aliases.roots.get(&EntityId::Loan(source))
            != Some(&BTreeSet::from([EntityId::Loan(parameter)]))
    {
        errors.push(error(
            VerifyErrorKind::ReturnType { index: 0 },
            location,
            origin,
        ));
        return;
    }
    if !require_source(source, state, location.clone(), origin, errors) {
        return;
    }
    if dependencies
        .active_descendant(source, aliases, state)
        .is_some()
    {
        errors.push(error(
            VerifyErrorKind::ReturnType { index: 0 },
            location,
            origin,
        ));
        return;
    }
    if !consume_value(
        module,
        function,
        view,
        aliases,
        state,
        &BTreeSet::new(),
        &BTreeSet::new(),
        location.clone(),
        origin,
        errors,
    ) {
        return;
    }
    let mut current = source;
    let mut visited = BTreeSet::new();
    while visited.insert(current) {
        state.loans.remove(&current);
        if dependencies.flows.equivalent(current, parameter) {
            return;
        }
        let Some(parent) = dependencies.parent_by_child.get(&current).copied() else {
            break;
        };
        current = parent;
    }
    errors.push(error(
        VerifyErrorKind::ReturnType { index: 0 },
        location,
        origin,
    ));
}
