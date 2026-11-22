//! 已完成 frontend 产物到 typed SSA 的标量 lowering。
#![deny(dead_code)]

pub(in crate::ssa) mod container;
pub(in crate::ssa) mod control;
pub(in crate::ssa) use super::integer_range_contract_tests::nullable_borrowed_cleanup;
pub(in crate::ssa) use super::loan_region_ssa_tests::multi_temporary::instances;
pub(in crate::ssa) use super::loan_region_ssa_tests::receiver_receipt_siblings::map_iteration;
pub(in crate::ssa) mod nullable_if_cleanup;
pub(in crate::ssa) use nullable_if_cleanup::constant_presence;
pub(in crate::ssa) use super::map_provider_exits_pipeline_tests::temporary_exits::query_inputs::query_cleanup::nullable_numeric_cleanup;
pub(in crate::ssa) use super::checked_integer_conversion_contract_tests::outer_borrowed_while;

pub(in crate::ssa) use std::collections::BTreeMap;

pub(in crate::ssa) use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{NameResolution, SymbolId, SymbolKind},
    ownership_checking::{DropPoint, LoanTarget, OwnershipCheckedFile},
    parser::{
        AssignmentOperator, BinaryOperator as AstBinaryOperator, Expression, Item, LiteralKind,
        NameMarker, ParsedFile, Statement,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallableTarget, Copyability, NominalKind, ParameterMode, TypeId, TypeKind,
        TypedFile,
    },
};

pub(in crate::ssa) use super::model::{
    BlockId, Edge, EntityId, EntityType, Function, LoanId, LoanKind, Operation, Origin, SsaTypeId,
    TerminatorKind, ValueId,
};

pub(crate) use super::lowering_support::{LoweringError, LoweringErrorKind};
pub(in crate::ssa) use super::{expression_core::LoweredValue, lowering_support::error};

pub(in crate::ssa) fn builtin_type(typed: &TypedFile, ty: TypeId) -> Option<BuiltinType> {
    match typed.types().get(ty) {
        Some(TypeKind::Builtin(builtin)) => Some(*builtin),
        _ => None,
    }
}
