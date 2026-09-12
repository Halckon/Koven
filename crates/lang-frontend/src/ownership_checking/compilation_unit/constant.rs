//! Runtime use plans remain private until the constant ownership capability is complete.
use crate::{
    ownership_checking::ConstantMaterializationKind, type_checking::UnitConstantUseDescriptor,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UnitConstantMaterializationPlan {
    pub(super) descriptor: UnitConstantUseDescriptor,
    pub(super) kind: ConstantMaterializationKind,
}
