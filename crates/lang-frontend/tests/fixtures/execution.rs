//! Source, Lexer and Parser fixture suites using the shared validation harness.
use super::*;

pub(super) fn run_suite(root: &Path) -> Result<Vec<CaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| CaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_case(case),
        })
        .collect())
}

pub(super) fn run_case(case: &FixtureCase) -> Result<SourcePassEvidence, CaseFailure> {
    let bytes = fs::read(&case.disk_path).map_err(|error| CaseFailure::Read(error.kind()))?;
    let text = String::from_utf8(bytes).map_err(|_| CaseFailure::InvalidUtf8Content)?;
    let byte_len = text.len();
    let expected_text = text.clone();
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source(case.relative_path.clone(), text)
        .map_err(|_| CaseFailure::SourceModel)?;
    let full_span = sources
        .span(source_id, 0, byte_len)
        .map_err(|_| CaseFailure::SourceModel)?;
    if sources
        .source_name(source_id)
        .map_err(|_| CaseFailure::SourceModel)?
        != case.relative_path
        || sources
            .slice(full_span)
            .map_err(|_| CaseFailure::SourceModel)?
            != expected_text
    {
        return Err(CaseFailure::WiringInvariant);
    }

    let mut ast = AstFile::<(), (), FixtureExpression, ()>::new(source_id);
    let expression_id = ast
        .add_expression(full_span, FixtureExpression { byte_len })
        .map_err(|_| CaseFailure::AstModel)?;
    let expression = ast
        .expressions()
        .get(expression_id)
        .map_err(|_| CaseFailure::AstModel)?;
    if expression.span() != full_span || expression.payload().byte_len != byte_len {
        return Err(CaseFailure::WiringInvariant);
    }

    let code = phase0_fixture_code();
    let diagnostic = Diagnostic::new(
        &sources,
        Severity::Warning,
        code,
        FIXTURE_MESSAGE,
        full_span,
    )
    .map_err(|_| CaseFailure::DiagnosticModel)?;
    if diagnostic.primary_span() != full_span
        || diagnostic.severity() != Severity::Warning
        || diagnostic.message() != FIXTURE_MESSAGE
        || diagnostic.code() != code
    {
        return Err(CaseFailure::WiringInvariant);
    }

    Ok(SourcePassEvidence {
        byte_len,
        ast_node_count: ast.expressions().len(),
        diagnostic_code: code.to_string(),
    })
}

pub(super) fn run_lexer_pass_suite(root: &Path) -> Result<Vec<LexerCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| LexerCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_lexer_pass_case(case),
        })
        .collect())
}

pub(super) fn run_lexer_pass_case(case: &FixtureCase) -> Result<LexerEvidence, LexerCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        lex_fixture_source(&case.relative_path, &case.disk_path)?;
    if !lexed.diagnostics().is_empty() {
        return Err(LexerCaseFailure::UnexpectedDiagnostics);
    }

    if sources
        .source_text(source_id)
        .map_err(|_| LexerCaseFailure::SourceModel)?
        .len()
        != byte_len
    {
        return Err(LexerCaseFailure::SourceModel);
    }
    Ok(LexerEvidence {
        byte_len,
        lexeme_count: lexed.lexemes().len(),
        diagnostic_count: 0,
    })
}

pub(super) fn run_lexer_fail_suite(root: &Path) -> Result<Vec<LexerCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| LexerCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_lexer_fail_case(case),
        })
        .collect())
}

pub(super) fn run_lexer_fail_case(case: &LexerFailCase) -> Result<LexerEvidence, LexerCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        lex_fixture_source(&case.relative_path, &case.source_path)?;

    let sidecar_bytes = fs::read(&case.sidecar_path)
        .map_err(|error| LexerCaseFailure::ReadSidecar(error.kind()))?;
    let sidecar =
        String::from_utf8(sidecar_bytes).map_err(|_| LexerCaseFailure::InvalidUtf8Sidecar)?;
    let catalog = codes::catalog().map_err(|_| LexerCaseFailure::DiagnosticModel)?;
    let expected = parse_sidecar(
        &sources,
        source_id,
        &sidecar,
        &catalog,
        SidecarSpanPolicy::LexerDiagnostics,
    )
    .map_err(LexerCaseFailure::InvalidSidecar)?;

    if lexed
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.severity() != Severity::Error)
    {
        return Err(LexerCaseFailure::DiagnosticSeverity);
    }
    let actual = diagnostic_expectations(lexed.diagnostics().iter());
    let ordered = ordered_diagnostics(&sources, lexed.diagnostics())
        .map_err(|_| LexerCaseFailure::DiagnosticModel)?;
    let ordered_actual = diagnostic_expectations(ordered.into_iter());
    if actual != ordered_actual {
        return Err(LexerCaseFailure::DiagnosticOrder);
    }
    if actual != expected {
        return Err(LexerCaseFailure::DiagnosticMismatch);
    }

    Ok(LexerEvidence {
        byte_len,
        lexeme_count: lexed.lexemes().len(),
        diagnostic_count: actual.len(),
    })
}

