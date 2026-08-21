mod expression;
mod item;
mod type_ref;

use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    diagnostic::{Diagnostic, DiagnosticCode, Severity, codes, ordered_diagnostics},
    name_resolution::{ExternalSymbolId, NameResolution, Namespace, ReferenceTarget, SymbolId},
    parser::{ParsedFile, SyntaxAst},
    source::{SourceMap, Span},
};

use super::{
    BuiltinType, DeferredReason, EnvironmentFunction, EnvironmentType, ExternalTypeBinding,
    FunctionParameterType, ParameterMode, TypeCheckingError, TypeEnvironment, TypeId, TypeKind,
    TypeTable, TypedFile,
};

#[derive(Clone, Copy)]
struct ExprCheck {
    ty: TypeId,
    falls_through: bool,
}

#[derive(Clone, Copy)]
struct StatementCheck {
    ty: TypeId,
    falls_through: bool,
}

#[derive(Clone, Copy)]
struct CallableContext {
    return_type: TypeId,
    annotation_span: Option<Span>,
}

pub(super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    environment: &TypeEnvironment,
) -> Result<TypedFile, TypeCheckingError> {
    sources.source_text(parsed.source_id())?;
    Checker::new(sources, parsed, names, environment)?.run()
}

struct Checker<'a> {
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
    environment: &'a TypeEnvironment,
    types: TypeTable,
    expression_types: Vec<Option<TypeId>>,
    type_ref_types: Vec<Option<TypeId>>,
    symbol_types: Vec<Option<TypeId>>,
    references: BTreeMap<(usize, usize, u8), ReferenceTarget>,
    symbols_by_span: BTreeMap<(usize, usize), SymbolId>,
    external_types: BTreeMap<ExternalSymbolId, TypeId>,
    callables: Vec<CallableContext>,
    diagnostics: Vec<Diagnostic>,
    builtin_arguments_code: DiagnosticCode,
    cannot_infer_code: DiagnosticCode,
    mismatch_code: DiagnosticCode,
    operands_code: DiagnosticCode,
    return_outside_code: DiagnosticCode,
    return_shape_code: DiagnosticCode,
    missing_return_code: DiagnosticCode,
    branch_type_code: DiagnosticCode,
    numeric_range_code: DiagnosticCode,
}

impl<'a> Checker<'a> {
    fn new(
        sources: &'a SourceMap,
        parsed: &'a ParsedFile,
        names: &'a NameResolution,
        environment: &'a TypeEnvironment,
    ) -> Result<Self, TypeCheckingError> {
        let catalog = codes::catalog()?;
        let mut references = BTreeMap::new();
        for reference in names.references() {
            references.insert(
                (
                    reference.span().start(),
                    reference.span().end(),
                    namespace_key(reference.namespace()),
                ),
                reference.target().clone(),
            );
        }
        let symbols_by_span = names
            .symbols()
            .iter()
            .map(|symbol| ((symbol.span().start(), symbol.span().end()), symbol.id()))
            .collect();
        Ok(Self {
            sources,
            parsed,
            environment,
            types: TypeTable::new(),
            expression_types: vec![None; parsed.ast().expressions().len()],
            type_ref_types: vec![None; parsed.ast().type_refs().len()],
            symbol_types: vec![None; names.symbols().len()],
            references,
            symbols_by_span,
            external_types: BTreeMap::new(),
            callables: Vec::new(),
            diagnostics: Vec::new(),
            builtin_arguments_code: catalog.resolve(codes::BUILTIN_TYPE_ARGUMENTS)?,
            cannot_infer_code: catalog.resolve(codes::CANNOT_INFER_TYPE)?,
            mismatch_code: catalog.resolve(codes::TYPE_MISMATCH)?,
            operands_code: catalog.resolve(codes::INVALID_OPERAND_TYPES)?,
            return_outside_code: catalog.resolve(codes::RETURN_OUTSIDE_CALLABLE)?,
            return_shape_code: catalog.resolve(codes::RETURN_SHAPE_MISMATCH)?,
            missing_return_code: catalog.resolve(codes::MISSING_RETURN)?,
            branch_type_code: catalog.resolve(codes::NO_COMMON_BRANCH_TYPE)?,
            numeric_range_code: catalog.resolve(codes::NUMERIC_LITERAL_OUT_OF_RANGE)?,
        })
    }

