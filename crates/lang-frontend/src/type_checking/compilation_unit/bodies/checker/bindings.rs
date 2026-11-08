//! SPEC-0197 compilation-unit body-local symbol 与参数契约事实。

use crate::{
    name_resolution::{Namespace, SourceUnitId, UnitSymbolId},
    parser::NameMarker,
    source::Span,
    type_checking::{ParameterMode, UnitExpressionId, UnitTypeId},
};

use super::{BodyChecker, namespace_rank, trial::DeferredFact};

impl BodyChecker<'_> {
    pub(super) fn track_inout_parameters(
        &mut self,
        parameters: impl IntoIterator<Item = (UnitSymbolId, UnitTypeId)>,
    ) {
        for (symbol, ty) in parameters {
            let pending = self.contains_unresolved_call_type(ty);
            self.flow_versions.declare_mutable(symbol, pending);
        }
    }

    pub(super) fn note_pending_binding_read(
        &mut self,
        read: UnitExpressionId,
        symbol: UnitSymbolId,
        ty: UnitTypeId,
    ) {
        if self.pending_immutable_bindings.contains(&symbol)
            || self.flow_versions.mutable_is_pending(symbol)
        {
            self.note_pending_storage_value(read, ty);
        }
    }

    pub(super) fn set_immutable_binding_pending(&mut self, symbol: UnitSymbolId, pending: bool) {
        if pending {
            self.pending_immutable_bindings.insert(symbol);
        } else {
            self.pending_immutable_bindings.remove(&symbol);
        }
    }

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
            self.record_symbol_type(symbol, ty);
        }
    }

    pub(super) fn record_symbol_type(&mut self, symbol: UnitSymbolId, ty: UnitTypeId) {
        let previous = self.parts.symbol_types.insert(symbol, ty);
        self.note_deferred_fact(DeferredFact::Symbol(symbol), previous, ty);
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
