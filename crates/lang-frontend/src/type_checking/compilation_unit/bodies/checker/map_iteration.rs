use super::*;
use crate::type_checking::{
    IntrinsicTypeConstructor, MapIterationKind, UnitMapIterationDescriptor,
};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_map_iteration(
        &mut self,
        source_unit: SourceUnitId,
        statement: StatementId,
        binding: &ForBinding,
        source: ExpressionId,
        body: StatementId,
        return_type: UnitTypeId,
        return_span: Option<Span>,
        source_type: UnitTypeId,
        constructor: IntrinsicTypeConstructor,
        key_type: UnitTypeId,
        value_type: UnitTypeId,
        diagnostic_start: usize,
    ) -> Result<(), CompilationUnitTypeError> {
        let symbols = if self.iteration_type_poisoned(source_type) {
            None
        } else {
            self.map_iteration_symbols(source_unit, binding)?
        };
        if let Some([key, value]) = symbols {
            self.parts.symbol_types.insert(key, key_type);
            self.parts.symbol_types.insert(value, value_type);
        } else {
            let error = self.error_type();
            match binding {
                ForBinding::Name(marker) => self.set_marker_symbol(source_unit, *marker, error),
                ForBinding::Destructuring { names, .. } => {
                    for marker in names {
                        self.set_marker_symbol(source_unit, *marker, error);
                    }
                }
            }
        }
        self.check_loop_body(source_unit, body, return_type, return_span)?;
        if self.diagnostics.len() == diagnostic_start
            && let Some([key_binding, value_binding]) = symbols
            && let Some(kind) = MapIterationKind::from_constructor(constructor)
        {
            self.parts
                .map_descriptors
                .iterations
                .push(UnitMapIterationDescriptor {
                    statement: UnitStatementId::new(source_unit, statement),
                    source: UnitExpressionId::new(source_unit, source),
                    source_type,
                    kind,
                    key_type,
                    value_type,
                    key_binding,
                    value_binding,
                });
        }
        Ok(())
    }

    fn map_iteration_symbols(
        &mut self,
        source: SourceUnitId,
        binding: &ForBinding,
    ) -> Result<Option<[UnitSymbolId; 2]>, CompilationUnitTypeError> {
        let (markers, span) = match binding {
            ForBinding::Name(NameMarker::Present(span)) => (Vec::new(), *span),
            ForBinding::Name(_) => return Ok(None),
            ForBinding::Destructuring {
                names,
                left_paren_span,
                right_paren_span,
            } => {
                let span = self
                    .sources
                    .span(
                        left_paren_span.source_id(),
                        left_paren_span.start(),
                        right_paren_span.map_or(left_paren_span.end(), |end| end.end()),
                    )
                    .map_err(TypeCheckingError::from)?;
                (names.clone(), span)
            }
        };
        if markers
            .iter()
            .any(|marker| !matches!(marker, NameMarker::Present(_)))
        {
            return Ok(None);
        }
        let mut named = Vec::new();
        let mut invalid = markers.len() != 2;
        for marker in markers {
            if let NameMarker::Present(name) = marker {
                if self.sources.slice(name).map_err(TypeCheckingError::from)? == "_" {
                    invalid = true;
                }
                named.push(self.symbol_at(source, name, Namespace::Value));
            }
        }
        if invalid {
            self.emit(
                codes::INVALID_ITERATION_PATTERN,
                "Map iteration requires exactly two named bindings (key, value)",
                span,
            )?;
            return Ok(None);
        }
        Ok(match named.as_slice() {
            [Some(key), Some(value)] if key != value => Some([*key, *value]),
            _ => None,
        })
    }
}
