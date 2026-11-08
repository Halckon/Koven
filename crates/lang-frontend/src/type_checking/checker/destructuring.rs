use std::collections::BTreeMap;

use crate::parser::NameMarker;

use super::*;
use crate::type_checking::{DestructuringComponent, DestructuringDescriptor, DestructuringMode};

impl Checker<'_> {
    pub(super) fn check_local_destructuring(
        &mut self,
        statement: StatementId,
        bindings: &[NameMarker],
        left_paren_span: Span,
        right_paren_span: Option<Span>,
        initializer: ExpressionId,
    ) -> Result<StatementCheck, TypeCheckingError> {
        let source = self.check_expression(initializer, None, None)?.ty;
        if self.iteration_recovery.is_pending(initializer) {
            for &binding in bindings {
                self.note_pending_iteration_binding(binding);
            }
        }
        let unit = self.builtin(BuiltinType::Unit);
        let TypeKind::Nominal { nominal, arguments } = self.kind(source).clone() else {
            let fallback = if self.is_error(source) {
                self.error_type()
            } else {
                self.deferred(DeferredReason::Destructuring)
            };
            for &binding in bindings {
                self.set_marker_symbol(binding, fallback);
            }
            return Ok(StatementCheck {
                ty: unit,
                falls_through: true,
            });
        };
        let descriptor = self
            .nominals
            .iter()
            .find(|descriptor| descriptor.id() == nominal)
            .cloned()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        if descriptor.kind() != NominalKind::ValueClass {
            let deferred = self.deferred(DeferredReason::Destructuring);
            for &binding in bindings {
                self.set_marker_symbol(binding, deferred);
            }
            return Ok(StatementCheck {
                ty: unit,
                falls_through: true,
            });
        }

        let substitutions = descriptor
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments)
            .collect::<BTreeMap<_, _>>();
        let component_types = descriptor
            .fields()
            .iter()
            .map(|field| {
                let ty = self
                    .symbol_type(*field)
                    .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                self.substitute_type(ty, &substitutions)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let declaration_span = self.symbol_spans[nominal.symbol().index()];
        let fields = descriptor
            .fields()
            .iter()
            .map(|field| self.sources.slice(self.symbol_spans[field.index()]))
            .collect::<Result<Vec<_>, _>>()?;
        let matches = crate::type_checking::destructuring::local_field_matches(
            self.sources,
            bindings,
            fields,
        )?;
        let private_pattern = matches.iter().any(|(span, index)| {
            index.is_some_and(|index| {
                self.private_field_inaccessible(descriptor.fields()[index], *span)
            })
        });
        let mut typed_components = vec![None; component_types.len()];
        let mut markers_valid = matches.len() == bindings.len();
        for (span, index) in matches {
            let Some(symbol) = self.symbol_at(span) else {
                markers_valid = false;
                continue;
            };
            let Some(index) = index else {
                markers_valid = false;
                let error = self.error_type();
                self.set_marker_symbol(NameMarker::Present(span), error);
                self.emit_with_label(
                    self.unresolved_name_code,
                    "unknown value class destructuring field",
                    span,
                    declaration_span,
                    "field must be declared by this value class",
                )?;
                continue;
            };
            let field = descriptor.fields()[index];
            if self.private_field_inaccessible(field, span) {
                self.emit_private_field_access(field, span)?;
            }
            if private_pattern {
                // No binding receives a value when the complete split is inaccessible.
                let error = self.error_type();
                self.set_marker_symbol(NameMarker::Present(span), error);
                continue;
            }
            let ty = component_types[index];
            self.set_marker_symbol(NameMarker::Present(span), ty);
            markers_valid &= typed_components[index]
                .replace(DestructuringComponent::new(symbol, ty))
                .is_none();
        }

        if bindings.len() != component_types.len() {
            let end = right_paren_span.map_or(left_paren_span.end(), |span| span.end());
            let pattern =
                self.sources
                    .span(self.parsed.source_id(), left_paren_span.start(), end)?;
            self.emit_with_label(
                self.destructuring_arity_code,
                "value class destructuring must bind every component exactly once",
                pattern,
                declaration_span,
                format!("expected {} bindings", component_types.len()),
            )?;
        } else if markers_valid && !private_pattern {
            let mode = match self.copyability_of(source) {
                Copyability::Copyable => Some(DestructuringMode::Copy),
                Copyability::MoveOnly => Some(DestructuringMode::Consume),
                Copyability::Unknown | Copyability::Error => None,
            };
            if let Some(mode) = mode {
                self.destructurings.push(DestructuringDescriptor::new(
                    statement,
                    source,
                    mode,
                    typed_components.into_iter().flatten().collect(),
                ));
            }
        }

        Ok(StatementCheck {
            ty: unit,
            falls_through: true,
        })
    }
}
