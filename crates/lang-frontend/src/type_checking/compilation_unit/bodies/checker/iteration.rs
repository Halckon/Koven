//! Compiler-bound unit provider；body 前确定 binding，失败不发布部分投影。
#[path = "map_iteration.rs"]
mod map_iteration;
use super::{BodyChecker, ExpressionCheck};
use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::codes,
    name_resolution::{Namespace, SourceUnitId, UnitSymbolId},
    parser::{ForBinding, NameMarker},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, NominalKind, TypeCheckingError, UnitExpressionId,
        UnitSequentialIterationBinding as Binding, UnitSequentialIterationComponent as Component,
        UnitSequentialIterationDescriptor, UnitStatementId, UnitTypeId, UnitTypeKind,
    },
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq)]
enum IterationTypeStatus {
    Ready,
    DiagnosticError,
    Incomplete,
}

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_for_statement(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        binding: &ForBinding,
        iteration_source: ExpressionId,
        body: StatementId,
        return_type: UnitTypeId,
        return_span: Option<Span>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let errors = self.diagnostics.len();
        let checked = self.check_expression(source, iteration_source, None, None, return_type)?;
        if let Some((constructor, key, value)) = self.map_parts(checked.ty) {
            self.check_map_iteration(
                source,
                statement,
                binding,
                iteration_source,
                body,
                return_type,
                return_span,
                checked.ty,
                constructor,
                key,
                value,
                errors,
            )?;
            return Ok(ExpressionCheck {
                ty: self.builtin(BuiltinType::Unit),
                falls_through: checked.falls_through,
            });
        }
        let provider = match self.signatures.types().get(checked.ty) {
            Some(UnitTypeKind::Builtin(BuiltinType::IntRange)) => Some((
                crate::type_checking::IterationProvider::NumericRange(BuiltinType::Int),
                self.builtin(BuiltinType::Int),
            )),
            Some(UnitTypeKind::Builtin(BuiltinType::LongRange)) => Some((
                crate::type_checking::IterationProvider::NumericRange(BuiltinType::Long),
                self.builtin(BuiltinType::Long),
            )),
            Some(UnitTypeKind::Intrinsic {
                constructor: crate::type_checking::IntrinsicTypeConstructor::View,
                arguments,
            }) if arguments.len() == 1 => Some((
                crate::type_checking::IterationProvider::RangeView,
                arguments[0],
            )),
            _ => self.container_parts(checked.ty).map(|(kind, element)| {
                (
                    crate::type_checking::IterationProvider::Sequential(kind),
                    element,
                )
            }),
        };
        let mut resolved = None;
        let mut header_type_status = self.iteration_type_status([checked.ty]);
        if header_type_status == IterationTypeStatus::Ready {
            if let Some((_, element)) = provider {
                (resolved, header_type_status) =
                    self.iteration_binding(source, binding, element)?;
            } else {
                let span = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(iteration_source)
                    .map_err(TypeCheckingError::from)?
                    .span();
                self.emit_iteration_error(
                    codes::INVALID_ITERATION_SOURCE,
                    "for source requires a compiler-bound sequential container, View, IntRange or LongRange",
                    span,
                    checked.ty,
                )?;
            }
        }
        if resolved.is_none() {
            let error = self.error_type();
            match binding {
                ForBinding::Name(marker) => self.set_marker_symbol(source, *marker, error),
                ForBinding::Destructuring { names, .. } => {
                    for marker in names {
                        self.set_marker_symbol(source, *marker, error);
                    }
                }
            }
        }
        let header_is_valid = self.diagnostics.len() == errors;
        if self.iteration_header_rejected(binding, header_type_status, errors) {
            self.parts
                .rejected_iteration_headers
                .push(UnitStatementId::new(source, statement));
        }
        self.check_loop_body(source, body, return_type, return_span)?;
        if header_is_valid
            && let (Some((provider, element_type)), Some(binding)) = (provider, resolved)
        {
            self.parts
                .iterations
                .push(UnitSequentialIterationDescriptor {
                    statement: UnitStatementId::new(source, statement),
                    source: UnitExpressionId::new(source, iteration_source),
                    source_type: checked.ty,
                    provider,
                    element_type,
                    binding,
                });
        }
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::Unit),
            falls_through: checked.falls_through,
        })
    }

    /// 只记录当前 header 的真实诊断来源，不把无 descriptor 或未完成类型视作错误。
    fn iteration_header_rejected(
        &self,
        binding: &ForBinding,
        type_status: IterationTypeStatus,
        diagnostic_start: usize,
    ) -> bool {
        if type_status == IterationTypeStatus::Incomplete {
            return false;
        }
        self.diagnostics[diagnostic_start..]
            .iter()
            .any(|diagnostic| diagnostic.severity() == crate::diagnostic::Severity::Error)
            || type_status == IterationTypeStatus::DiagnosticError
            || match binding {
                ForBinding::Name(marker) => !matches!(marker, NameMarker::Present(_)),
                ForBinding::Destructuring { names, .. } => names
                    .iter()
                    .any(|marker| !matches!(marker, NameMarker::Present(_))),
            }
    }

    fn iteration_binding(
        &mut self,
        source: SourceUnitId,
        binding: &ForBinding,
        element: UnitTypeId,
    ) -> Result<(Option<Binding>, IterationTypeStatus), CompilationUnitTypeError> {
        match binding {
            ForBinding::Name(marker) => Ok((
                self.iteration_symbol(source, *marker, element)?
                    .map(|symbol| symbol.map_or(Binding::Discard, Binding::Name)),
                IterationTypeStatus::Ready,
            )),
            ForBinding::Destructuring {
                names,
                left_paren_span,
                right_paren_span,
            } => {
                let pattern = self
                    .sources
                    .span(
                        left_paren_span.source_id(),
                        left_paren_span.start(),
                        right_paren_span.map_or(left_paren_span.end(), |span| span.end()),
                    )
                    .map_err(TypeCheckingError::from)?;
                let nominal = match self.signatures.types().get(element).cloned() {
                    Some(UnitTypeKind::Nominal {
                        declaration,
                        arguments,
                    }) => {
                        let nominal = self
                            .signatures
                            .declaration(declaration)
                            .and_then(|signature| signature.nominal())
                            .filter(|nominal| {
                                nominal.declaration() == declaration
                                    && nominal.type_parameters().len() == arguments.len()
                            })
                            .cloned()
                            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                        Some((declaration, nominal, arguments))
                    }
                    _ => None,
                };
                let Some((declaration, nominal, arguments)) =
                    nominal.filter(|(_, nominal, _)| nominal.kind() == NominalKind::ValueClass)
                else {
                    self.emit_iteration_error(
                        codes::INVALID_ITERATION_PATTERN,
                        "for destructuring requires a concrete value class element",
                        pattern,
                        element,
                    )?;
                    return Ok((None, IterationTypeStatus::Ready));
                };
                if names.len() != nominal.fields().len() {
                    let declaration_span =
                        self.names.index().declarations()[declaration.index()].name_span();
                    self.emit_maybe_label(
                        codes::DESTRUCTURING_ARITY,
                        "value class destructuring must bind every component exactly once",
                        pattern,
                        Some(declaration_span),
                        format!("expected {} bindings", nominal.fields().len()),
                    )?;
                    return Ok((None, IterationTypeStatus::Ready));
                }
                // substitute 对缺失 ID 的旧 fallback 不能成为恢复证明。
                // 先核查真实字段图，且所有字段中的 None/Deferred 优先保留为未完成。
                let field_status =
                    self.iteration_type_status(nominal.fields().iter().map(|field| field.ty()));
                if field_status != IterationTypeStatus::Ready {
                    return Ok((None, field_status));
                }
                let substitutions = nominal
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments)
                    .collect::<BTreeMap<_, _>>();
                let field_types = nominal
                    .fields()
                    .iter()
                    .map(|field| self.substitute_type(field.ty(), &substitutions))
                    .collect::<Result<Vec<_>, _>>()?;
                let field_status = self.iteration_type_status(field_types.iter().copied());
                if field_status != IterationTypeStatus::Ready {
                    return Ok((None, field_status));
                }
                let mut components = Vec::new();
                for ((&marker, field), ty) in names.iter().zip(nominal.fields()).zip(field_types) {
                    let Some(symbol) = self.iteration_symbol(source, marker, ty)? else {
                        return Ok((None, IterationTypeStatus::Ready));
                    };
                    components.push(Component {
                        field: field.symbol(),
                        symbol,
                        ty,
                    });
                }
                Ok((
                    Some(Binding::Destructure(components)),
                    IterationTypeStatus::Ready,
                ))
            }
        }
    }

    fn iteration_symbol(
        &mut self,
        source: SourceUnitId,
        marker: NameMarker,
        ty: UnitTypeId,
    ) -> Result<Option<Option<UnitSymbolId>>, CompilationUnitTypeError> {
        let NameMarker::Present(span) = marker else {
            return Ok(None);
        };
        if self.sources.slice(span).map_err(TypeCheckingError::from)? == "_" {
            return Ok(Some(None));
        }
        let Some(symbol) = self.symbol_at(source, span, Namespace::Value) else {
            return Ok(None);
        };
        self.parts.symbol_types.insert(symbol, ty);
        Ok(Some(Some(symbol)))
    }

    fn emit_iteration_error(
        &mut self,
        code: &'static str,
        message: &'static str,
        span: Span,
        ty: UnitTypeId,
    ) -> Result<(), CompilationUnitTypeError> {
        let mut ty = ty;
        loop {
            match self.signatures.types().get(ty) {
                Some(
                    UnitTypeKind::Nullable(inner)
                    | UnitTypeKind::StaticSelf(inner)
                    | UnitTypeKind::EnumCase { root: inner, .. },
                ) => ty = *inner,
                Some(UnitTypeKind::Nominal { declaration, .. }) => {
                    let declaration_span =
                        self.names.index().declarations()[declaration.index()].name_span();
                    return self.emit_maybe_label(
                        code,
                        message,
                        span,
                        Some(declaration_span),
                        "type declared here",
                    );
                }
                _ => return self.emit(code, message, span),
            }
        }
    }

    /// 沿真实类型边读取 Error；绝不由缺失 ID、Deferred 或 copyability 推断已诊断。
    fn iteration_type_status(
        &self,
        roots: impl IntoIterator<Item = UnitTypeId>,
    ) -> IterationTypeStatus {
        let mut pending = roots.into_iter().collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        let mut status = IterationTypeStatus::Ready;
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            match self.signatures.types().get(ty) {
                None | Some(UnitTypeKind::Deferred(_)) => return IterationTypeStatus::Incomplete,
                Some(UnitTypeKind::Error) => status = IterationTypeStatus::DiagnosticError,
                Some(
                    UnitTypeKind::Nullable(inner)
                    | UnitTypeKind::StaticSelf(inner)
                    | UnitTypeKind::EnumCase { root: inner, .. },
                ) => pending.push(*inner),
                Some(UnitTypeKind::Nominal {
                    declaration,
                    arguments,
                }) => {
                    let nominal = self
                        .signatures
                        .declaration(*declaration)
                        .and_then(|signature| signature.nominal());
                    if nominal.is_none_or(|nominal| {
                        nominal.declaration() != *declaration
                            || nominal.type_parameters().len() != arguments.len()
                    }) {
                        return IterationTypeStatus::Incomplete;
                    }
                    pending.extend(arguments);
                }
                Some(UnitTypeKind::Intrinsic { arguments, .. }) => pending.extend(arguments),
                Some(UnitTypeKind::Function {
                    parameters,
                    return_type,
                    ..
                }) => {
                    pending.extend(parameters.iter().map(|parameter| parameter.ty()));
                    pending.push(*return_type);
                }
                _ => {}
            }
        }
        status
    }
}
