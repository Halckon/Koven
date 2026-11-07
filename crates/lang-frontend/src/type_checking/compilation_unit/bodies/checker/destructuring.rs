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
        NominalKind, UnitDestructuringComponent, UnitDestructuringDescriptor, UnitExpressionId,
        UnitStatementId, UnitTypeId, UnitTypeKind,
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
        let deferred = self.deferred_facts.len();
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
        let initializer = UnitExpressionId::new(source, initializer);
        let (copyability, pending_fields) = self.copyability_and_pending_fields(source_type);
        let pending = pending_fields
            || self.storage_value_is_pending(initializer)
            || self.replacement_value_pending(initializer, source_type, deferred);
        let declaration_span = self.names.index().declarations()[declaration.index()].name_span();
        let matches = crate::type_checking::destructuring::local_field_matches(
            self.sources,
            bindings,
            nominal.fields().iter().map(|field| field.name()),
        )?;
        let private_pattern = matches.iter().any(|(_, index)| {
            index.is_some_and(|index| {
                self.private_field_inaccessible(declaration, &nominal.fields()[index])
            })
        });
        let mut typed_components = vec![None; component_types.len()];
        let mut markers_valid = matches.len() == bindings.len();
        for (span, index) in matches {
            let Some(symbol) = self.symbol_at(source, span, Namespace::Value) else {
                markers_valid = false;
                continue;
            };
            let Some(index) = index else {
                markers_valid = false;
                let error = self.error_type();
                self.record_symbol_type(symbol, error);
                self.set_immutable_binding_pending(symbol, false);
                self.emit_maybe_label(
                    codes::UNRESOLVED_NAME,
                    "unknown value class destructuring field",
                    span,
                    Some(declaration_span),
                    "field must be declared by this value class",
                )?;
                continue;
            };
            let field = &nominal.fields()[index];
            if self.private_field_inaccessible(declaration, field) {
                self.emit_private_member_access(span, field.span())?;
            }
            if private_pattern {
                // No binding receives a value when the complete split is inaccessible.
                let error = self.error_type();
                self.record_symbol_type(symbol, error);
                self.set_immutable_binding_pending(symbol, false);
                continue;
            }
            let ty = component_types[index];
            self.record_symbol_type(symbol, ty);
            self.set_immutable_binding_pending(symbol, pending);
            markers_valid &= typed_components[index]
                .replace(UnitDestructuringComponent::new(symbol, ty))
                .is_none();
        }

        if bindings.len() != component_types.len() {
            let end = right_paren_span.map_or(left_paren_span.end(), |span| span.end());
            let pattern = self
                .sources
                .span(left_paren_span.source_id(), left_paren_span.start(), end)
                .map_err(crate::type_checking::TypeCheckingError::from)?;
            self.emit_maybe_label(
                codes::DESTRUCTURING_ARITY,
                "value class destructuring must bind every component exactly once",
                pattern,
                Some(declaration_span),
                format!("expected {} bindings", component_types.len()),
            )?;
        } else if markers_valid && !private_pattern && !pending {
            let mode = match copyability {
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
                        typed_components.into_iter().flatten().collect(),
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
        super::type_graph::substitute(
            self.signatures.types_mut(),
            ty,
            substitutions,
            &mut BTreeMap::new(),
        )
    }
}