pub(super) fn run_parser_pass_suite(root: &Path) -> Result<Vec<ParserCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| ParserCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_parser_pass_case(case),
        })
        .collect())
}

pub(super) fn run_parser_pass_case(case: &FixtureCase) -> Result<ParserEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.disk_path)?;
    let parsed = parse_fixture_twice(&sources, &lexed, parse_expression)?;
    if !parsed.diagnostics().is_empty() {
        return Err(ParserCaseFailure::UnexpectedDiagnostics);
    }
    if parsed.ast().source_id() != source_id
        || parsed.ast().expressions().get(parsed.root()).is_err()
    {
        return Err(ParserCaseFailure::RootInvariant);
    }

    Ok(ParserEvidence {
        byte_len,
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: 0,
    })
}

pub(super) fn run_parser_fail_suite(root: &Path) -> Result<Vec<ParserCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| ParserCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_parser_fail_case(case),
        })
        .collect())
}

pub(super) fn run_parser_fail_case(case: &LexerFailCase) -> Result<ParserEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.source_path)?;
    let parsed = parse_fixture_twice(&sources, &lexed, parse_expression)?;
    if parsed.ast().source_id() != source_id
        || parsed.ast().expressions().get(parsed.root()).is_err()
    {
        return Err(ParserCaseFailure::RootInvariant);
    }

    let sidecar_bytes = fs::read(&case.sidecar_path)
        .map_err(|error| ParserCaseFailure::ReadSidecar(error.kind()))?;
    let sidecar =
        String::from_utf8(sidecar_bytes).map_err(|_| ParserCaseFailure::InvalidUtf8Sidecar)?;
    let catalog = codes::catalog().map_err(|_| ParserCaseFailure::DiagnosticModel)?;
    let expected = parse_sidecar(
        &sources,
        source_id,
        &sidecar,
        &catalog,
        SidecarSpanPolicy::MergedParserDiagnostics,
    )
    .map_err(ParserCaseFailure::InvalidSidecar)?;

    if parsed
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.severity() != Severity::Error)
    {
        return Err(ParserCaseFailure::DiagnosticSeverity);
    }
    let actual = diagnostic_expectations(parsed.diagnostics().iter());
    let ordered = ordered_diagnostics(&sources, parsed.diagnostics())
        .map_err(|_| ParserCaseFailure::DiagnosticModel)?;
    if actual != diagnostic_expectations(ordered.into_iter()) {
        return Err(ParserCaseFailure::DiagnosticOrder);
    }
    if actual != expected {
        return Err(ParserCaseFailure::DiagnosticMismatch);
    }

    Ok(ParserEvidence {
        byte_len,
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: actual.len(),
    })
}

pub(super) fn validate_lambda_root(
    ast: &SyntaxAst,
    root: lang_frontend::ast::ExpressionId,
) -> Result<(), ParserCaseFailure> {
    let Expression::Lambda { body, .. } = ast
        .expressions()
        .get(root)
        .map_err(|_| ParserCaseFailure::RootInvariant)?
        .payload()
    else {
        return Err(ParserCaseFailure::RootInvariant);
    };
    if !matches!(
        ast.statements()
            .get(*body)
            .map_err(|_| ParserCaseFailure::RootInvariant)?
            .payload(),
        Statement::LambdaBody { .. }
    ) {
        return Err(ParserCaseFailure::RootInvariant);
    }
    Ok(())
}

pub(super) fn run_lambda_pass_suite(root: &Path) -> Result<Vec<ParserCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| ParserCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_lambda_pass_case(case),
        })
        .collect())
}

pub(super) fn run_lambda_pass_case(case: &FixtureCase) -> Result<ParserEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.disk_path)?;
    let parsed = parse_fixture_twice(&sources, &lexed, parse_expression)?;
    if !parsed.diagnostics().is_empty() {
        return Err(ParserCaseFailure::UnexpectedDiagnostics);
    }
    if parsed.ast().source_id() != source_id {
        return Err(ParserCaseFailure::RootInvariant);
    }
    // 独立入口无 trailing 诊断意味着已消费全部非 trivia 输入；这里另锁定 typed child。
    validate_lambda_root(parsed.ast(), parsed.root())?;
    Ok(ParserEvidence {
        byte_len,
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: 0,
    })
}

