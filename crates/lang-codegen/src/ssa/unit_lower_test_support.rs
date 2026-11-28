use lang_frontend::{
    lexer::lex,
    name_resolution::{
        DeclarationId, NameEnvironment, SourceUnitInput, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{ValidatedCompilationUnitOwnership, check_compilation_unit_ownership},
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{TypeEnvironment, ValidatedCompilationUnitTypes, check_compilation_unit_types},
};

pub(super) fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique source");
    let lexed = lex(sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (source, parsed)
}

pub(super) fn analyze(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    name_environment: &NameEnvironment,
    type_environment: &TypeEnvironment,
) -> (
    ValidatedCompilationUnitNames,
    ValidatedCompilationUnitTypes,
    ValidatedCompilationUnitOwnership,
) {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    let names = resolve_compilation_unit_names(sources, inputs, &index, name_environment)
        .expect("name resolution succeeds internally")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(sources, inputs, &names, type_environment)
        .expect("type checking succeeds internally")
        .validate()
        .expect("valid types");
    let owned = check_compilation_unit_ownership(sources, inputs, &names, type_environment, &typed)
        .expect("ownership checking succeeds internally")
        .validate()
        .expect("valid ownership");
    (names, typed, owned)
}

pub(super) fn declaration(
    names: &ValidatedCompilationUnitNames,
    package: &str,
    name: &str,
) -> DeclarationId {
    names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| {
            declaration.name() == name
                && names.names().index().packages()[declaration.package().index()]
                    .name()
                    .expect("named package")
                    .segments()
                    .iter()
                    .map(String::as_str)
                    .eq(package.split('.'))
        })
        .expect("declaration exists")
        .id()
}

pub(in crate::ssa) mod local_storage {
    //! Legacy test-only integer-lowering adapter.

    pub(in crate::ssa) mod integer {
        //! Local facts and submission adapt into the shared integer, checked, and range algorithms.

        use crate::ssa::lower_frontend::{
            BuiltinType, LoweredValue, LoweringError, LoweringErrorKind, Operation, Span,
            SsaTypeId, SymbolId, error,
        };
        use lang_frontend::type_checking::IntegerOperationKind;
        use std::collections::BTreeMap;

        pub(in crate::ssa) struct CheckedRoles {
            baseline: BTreeMap<SymbolId, LoweredValue>,
            carried: crate::ssa::lower_frontend::control::LinearBindings,
        }

        #[derive(Clone, Copy)]
        pub(in crate::ssa) struct IntegerFact<E, T> {
            pub(in crate::ssa) expression: E,
            pub(in crate::ssa) receiver: E,
            pub(in crate::ssa) receiver_type: T,
            pub(in crate::ssa) result_type: T,
            pub(in crate::ssa) kind: IntegerOperationKind,
        }

        /// Only the fact queries and submission boundaries required by integer intrinsics.
        pub(in crate::ssa) trait IntegerLowering {
            type Expr: Copy + Eq;
            type Type: Copy + Eq;

            fn fact(&self, expression: Self::Expr) -> Option<IntegerFact<Self::Expr, Self::Type>>;
            fn receiver_belongs_here(&self, receiver: Self::Expr) -> bool;
            fn expression_type(&self, expression: Self::Expr) -> Option<Self::Type>;
            fn builtin_type(&self, ty: Self::Type) -> Option<BuiltinType>;
            fn receiver_matches_ast(&self, expression: Self::Expr, receiver: Self::Expr) -> bool;
            fn lower_operand(
                &mut self,
                receiver: Self::Expr,
            ) -> Result<LoweredValue, LoweringError>;
            fn result_ssa_type(
                &self,
                expression: Self::Expr,
                span: Span,
            ) -> Result<SsaTypeId, LoweringError>;
            fn append_scalar(
                &mut self,
                operation: Operation,
                ty: SsaTypeId,
                span: Span,
            ) -> Result<LoweredValue, LoweringError>;
            fn append_checked(
                &mut self,
                operation: Operation,
                ty: SsaTypeId,
                span: Span,
            ) -> Result<LoweredValue, LoweringError>;
        }

        pub(in crate::ssa) fn lower_integer<H: IntegerLowering>(
            host: &mut H,
            expression: H::Expr,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let descriptor = host
                .fact(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if descriptor.expression != expression
                || !host.receiver_belongs_here(descriptor.receiver)
                || host.expression_type(descriptor.receiver) != Some(descriptor.receiver_type)
                || host.expression_type(expression) != Some(descriptor.result_type)
                || !host.receiver_matches_ast(expression, descriptor.receiver)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let integer_types = [descriptor.receiver_type, descriptor.result_type]
                .into_iter()
                .all(|ty| {
                    matches!(
                        host.builtin_type(ty),
                        Some(
                            BuiltinType::Byte
                                | BuiltinType::Short
                                | BuiltinType::Int
                                | BuiltinType::Long
                                | BuiltinType::UByte
                                | BuiltinType::UShort
                                | BuiltinType::UInt
                                | BuiltinType::ULong
                        )
                    )
                });
            if !integer_types {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            match descriptor.kind {
                IntegerOperationKind::Invert
                    if descriptor.receiver_type != descriptor.result_type =>
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                IntegerOperationKind::Invert | IntegerOperationKind::CheckedConvert => {}
            }
            let operand = match host.lower_operand(descriptor.receiver)? {
                LoweredValue::Value(value) => value,
                LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            // Keep this after receiver evaluation: adapters retain their original type errors.
            let ty = host.result_ssa_type(expression, span)?;
            if descriptor.kind == IntegerOperationKind::CheckedConvert {
                return host.append_checked(Operation::CheckedIntegerConvert { operand }, ty, span);
            }
            host.append_scalar(Operation::IntegerNot { operand }, ty, span)
        }
    }
}
