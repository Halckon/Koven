//! SPEC-0197 compilation-unit body-local symbol 与参数契约事实。

use crate::{
    name_resolution::{Namespace, SourceUnitId, UnitSymbolId},
    parser::NameMarker,
    source::Span,
    type_checking::{ParameterMode, UnitTypeId},
};

use super::{BodyChecker, namespace_rank};

impl BodyChecker<'_> {
    pub(super) fn set_marker_symbol(
        &mut self,
        source: SourceUnitId,
        marker: NameMarker,
        ty: UnitTypeId,
    ) {
        if let NameMarker::Present(span) = marker {
            self.set_span_symbol(source, span, ty);
        }
    }

    pub(super) fn set_span_symbol(&mut self, source: SourceUnitId, span: Span, ty: UnitTypeId) {
        if let Some(symbol) = self.symbol_at(source, span, Namespace::Value) {
            self.parts.symbol_types.insert(symbol, ty);
        }
    }

    pub(super) fn set_parameter_mode(
        &mut self,
        source: SourceUnitId,
        span: Span,
        mode: ParameterMode,
    ) {
        if let Some(symbol) = self.symbol_at(source, span, Namespace::Value) {
            self.parts.parameter_modes.insert(symbol, mode);
        }
    }

    pub(super) fn symbol_at(
        &self,
        source: SourceUnitId,
        span: Span,
        namespace: Namespace,
    ) -> Option<UnitSymbolId> {
        self.symbols_by_span
            .get(&(source, span.start(), span.end(), namespace_rank(namespace)))
            .copied()
    }
}
