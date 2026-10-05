//! Atomic owned-root exchanges keep root identity separate from overlap aliasing.

use super::*;

pub(super) struct Request<'a> {
    pub(super) owners: &'a [ValueId],
    pub(super) loans: &'a [LoanId],
    pub(super) replacement: Option<ValueId>,
}

pub(super) fn apply(
    module: &Module,
    function: &Function,
    request: Request<'_>,
    aliases: &AliasRoots,
    state: &mut BlockState,
    instruction: &super::super::model::Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let Request {
        owners,
        loans,
        replacement,
    } = request;
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    let before = errors.len();
    let allowed_loans = loans.iter().copied().collect::<BTreeSet<_>>();
    for (owner, loan) in owners.iter().zip(loans) {
        require_value(
            module,
            function,
            *owner,
            state,
            location.clone(),
            origin,
            errors,
        );
        if !state.loans.contains(loan) {
            errors.push(error(
                VerifyErrorKind::LoanInactive { loan: *loan },
                location.clone(),
                origin,
            ));
        }
        if !exact_root(function, *owner, *loan, aliases) {
            errors.push(error(
                VerifyErrorKind::OperationContract {
                    reason: "root exchange requires an exclusive loan of the exact owned root",
                },
                location.clone(),
                origin,
            ));
        }
        if state.loans.iter().any(|active| {
            !allowed_loans.contains(active)
                && aliases.overlap(EntityId::Value(*owner), EntityId::Loan(*active))
        }) {
            errors.push(error(
                VerifyErrorKind::OwnerLoanConflict { value: *owner },
                location.clone(),
                origin,
            ));
        }
    }
    if allowed_loans.len() != loans.len()
        || owners.iter().enumerate().any(|(index, owner)| {
            owners[..index]
                .iter()
                .any(|other| aliases.overlap(EntityId::Value(*owner), EntityId::Value(*other)))
        })
    {
        errors.push(error(
            VerifyErrorKind::OperationContract {
                reason: "root swap operands must be disjoint",
            },
            location.clone(),
            origin,
        ));
    }
    if let Some(replacement) = replacement {
        require_value(
            module,
            function,
            replacement,
            state,
            location.clone(),
            origin,
            errors,
        );
        if owners
            .iter()
            .any(|owner| aliases.overlap(EntityId::Value(*owner), EntityId::Value(replacement)))
            || has_any_value_loan(replacement, aliases, state)
        {
            errors.push(error(
                VerifyErrorKind::OwnerLoanConflict { value: replacement },
                location.clone(),
                origin,
            ));
        }
    }
    if errors.len() != before {
        return;
    }

    // Commit only once all operands have been checked; no intermediate state escapes.
    for loan in loans {
        state.loans.remove(loan);
    }
    for owner in owners.iter().copied().chain(replacement) {
        consume_value(
            module,
            function,
            owner,
            aliases,
            state,
            &BTreeSet::new(),
            &BTreeSet::new(),
            location.clone(),
            origin,
            errors,
        );
        // Copyable values are independent snapshots, but their old place identity expires too.
        state
            .places
            .retain(|place| !aliases.overlap(EntityId::Value(owner), EntityId::Place(*place)));
    }
}

pub(in crate::ssa) fn exact_place(
    function: &Function,
    owner: ValueId,
    place: PlaceId,
    aliases: &AliasRoots,
) -> bool {
    let owner_roots = &aliases.roots[&EntityId::Value(owner)];
    let place_roots = &aliases.roots[&EntityId::Place(place)];
    !owner_roots.is_empty()
        && owner_roots == place_roots
        && exact_path(
            function,
            owner,
            EntityId::Place(place),
            &mut BTreeSet::new(),
            false,
        )
}

/// Inline fields remain inside the overwritten root storage; allocation projections do not.
pub(in crate::ssa) fn exact_inline_place(
    function: &Function,
    owner: ValueId,
    place: PlaceId,
    aliases: &AliasRoots,
) -> bool {
    let owner_roots = &aliases.roots[&EntityId::Value(owner)];
    let place_roots = &aliases.roots[&EntityId::Place(place)];
    !owner_roots.is_empty()
        && owner_roots == place_roots
        && exact_path(
            function,
            owner,
            EntityId::Place(place),
            &mut BTreeSet::new(),
            true,
        )
}

