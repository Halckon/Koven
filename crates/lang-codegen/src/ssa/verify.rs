use std::{collections::BTreeSet, error::Error, fmt};

use super::model::{
    BlockId, Definition, EntityId, EntityType, Function, FunctionId, InstructionId, Module,
    ModuleId, Origin, Program, SsaTypeId, SsaTypeKind, TerminatorKind,
};

pub(in crate::ssa) mod closure_content;
mod closure_escape;
mod content_flow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum VerifyLocation {
    Module(ModuleId),
    Type(SsaTypeId),
    Function(FunctionId),
    Block(BlockId),
    Instruction(InstructionId),
    Terminator(BlockId),
    Edge { source: BlockId, successor: usize },
    Entity(EntityId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum VerifyErrorKind {
    InvalidModuleId,
    InvalidFunctionId,
    MissingEntryBlock,
    EntryHasPredecessor,
    InvalidBlockId,
    InvalidInstructionId,
    UnknownBlock,
    UnknownInstruction,
    UnknownEntity,
    UnknownType(SsaTypeId),
    InvalidTypeDefinition {
        reason: &'static str,
    },
    WrongOwner,
    InstructionPlacement,
    MissingTerminator,
    InvalidEntityDefinition,
    InvalidEntityType,
    EdgeArity {
        expected: usize,
        actual: usize,
    },
    EdgeType {
        index: usize,
    },
    ConditionType,
    ReturnArity {
        expected: usize,
        actual: usize,
    },
    ReturnType {
        index: usize,
    },
    OperationContract {
        reason: &'static str,
    },
    UseBeforeDefinition {
        entity: EntityId,
    },
    NonDominatingUse {
        entity: EntityId,
    },
    HiddenLinearLiveIn {
        entity: EntityId,
    },
    CopyMoveOnly {
        value: super::model::ValueId,
    },
    DropCopyable {
        value: super::model::ValueId,
    },
    MoveOnlyPlaceRead {
        entity: EntityId,
    },
    ValueUnavailable {
        value: super::model::ValueId,
    },
    PlaceUnavailable {
        place: super::model::PlaceId,
    },
    LoanInactive {
        loan: super::model::LoanId,
    },
    LoanDependencyActive {
        parent: super::model::LoanId,
        dependent: super::model::LoanId,
    },
    BorrowConflict {
        place: super::model::PlaceId,
    },
    MutationConflict {
        place: super::model::PlaceId,
    },
    OwnerLoanConflict {
        value: super::model::ValueId,
    },
    NullableProofMismatch {
        owner: super::model::ValueId,
        proof: super::model::LoanId,
    },
    MissingOwnedExit {
        value: super::model::ValueId,
    },
    ActiveLoanAtExit {
        loan: super::model::LoanId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct VerifyError {
    pub(super) kind: VerifyErrorKind,
    pub(super) location: VerifyLocation,
    pub(super) origin: Option<Origin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifyErrors {
    pub(super) errors: Vec<VerifyError>,
}

impl fmt::Display for VerifyErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "SSA verification failed with {} error(s)",
            self.errors.len()
        )
    }
}

impl Error for VerifyErrors {}

pub(crate) fn verify_program(program: &Program) -> Result<(), VerifyErrors> {
    let mut errors = Vec::new();
    for (module_index, module) in program.modules.iter().enumerate() {
        if module.id.index() != module_index || program.module(module.id).is_none() {
            errors.push(VerifyError {
                kind: VerifyErrorKind::InvalidModuleId,
                location: VerifyLocation::Module(module.id),
                origin: None,
            });
            continue;
        }
        verify_module(module, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(VerifyErrors { errors })
    }
}

fn verify_module(module: &Module, errors: &mut Vec<VerifyError>) {
    let module_before = errors.len();
    super::verify_types::verify_types(module, errors);
    super::deinit::verify_deinits(module, errors);
    let capture_types =
        (errors.len() == module_before).then(|| closure_escape::TypeCaptures::compute(module));

    for (function_index, function) in module.functions.iter().enumerate() {
        if function.id.module() != module.id || function.id.index() != function_index {
            errors.push(VerifyError {
                kind: VerifyErrorKind::InvalidFunctionId,
                location: VerifyLocation::Function(function.id),
                origin: Some(function.origin.clone()),
            });
            continue;
        }
        let before = errors.len();
        verify_function_structure(module, function, errors);
        if errors.len() == before {
            verify_cfg_types(module, function, errors);
            verify_dominance(function, errors);
            // Ownership content proofs require a validated module type/deinit graph.
            if errors.len() == before
                && let Some(types) = &capture_types
            {
                super::verify_ownership::verify_ownership(module, function, errors);
                if errors.len() == before {
                    closure_escape::verify(function, types, errors);
                }
            }
        }
    }
}

fn verify_function_structure(module: &Module, function: &Function, errors: &mut Vec<VerifyError>) {
    if function.blocks.is_empty() {
        errors.push(function_error(function, VerifyErrorKind::MissingEntryBlock));
        return;
    }

    if let Some(receiver) = function.receiver {
        let valid_receiver = !matches!(receiver, EntityType::Place(_))
            && function.blocks[0]
                .parameters
                .first()
                .and_then(|parameter| function.entity(*parameter))
                .is_some_and(|parameter| parameter.ty == receiver);
        if !valid_receiver {
            errors.push(function_error(
                function,
                VerifyErrorKind::OperationContract {
                    reason: "instance receiver must be the first entry parameter with the declared mode and type",
                },
            ));
        }
    }

    for ty in &function.return_types {
        verify_type(
            module,
            *ty,
            VerifyLocation::Function(function.id),
            &function.origin,
            errors,
        );
    }

    for (index, block) in function.blocks.iter().enumerate() {
        if block.id.function() != function.id || block.id.index() != index {
            errors.push(VerifyError {
                kind: VerifyErrorKind::InvalidBlockId,
                location: VerifyLocation::Block(block.id),
                origin: Some(block.origin.clone()),
            });
        }
        if block.terminator.is_none() {
            errors.push(VerifyError {
                kind: VerifyErrorKind::MissingTerminator,
                location: VerifyLocation::Block(block.id),
                origin: Some(block.origin.clone()),
            });
        }
    }

    for (index, instruction) in function.instructions.iter().enumerate() {
        if instruction.id.function() != function.id || instruction.id.index() != index {
            errors.push(VerifyError {
                kind: VerifyErrorKind::InvalidInstructionId,
                location: VerifyLocation::Instruction(instruction.id),
                origin: Some(instruction.origin.clone()),
            });
        }
    }

    let mut placements = vec![0_usize; function.instructions.len()];
    for block in &function.blocks {
        for instruction_id in &block.instructions {
            if instruction_id.function() != function.id {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::WrongOwner,
                    location: VerifyLocation::Block(block.id),
                    origin: Some(block.origin.clone()),
                });
                continue;
            }
            let Some(instruction) = function.instruction(*instruction_id) else {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::UnknownInstruction,
                    location: VerifyLocation::Block(block.id),
                    origin: Some(block.origin.clone()),
                });
                continue;
            };
            placements[instruction_id.index()] += 1;
            if instruction.block != block.id {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::InstructionPlacement,
                    location: VerifyLocation::Instruction(*instruction_id),
                    origin: Some(instruction.origin.clone()),
                });
            }
        }
    }
    for (index, count) in placements.into_iter().enumerate() {
        if count != 1 {
            let instruction = &function.instructions[index];
            errors.push(VerifyError {
                kind: VerifyErrorKind::InstructionPlacement,
                location: VerifyLocation::Instruction(instruction.id),
                origin: Some(instruction.origin.clone()),
            });
        }
    }

    verify_entity_table(module, function, errors);

    for block in &function.blocks {
        for (index, entity) in block.parameters.iter().enumerate() {
            verify_entity_reference(
                function,
                *entity,
                VerifyLocation::Block(block.id),
                &block.origin,
                errors,
            );
            let expected = Definition::BlockParameter {
                block: block.id,
                index,
            };
            if function
                .entity(*entity)
                .is_some_and(|data| data.definition != expected)
            {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::InvalidEntityDefinition,
                    location: VerifyLocation::Entity(*entity),
                    origin: function.entity(*entity).map(|data| data.origin.clone()),
                });
            }
        }
        for instruction_id in &block.instructions {
            let Some(instruction) = function.instruction(*instruction_id) else {
                continue;
            };
            for (index, entity) in instruction.results.iter().enumerate() {
                verify_entity_reference(
                    function,
                    *entity,
                    VerifyLocation::Instruction(instruction.id),
                    &instruction.origin,
                    errors,
                );
                let expected = Definition::InstructionResult {
                    instruction: instruction.id,
                    index,
                };
                if function
                    .entity(*entity)
                    .is_some_and(|data| data.definition != expected)
                {
                    errors.push(VerifyError {
                        kind: VerifyErrorKind::InvalidEntityDefinition,
                        location: VerifyLocation::Entity(*entity),
                        origin: function.entity(*entity).map(|data| data.origin.clone()),
                    });
                }
            }
            for entity in instruction.operation.entities() {
                verify_entity_reference(
                    function,
                    entity,
                    VerifyLocation::Instruction(instruction.id),
                    &instruction.origin,
                    errors,
                );
            }
        }
        let Some(terminator) = &block.terminator else {
            continue;
        };
        for target in terminator.kind.targets() {
            if target.function() != function.id {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::WrongOwner,
                    location: VerifyLocation::Terminator(block.id),
                    origin: Some(terminator.origin.clone()),
                });
            } else if function.block(target).is_none() {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::UnknownBlock,
                    location: VerifyLocation::Terminator(block.id),
                    origin: Some(terminator.origin.clone()),
                });
            }
        }
        for entity in terminator.kind.entities() {
            verify_entity_reference(
                function,
                entity,
                VerifyLocation::Terminator(block.id),
                &terminator.origin,
                errors,
            );
        }
    }
}

