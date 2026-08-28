//! SPEC-0197 compilation-unit 局部 value-class 结构化解构。

use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::codes,
    name_resolution::{Namespace, SourceUnitId, UnitSymbolId},
    parser::NameMarker,
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, Copyability, DeferredReason, DestructuringMode,
        NominalKind, UnitDestructuringComponent, UnitDestructuringDescriptor,
        UnitFunctionParameterType, UnitStatementId, UnitTypeId, UnitTypeKind,
    },
};

use super::{BodyChecker, ExpressionCheck};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_local_destructuring(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        bindings: &[NameMarker],
        left_paren_span: Span,
        right_paren_span: Option<Span>,
        initializer: ExpressionId,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let source_type = self
            .check_expression(source, initializer, None, None, return_type)?
            .ty;
        let unit = self.builtin(BuiltinType::Unit);
        let UnitTypeKind::Nominal {
            declaration,
            arguments,
        } = self
            .signatures
            .types()
            .get(source_type)
            .cloned()
            .unwrap_or(UnitTypeKind::Error)
        else {
            let fallback = if self.is_error(source_type) {
                self.error_type()
            } else {
                self.deferred_type(DeferredReason::Destructuring)
            };
            for &binding in bindings {
                self.set_marker_symbol(source, binding, fallback);
            }
            return Ok(ExpressionCheck {
                ty: unit,
                falls_through: true,
            });
        };
        let Some(nominal) = self
            .signatures
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
            .cloned()
        else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        if nominal.kind() != NominalKind::ValueClass {
            let deferred = self.deferred_type(DeferredReason::Destructuring);
            for &binding in bindings {
                self.set_marker_symbol(source, binding, deferred);
            }
            return Ok(ExpressionCheck {
                ty: unit,
                falls_through: true,
            });
        }

        let substitutions = nominal
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments)
            .collect::<BTreeMap<_, _>>();
        let component_types = nominal
            .fields()
            .iter()
            .map(|field| self.substitute_type(field.ty(), &substitutions))
            .collect::<Result<Vec<_>, _>>()?;
        let error = self.error_type();
        let mut typed_components = Vec::new();
        let mut markers_valid = true;
        for (index, &binding) in bindings.iter().enumerate() {
            let ty = component_types.get(index).copied().unwrap_or(error);
            self.set_marker_symbol(source, binding, ty);
            let NameMarker::Present(span) = binding else {
                markers_valid = false;
                continue;
            };
            let Some(symbol) = self.symbol_at(source, span, Namespace::Value) else {
                markers_valid = false;
                continue;
            };
            if index < component_types.len() {
                typed_components.push(UnitDestructuringComponent::new(symbol, ty));
            }
        }

        if bindings.len() != component_types.len() {
            let end = right_paren_span.map_or(left_paren_span.end(), |span| span.end());
            let pattern = self
                .sources
                .span(left_paren_span.source_id(), left_paren_span.start(), end)
                .map_err(crate::type_checking::TypeCheckingError::from)?;
            let declaration_span =
                self.names.names().index().declarations()[declaration.index()].name_span();
            self.emit_maybe_label(
                codes::DESTRUCTURING_ARITY,
                "value class destructuring must bind every component exactly once",
                pattern,
                Some(declaration_span),
                format!("expected {} bindings", component_types.len()),
            )?;
        } else if markers_valid {
            let mode = match self.copyability_of(source_type) {
                Copyability::Copyable => Some(DestructuringMode::Copy),
                Copyability::MoveOnly => Some(DestructuringMode::Consume),
                Copyability::Unknown | Copyability::Error => None,
            };
            if let Some(mode) = mode {
                self.parts
                    .destructurings
                    .push(UnitDestructuringDescriptor::new(
                        UnitStatementId::new(source, statement),
                        source_type,
                        mode,
                        typed_components,
                    ));
            }
        }

        Ok(ExpressionCheck {
            ty: unit,
            falls_through: true,
        })
    }

    pub(super) fn substitute_type(
        &mut self,
        ty: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let kind = self
            .signatures
            .types()
            .get(ty)
            .cloned()
            .unwrap_or(UnitTypeKind::Error);
        let substituted = match kind {
            UnitTypeKind::TypeParameter(symbol) => {
                return Ok(substitutions.get(&symbol).copied().unwrap_or(ty));
            }
            UnitTypeKind::Nullable(inner) => {
                UnitTypeKind::Nullable(self.substitute_type(inner, substitutions)?)
            }
            UnitTypeKind::Function {
                move_only,
                parameters,
                return_type,
            } => UnitTypeKind::Function {
                move_only,
                parameters: parameters
                    .into_iter()
                    .map(|parameter| {
                        Ok(UnitFunctionParameterType::new(
                            parameter.mode(),
                            self.substitute_type(parameter.ty(), substitutions)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompilationUnitTypeError>>()?,
                return_type: self.substitute_type(return_type, substitutions)?,
            },
            UnitTypeKind::Nominal {
                declaration,
                arguments,
            } => UnitTypeKind::Nominal {
                declaration,
                arguments: arguments
                    .into_iter()
                    .map(|argument| self.substitute_type(argument, substitutions))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            UnitTypeKind::Intrinsic {
                constructor,
                arguments,
            } => UnitTypeKind::Intrinsic {
                constructor,
                arguments: arguments
                    .into_iter()
                    .map(|argument| self.substitute_type(argument, substitutions))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            UnitTypeKind::StaticSelf(interface) => {
                UnitTypeKind::StaticSelf(self.substitute_type(interface, substitutions)?)
            }
            other => return Ok(self.signatures.types_mut().intern(other)),
        };
        Ok(self.signatures.types_mut().intern(substituted))
    }
}