fn exact_root(function: &Function, owner: ValueId, loan: LoanId, aliases: &AliasRoots) -> bool {
    // Alias equality alone is insufficient: a projected field has the same overlap roots,
    // and CFG joins can have equal origin sets with mismatched owner/loan pairings.
    let owner_roots = &aliases.roots[&EntityId::Value(owner)];
    let loan_roots = &aliases.roots[&EntityId::Loan(loan)];
    !owner_roots.is_empty()
        && owner_roots == loan_roots
        && exact_path(
            function,
            owner,
            EntityId::Loan(loan),
            &mut BTreeSet::new(),
            false,
        )
}

fn exact_path(
    function: &Function,
    owner: ValueId,
    access: EntityId,
    visiting: &mut BTreeSet<(ValueId, EntityId)>,
    allow_inline: bool,
) -> bool {
    if access == EntityId::Value(owner) {
        return true;
    }
    if !visiting.insert((owner, access)) {
        // Loops are checked coinductively; exact_root also requires a real, nonempty origin.
        return true;
    }
    let valid = match function.entity(access).expect("verified entity").definition {
        Definition::InstructionResult {
            instruction,
            index: 0,
        } => {
            match (
                &function
                    .instruction(instruction)
                    .expect("verified instruction")
                    .operation,
                access,
            ) {
                (
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Exclusive,
                    },
                    EntityId::Loan(_),
                ) => exact_path(
                    function,
                    owner,
                    EntityId::Place(*place),
                    visiting,
                    allow_inline,
                ),
                (Operation::RootPlace { owner: source }, EntityId::Place(_)) => exact_path(
                    function,
                    owner,
                    EntityId::Value(*source),
                    visiting,
                    allow_inline,
                ),
                (Operation::FieldPlace { base, .. }, EntityId::Place(_)) if allow_inline => {
                    exact_path(
                        function,
                        owner,
                        EntityId::Place(*base),
                        visiting,
                        allow_inline,
                    )
                }
                (_, EntityId::Value(_)) => {
                    expand_owner(function, owner, access, visiting, allow_inline)
                }
                _ => false,
            }
        }
        Definition::BlockParameter { block, index } => {
            let incoming = incoming_edges(function, block);
            if incoming.is_empty() {
                matches!(access, EntityId::Value(_))
                    && expand_owner(function, owner, access, visiting, allow_inline)
            } else {
                incoming.iter().all(|edge| {
                    exact_path(
                        function,
                        rebound_owner(function, owner, block, edge),
                        edge.arguments[index],
                        visiting,
                        allow_inline,
                    )
                })
            }
        }
        Definition::InstructionResult { .. } => {
            matches!(access, EntityId::Value(_))
                && expand_owner(function, owner, access, visiting, allow_inline)
        }
    };
    visiting.remove(&(owner, access));
    valid
}

fn expand_owner(
    function: &Function,
    owner: ValueId,
    access: EntityId,
    visiting: &mut BTreeSet<(ValueId, EntityId)>,
    allow_inline: bool,
) -> bool {
    let Definition::BlockParameter { block, index } = function
        .entity(EntityId::Value(owner))
        .expect("verified owner")
        .definition
    else {
        return false;
    };
    let incoming = incoming_edges(function, block);
    !incoming.is_empty()
        && incoming.iter().all(|edge| {
            let EntityId::Value(source) = edge.arguments[index] else {
                return false;
            };
            let access = match function.entity(access).expect("verified access").definition {
                Definition::BlockParameter {
                    block: access_block,
                    index,
                } if access_block == block => edge.arguments[index],
                _ => access,
            };
            exact_path(function, source, access, visiting, allow_inline)
        })
}

fn rebound_owner(function: &Function, owner: ValueId, block: BlockId, edge: &Edge) -> ValueId {
    match function
        .entity(EntityId::Value(owner))
        .expect("verified owner")
        .definition
    {
        Definition::BlockParameter {
            block: owner_block,
            index,
        } if owner_block == block => {
            let EntityId::Value(owner) = edge.arguments[index] else {
                unreachable!("verified edge value type")
            };
            owner
        }
        _ => owner,
    }
}

fn incoming_edges(function: &Function, target: BlockId) -> Vec<&Edge> {
    function
        .blocks
        .iter()
        .flat_map(|block| edges(&block.terminator.as_ref().expect("verified terminator").kind))
        .filter(|edge| edge.target == target)
        .collect()
}