pub(super) fn run_lambda_fail_suite(root: &Path) -> Result<Vec<ParserCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| ParserCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_lambda_fail_case(case),
        })
        .collect())
}

pub(super) fn run_lambda_fail_case(case: &LexerFailCase) -> Result<ParserEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.source_path)?;
    let parsed = parse_fixture_twice(&sources, &lexed, parse_expression)?;
    if parsed.ast().source_id() != source_id {
        return Err(ParserCaseFailure::RootInvariant);
    }
    validate_lambda_root(parsed.ast(), parsed.root())?;
    let expected = load_parser_sidecar(&sources, source_id, &case.sidecar_path)?;
    validate_parser_diagnostics(&sources, parsed.diagnostics(), &expected)?;
    Ok(ParserEvidence {
        byte_len,
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: expected.len(),
    })
}

pub(super) fn run_declaration_pass_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_declaration_pass_case(case),
        })
        .collect())
}

pub(super) fn run_declaration_pass_case(case: &FixtureCase) -> Result<DeclarationEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.disk_path)?;
    let parsed = parse_fixture_twice(&sources, &lexed, parse_declaration)?;
    if !parsed.diagnostics().is_empty() {
        return Err(ParserCaseFailure::UnexpectedDiagnostics);
    }
    if parsed.ast().source_id() != source_id || parsed.ast().items().get(parsed.root()).is_err() {
        return Err(ParserCaseFailure::RootInvariant);
    }

    Ok(DeclarationEvidence {
        byte_len,
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        type_ref_count: parsed.ast().type_refs().len(),
        diagnostic_count: 0,
    })
}

pub(super) fn run_declaration_fail_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_declaration_fail_case(case),
        })
        .collect())
}

pub(super) fn run_declaration_fail_case(
    case: &LexerFailCase,
) -> Result<DeclarationEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.source_path)?;
    let parsed = parse_fixture_twice(&sources, &lexed, parse_declaration)?;
    if parsed.ast().source_id() != source_id || parsed.ast().items().get(parsed.root()).is_err() {
        return Err(ParserCaseFailure::RootInvariant);
    }

    let expected = load_parser_sidecar(&sources, source_id, &case.sidecar_path)?;
    validate_parser_diagnostics(&sources, parsed.diagnostics(), &expected)?;

    Ok(DeclarationEvidence {
        byte_len,
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        type_ref_count: parsed.ast().type_refs().len(),
        diagnostic_count: expected.len(),
    })
}

pub(super) fn run_file_pass_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_file_pass_case(case),
        })
        .collect())
}

pub(super) fn run_file_pass_case(case: &FixtureCase) -> Result<DeclarationEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.disk_path)?;
    let parsed = parse_fixture_twice(&sources, &lexed, parse_file)?;
    if !parsed.diagnostics().is_empty() || parsed.ast().source_id() != source_id {
        return Err(ParserCaseFailure::UnexpectedDiagnostics);
    }
    if parsed
        .roots()
        .iter()
        .any(|root| parsed.ast().items().get(*root).is_err())
    {
        return Err(ParserCaseFailure::RootInvariant);
    }
    Ok(DeclarationEvidence {
        byte_len,
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        type_ref_count: parsed.ast().type_refs().len(),
        diagnostic_count: 0,
    })
}

pub(super) fn run_file_fail_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_file_fail_case(case),
        })
        .collect())
}

pub(super) fn run_file_fail_case(case: &LexerFailCase) -> Result<DeclarationEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.source_path)?;
    let parsed = parse_fixture_twice(&sources, &lexed, parse_file)?;
    if parsed.ast().source_id() != source_id
        || parsed
            .roots()
            .iter()
            .any(|root| parsed.ast().items().get(*root).is_err())
    {
        return Err(ParserCaseFailure::RootInvariant);
    }
    let expected = load_parser_sidecar(&sources, source_id, &case.sidecar_path)?;
    validate_parser_diagnostics(&sources, parsed.diagnostics(), &expected)?;
    Ok(DeclarationEvidence {
        byte_len,
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        type_ref_count: parsed.ast().type_refs().len(),
        diagnostic_count: expected.len(),
    })
}

