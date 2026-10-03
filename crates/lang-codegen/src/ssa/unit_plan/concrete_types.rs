//! 仅查找 frontend 已发布的 canonical type，保留 preflight 与正式替换的各自边界。

use std::collections::BTreeMap;

use lang_frontend::{
    name_resolution::UnitSymbolId,
    source::Span,
    type_checking::{CompilationUnitTypes, UnitTypeId, UnitTypeKind},
};

use super::lowering_error;
use crate::ssa::{LoweringError, LoweringErrorKind};

/// 只查找 frontend 已发布的 canonical specialization，不创建新的 concrete type。
pub(super) fn specialize_preflight_type(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    static_self: Option<UnitTypeId>,
    substitutions: Option<&BTreeMap<UnitSymbolId, UnitTypeId>>,
) -> Option<UnitTypeId> {
    match typed.types().get(ty)? {
        UnitTypeKind::TypeParameter(parameter) => substitutions
            .and_then(|substitutions| substitutions.get(parameter).copied())
            .or(Some(ty)),
        UnitTypeKind::StaticSelf(_) => static_self,
        UnitTypeKind::Nullable(inner) => {
            let inner = specialize_preflight_type(typed, *inner, static_self, substitutions)?;
            typed.types().find(&UnitTypeKind::Nullable(inner))
        }
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            let arguments = arguments
                .iter()
                .map(|&argument| {
                    specialize_preflight_type(typed, argument, static_self, substitutions)
                })
                .collect::<Option<Vec<_>>>()?;
            typed.types().find(&UnitTypeKind::Nominal {
                declaration: *declaration,
                arguments,
            })
        }
        UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        } => {
            let arguments = arguments
                .iter()
                .map(|&argument| {
                    specialize_preflight_type(typed, argument, static_self, substitutions)
                })
                .collect::<Option<Vec<_>>>()?;
            typed.types().find(&UnitTypeKind::Intrinsic {
                constructor: *constructor,
                arguments,
            })
        }
        _ => Some(ty),
    }
}

pub(crate) fn resolve_concrete_type(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    static_self: Option<UnitTypeId>,
    span: Span,
) -> Result<UnitTypeId, LoweringError> {
    match typed.types().get(ty) {
        Some(UnitTypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
        Some(UnitTypeKind::StaticSelf(_)) => {
            static_self.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(UnitTypeKind::Nullable(inner)) => {
            let inner = resolve_concrete_type(typed, *inner, substitutions, static_self, span)?;
            typed
                .types()
                .find(&UnitTypeKind::Nullable(inner))
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) => {
            let arguments = arguments
                .iter()
                .map(|argument| {
                    resolve_direct_type_argument(typed, *argument, substitutions, static_self, span)
                })
                .collect::<Result<Vec<_>, _>>()?;
            typed
                .types()
                .find(&UnitTypeKind::Nominal {
                    declaration: *declaration,
                    arguments,
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }
}

pub(super) fn resolve_direct_type_argument(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    static_self: Option<UnitTypeId>,
    span: Span,
) -> Result<UnitTypeId, LoweringError> {
    match typed.types().get(ty) {
        Some(UnitTypeKind::TypeParameter(parameter)) => substitutions
            .get(parameter)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
        Some(UnitTypeKind::StaticSelf(_)) => {
            static_self.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }
        Some(kind) if contains_type_parameter(typed, kind) => {
            Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
        }
        Some(_) => Ok(ty),
        None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
    }
}

pub(super) fn contains_type_parameter(typed: &CompilationUnitTypes, kind: &UnitTypeKind) -> bool {
    let contains = |ty| {
        typed
            .types()
            .get(ty)
            .is_some_and(|kind| contains_type_parameter(typed, kind))
    };
    match kind {
        UnitTypeKind::TypeParameter(_) => true,
        UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner) => contains(*inner),
        UnitTypeKind::Function {
            parameters,
            return_type,
            ..
        } => parameters.iter().any(|parameter| contains(parameter.ty())) || contains(*return_type),
        UnitTypeKind::Nominal { arguments, .. } | UnitTypeKind::Intrinsic { arguments, .. } => {
            arguments.iter().copied().any(contains)
        }
        UnitTypeKind::EnumCase { root, .. } => contains(*root),
        UnitTypeKind::Builtin(_)
        | UnitTypeKind::Capability(_)
        | UnitTypeKind::IntegerLiteral(_)
        | UnitTypeKind::Deferred(_)
        | UnitTypeKind::Error => false,
    }
}
