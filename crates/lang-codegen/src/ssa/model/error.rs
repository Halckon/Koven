//! Typed SSA construction errors and their existing diagnostic text.
use super::{BlockId, EntityId, FunctionId, ModuleId, SsaTypeId};
use std::{error::Error, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModelError {
    WrongTypeOwner {
        expected: ModuleId,
        actual: ModuleId,
    },
    WrongFunctionOwner {
        expected: FunctionId,
        actual: FunctionId,
    },
    UnknownBlock {
        block: BlockId,
    },
    UnknownType {
        ty: SsaTypeId,
    },
    InvalidDeinit {
        owner: SsaTypeId,
        function: FunctionId,
    },
    DeinitAlreadyDefined {
        owner: SsaTypeId,
    },
    EmptyTypeName,
    DuplicateTypeName {
        name: String,
    },
    ExpectedHeapOwner {
        ty: SsaTypeId,
    },
    ExpectedSharedOwner {
        ty: SsaTypeId,
    },
    ExpectedPointerLikeOwner {
        ty: SsaTypeId,
    },
    ExpectedInlineNullableInner {
        ty: SsaTypeId,
    },
    ExpectedAggregate {
        ty: SsaTypeId,
    },
    ExpectedMapContainer {
        ty: SsaTypeId,
    },
    EmptyClosureCaptures,
    InvalidCallableReturnArity,
    InvalidCallableParameter,
    InvalidClosureEnvironment,
    TypeAlreadyDefined {
        ty: SsaTypeId,
    },
    UnknownEntity {
        entity: EntityId,
    },
    BlockAlreadyTerminated {
        block: BlockId,
    },
    TerminatorAlreadySet {
        block: BlockId,
    },
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongTypeOwner { expected, actual } => {
                write!(
                    formatter,
                    "type belongs to {actual:?}, expected {expected:?}"
                )
            }
            Self::WrongFunctionOwner { expected, actual } => {
                write!(
                    formatter,
                    "entity belongs to {actual:?}, expected {expected:?}"
                )
            }
            Self::UnknownBlock { block } => write!(formatter, "unknown block {block:?}"),
            Self::UnknownType { ty } => write!(formatter, "unknown type {ty:?}"),
            Self::InvalidDeinit { owner, function } => write!(
                formatter,
                "deinit {function:?} must accept only a shared receiver for heap owner {owner:?} and return Unit"
            ),
            Self::DeinitAlreadyDefined { owner } => {
                write!(formatter, "heap owner {owner:?} already has a deinit")
            }
            Self::EmptyTypeName => write!(formatter, "named SSA type must not be empty"),
            Self::DuplicateTypeName { name } => {
                write!(formatter, "duplicate named SSA type {name:?}")
            }
            Self::ExpectedHeapOwner { ty } => write!(formatter, "type {ty:?} is not a heap owner"),
            Self::ExpectedSharedOwner { ty } => {
                write!(formatter, "type {ty:?} is not a shared owner")
            }
            Self::ExpectedPointerLikeOwner { ty } => {
                write!(formatter, "type {ty:?} is not a pointer-like owner")
            }
            Self::ExpectedInlineNullableInner { ty } => {
                write!(
                    formatter,
                    "type {ty:?} cannot carry an inline nullable wrapper"
                )
            }
            Self::ExpectedAggregate { ty } => write!(formatter, "type {ty:?} is not an aggregate"),
            Self::ExpectedMapContainer { ty } => {
                write!(
                    formatter,
                    "type {ty:?} is not a defined MoveOnly Map container"
                )
            }
            Self::EmptyClosureCaptures => {
                write!(formatter, "concrete closure must capture a value")
            }
            Self::InvalidCallableReturnArity => {
                write!(
                    formatter,
                    "callable signature supports at most one result and no result when it never returns"
                )
            }
            Self::InvalidCallableParameter => {
                write!(formatter, "callable signature parameters cannot be places")
            }
            Self::InvalidClosureEnvironment => {
                write!(
                    formatter,
                    "closure environment fields do not match its captures"
                )
            }
            Self::TypeAlreadyDefined { ty } => write!(formatter, "type {ty:?} is already defined"),
            Self::UnknownEntity { entity } => write!(formatter, "unknown entity {entity:?}"),
            Self::BlockAlreadyTerminated { block } => {
                write!(formatter, "cannot append to terminated block {block:?}")
            }
            Self::TerminatorAlreadySet { block } => {
                write!(formatter, "block {block:?} already has a terminator")
            }
        }
    }
}

impl Error for ModelError {}
