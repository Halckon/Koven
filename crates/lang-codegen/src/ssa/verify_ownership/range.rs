//! Descriptor metadata and protecting source loan have a single explicit lifetime.
use super::*;

pub(super) mod pairs {
    use super::*;

    pub(super) fn compute(function: &Function) -> BTreeSet<(ValueId, LoanId)> {
        let seeds = function
            .instructions
            .iter()
            .filter_map(|instruction| {
                if !matches!(
                    instruction.operation,
                    Operation::RangeConstruct { .. } | Operation::RangeCall { .. }
                ) {
                    return None;
                }
                let [EntityId::Value(view), EntityId::Loan(loan)] = instruction.results.as_slice()
                else {
                    return None;
                };
                Some((*view, *loan))
            })
            .collect::<BTreeSet<_>>();
        if seeds.is_empty() {
            return seeds;
        }
        let seed_types = seeds
            .iter()
            .filter_map(|(view, loan)| {
                Some((
                    function.entity(EntityId::Value(*view))?.ty,
                    function.entity(EntityId::Loan(*loan))?.ty,
                ))
            })
            .collect::<Vec<_>>();
        let mut incoming = BTreeMap::<_, Vec<_>>::new();
        for block in &function.blocks {
            for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind) {
                incoming.entry(edge.target).or_default().push(edge);
            }
        }
        let mut candidates = seeds.clone();
        let mut parameters = Vec::new();
        for block in &function.blocks {
            if !incoming.contains_key(&block.id) {
                continue;
            }
            for (vi, entity) in block.parameters.iter().enumerate() {
                let EntityId::Value(view) = entity else {
                    continue;
                };
                for (li, entity) in block.parameters.iter().enumerate() {
                    let EntityId::Loan(loan) = entity else {
                        continue;
                    };
                    if seed_types.iter().any(|(v, l)| {
                        function.entity(EntityId::Value(*view)).map(|d| d.ty) == Some(*v)
                            && function.entity(EntityId::Loan(*loan)).map(|d| d.ty) == Some(*l)
                    }) {
                        candidates.insert((*view, *loan));
                        parameters.push((block.id, vi, li, *view, *loan));
                    }
                }
            }
        }
        // Greatest fixed point keeps a loop only if every predecessor transports the same pair.
        loop {
            let invalid = parameters
                .iter()
                .filter_map(|(block, vi, li, view, loan)| {
                    let valid = incoming[block].iter().all(|edge| {
                        match (edge.arguments.get(*vi), edge.arguments.get(*li)) {
                            (Some(EntityId::Value(v)), Some(EntityId::Loan(l))) => {
                                candidates.contains(&(*v, *l))
                            }
                            _ => false,
                        }
                    });
                    (!valid).then_some((*view, *loan))
                })
                .collect::<Vec<_>>();
            let mut changed = false;
            for pair in invalid {
                changed |= candidates.remove(&pair);
            }
            if !changed {
                break;
            }
        }
        // Then require a path from a real construct/call; a self-supporting cycle is no proof.
        let mut reachable = seeds;
        loop {
            let mut changed = false;
            for (block, vi, li, view, loan) in &parameters {
                if !candidates.contains(&(*view, *loan)) {
                    continue;
                }
                if incoming[block].iter().any(|edge| matches!((edge.arguments.get(*vi),edge.arguments.get(*li)),(Some(EntityId::Value(v)),Some(EntityId::Loan(l))) if reachable.contains(&(*v,*l)))) {
                    changed|=reachable.insert((*view,*loan));
                }
            }
            if !changed {
                break;
            }
        }
        reachable
    }

    pub(in crate::ssa::verify_ownership) fn is_view(
        module: &Module,
        function: &Function,
        loan: LoanId,
    ) -> bool {
        function.entity(EntityId::Loan(loan)).is_some_and(|data| {
            matches!(
                module.type_kind(data.ty.semantic_type()),
                Some(crate::ssa::model::SsaTypeKind::RangeView { .. })
            )
        })
    }

    pub(in crate::ssa::verify_ownership) fn root(
        function: &Function,
        loan: LoanId,
    ) -> Option<LoanId> {
        let pairs = compute(function);
        let flows = LoanFlowAliases::compute(function);
        resolve(function, loan, &pairs, &flows, &mut BTreeSet::new(), true)
    }

    /// A child's capability remains live even after the original caller loan and parent metadata end.
    pub(in crate::ssa::verify_ownership) fn capability(
        function: &Function,
        loan: LoanId,
    ) -> Option<LoanId> {
        let pairs = compute(function);
        let flows = LoanFlowAliases::compute(function);
        resolve(function, loan, &pairs, &flows, &mut BTreeSet::new(), false)
    }

    fn resolve(
        function: &Function,
        loan: LoanId,
        pairs: &BTreeSet<(ValueId, LoanId)>,
        flows: &LoanFlowAliases,
        visiting: &mut BTreeSet<LoanId>,
        flatten: bool,
    ) -> Option<LoanId> {
        if !visiting.insert(loan) {
            return None;
        }
        let result = (|| match function.entity(EntityId::Loan(loan))?.definition {
            Definition::BlockParameter { block, .. } if block.index() == 0 => Some(loan),
            Definition::BlockParameter { .. } => {
                let mut roots = BTreeSet::new();
                for origin in flows.origins.get(&loan)? {
                    roots.insert(resolve(function, *origin, pairs, flows, visiting, flatten)?);
                }
                (roots.len() == 1).then(|| *roots.first().expect("one root"))
            }
            Definition::InstructionResult { instruction, .. } => {
                match function.instruction(instruction)?.operation {
                    Operation::SharedReborrow { source } | Operation::BorrowCall { source, .. } => {
                        resolve(function, source, pairs, flows, visiting, flatten)
                    }
                    Operation::RangeConstruct { source, .. }
                    | Operation::RangeCall { source, .. } => {
                        if flatten {
                            resolve(function, source, pairs, flows, visiting, flatten)
                        } else {
                            Some(loan)
                        }
                    }
                    Operation::BorrowBegin { place, .. }
                        if pairs.iter().any(|(view, _)| {
                            function
                                .entity(EntityId::Value(*view))
                                .map(|data| data.ty.semantic_type())
                                == function
                                    .entity(EntityId::Loan(loan))
                                    .map(|data| data.ty.semantic_type())
                        }) =>
                    {
                        let Definition::InstructionResult { instruction, .. } =
                            function.entity(EntityId::Place(place))?.definition
                        else {
                            return None;
                        };
                        let Operation::RootPlace { owner } =
                            function.instruction(instruction)?.operation
                        else {
                            return None;
                        };
                        let mut roots = BTreeSet::new();
                        for (_, source) in pairs.iter().filter(|(view, _)| *view == owner) {
                            roots.insert(if flatten {
                                resolve(function, *source, pairs, flows, visiting, flatten)?
                            } else {
                                *source
                            });
                        }
                        (roots.len() == 1).then(|| *roots.first().expect("one root"))
                    }
                    Operation::BorrowBegin { .. } => Some(loan),
                    _ => None,
                }
            }
        })();
        visiting.remove(&loan);
        result
    }
}
pub(super) use pairs as source;
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
    if pairs::capability(function, loan).is_none_or(|root| {
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
                        closure::check_borrowed_contents(
                            *loan,
                            closure_loans,
                            state,
                            instruction.id,
                            origin,
                            errors,
                            "range call Borrow requires proved current callable capture contents",
                        );
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