fn verify_entity_table(module: &Module, function: &Function, errors: &mut Vec<VerifyError>) {
    for (index, data) in function.values.iter().enumerate() {
        let entity = EntityId::Value(super::model::ValueId {
            function: function.id,
            index,
        });
        if !matches!(data.ty, EntityType::Value(_)) {
            errors.push(entity_error(
                entity,
                data.origin.clone(),
                VerifyErrorKind::InvalidEntityType,
            ));
        }
        verify_entity_data(module, function, entity, data, errors);
    }
    for (index, data) in function.places.iter().enumerate() {
        let entity = EntityId::Place(super::model::PlaceId {
            function: function.id,
            index,
        });
        if !matches!(data.ty, EntityType::Place(_)) {
            errors.push(entity_error(
                entity,
                data.origin.clone(),
                VerifyErrorKind::InvalidEntityType,
            ));
        }
        verify_entity_data(module, function, entity, data, errors);
    }
    for (index, data) in function.loans.iter().enumerate() {
        let entity = EntityId::Loan(super::model::LoanId {
            function: function.id,
            index,
        });
        if !matches!(data.ty, EntityType::Loan { .. }) {
            errors.push(entity_error(
                entity,
                data.origin.clone(),
                VerifyErrorKind::InvalidEntityType,
            ));
        }
        verify_entity_data(module, function, entity, data, errors);
    }
}

