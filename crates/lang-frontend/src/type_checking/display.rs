//! Display's built-in witnesses and the canonical nominal interface identity.
use crate::name_resolution::DeclarationId;

use super::{BuiltinType, UnitTypeId, UnitTypeKind, UnitTypeTable};

pub(super) fn builtin_witness(ty: BuiltinType) -> bool {
    matches!(
        ty,
        BuiltinType::Byte
            | BuiltinType::Short
            | BuiltinType::Int
            | BuiltinType::Long
            | BuiltinType::UByte
            | BuiltinType::UShort
            | BuiltinType::UInt
            | BuiltinType::ULong
            | BuiltinType::Float
            | BuiltinType::Double
            | BuiltinType::Boolean
            | BuiltinType::Char
            | BuiltinType::String
    )
}

pub(super) fn is_interface(
    types: &UnitTypeTable,
    ty: UnitTypeId,
    display: Option<DeclarationId>,
) -> bool {
    matches!(types.get(ty), Some(UnitTypeKind::Nominal { declaration, arguments })
        if arguments.is_empty() && Some(*declaration) == display)
}
