use super::*;
use crate::type_checking::{MapIterationDescriptor, MapIterationKind};

impl Checker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_map_iteration(
        &mut self,
        statement: StatementId,
        binding: &ForBinding,
        source: ExpressionId,
        body: StatementId,
        source_type: TypeId,
        constructor: IntrinsicTypeConstructor,
        key_type: TypeId,
        value_type: TypeId,
        diagnostic_start: usize,
    ) -> Result<(), TypeCheckingError> {
        let mut symbols = None;
        if !self.iteration_type_poisoned(source_type) {
            symbols = self.map_iteration_symbols(binding)?;
        }
        if let Some([key, value]) = symbols {
            self.set_symbol(key, key_type);
            self.set_symbol(value, value_type);
        } else {
            let error = self.error_type();
            match binding {
                ForBinding::Name(marker) => self.set_marker_symbol(*marker, error),
                ForBinding::Destructuring { names, .. } => {
                    for marker in names {
                        self.set_marker_symbol(*marker, error);
                    }
                }
            }
        }
        self.check_loop_body(body)?;
        let span = self.ast().statements().get(statement)?.span();
        let upstream_error = self
            .input_error_spans
            .iter()
            .any(|primary| primary.start() >= span.start() && primary.end() <= span.end());
        if self.diagnostics.len() == diagnostic_start
            && !upstream_error
            && let Some([key_binding, value_binding]) = symbols
            && let Some(kind) = MapIterationKind::from_constructor(constructor)
        {
            self.map_descriptors
                .iterations
                .push(MapIterationDescriptor {
                    statement,
                    source,
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
        binding: &ForBinding,
    ) -> Result<Option<[SymbolId; 2]>, TypeCheckingError> {
        let (markers, span) = match binding {
            ForBinding::Name(NameMarker::Present(span)) => (Vec::new(), *span),
            ForBinding::Name(_) => return Ok(None),
            ForBinding::Destructuring {
                names,
                left_paren_span,
                right_paren_span,
            } => {
                let span = self.sources.span(
                    self.parsed.source_id(),
                    left_paren_span.start(),
                    right_paren_span.map_or(left_paren_span.end(), |end| end.end()),
                )?;
                (names.clone(), span)
            }
        };
        // Parser/name recovery 先保留其根因；有效但不支持的模式才给 L0160。
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
                if self.sources.slice(name)? == "_" {
                    invalid = true;
                }
                named.push(self.symbol_at(name));
            }
        }
        if invalid {
            self.emit(
                self.invalid_iteration_pattern_code,
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
