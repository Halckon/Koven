//! 只消费已规范化类型 identity 的 N1a 使用位置检查；single/unit 共用规则。
use super::{
    IntrinsicTypeConstructor, TypeId, TypeKind, TypeTable, UnitTypeId, UnitTypeKind, UnitTypeTable,
};
use crate::{
    ast::{AstError, ExpressionId, TypeRefId},
    diagnostic::codes,
    parser::{FunctionForm, Item, ParameterModeMarker, SyntaxAst, TypeRef, VariableKind},
    source::Span,
};

pub(super) struct RangeTypeIssue {
    pub(super) code: &'static str,
    pub(super) message: &'static str,
    pub(super) span: Span,
}

pub(super) fn file_contains_range(table: &TypeTable, ty: TypeId) -> bool {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match table.get(ty) {
            Some(TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                ..
            }) => return true,
            Some(TypeKind::Intrinsic { arguments, .. } | TypeKind::Nominal { arguments, .. }) => {
                pending.extend(arguments)
            }
            Some(
                TypeKind::Nullable(inner)
                | TypeKind::StaticSelf(inner)
                | TypeKind::EnumCase { root: inner, .. },
            ) => pending.push(*inner),
            Some(TypeKind::Function { return_type, .. }) => {
                pending.push(*return_type);
            }
            _ => {}
        }
    }
    false
}

pub(super) fn unit_contains_range(table: &UnitTypeTable, ty: UnitTypeId) -> bool {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match table.get(ty) {
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                ..
            }) => return true,
            Some(
                UnitTypeKind::Intrinsic { arguments, .. } | UnitTypeKind::Nominal { arguments, .. },
            ) => pending.extend(arguments),
            Some(
                UnitTypeKind::Nullable(inner)
                | UnitTypeKind::StaticSelf(inner)
                | UnitTypeKind::EnumCase { root: inner, .. },
            ) => pending.push(*inner),
            Some(UnitTypeKind::Function { return_type, .. }) => {
                pending.push(*return_type);
            }
            _ => {}
        }
    }
    false
}

pub(super) fn range_type_issues(
    ast: &SyntaxAst,
    has_range_ref: impl Fn(TypeRefId) -> bool,
    has_range_expression: impl Fn(ExpressionId) -> bool,
) -> Result<Vec<RangeTypeIssue>, AstError> {
    let mut issues = Vec::new();
    let mut escape = |span, message| {
        issues.push(RangeTypeIssue {
            code: codes::BORROW_RESULT_ESCAPE,
            message,
            span,
        })
    };
    for (id, node) in ast.type_refs().iter() {
        match node.payload() {
            TypeRef::Qualified {
                segments,
                nullable_span,
            } => {
                if let Some(span) = nullable_span
                    && has_range_ref(id)
                {
                    escape(*span, "range carrier cannot be optional");
                }
                for argument in segments.iter().flat_map(|s| &s.arguments) {
                    if has_range_ref(*argument) {
                        escape(
                            ast.type_refs().get(*argument)?.span(),
                            "range carrier cannot be a generic type argument",
                        );
                    }
                }
            }
            TypeRef::Function {
                parameters,
                return_type,
                ..
            } => {
                if has_range_ref(*return_type) {
                    escape(
                        node.span(),
                        "function type cannot erase range carrier result provenance",
                    );
                }
                for parameter in parameters {
                    if has_range_ref(parameter.type_ref)
                        && matches!(
                            parameter.mode_marker,
                            Some(ParameterModeMarker::Own(_) | ParameterModeMarker::Inout(_))
                        )
                    {
                        escape(parameter.span, "range carrier parameter must be Borrow");
                    }
                }
            }
            _ => {}
        }
    }
    for (_, node) in ast.items().iter() {
        match node.payload() {
            Item::Function { parameters, .. } => {
                for parameter in parameters {
                    if has_range_ref(parameter.type_ref)
                        && matches!(
                            parameter.mode_marker,
                            Some(ParameterModeMarker::Own(_) | ParameterModeMarker::Inout(_))
                        )
                    {
                        escape(parameter.span, "range carrier parameter must be Borrow");
                    }
                }
            }
            Item::Classifier(classifier) => {
                if let Some(constructor) = &classifier.primary_constructor {
                    for field in &constructor.fields {
                        if has_range_ref(field.type_ref) {
                            escape(
                                ast.type_refs().get(field.type_ref)?.span(),
                                "range carrier cannot be stored in a field",
                            );
                        }
                    }
                }
                if let Some(body) = &classifier.body {
                    for parameter in body.variants.iter().flat_map(|v| &v.parameters) {
                        if has_range_ref(parameter.type_ref) {
                            escape(
                                ast.type_refs().get(parameter.type_ref)?.span(),
                                "range carrier cannot be stored in an enum payload",
                            );
                        }
                    }
                }
            }
            Item::Variable {
                kind,
                type_ref,
                initializer,
                ..
            } if !matches!(kind, VariableKind::BorrowVal(_))
                && (type_ref.is_some_and(&has_range_ref) || has_range_expression(*initializer)) =>
            {
                escape(
                    ast.expressions().get(*initializer)?.span(),
                    "range carrier local requires 'borrow val'",
                );
            }
            Item::Constant {
                type_ref,
                initializer,
                ..
            } if type_ref.is_some_and(&has_range_ref) || has_range_expression(*initializer) => {
                escape(node.span(), "range carrier cannot be stored as a constant");
            }
            _ => {}
        }
    }
    for (_, node) in ast.items().iter() {
        if let Item::Function {
            form:
                FunctionForm::Explicit {
                    type_ref,
                    result_source: None,
                    ..
                },
            ..
        } = node.payload()
            && has_range_ref(*type_ref)
        {
            issues.push(RangeTypeIssue {
                code: codes::INVALID_BORROW_CONTRACT,
                message: "range carrier result requires a unique 'from' source",
                span: ast.type_refs().get(*type_ref)?.span(),
            });
        }
    }
    Ok(issues)
}
