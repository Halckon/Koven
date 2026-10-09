//! N1a body 推断结果不能绕过签名的非逃逸位置合同。
use super::*;
use crate::type_checking::{
    IntrinsicTypeConstructor, UnitTypeRefId,
    range_type_uses::{range_type_issues, unit_contains_range},
};

impl BodyChecker<'_> {
    pub(super) fn check_range_types(&mut self) -> Result<(), CompilationUnitTypeError> {
        let sources: Vec<_> = self
            .names
            .names()
            .index()
            .source_units()
            .iter()
            .map(|s| s.id())
            .collect();
        for source in sources {
            let issues = range_type_issues(
                self.file(source).ast(),
                |id| {
                    self.parts
                        .type_ref_types
                        .get(&UnitTypeRefId::new(source, id))
                        .copied()
                        .or_else(|| {
                            self.signatures
                                .type_ref_type(UnitTypeRefId::new(source, id))
                        })
                        .is_some_and(|ty| unit_contains_range(self.signatures.types(), ty))
                },
                |id| {
                    self.parts
                        .expression_types
                        .get(&UnitExpressionId::new(source, id))
                        .is_some_and(|&ty| unit_contains_range(self.signatures.types(), ty))
                },
            )
            .map_err(TypeCheckingError::from)?;
            for issue in issues {
                if !self
                    .signatures
                    .diagnostics()
                    .iter()
                    .any(|d| d.primary_span() == issue.span && d.code().to_string() == issue.code)
                {
                    self.emit(issue.code, issue.message, issue.span)?;
                }
            }
        }
        Ok(())
    }
}

impl BodyChecker<'_> {
    pub(super) fn range_member_type(
        &mut self,
        expression: UnitExpressionId,
        receiver_expression: UnitExpressionId,
        receiver: UnitTypeId,
        name_span: Span,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        if self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?
            == "size"
            && matches!(
                self.signatures.types().get(receiver),
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::View,
                    ..
                })
            )
        {
            let result = self.builtin(BuiltinType::Int);
            self.parts
                .range_sizes
                .push(crate::type_checking::RangeSizeDescriptor {
                    expression,
                    receiver: receiver_expression,
                    receiver_type: receiver,
                    result_type: result,
                    span: self
                        .file(expression.source_unit())
                        .ast()
                        .expressions()
                        .get(expression.expression())
                        .map_err(TypeCheckingError::from)?
                        .span(),
                });
            return Ok(Some(result));
        }
        Ok(None)
    }
}
