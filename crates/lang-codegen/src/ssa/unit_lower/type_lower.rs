//! compilation-unit concrete storage type 到 SSA type identity 的映射。

use std::collections::BTreeMap;

use lang_frontend::{
    source::Span,
    type_checking::{
        BuiltinType, IntrinsicTypeConstructor, UnitTypeId, UnitTypeKind,
        ValidatedCompilationUnitTypes,
    },
};

use super::lowering_error;
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{Module, SsaTypeId, SsaTypeKind},
};

pub(super) fn intern_supported_type(
    module: &mut Module,
    typed: &ValidatedCompilationUnitTypes,
    type_ids: &mut BTreeMap<UnitTypeId, SsaTypeId>,
    ty: UnitTypeId,
    span: Span,
) -> Result<SsaTypeId, LoweringError> {
    if let Some(id) = type_ids.get(&ty).copied() {
        return Ok(id);
    }
    let kind = typed
        .types()
        .types()
        .get(ty)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
    let id = match kind {
        UnitTypeKind::Builtin(BuiltinType::Boolean) => module.intern_type(SsaTypeKind::Boolean),
        UnitTypeKind::Builtin(BuiltinType::Byte) => module.intern_type(integer_type(8, true)),
        UnitTypeKind::Builtin(BuiltinType::UByte) => module.intern_type(integer_type(8, false)),
        UnitTypeKind::Builtin(BuiltinType::Short) => module.intern_type(integer_type(16, true)),
        UnitTypeKind::Builtin(BuiltinType::UShort) => module.intern_type(integer_type(16, false)),
        UnitTypeKind::Builtin(BuiltinType::Int) => module.intern_type(integer_type(32, true)),
        UnitTypeKind::Builtin(BuiltinType::UInt) => module.intern_type(integer_type(32, false)),
        UnitTypeKind::Builtin(BuiltinType::Long) => module.intern_type(integer_type(64, true)),
        UnitTypeKind::Builtin(BuiltinType::ULong) => module.intern_type(integer_type(64, false)),
        UnitTypeKind::Builtin(BuiltinType::String) => module.add_string_owner_type(),
        UnitTypeKind::Builtin(BuiltinType::Unit) => module.intern_type(SsaTypeKind::Unit),
        UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Rc,
            arguments,
        } => {
            let [payload] = arguments.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let owner = module
                .declare_shared_owner(format!("Rc#u{}", ty.index()))
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            // 先登记 identity，使有限的嵌套 owner 可以递归建立 payload 类型。
            type_ids.insert(ty, owner);
            let payload = intern_supported_type(module, typed, type_ids, *payload, span)?;
            module
                .define_shared_owner(owner, payload)
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            return Ok(owner);
        }
        _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
    };
    type_ids.insert(ty, id);
    Ok(id)
}

pub(super) fn is_supported_storage_type(
    typed: &ValidatedCompilationUnitTypes,
    ty: UnitTypeId,
) -> bool {
    match typed.types().types().get(ty) {
        Some(UnitTypeKind::Builtin(builtin)) => matches!(
            builtin,
            BuiltinType::Boolean
                | BuiltinType::Byte
                | BuiltinType::UByte
                | BuiltinType::Short
                | BuiltinType::UShort
                | BuiltinType::Int
                | BuiltinType::UInt
                | BuiltinType::Long
                | BuiltinType::ULong
                | BuiltinType::String
                | BuiltinType::Unit
        ),
        Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Rc,
            arguments,
        }) => {
            matches!(arguments.as_slice(), [payload] if is_supported_storage_type(typed, *payload))
        }
        _ => false,
    }
}

const fn integer_type(bits: u16, signed: bool) -> SsaTypeKind {
    SsaTypeKind::Integer { bits, signed }
}
