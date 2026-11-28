mod dominance;

pub(super) use dominance::compute_dominators;
pub(in crate::ssa) use dominance::verify_dominance;
use std::{error::Error, fmt};

use super::model::{
    BlockId, Definition, EntityId, EntityType, Function, FunctionId, InstructionId, Module,
    ModuleId, Origin, Program, SsaTypeId, SsaTypeKind, TerminatorKind,
};

pub(in crate::ssa) use super::closure::closure_content;
pub(in crate::ssa) use super::verify_ownership::field_exchange::closure_escape;
use super::verify_types::module;
pub(in crate::ssa) use super::verify_types::module as content_flow;

pub(in crate::ssa) use closure_escape::TypeCaptures;

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
        module::verify_module(module, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(VerifyErrors { errors })
    }
}

pub(in crate::ssa) fn verify_function_structure(
    module: &Module,
    function: &Function,
    errors: &mut Vec<VerifyError>,
) {
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

    if function.never_returns
        && (!function.return_types.is_empty()
            || function.borrow_return.is_some()
            || function.carrier_return.is_some())
    {
        errors.push(function_error(
            function,
            VerifyErrorKind::OperationContract {
                reason: "non-returning function cannot declare a result",
            },
        ));
    }
    module::verify_callable_view_returns(module, function, errors);
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
    module::verify_callable_view_entity(module, entity, data, errors);
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
    } else if module.type_kind(ty) == Some(&SsaTypeKind::Uninhabited) {
        errors.push(VerifyError {
            kind: VerifyErrorKind::OperationContract {
                reason: "uninhabited payload cannot be an SSA entity or function result",
            },
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

pub(in crate::ssa) fn verify_cfg_types(
    module: &Module,
    function: &Function,
    errors: &mut Vec<VerifyError>,
) {
    for block in &function.blocks {
        let terminator = block
            .terminator
            .as_ref()
            .expect("structure phase proved terminator existence");
        for instruction_id in &block.instructions {
            let instruction = function
                .instruction(*instruction_id)
                .expect("structure phase proved instruction existence");
            super::verify_operation::verify_operation(module, function, instruction, errors);
        }
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
                    EntityId::Value(*owner),
                    when_non_null,
                    *view,
                    &terminator.origin,
                    errors,
                );
            }
            TerminatorKind::NullableLoanBranch {
                source,
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
                    EntityId::Loan(*source),
                    when_non_null,
                    *view,
                    &terminator.origin,
                    errors,
                );
            }
            TerminatorKind::Return { values } => {
                if function.borrow_return.is_some() || function.never_returns {
                    errors.push(VerifyError {
                        kind: VerifyErrorKind::ReturnType { index: 0 },
                        location: VerifyLocation::Terminator(block.id),
                        origin: Some(terminator.origin.clone()),
                    });
                }
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
            TerminatorKind::Unreachable => {
                let final_call = block
                    .instructions
                    .last()
                    .and_then(|&id| function.instruction(id));
                if !final_call.is_some_and(|instruction| {
                    super::verify_operation::callable::call_never_returns(
                        module,
                        function,
                        &instruction.operation,
                    )
                }) {
                    errors.push(VerifyError {
                        kind: VerifyErrorKind::OperationContract {
                            reason: "unreachable requires a final non-returning call",
                        },
                        location: VerifyLocation::Terminator(block.id),
                        origin: Some(terminator.origin.clone()),
                    });
                }
            }
            TerminatorKind::RangeReturn { view, source } => {
                super::verify_operation::range_return_contract(
                    module,
                    function,
                    *view,
                    *source,
                    block.id,
                    &terminator.origin,
                    errors,
                );
            }
            TerminatorKind::BorrowReturn { loan } => {
                super::verify_borrow_result::return_contract(
                    function,
                    *loan,
                    block.id,
                    &terminator.origin,
                    errors,
                );
            }
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
    nullable: EntityId,
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
    let inner = function
        .entity(nullable)
        .and_then(|entity| match entity.ty {
            EntityType::Value(owner) => module.nullable_inner(owner),
            EntityType::Loan { target, .. } => {
                if let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(target) {
                    return matches!(
                        module.type_kind(*inner),
                        Some(SsaTypeKind::SharedOwner { .. })
                    )
                    .then_some(*inner);
                }
                module.nullable_string_payload(target)
            }
            _ => None,
        });
    let Some(inner) = inner else {
        errors.push(VerifyError {
            kind: VerifyErrorKind::OperationContract {
                reason: "nullable branch requires a nullable owner, a String? loan, or an Rc? loan",
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
    // The borrowed proof has one definite parent. Sharing its implicit parameter
    // between different branch births would lose that identity.
    let unique_source = matches!(nullable, EntityId::Loan(_)).then_some(source);
    if !expected_view
        || !non_null_target_is_proof_closed(function, edge.target, view, unique_source)
    {
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
    unique_source: Option<BlockId>,
) -> bool {
    if unique_source.is_some() && target.index() == 0 {
        return false;
    }
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
            }
            | TerminatorKind::NullableLoanBranch {
                when_null,
                when_non_null,
                view,
                ..
            } => {
                when_null.target != target
                    && (when_non_null.target != target
                        || (*view == expected_view
                            && unique_source.is_none_or(|source| source == block.id)))
            }
            TerminatorKind::Return { .. }
            | TerminatorKind::RangeReturn { .. }
            | TerminatorKind::BorrowReturn { .. }
            | TerminatorKind::Abort
            | TerminatorKind::Unreachable => true,
        }
    })
}

pub(in crate::ssa) fn function_error(function: &Function, kind: VerifyErrorKind) -> VerifyError {
    VerifyError {
        kind,
        location: VerifyLocation::Function(function.id),
        origin: Some(function.origin.clone()),
    }
}

pub(in crate::ssa) fn entity_error(
    entity: EntityId,
    origin: Origin,
    kind: VerifyErrorKind,
) -> VerifyError {
    VerifyError {
        kind,
        location: VerifyLocation::Entity(entity),
        origin: Some(origin),
    }
}