fn verify_entity_data(
    module: &Module,
    function: &Function,
    entity: EntityId,
    data: &super::model::EntityData,
    errors: &mut Vec<VerifyError>,
) {
    verify_type(
        module,
        data.ty.semantic_type(),
        VerifyLocation::Entity(entity),
        &data.origin,
        errors,
    );
    let valid = match data.definition {
        Definition::BlockParameter { block, index } => function
            .block(block)
            .and_then(|block| block.parameters.get(index))
            .is_some_and(|candidate| *candidate == entity),
        Definition::InstructionResult { instruction, index } => function
            .instruction(instruction)
            .and_then(|instruction| instruction.results.get(index))
            .is_some_and(|candidate| *candidate == entity),
    };
    if !valid {
        errors.push(entity_error(
            entity,
            data.origin.clone(),
            VerifyErrorKind::InvalidEntityDefinition,
        ));
    }
}

fn verify_type(
    module: &Module,
    ty: SsaTypeId,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    if ty.module() != module.id {
        errors.push(VerifyError {
            kind: VerifyErrorKind::WrongOwner,
            location,
            origin: Some(origin.clone()),
        });
    } else if module.type_kind(ty).is_none() {
        errors.push(VerifyError {
            kind: VerifyErrorKind::UnknownType(ty),
            location,
            origin: Some(origin.clone()),
        });
    }
}

