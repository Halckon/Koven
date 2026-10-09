use super::*;
use crate::{
    parser::ForBinding,
    type_checking::{
        SequentialIterationBinding, SequentialIterationComponent, SequentialIterationDescriptor,
    },
};

impl Checker<'_> {
    /// 类型与 binding 先封闭，再检查 body；错误恢复不向下游泄漏半份计划。
    pub(super) fn check_sequential_iteration(
        &mut self,
        statement: StatementId,
        binding: &ForBinding,
        source: ExpressionId,
        body: StatementId,
    ) -> Result<(), TypeCheckingError> {
        let diagnostic_start = self.diagnostics.len();
        let source_type = self.check_expression(source, None, None)?.ty;
        let provider = match self.kind(source_type) {
            TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                arguments,
            } if arguments.len() == 1 => Some((
                crate::type_checking::IterationProvider::RangeView,
                arguments[0],
            )),
            _ => self.container_parts(source_type).map(|(kind, element)| {
                (
                    crate::type_checking::IterationProvider::Sequential(kind),
                    element,
                )
            }),
        };
        let element = provider.map(|(_, element)| element);
        let mut plan_binding = None;
        if !self.iteration_type_poisoned(source_type) {
            if let Some(element) = element {
                if !self.iteration_type_poisoned(element) {
                    plan_binding = self.iteration_binding(binding, element)?;
                }
            } else {
                let span = self.ast().expressions().get(source)?.span();
                self.emit_iteration_error(
                    self.invalid_iteration_source_code,
                    "for source requires a compiler-bound sequential container or View",
                    span,
                    source_type,
                )?;
            }
        }
        if plan_binding.is_none() {
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
            && let (Some((provider, element_type)), Some(binding)) = (provider, plan_binding)
        {
            self.iterations.push(SequentialIterationDescriptor {
                statement,
                source,
                source_type,
                provider,
                element_type,
                binding,
            });
        }
        Ok(())
    }

    fn iteration_binding(
        &mut self,
        binding: &ForBinding,
        element: TypeId,
    ) -> Result<Option<SequentialIterationBinding>, TypeCheckingError> {
        match binding {
            ForBinding::Name(marker) => {
                Ok(self.iteration_symbol(*marker, element)?.map(|symbol| {
                    symbol.map_or(
                        SequentialIterationBinding::Discard,
                        SequentialIterationBinding::Name,
                    )
                }))
            }
            ForBinding::Destructuring {
                names,
                left_paren_span,
                right_paren_span,
            } => {
                let pattern = self.sources.span(
                    self.parsed.source_id(),
                    left_paren_span.start(),
                    right_paren_span.map_or(left_paren_span.end(), |span| span.end()),
                )?;
                let nominal = match self.kind(element).clone() {
                    TypeKind::Nominal { nominal, arguments } => self
                        .nominals
                        .iter()
                        .find(|d| d.id() == nominal)
                        .cloned()
                        .map(|d| (d, arguments)),
                    _ => None,
                };
                let Some((descriptor, arguments)) =
                    nominal.filter(|(d, _)| d.kind() == NominalKind::ValueClass)
                else {
                    self.emit_iteration_error(
                        self.invalid_iteration_pattern_code,
                        "for destructuring requires a concrete value class element",
                        pattern,
                        element,
                    )?;
                    return Ok(None);
                };
                if names.len() != descriptor.fields().len() {
                    self.emit_with_label(
                        self.destructuring_arity_code,
                        "value class destructuring must bind every component exactly once",
                        pattern,
                        self.symbol_spans[descriptor.id().symbol().index()],
                        format!("expected {} bindings", descriptor.fields().len()),
                    )?;
                    return Ok(None);
                }
                let substitutions = descriptor
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments)
                    .collect::<BTreeMap<_, _>>();
                let mut components = Vec::new();
                for (&marker, &field) in names.iter().zip(descriptor.fields()) {
                    let ty = self
                        .symbol_type(field)
                        .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                    let ty = self.substitute_type(ty, &substitutions)?;
                    if self.iteration_type_poisoned(ty) {
                        return Ok(None);
                    }
                    let Some(symbol) = self.iteration_symbol(marker, ty)? else {
                        return Ok(None);
                    };
                    components.push(SequentialIterationComponent { field, symbol, ty });
                }
                Ok(Some(SequentialIterationBinding::Destructure(components)))
            }
        }
    }

    fn emit_iteration_error(
        &mut self,
        code: DiagnosticCode,
        message: &str,
        span: Span,
        ty: TypeId,
    ) -> Result<(), TypeCheckingError> {
        // 只为诊断定位解开包装，不赋予 wrapped source/provider 能力。
        let mut declaration_type = ty;
        while let TypeKind::Nullable(inner)
        | TypeKind::StaticSelf(inner)
        | TypeKind::EnumCase { root: inner, .. } = self.kind(declaration_type)
        {
            declaration_type = *inner;
        }
        if let TypeKind::Nominal { nominal, .. } = self.kind(declaration_type) {
            self.emit_with_label(
                code,
                message,
                span,
                self.symbol_spans[nominal.symbol().index()],
                "type declared here",
            )
        } else {
            self.emit(code, message, span)
        }
    }

    /// 复合类型中的 recovery 也不能成为 provider 或 projection 的精确事实。
    fn iteration_type_poisoned(&self, ty: TypeId) -> bool {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            match self.kind(ty) {
                TypeKind::Error | TypeKind::Deferred(_) => return true,
                TypeKind::Nullable(inner)
                | TypeKind::StaticSelf(inner)
                | TypeKind::EnumCase { root: inner, .. } => pending.push(*inner),
                TypeKind::Nominal { arguments, .. } | TypeKind::Intrinsic { arguments, .. } => {
                    pending.extend(arguments)
                }
                TypeKind::Function {
                    parameters,
                    return_type,
                    ..
                } => {
                    pending.extend(parameters.iter().map(|parameter| parameter.ty));
                    pending.push(*return_type);
                }
                _ => {}
            }
        }
        false
    }

    /// 外层 None 表示恢复 marker；内层 None 表示真实 discard，不创建 symbol。
    fn iteration_symbol(
        &mut self,
        marker: NameMarker,
        ty: TypeId,
    ) -> Result<Option<Option<SymbolId>>, TypeCheckingError> {
        let NameMarker::Present(span) = marker else {
            return Ok(None);
        };
        if self.sources.slice(span)? == "_" {
            return Ok(Some(None));
        }
        let Some(symbol) = self.symbol_at(span) else {
            return Ok(None);
        };
        self.set_symbol(symbol, ty);
        Ok(Some(Some(symbol)))
    }
}
