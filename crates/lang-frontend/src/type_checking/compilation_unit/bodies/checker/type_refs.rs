//! SPEC-0197 compilation-unit body 局部类型标注解析。

use crate::{
    ast::TypeRefId,
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget},
    parser::{ParameterModeMarker, TypeRef},
    type_checking::{
        CompilationUnitTypeError, DeferredReason, ExternalTypeBinding, ParameterMode,
        TypeCheckingError, UnitFunctionParameterType, UnitTypeId, UnitTypeKind, UnitTypeRefId,
    },
};

use super::BodyChecker;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BodyTypeUse {
    Runtime,
    TypeTest,
}

impl BodyChecker<'_> {
    /// 解析 `is` / `!is` 的目标类型；case refinement 只由该调用路径消费。
    pub(super) fn resolve_type_test_ref(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        self.resolve_body_type_ref_for(source, id, BodyTypeUse::TypeTest)
    }

    /// 解析首批 body-local 标注，并把结果写回 body 自己的 source-qualified facts。
    ///
    /// 泛型实参仍由后续 SPEC-0197 子切片承接；在此之前显式 fail-loud，避免
    /// 将尚未完成约束检查的类型伪装成 validated fact。
    pub(super) fn resolve_body_type_ref(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        self.resolve_body_type_ref_for(source, id, BodyTypeUse::Runtime)
    }

    fn resolve_body_type_ref_for(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
        usage: BodyTypeUse,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let key = UnitTypeRefId::new(source, id);
        if let Some(ty) = self
            .parts
            .type_ref_types
            .get(&key)
            .copied()
            .or_else(|| self.signatures.type_ref_type(key))
        {
            return Ok(ty);
        }
        let node = self
            .file(source)
            .ast()
            .type_refs()
            .get(id)
            .map_err(TypeCheckingError::from)?;
        let span = node.span();
        let mut ty = match node.payload().clone() {
            TypeRef::Error => self.error_type(),
            TypeRef::Function {
                move_span,
                parameters,
                return_type,
                ..
            } => {
                let mut resolved = Vec::with_capacity(parameters.len());
                let mut contains_deferred = false;
                let mut contains_error = false;
                for parameter in parameters {
                    let ty = self.resolve_body_type_ref(source, parameter.type_ref)?;
                    contains_deferred |= self.is_deferred(ty);
                    contains_error |= self.is_error(ty);
                    resolved.push(UnitFunctionParameterType::new(
                        parameter_mode(parameter.mode_marker),
                        ty,
                    ));
                }
                let return_type = self.resolve_body_type_ref(source, return_type)?;
                contains_deferred |= self.is_deferred(return_type);
                contains_error |= self.is_error(return_type);
                if contains_error {
                    self.error_type()
                } else if contains_deferred {
                    self.deferred_type(DeferredReason::FunctionContainsDeferred)
                } else {
                    self.signatures.types_mut().intern(UnitTypeKind::Function {
                        move_only: move_span.is_some(),
                        parameters: resolved,
                        return_type,
                    })
                }
            }
            TypeRef::Qualified {
                segments,
                nullable_span,
            } => {
                let Some(segment) = segments.last() else {
                    return Ok(self.error_type());
                };
                if !segment.arguments.is_empty() {
                    return Err(CompilationUnitTypeError::UnsupportedBody(span));
                }
                let target = self
                    .reference(source, segment.name_span, Namespace::Type)
                    .cloned();
                let mut base = match target {
                    Some(UnitReferenceTarget::Declaration(declaration)) => {
                        let Some(nominal) = self
                            .signatures
                            .declaration(declaration)
                            .and_then(|signature| signature.nominal())
                        else {
                            return Ok(self.error_type());
                        };
                        if !nominal.type_parameters().is_empty() {
                            return Err(CompilationUnitTypeError::UnsupportedBody(span));
                        }
                        self.signatures.types_mut().intern(UnitTypeKind::Nominal {
                            declaration,
                            arguments: Vec::new(),
                        })
                    }
                    Some(UnitReferenceTarget::Symbol(symbol)) => self
                        .signatures
                        .symbol_type(symbol)
                        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?,
                    Some(UnitReferenceTarget::Symbols(symbols)) if symbols.len() == 1 => self
                        .signatures
                        .symbol_type(symbols[0])
                        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?,
                    Some(UnitReferenceTarget::External(external)) => {
                        match self.environment.binding(external) {
                            Some(ExternalTypeBinding::Builtin(builtin)) => self.builtin(*builtin),
                            _ => return Err(CompilationUnitTypeError::UnsupportedBody(span)),
                        }
                    }
                    _ => return Ok(self.error_type()),
                };
                if nullable_span.is_some() && !self.is_error(base) {
                    base = self
                        .signatures
                        .types_mut()
                        .intern(UnitTypeKind::Nullable(base));
                }
                base
            }
        };
        let case_root = match self.signatures.types().get(ty) {
            Some(UnitTypeKind::EnumCase { root, .. }) => Some(*root),
            Some(UnitTypeKind::Nullable(inner)) => match self.signatures.types().get(*inner) {
                Some(UnitTypeKind::EnumCase { root, .. }) => Some(*root),
                _ => None,
            },
            _ => None,
        };
        if usage == BodyTypeUse::Runtime
            && let Some(root) = case_root
        {
            let UnitTypeKind::Nominal { declaration, .. } = self
                .signatures
                .types()
                .get(root)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
            else {
                return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
            };
            let root_span = self
                .names
                .names()
                .index()
                .declarations()
                .get(declaration.index())
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
                .name_span();
            self.emit_maybe_label(
                crate::diagnostic::codes::ENUM_CASE_TYPE_POSITION,
                "enum case type is only allowed as an is or !is target",
                span,
                Some(root_span),
                "root enum declared here",
            )?;
            ty = self.error_type();
        }
        self.parts.type_ref_types.insert(key, ty);
        Ok(ty)
    }
}

const fn parameter_mode(marker: Option<ParameterModeMarker>) -> ParameterMode {
    match marker {
        None | Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
        Some(ParameterModeMarker::Own(_)) => ParameterMode::Value,
        Some(ParameterModeMarker::Inout(_)) => ParameterMode::Inout,
    }
}
