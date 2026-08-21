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
        let error = self.error_type();
        let mut typed_components = Vec::new();
        let mut markers_valid = true;
        for (index, &binding) in bindings.iter().enumerate() {
            let ty = component_types.get(index).copied().unwrap_or(error);
            self.set_marker_symbol(binding, ty);
            let NameMarker::Present(span) = binding else {
                markers_valid = false;
                continue;
            };
            let Some(symbol) = self.symbol_at(span) else {
                markers_valid = false;
                continue;
            };
            if index < component_types.len() {
                typed_components.push(DestructuringComponent::new(symbol, ty));
            }
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
                self.symbol_spans[nominal.symbol().index()],
                format!("expected {} bindings", component_types.len()),
            )?;
        } else if markers_valid {
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
                    typed_components,
                ));
            }
        }

        Ok(StatementCheck {
            ty: unit,
            falls_through: true,
        })
    }
}
