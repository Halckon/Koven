//! SPEC-0197 compilation-unit body 局部类型标注解析。

use crate::{
    ast::TypeRefId,
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget},
    parser::TypeRef,
    type_checking::{
        CompilationUnitTypeError, ExternalTypeBinding, TypeCheckingError, UnitTypeId, UnitTypeKind,
        UnitTypeRefId,
    },
};

use super::BodyChecker;

impl BodyChecker<'_> {
    /// 解析首批 body-local 标注，并把结果写回 body 自己的 source-qualified facts。
    ///
    /// 泛型实参与函数类型仍由后续 SPEC-0197 子切片承接；在此之前显式 fail-loud，避免
    /// 将尚未完成约束检查的类型伪装成 validated fact。
    pub(super) fn resolve_body_type_ref(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
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
        let ty = match node.payload().clone() {
            TypeRef::Error => self.error_type(),
            TypeRef::Function { .. } => {
                return Err(CompilationUnitTypeError::UnsupportedBody(span));
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
        self.parts.type_ref_types.insert(key, ty);
        Ok(ty)
    }
}
