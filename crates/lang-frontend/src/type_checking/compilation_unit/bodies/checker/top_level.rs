//! SPEC-0197 compilation-unit 顶层 variable/constant initializer 类型检查。

use crate::{
    ast::TypeRefId,
    diagnostic::codes,
    name_resolution::{DeclarationId, SourceUnitId, SymbolKind, UnitSymbolId},
    parser::{Item, VariableKind},
    source::Span,
    type_checking::{
        DeferredReason, TypeCheckingError, UnitExpressionId, UnitTypeId, UnitTypeKind,
        constant_value::accepts_constant_type,
    },
};

use super::{BodyChecker, CompilationUnitTypeError};

impl BodyChecker<'_> {
    pub(super) fn check_top_level_initializer(
        &mut self,
        declaration: DeclarationId,
        source: SourceUnitId,
        item: &Item,
        item_span: Span,
    ) -> Result<(), CompilationUnitTypeError> {
        let (type_ref, initializer) = match item {
            Item::Variable {
                type_ref,
                initializer,
                ..
            }
            | Item::Constant {
                type_ref,
                initializer,
                ..
            } => (*type_ref, *initializer),
            _ => return Err(CompilationUnitTypeError::MissingDeclarationSymbol),
        };
        if let Item::Variable { kind, .. } = item {
            // Guide v0.46: top-level values are `const val` only. The initializer is still
            // checked below so later diagnostics recover, but the unit cannot validate.
            let keyword = self.top_level_variable_keyword(item_span, *kind)?;
            self.emit(
                codes::UNSUPPORTED_TOP_LEVEL_RUNTIME_VALUE,
                "top-level runtime values are not supported; use `const val` or construct the value in `main` and pass it as an argument",
                keyword,
            )?;
        }
        let signature = self
            .signatures
            .declaration(declaration)
            .cloned()
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        self.check_value_initializer(source, signature.symbol(), type_ref, initializer)
    }

    pub(super) fn check_value_initializer(
        &mut self,
        source: SourceUnitId,
        symbol: UnitSymbolId,
        type_ref: Option<TypeRefId>,
        initializer: crate::ast::ExpressionId,
    ) -> Result<(), CompilationUnitTypeError> {
        if !self.checking_constants && self.constant_prechecked.contains(&symbol) {
            return Ok(());
        }
        let expected = type_ref
            .map(|_| {
                self.signatures
                    .symbol_type(symbol)
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
            })
            .transpose()?;
        let expected_span = type_ref
            .map(|type_ref| self.top_level_type_ref_span(source, type_ref))
            .transpose()?;

        self.flow_facts.clear();
        self.flow_versions.clear();
        self.current_return_span = None;
        let return_type = self.error_type();
        let before = self.diagnostics.len();
        let result =
            self.check_expression(source, initializer, expected, expected_span, return_type)?;
        let ty = expected.unwrap_or(result.ty);
        self.parts.symbol_types.insert(symbol, ty);
        let constant = self.names.source_units()[source.index()]
            .resolution()
            .symbols()[symbol.symbol().index()]
        .kind()
            == SymbolKind::Constant;
        // Signature resolution precedes body diagnostics and may wrap Error inside
        // nullable/container types. Preserve the annotation's original diagnostic.
        let signature_error = expected_span.is_some_and(|annotation| {
            self.signatures.diagnostics().iter().any(|diagnostic| {
                let span = diagnostic.primary_span();
                span.source_id() == annotation.source_id()
                    && annotation.start() <= span.start()
                    && span.end() <= annotation.end()
            })
        });
        if constant
            && !signature_error
            && self.diagnostics.len() == before
            && !self.constant_type_has_error(ty)
        {
            let allowed = match self.signatures.types().get(ty) {
                Some(UnitTypeKind::Error) => true,
                Some(UnitTypeKind::Deferred(reason)) => {
                    *reason != DeferredReason::AnyValueRepresentation
                }
                Some(UnitTypeKind::Builtin(ty)) => accepts_constant_type(*ty),
                _ => false,
            };
            if !allowed {
                let span = match expected_span {
                    Some(span) => span,
                    None => self
                        .file(source)
                        .ast()
                        .expressions()
                        .get(initializer)
                        .map_err(TypeCheckingError::from)?
                        .span(),
                };
                self.emit(
                    codes::INVALID_CONSTANT_TYPE,
                    "constant type must be Boolean, an integer, Char, or String",
                    span,
                )?;
            } else {
                self.check_constant_expression(UnitExpressionId::new(source, initializer))?;
            }
        }
        Ok(())
    }

    /// Recovery types can carry an upstream Error through a cross-file constant reference.
    fn constant_type_has_error(&self, root: UnitTypeId) -> bool {
        let mut pending = vec![root];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }
            match self.signatures.types().get(ty) {
                Some(UnitTypeKind::Error) | None => return true,
                Some(UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner)) => {
                    pending.push(*inner)
                }
                Some(
                    UnitTypeKind::Nominal { arguments, .. }
                    | UnitTypeKind::Intrinsic { arguments, .. },
                ) => pending.extend(arguments),
                Some(UnitTypeKind::Function {
                    parameters,
                    return_type,
                    ..
                }) => {
                    pending.push(*return_type);
                    pending.extend(parameters.iter().map(|parameter| parameter.ty()));
                }
                Some(UnitTypeKind::EnumCase { root, .. }) => pending.push(*root),
                _ => {}
            }
        }
        false
    }

    /// `val`/`var` 关键字范围：未包裹的 Variable item 恰好从该关键字开始。
    fn top_level_variable_keyword(
        &self,
        item_span: Span,
        kind: VariableKind,
    ) -> Result<Span, CompilationUnitTypeError> {
        let length = match kind {
            VariableKind::Val | VariableKind::Var => 3,
            VariableKind::BorrowVal(marker) => {
                return Ok(marker);
            }
        };
        self.sources
            .span(
                item_span.source_id(),
                item_span.start(),
                item_span.start() + length,
            )
            .map_err(TypeCheckingError::from)
            .map_err(CompilationUnitTypeError::from)
    }

    fn top_level_type_ref_span(
        &self,
        source: SourceUnitId,
        type_ref: TypeRefId,
    ) -> Result<Span, CompilationUnitTypeError> {
        self.file(source)
            .ast()
            .type_refs()
            .get(type_ref)
            .map(|node| node.span())
            .map_err(TypeCheckingError::from)
            .map_err(CompilationUnitTypeError::from)
    }
}