fn verify_entity_reference(
    function: &Function,
    entity: EntityId,
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    if entity.function() != function.id {
        errors.push(VerifyError {
            kind: VerifyErrorKind::WrongOwner,
            location,
            origin: Some(origin.clone()),
        });
    } else if function.entity(entity).is_none() {
        errors.push(VerifyError {
            kind: VerifyErrorKind::UnknownEntity,
            location,
            origin: Some(origin.clone()),
        });
    }
}

fn verify_cfg_types(module: &Module, function: &Function, errors: &mut Vec<VerifyError>) {
    for block in &function.blocks {
        for instruction_id in &block.instructions {
            let instruction = function
                .instruction(*instruction_id)
                .expect("structure phase proved instruction existence");
            super::verify_operation::verify_operation(module, function, instruction, errors);
        }
        let terminator = block
            .terminator
            .as_ref()
            .expect("structure phase proved terminator existence");
        if terminator
            .kind
            .targets()
            .iter()
            .any(|target| target.index() == 0)
        {
            errors.push(VerifyError {
                kind: VerifyErrorKind::EntryHasPredecessor,
                location: VerifyLocation::Terminator(block.id),
                origin: Some(terminator.origin.clone()),
            });
        }
        match &terminator.kind {
            TerminatorKind::Branch(edge) => {
                verify_edge(function, block.id, 0, edge, &terminator.origin, errors);
            }
            TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                let condition_type = function
                    .entity(EntityId::Value(*condition))
                    .expect("structure phase proved condition existence")
                    .ty
                    .semantic_type();
                if !matches!(module.type_kind(condition_type), Some(SsaTypeKind::Boolean)) {
                    errors.push(VerifyError {
                        kind: VerifyErrorKind::ConditionType,
                        location: VerifyLocation::Terminator(block.id),
                        origin: Some(terminator.origin.clone()),
                    });
                }
                verify_edge(function, block.id, 0, when_true, &terminator.origin, errors);
                verify_edge(
                    function,
                    block.id,
                    1,
                    when_false,
                    &terminator.origin,
                    errors,
                );
            }
            TerminatorKind::NullableBranch {
                owner,
                when_null,
                when_non_null,
                view,
            } => {
                verify_edge(function, block.id, 0, when_null, &terminator.origin, errors);
                verify_non_null_edge(
                    module,
                    function,
                    block.id,
                    1,
                    *owner,
                    when_non_null,
                    *view,
                    &terminator.origin,
                    errors,
                );
            }
            TerminatorKind::Return { values } => {
                if values.len() != function.return_types.len() {
                    errors.push(VerifyError {
                        kind: VerifyErrorKind::ReturnArity {
                            expected: function.return_types.len(),
                            actual: values.len(),
                        },
                        location: VerifyLocation::Terminator(block.id),
                        origin: Some(terminator.origin.clone()),
                    });
                } else {
                    for (index, (value, expected)) in
                        values.iter().zip(&function.return_types).enumerate()
                    {
                        let actual = function
                            .entity(EntityId::Value(*value))
                            .expect("structure phase proved return value existence")
                            .ty;
                        if actual != EntityType::Value(*expected) {
                            errors.push(VerifyError {
                                kind: VerifyErrorKind::ReturnType { index },
                                location: VerifyLocation::Terminator(block.id),
                                origin: Some(terminator.origin.clone()),
                            });
                        }
                    }
                }
            }
            TerminatorKind::Abort => {}
        }
    }
}