    fn run(mut self) -> Result<TypedFile, TypeCheckingError> {
        self.predeclare_signatures()?;
        for &root in self.parsed.roots() {
            self.check_item(root)?;
        }
        let error = self.error_type();
        let expression_types = self
            .expression_types
            .into_iter()
            .map(|ty| ty.unwrap_or(error))
            .collect();
        let type_ref_types = self
            .type_ref_types
            .into_iter()
            .map(|ty| ty.unwrap_or(error))
            .collect();
        let symbol_types = self
            .symbol_types
            .into_iter()
            .map(|ty| ty.unwrap_or(error))
            .collect();
        let diagnostics = ordered_diagnostics(self.sources, &self.diagnostics)?
            .into_iter()
            .cloned()
            .collect();
        Ok(TypedFile::new(
            self.parsed.source_id(),
            self.types,
            expression_types,
            type_ref_types,
            symbol_types,
            diagnostics,
        ))
    }

    fn ast(&self) -> &SyntaxAst {
        self.parsed.ast()
    }

    fn builtin(&mut self, builtin: BuiltinType) -> TypeId {
        self.types.intern(TypeKind::Builtin(builtin))
    }

    fn error_type(&mut self) -> TypeId {
        self.types.intern(TypeKind::Error)
    }

    fn deferred(&mut self, reason: DeferredReason) -> TypeId {
        self.types.intern(TypeKind::Deferred(reason))
    }

    fn kind(&self, id: TypeId) -> &TypeKind {
        self.types
            .get(id)
            .expect("TypeId is always allocated by this checker")
    }

    fn set_expression(&mut self, id: ExpressionId, ty: TypeId) {
        self.expression_types[id.index()] = Some(ty);
    }

    fn set_type_ref(&mut self, id: TypeRefId, ty: TypeId) {
        self.type_ref_types[id.index()] = Some(ty);
    }

    fn set_symbol(&mut self, id: SymbolId, ty: TypeId) {
        self.symbol_types[id.index()] = Some(ty);
    }

    fn symbol_type(&self, id: SymbolId) -> Option<TypeId> {
        self.symbol_types.get(id.index()).copied().flatten()
    }

    fn symbol_at(&self, span: Span) -> Option<SymbolId> {
        self.symbols_by_span
            .get(&(span.start(), span.end()))
            .copied()
    }

    fn reference(&self, span: Span, namespace: Namespace) -> Option<&ReferenceTarget> {
        self.references
            .get(&(span.start(), span.end(), namespace_key(namespace)))
    }

    fn external_type(&mut self, id: ExternalSymbolId) -> Result<TypeId, TypeCheckingError> {
        if let Some(ty) = self.external_types.get(&id).copied() {
            return Ok(ty);
        }
        let ty = match self.environment.binding(id).cloned() {
            Some(ExternalTypeBinding::Builtin(builtin)) => self.builtin(builtin),
            Some(ExternalTypeBinding::Value(ty)) => self.normalize_environment_type(&ty),
            Some(ExternalTypeBinding::Function(signature)) => {
                self.normalize_environment_function(&signature)
            }
            None => self.deferred(DeferredReason::UnboundExternalType),
        };
        self.external_types.insert(id, ty);
        Ok(ty)
    }

    fn normalize_environment_function(&mut self, signature: &EnvironmentFunction) -> TypeId {
        let parameters = signature
            .parameters
            .iter()
            .map(|parameter| FunctionParameterType {
                mode: parameter.mode,
                ty: self.normalize_environment_type(&parameter.ty),
            })
            .collect();
        let return_type = self.normalize_environment_type(&signature.return_type);
        self.types.intern(TypeKind::Function {
            move_only: false,
            parameters,
            return_type,
        })
    }

    fn normalize_environment_type(&mut self, ty: &EnvironmentType) -> TypeId {
        match ty {
            EnvironmentType::Builtin(builtin) => self.builtin(*builtin),
            EnvironmentType::Nullable(inner) => {
                let inner = self.normalize_environment_type(inner);
                self.types.intern(TypeKind::Nullable(inner))
            }
            EnvironmentType::Function {
                move_only,
                parameters,
                return_type,
            } => {
                let parameters = parameters
                    .iter()
                    .map(|parameter| FunctionParameterType {
                        mode: parameter.mode,
                        ty: self.normalize_environment_type(&parameter.ty),
                    })
                    .collect();
                let return_type = self.normalize_environment_type(return_type);
                self.types.intern(TypeKind::Function {
                    move_only: *move_only,
                    parameters,
                    return_type,
                })
            }
        }
    }

