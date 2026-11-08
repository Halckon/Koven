//! Signature TypeRef 不能携带仅用于 type-test/flow 的 case 类型。
use super::{
    CompilationUnitTypeError, Namespace, SignatureCollector, SourceUnitId, TypeCheckingError,
    TypePathSegment, TypeRefId, UnitReferenceTarget, codes,
};

impl SignatureCollector<'_> {
    pub(super) fn reject_enum_case_type(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
        segments: &[TypePathSegment],
    ) -> Result<bool, CompilationUnitTypeError> {
        let Some(segment) = segments.last() else {
            return Ok(false);
        };
        let symbol = match self.reference(source, segment.name_span, Namespace::Type) {
            Some(UnitReferenceTarget::Symbol(symbol)) => *symbol,
            Some(UnitReferenceTarget::Symbols(symbols)) if symbols.len() == 1 => symbols[0],
            _ => return Ok(false),
        };
        // 读取完整名称身份，不依赖 case signature 的收集顺序。
        let resolution = self.names.source_units()[symbol.source_unit().index()].resolution();
        let Some(case) = resolution
            .enum_cases()
            .iter()
            .find(|case| case.type_symbol() == symbol.symbol())
        else {
            return Ok(false);
        };
        let root_span = resolution
            .symbols()
            .get(case.root().index())
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
            .span();
        let primary = self.inputs[source.index()]
            .ast()
            .type_refs()
            .get(id)
            .map_err(TypeCheckingError::from)?
            .span();
        self.emit_with_label(
            codes::ENUM_CASE_TYPE_POSITION,
            "enum case type is only allowed as an is or !is target",
            primary,
            root_span,
            "root enum declared here",
        )?;
        Ok(true)
    }
}
