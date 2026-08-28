//! SPEC-0197 compilation-unit 顶层 variable/constant initializer 类型检查。

use crate::{
    ast::TypeRefId,
    name_resolution::{DeclarationId, SourceUnitId},
    parser::Item,
    source::Span,
    type_checking::TypeCheckingError,
};

use super::{BodyChecker, CompilationUnitTypeError};

impl BodyChecker<'_> {
    pub(super) fn check_top_level_initializer(
        &mut self,
        declaration: DeclarationId,
        source: SourceUnitId,
        item: &Item,
    ) -> Result<(), CompilationUnitTypeError> {
        let (type_ref, initializer) = match item {
            Item::Variable {
                type_ref,
                initializer,
                ..
            }
            | Item::Constant {
                type_ref,
                initializer,
                ..
            } => (*type_ref, *initializer),
            _ => return Err(CompilationUnitTypeError::MissingDeclarationSymbol),
        };
        let signature = self
            .signatures
            .declaration(declaration)
            .cloned()
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        let expected = type_ref.map(|_| signature.ty());
        let expected_span = type_ref
            .map(|type_ref| self.top_level_type_ref_span(source, type_ref))
            .transpose()?;

        self.flow_facts.clear();
        self.current_return_span = None;
        let return_type = self.error_type();
        let result =
            self.check_expression(source, initializer, expected, expected_span, return_type)?;
        self.parts
            .symbol_types
            .insert(signature.symbol(), expected.unwrap_or(result.ty));
        Ok(())
    }

    fn top_level_type_ref_span(
        &self,
        source: SourceUnitId,
        type_ref: TypeRefId,
    ) -> Result<Span, CompilationUnitTypeError> {
        self.file(source)
            .ast()
            .type_refs()
            .get(type_ref)
            .map(|node| node.span())
            .map_err(TypeCheckingError::from)
            .map_err(CompilationUnitTypeError::from)
    }
}