pub(super) fn validate_implicit_unit_root(
    ast: &SyntaxAst,
    root: lang_frontend::ast::ItemId,
    relative_path: &str,
) -> Result<(), ParserCaseFailure> {
    let Item::Function { form, .. } = ast
        .items()
        .get(root)
        .map_err(|_| ParserCaseFailure::RootInvariant)?
        .payload()
    else {
        return Err(ParserCaseFailure::RootInvariant);
    };
    let valid = match relative_path {
        "absent.ko" => matches!(form, FunctionForm::ImplicitUnitAbsent),
        "empty-block.ko" | "nonempty-block.ko" => {
            matches!(form, FunctionForm::ImplicitUnitBlock(_))
        }
        "explicit-unit.ko" | "explicit-other.ko" => matches!(
            form,
            FunctionForm::Explicit {
                type_ref,
                body: FunctionBody::Absent | FunctionBody::Block(_),
                ..
            } if matches!(
                ast.type_refs()
                    .get(*type_ref)
                    .map_err(|_| ParserCaseFailure::RootInvariant)?
                    .payload(),
                TypeRef::Qualified { .. }
            )
        ),
        "missing-expression-return.ko" | "missing-type.ko" => matches!(
            form,
            FunctionForm::Explicit {
                type_ref,
                body: FunctionBody::Expression { .. },
                ..
            } if matches!(
                ast.type_refs()
                    .get(*type_ref)
                    .map_err(|_| ParserCaseFailure::RootInvariant)?
                    .payload(),
                TypeRef::Error
            )
        ),
        _ => false,
    };
    valid.then_some(()).ok_or(ParserCaseFailure::RootInvariant)
}

pub(super) fn run_implicit_unit_pass_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: (|| {
                let (sources, source_id, lexed, byte_len) =
                    parser_fixture_source(&case.relative_path, &case.disk_path)?;
                let parsed = parse_fixture_twice(&sources, &lexed, parse_declaration)?;
                if !parsed.diagnostics().is_empty() {
                    return Err(ParserCaseFailure::UnexpectedDiagnostics);
                }
                if parsed.source_id() != source_id {
                    return Err(ParserCaseFailure::RootInvariant);
                }
                validate_implicit_unit_root(parsed.ast(), parsed.root(), &case.relative_path)?;
                Ok(DeclarationEvidence {
                    byte_len,
                    item_count: parsed.ast().items().len(),
                    expression_count: parsed.ast().expressions().len(),
                    type_ref_count: parsed.ast().type_refs().len(),
                    diagnostic_count: 0,
                })
            })(),
        })
        .collect())
}

pub(super) fn run_implicit_unit_fail_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: (|| {
                let (sources, source_id, lexed, byte_len) =
                    parser_fixture_source(&case.relative_path, &case.source_path)?;
                let parsed = parse_fixture_twice(&sources, &lexed, parse_declaration)?;
                if parsed.source_id() != source_id {
                    return Err(ParserCaseFailure::RootInvariant);
                }
                validate_implicit_unit_root(parsed.ast(), parsed.root(), &case.relative_path)?;
                let expected = load_parser_sidecar(&sources, source_id, &case.sidecar_path)?;
                validate_parser_diagnostics(&sources, parsed.diagnostics(), &expected)?;
                Ok(DeclarationEvidence {
                    byte_len,
                    item_count: parsed.ast().items().len(),
                    expression_count: parsed.ast().expressions().len(),
                    type_ref_count: parsed.ast().type_refs().len(),
                    diagnostic_count: expected.len(),
                })
            })(),
        })
        .collect())
}

pub(super) fn run_block_pass_suite(root: &Path) -> Result<Vec<BlockCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| BlockCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_block_pass_case(case),
        })
        .collect())
}

pub(super) fn run_block_pass_case(case: &FixtureCase) -> Result<BlockEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.disk_path)?;
    let parsed = parse_fixture_twice(&sources, &lexed, parse_block)?;
    if !parsed.diagnostics().is_empty() {
        return Err(ParserCaseFailure::UnexpectedDiagnostics);
    }
    if parsed.ast().source_id() != source_id
        || parsed.ast().statements().get(parsed.root()).is_err()
    {
        return Err(ParserCaseFailure::RootInvariant);
    }
    Ok(BlockEvidence {
        byte_len,
        statement_count: parsed.ast().statements().len(),
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: 0,
    })
}

pub(super) fn run_block_fail_suite(root: &Path) -> Result<Vec<BlockCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| BlockCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_block_fail_case(case),
        })
        .collect())
}

pub(super) fn run_block_fail_case(case: &LexerFailCase) -> Result<BlockEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.source_path)?;
    let parsed = parse_fixture_twice(&sources, &lexed, parse_block)?;
    if parsed.ast().source_id() != source_id
        || parsed.ast().statements().get(parsed.root()).is_err()
    {
        return Err(ParserCaseFailure::RootInvariant);
    }
    let expected = load_parser_sidecar(&sources, source_id, &case.sidecar_path)?;
    validate_parser_diagnostics(&sources, parsed.diagnostics(), &expected)?;
    Ok(BlockEvidence {
        byte_len,
        statement_count: parsed.ast().statements().len(),
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: expected.len(),
    })
}
