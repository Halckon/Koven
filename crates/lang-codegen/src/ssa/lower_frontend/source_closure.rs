//! Source-qualified concrete callable layouts and capture formation.

mod abi;
mod declare;
mod formation;

use super::instances::{FunctionInstancePlan, resolve_concrete_type};
use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, builtin_type, error,
    nominal::NominalTypeMapper, value,
};
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{
        ClosureCaptureMode as SsaCaptureMode, EntityId, EntityType, FunctionId, LoanKind,
        Operation, SsaTypeId,
    },
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{NameResolution, SymbolId},
    ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource, OwnershipCheckedFile,
    },
    parser::{Expression, ParsedFile},
    source::Span,
    type_checking::{BuiltinType, TypeId, TypeKind, TypedFile},
};
use std::collections::BTreeMap;

pub(super) use declare::{Inputs, declare};

#[derive(Clone, Copy)]
pub(super) struct CapturePlan {
    pub(super) symbol: SymbolId,
    pub(super) ty: SsaTypeId,
    pub(super) field_type: SsaTypeId,
    pub(super) mode: SsaCaptureMode,
    pub(super) effect: ClosureCaptureEffect,
    pub(super) span: Span,
}

#[derive(Clone)]
pub(super) struct ClosurePlan {
    pub(super) owner: SourceToken,
    pub(super) expression: ExpressionId,
    pub(super) callable: SsaTypeId,
    pub(super) thunk: FunctionId,
    pub(super) body: StatementId,
    pub(super) span: Span,
    pub(super) captures: Vec<CapturePlan>,
    pub(super) parameter_symbols: Vec<SymbolId>,
    pub(super) return_type: TypeId,
    pub(super) result_expression: Option<ExpressionId>,
    pub(super) substitutions: BTreeMap<SymbolId, TypeId>,
}

#[derive(Default)]
pub(super) struct CallableLayouts {
    pub(super) lambdas: BTreeMap<(SourceToken, usize), ClosurePlan>,
    pub(super) named: BTreeMap<SourceToken, SsaTypeId>,
}

#[cfg(test)]
mod tests;