    fn is_error(&self, ty: TypeId) -> bool {
        matches!(self.kind(ty), TypeKind::Error)
    }

    fn is_deferred(&self, ty: TypeId) -> bool {
        matches!(self.kind(ty), TypeKind::Deferred(_))
    }

    fn assignable(&self, actual: TypeId, expected: TypeId) -> bool {
        if actual == expected || self.is_error(actual) || self.is_error(expected) {
            return true;
        }
        if matches!(self.kind(actual), TypeKind::Builtin(BuiltinType::Nothing)) {
            return true;
        }
        match (self.kind(actual), self.kind(expected)) {
            (TypeKind::Builtin(BuiltinType::Nothing), _) => true,
            (TypeKind::Nullable(inner), TypeKind::Nullable(expected))
                if matches!(self.kind(*inner), TypeKind::Builtin(BuiltinType::Nothing)) =>
            {
                !self.is_deferred(*expected)
            }
            (TypeKind::Nullable(actual), TypeKind::Nullable(expected)) => actual == expected,
            (TypeKind::Builtin(_), TypeKind::Nullable(inner)) => actual == *inner,
            _ => false,
        }
    }

    fn join(&mut self, left: TypeId, right: TypeId) -> Option<TypeId> {
        if left == right {
            return Some(left);
        }
        if matches!(self.kind(left), TypeKind::Builtin(BuiltinType::Nothing)) {
            return Some(right);
        }
        if matches!(self.kind(right), TypeKind::Builtin(BuiltinType::Nothing)) {
            return Some(left);
        }
        match (self.kind(left).clone(), self.kind(right).clone()) {
            (TypeKind::Nullable(inner), _) if inner == right => Some(left),
            (_, TypeKind::Nullable(inner)) if inner == left => Some(right),
            (TypeKind::Error, _) => Some(right),
            (_, TypeKind::Error) => Some(left),
            _ => None,
        }
    }

    fn type_name(&self, ty: TypeId) -> String {
        match self.kind(ty) {
            TypeKind::Builtin(builtin) => builtin.name().to_owned(),
            TypeKind::Nullable(inner) => format!("{}?", self.type_name(*inner)),
            TypeKind::Function { .. } => "function type".to_owned(),
            TypeKind::IntegerLiteral(_) => "integer literal".to_owned(),
            TypeKind::Error => "<error>".to_owned(),
            TypeKind::Deferred(reason) => format!("<deferred:{reason:?}>"),
        }
    }

    fn emit(
        &mut self,
        code: DiagnosticCode,
        message: &str,
        primary: Span,
    ) -> Result<(), TypeCheckingError> {
        self.diagnostics.push(Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            message,
            primary,
        )?);
        Ok(())
    }

    fn emit_with_label(
        &mut self,
        code: DiagnosticCode,
        message: &str,
        primary: Span,
        label: Span,
        label_message: impl Into<String>,
    ) -> Result<(), TypeCheckingError> {
        let mut diagnostic =
            Diagnostic::new(self.sources, Severity::Error, code, message, primary)?;
        diagnostic.add_label(self.sources, label, label_message)?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn mismatch(
        &mut self,
        primary: Span,
        expected_span: Option<Span>,
        actual: TypeId,
        expected: TypeId,
    ) -> Result<(), TypeCheckingError> {
        if self.is_error(actual) || self.is_deferred(actual) {
            return Ok(());
        }
        let message = "expression type does not match the expected type";
        if let Some(label) = expected_span {
            self.emit_with_label(
                self.mismatch_code,
                message,
                primary,
                label,
                format!(
                    "expected {}, found {}",
                    self.type_name(expected),
                    self.type_name(actual)
                ),
            )
        } else {
            self.emit(self.mismatch_code, message, primary)
        }
    }
}

fn namespace_key(namespace: Namespace) -> u8 {
    match namespace {
        Namespace::Type => 0,
        Namespace::Value => 1,
    }
}
