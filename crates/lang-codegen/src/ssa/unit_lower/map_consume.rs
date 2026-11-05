//! Source-qualified owned-root delivery into a readonly Map owner.

pub(super) mod nullable;

use super::*;
use crate::ssa::model::{MapContainerKind, SsaTypeKind};
use lang_frontend::{
    ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
    type_checking::{IntrinsicTypeConstructor, UnitCallReceiverOrigin},
};
