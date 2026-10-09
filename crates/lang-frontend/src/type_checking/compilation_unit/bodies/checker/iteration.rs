//! Compiler-bound unit provider；body 前确定 binding，失败不发布部分投影。
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
use std::collections::BTreeMap;

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
        let provider = match self.signatures.types().get(checked.ty) {
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
        if !self.iteration_type_poisoned(checked.ty) {
            if let Some((_, element)) = provider {
                resolved = self.iteration_binding(source, binding, element)?;
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
                    "for source requires a compiler-bound sequential container or View",
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
        self.check_loop_body(source, body, return_type, return_span)?;
        if self.diagnostics.len() == errors
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

    fn iteration_binding(
        &mut self,
        source: SourceUnitId,
        binding: &ForBinding,
        element: UnitTypeId,
    ) -> Result<Option<Binding>, CompilationUnitTypeError> {
        match binding {
            ForBinding::Name(marker) => Ok(self
                .iteration_symbol(source, *marker, element)?
                .map(|symbol| symbol.map_or(Binding::Discard, Binding::Name))),
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
                    }) => self
                        .signatures
                        .declaration(declaration)
                        .and_then(|signature| signature.nominal())
                        .cloned()
                        .map(|nominal| (declaration, nominal, arguments)),
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
                    return Ok(None);
                };
                if names.len() != nominal.fields().len() {
                    let declaration_span =
                        self.names.names().index().declarations()[declaration.index()].name_span();
                    self.emit_maybe_label(
                        codes::DESTRUCTURING_ARITY,
                        "value class destructuring must bind every component exactly once",
                        pattern,
                        Some(declaration_span),
                        format!("expected {} bindings", nominal.fields().len()),
                    )?;
                    return Ok(None);
                }
                let substitutions = nominal
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments)
                    .collect::<BTreeMap<_, _>>();
                let mut components = Vec::new();
                for (&marker, field) in names.iter().zip(nominal.fields()) {
                    let ty = self.substitute_type(field.ty(), &substitutions)?;
                    if self.iteration_type_poisoned(ty) {
                        return Ok(None);
                    }
                    let Some(symbol) = self.iteration_symbol(source, marker, ty)? else {
                        return Ok(None);
                    };
                    components.push(Component {
                        field: field.symbol(),
                        symbol,
                        ty,
                    });
                }
                Ok(Some(Binding::Destructure(components)))
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
                        self.names.names().index().declarations()[declaration.index()].name_span();
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

    fn iteration_type_poisoned(&self, ty: UnitTypeId) -> bool {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            match self.signatures.types().get(ty) {
                None | Some(UnitTypeKind::Error | UnitTypeKind::Deferred(_)) => return true,
                Some(
                    UnitTypeKind::Nullable(inner)
                    | UnitTypeKind::StaticSelf(inner)
                    | UnitTypeKind::EnumCase { root: inner, .. },
                ) => pending.push(*inner),
                Some(
                    UnitTypeKind::Nominal { arguments, .. }
                    | UnitTypeKind::Intrinsic { arguments, .. },
                ) => pending.extend(arguments),
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
        false
    }
}
