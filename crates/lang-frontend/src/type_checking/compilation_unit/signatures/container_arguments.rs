//! Reject direct non-storable builtin elements while collecting sequential signatures.
//! Nominal layout, interface diagnostics and range restrictions remain with their passes.
use super::*;

impl SignatureCollector<'_> {
    pub(super) fn check_sequential_element(
        &mut self,
        source: SourceUnitId,
        segment: &TypePathSegment,
        constructor: IntrinsicTypeConstructor,
        arguments: &[UnitTypeId],
    ) -> Result<(), CompilationUnitTypeError> {
        if !matches!(
            constructor,
            IntrinsicTypeConstructor::Array
                | IntrinsicTypeConstructor::List
                | IntrinsicTypeConstructor::MutableList
        ) {
            return Ok(());
        }
        let argument = arguments[0];
        let mut element = argument;
        while let Some(UnitTypeKind::Nullable(inner)) = self.types.get(element) {
            element = *inner;
        }
        if matches!(
            self.types.get(element),
            Some(UnitTypeKind::Builtin(
                BuiltinType::Any | BuiltinType::Nothing
            ))
        ) {
            let primary = self.inputs[source.index()]
                .ast()
                .type_refs()
                .get(segment.arguments[0])
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit(
                codes::INVALID_CONTAINER_ELEMENT,
                "sequential container element type is not structurally storable",
                primary,
            )?;
        }
        // Preserve raw signature shape, as the Map bounds pass does. Replacing every
        // invalid container with Error would conflate distinct overloads into L0097.
        // Published diagnostics prevent validated/final consumption of this product.
        Ok(())
    }
}