fn verify_edge(
    function: &Function,
    source: BlockId,
    successor: usize,
    edge: &super::model::Edge,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    let target = function
        .block(edge.target)
        .expect("structure phase proved edge target existence");
    let location = VerifyLocation::Edge { source, successor };
    if edge.arguments.len() != target.parameters.len() {
        errors.push(VerifyError {
            kind: VerifyErrorKind::EdgeArity {
                expected: target.parameters.len(),
                actual: edge.arguments.len(),
            },
            location,
            origin: Some(origin.clone()),
        });
        return;
    }
    for (index, (argument, parameter)) in edge.arguments.iter().zip(&target.parameters).enumerate()
    {
        let argument_type = function
            .entity(*argument)
            .expect("structure phase proved edge argument existence")
            .ty;
        let parameter_type = function
            .entity(*parameter)
            .expect("structure phase proved block parameter existence")
            .ty;
        if argument_type != parameter_type {
            errors.push(VerifyError {
                kind: VerifyErrorKind::EdgeType { index },
                location: location.clone(),
                origin: Some(origin.clone()),
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn verify_non_null_edge(
    module: &Module,
    function: &Function,
    source: BlockId,
    successor: usize,
    owner: super::model::ValueId,
    edge: &super::model::Edge,
    view: super::model::LoanId,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    let target = function
        .block(edge.target)
        .expect("structure phase proved edge target existence");
    let location = VerifyLocation::Edge { source, successor };
    if target.parameters.len() != edge.arguments.len() + 1 {
        errors.push(VerifyError {
            kind: VerifyErrorKind::EdgeArity {
                expected: target.parameters.len(),
                actual: edge.arguments.len() + 1,
            },
            location,
            origin: Some(origin.clone()),
        });
        return;
    }
    for (index, (argument, parameter)) in edge.arguments.iter().zip(&target.parameters).enumerate()
    {
        let argument_type = function
            .entity(*argument)
            .expect("structure phase proved edge argument existence")
            .ty;
        let parameter_type = function
            .entity(*parameter)
            .expect("structure phase proved block parameter existence")
            .ty;
        if argument_type != parameter_type {
            errors.push(VerifyError {
                kind: VerifyErrorKind::EdgeType { index },
                location: location.clone(),
                origin: Some(origin.clone()),
            });
        }
    }
    let Some(inner) = function
        .entity(EntityId::Value(owner))
        .and_then(|entity| module.nullable_inner(entity.ty.semantic_type()))
    else {
        errors.push(VerifyError {
            kind: VerifyErrorKind::OperationContract {
                reason: "nullable branch owner must be a nullable handle",
            },
            location,
            origin: Some(origin.clone()),
        });
        return;
    };
    let expected_view = target.parameters.last().copied() == Some(EntityId::Loan(view))
        && function.entity(EntityId::Loan(view)).is_some_and(|entity| {
            entity.ty
                == EntityType::Loan {
                    kind: super::model::LoanKind::Shared,
                    target: inner,
                }
        });
    if !expected_view || !non_null_target_is_proof_closed(function, edge.target, view) {
        errors.push(VerifyError {
            kind: VerifyErrorKind::EdgeType {
                index: edge.arguments.len(),
            },
            location,
            origin: Some(origin.clone()),
        });
    }
}

fn non_null_target_is_proof_closed(
    function: &Function,
    target: BlockId,
    expected_view: super::model::LoanId,
) -> bool {
    function.blocks.iter().all(|block| {
        let Some(terminator) = &block.terminator else {
            return true;
        };
        match &terminator.kind {
            TerminatorKind::Branch(edge) => edge.target != target,
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => when_true.target != target && when_false.target != target,
            TerminatorKind::NullableBranch {
                when_null,
                when_non_null,
                view,
                ..
            } => {
                when_null.target != target
                    && (when_non_null.target != target || *view == expected_view)
            }
            TerminatorKind::Return { .. } | TerminatorKind::Abort => true,
        }
    })
}

fn verify_dominance(function: &Function, errors: &mut Vec<VerifyError>) {
    let dominators = compute_dominators(function);
    let instruction_positions = instruction_positions(function);
    for block in &function.blocks {
        for (position, instruction_id) in block.instructions.iter().enumerate() {
            let instruction = function
                .instruction(*instruction_id)
                .expect("structure phase proved instruction existence");
            for entity in instruction.operation.entities() {
                verify_entity_dominates(
                    function,
                    entity,
                    block.id,
                    Some(position),
                    &dominators,
                    &instruction_positions,
                    VerifyLocation::Instruction(instruction.id),
                    &instruction.origin,
                    errors,
                );
            }
        }
        let terminator = block
            .terminator
            .as_ref()
            .expect("structure phase proved terminator existence");
        for entity in terminator.kind.entities() {
            verify_entity_dominates(
                function,
                entity,
                block.id,
                None,
                &dominators,
                &instruction_positions,
                VerifyLocation::Terminator(block.id),
                &terminator.origin,
                errors,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn verify_entity_dominates(
    function: &Function,
    entity: EntityId,
    use_block: BlockId,
    use_position: Option<usize>,
    dominators: &[BTreeSet<usize>],
    instruction_positions: &[usize],
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    let definition = function
        .entity(entity)
        .expect("structure phase proved entity existence")
        .definition;
    let (definition_block, definition_position) = match definition {
        Definition::BlockParameter { block, .. } => (block, None),
        Definition::InstructionResult { instruction, .. } => {
            let block = function
                .instruction(instruction)
                .expect("structure phase proved definition existence")
                .block;
            (block, Some(instruction_positions[instruction.index()]))
        }
    };
    if definition_block == use_block {
        if let Some(definition_position) = definition_position
            && use_position.is_some_and(|use_position| definition_position >= use_position)
        {
            errors.push(VerifyError {
                kind: VerifyErrorKind::UseBeforeDefinition { entity },
                location,
                origin: Some(origin.clone()),
            });
        }
    } else if !dominators[use_block.index()].contains(&definition_block.index()) {
        errors.push(VerifyError {
            kind: VerifyErrorKind::NonDominatingUse { entity },
            location,
            origin: Some(origin.clone()),
        });
    }
}

fn instruction_positions(function: &Function) -> Vec<usize> {
    let mut positions = vec![0; function.instructions.len()];
    for block in &function.blocks {
        for (position, instruction) in block.instructions.iter().enumerate() {
            positions[instruction.index()] = position;
        }
    }
    positions
}

fn compute_dominators(function: &Function) -> Vec<BTreeSet<usize>> {
    let block_count = function.blocks.len();
    let mut predecessors = vec![Vec::new(); block_count];
    for block in &function.blocks {
        let terminator = block
            .terminator
            .as_ref()
            .expect("structure phase proved terminator existence");
        for target in terminator.kind.targets() {
            predecessors[target.index()].push(block.id.index());
        }
    }
    for predecessor_list in &mut predecessors {
        predecessor_list.sort_unstable();
        predecessor_list.dedup();
    }

    let reachable = reachable_blocks(function);
    let all_reachable = reachable
        .iter()
        .enumerate()
        .filter_map(|(index, reachable)| reachable.then_some(index))
        .collect::<BTreeSet<_>>();
    let mut dominators = (0..block_count)
        .map(|index| {
            if index == 0 || !reachable[index] {
                BTreeSet::from([index])
            } else {
                all_reachable.clone()
            }
        })
        .collect::<Vec<_>>();

    loop {
        let mut changed = false;
        for block in 1..block_count {
            if !reachable[block] {
                continue;
            }
            let mut incoming = predecessors[block]
                .iter()
                .filter(|predecessor| reachable[**predecessor]);
            let mut next = incoming
                .next()
                .map(|predecessor| dominators[*predecessor].clone())
                .unwrap_or_default();
            for predecessor in incoming {
                next = next
                    .intersection(&dominators[*predecessor])
                    .copied()
                    .collect();
            }
            next.insert(block);
            if next != dominators[block] {
                dominators[block] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    dominators
}

fn reachable_blocks(function: &Function) -> Vec<bool> {
    let mut reachable = vec![false; function.blocks.len()];
    let mut pending = vec![0_usize];
    while let Some(block_index) = pending.pop() {
        if reachable[block_index] {
            continue;
        }
        reachable[block_index] = true;
        let block = &function.blocks[block_index];
        let terminator = block
            .terminator
            .as_ref()
            .expect("structure phase proved terminator existence");
        let mut targets = terminator.kind.targets();
        targets.sort_by_key(|target| target.index());
        for target in targets.into_iter().rev() {
            pending.push(target.index());
        }
    }
    reachable
}

fn function_error(function: &Function, kind: VerifyErrorKind) -> VerifyError {
    VerifyError {
        kind,
        location: VerifyLocation::Function(function.id),
        origin: Some(function.origin.clone()),
    }
}

fn entity_error(entity: EntityId, origin: Origin, kind: VerifyErrorKind) -> VerifyError {
    VerifyError {
        kind,
        location: VerifyLocation::Entity(entity),
        origin: Some(origin),
    }
}
