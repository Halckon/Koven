//! Direct heap-field exchange preserves the parent owner and proves exact field provenance.

use super::*;

pub(super) struct Request {
    pub(super) owner: ValueId,
    pub(super) field: usize,
    pub(super) loan: LoanId,
    pub(super) replacement: ValueId,
}

pub(super) fn apply(
    module: &Module,
    function: &Function,
    request: Request,
    aliases: &AliasRoots,
    state: &mut BlockState,
    instruction: &super::super::model::Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let Request {
        owner,
        field,
        loan,
        replacement,
    } = request;
    let location = VerifyLocation::Instruction(instruction.id);
    let origin = &instruction.origin;
    let before = errors.len();
    require_value(
        module,
        function,
        owner,
        state,
        location.clone(),
        origin,
        errors,
    );
    require_value(
        module,
        function,
        replacement,
        state,
        location.clone(),
        origin,
        errors,
    );
    if !state.loans.contains(&loan) {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan },
            location.clone(),
            origin,
        ));
    }
    let owner_roots = &aliases.roots[&EntityId::Value(owner)];
    let loan_roots = &aliases.roots[&EntityId::Loan(loan)];
    if owner_roots.is_empty()
        || owner_roots != loan_roots
        || !exact_path(
            function,
            owner,
            EntityId::Loan(loan),
            field,
            Stage::Loan,
            &mut BTreeSet::new(),
        )
    {
        errors.push(error(VerifyErrorKind::OperationContract {
            reason: "field exchange requires an exclusive loan of the exact direct owned heap field",
        }, location.clone(), origin));
    }
    if state.loans.iter().any(|active| {
        *active != loan && aliases.overlap(EntityId::Value(owner), EntityId::Loan(*active))
    }) {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value: owner },
            location.clone(),
            origin,
        ));
    }
    if aliases.overlap(EntityId::Value(owner), EntityId::Value(replacement))
        || has_any_value_loan(replacement, aliases, state)
    {
        errors.push(error(
            VerifyErrorKind::OwnerLoanConflict { value: replacement },
            location.clone(),
            origin,
        ));
    }
    if errors.len() != before {
        return;
    }

    // The parent remains a single complete owner. Only replacement and loan are consumed;
    // result registration gives the old field a fresh, independent owner identity.
    state.loans.remove(&loan);
    consume_value(
        module,
        function,
        replacement,
        aliases,
        state,
        &BTreeSet::new(),
        &BTreeSet::new(),
        location,
        origin,
        errors,
    );
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    Loan,
    Field,
    Payload,
    Owner,
}

type Visit = (ValueId, EntityId, Stage);

fn exact_path(
    function: &Function,
    owner: ValueId,
    access: EntityId,
    field: usize,
    stage: Stage,
    visiting: &mut BTreeSet<Visit>,
) -> bool {
    if stage == Stage::Owner && access == EntityId::Value(owner) {
        return true;
    }
    if !visiting.insert((owner, access, stage)) {
        // A loop may revisit the same proof obligation. The caller additionally requires
        // nonempty matching origins, and every incoming edge is still checked.
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
                stage,
                access,
            ) {
                (
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Exclusive,
                    },
                    Stage::Loan,
                    EntityId::Loan(_),
                ) => exact_path(
                    function,
                    owner,
                    EntityId::Place(*place),
                    field,
                    Stage::Field,
                    visiting,
                ),
                (
                    Operation::FieldPlace {
                        base,
                        field: actual,
                    },
                    Stage::Field,
                    EntityId::Place(_),
                ) if *actual == field => exact_path(
                    function,
                    owner,
                    EntityId::Place(*base),
                    field,
                    Stage::Payload,
                    visiting,
                ),
                (
                    Operation::HeapPayloadPlace { owner: source },
                    Stage::Payload,
                    EntityId::Place(_),
                ) => exact_path(
                    function,
                    owner,
                    EntityId::Value(*source),
                    field,
                    Stage::Owner,
                    visiting,
                ),
                (_, Stage::Owner, EntityId::Value(_)) => {
                    expand_owner(function, owner, access, field, stage, visiting)
                }
                _ => false,
            }
        }
        Definition::BlockParameter { block, index } => {
            let incoming = incoming_edges(function, block);
            if incoming.is_empty() {
                stage == Stage::Owner
                    && matches!(access, EntityId::Value(_))
                    && expand_owner(function, owner, access, field, stage, visiting)
            } else {
                incoming.iter().all(|edge| {
                    exact_path(
                        function,
                        rebound_owner(function, owner, block, edge),
                        edge.arguments[index],
                        field,
                        stage,
                        visiting,
                    )
                })
            }
        }
        Definition::InstructionResult { .. } => {
            stage == Stage::Owner
                && matches!(access, EntityId::Value(_))
                && expand_owner(function, owner, access, field, stage, visiting)
        }
    };
    visiting.remove(&(owner, access, stage));
    valid
}

fn expand_owner(
    function: &Function,
    owner: ValueId,
    access: EntityId,
    field: usize,
    stage: Stage,
    visiting: &mut BTreeSet<Visit>,
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
            exact_path(function, source, access, field, stage, visiting)
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
