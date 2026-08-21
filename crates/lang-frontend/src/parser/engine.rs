mod block;
mod boundary;
mod class;
mod core;
mod declaration;
mod destructuring;
mod expression;
mod file;
mod operator;
mod postfix;
mod recovery;
#[cfg(test)]
mod tests;
mod type_ref;

use boundary::*;
use recovery::LexicalRecoveryIndex;
pub(super) use recovery::{TerminalOwnerEvent, TerminalOwnerKind};

use crate::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    diagnostic::{Diagnostic, DiagnosticCode, Severity, codes, ordered_diagnostics},
    lexer::{
        FloatLiteralSuffix, IntegerLiteralSuffix, InvalidKind, Keyword, LexedFile, Lexeme,
        LexemeKind, Symbol, TokenKind,
    },
    source::{SourceMap, Span},
};

#[cfg(test)]
use std::cell::Cell;

use super::lambda_trial::{LambdaHeaderIndex, LambdaHeaderTrial};
use super::trial::{CallTrial, StrictCallTrialIndex};
use super::{
    AssignmentOperator, BinaryOperator, CallArgument, CastOperator, ClassField, ClassifierBody,
    ClassifierDeclaration, ClassifierKind, CompanionObject, DeclarationModifiers, DelegationClause,
    EnumVariant, EnumVariantParameter, Expression, ExpressionAst, FloatLiteralKind, ForBinding,
    FunctionBody, FunctionForm, FunctionTypeParameter, ImportAlias, ImportDirective,
    IntegerLiteralKind, Item, LiteralKind, MAX_RECURSION_DEPTH, NameMarker, NamedArgumentPrefix,
    PackageDirective, ParameterModeMarker, ParsedBlock, ParsedDeclaration, ParsedExpression,
    ParsedFile, ParserInternalError, PrefixOperator, PrimaryConstructor, QualifiedNameSegment,
    Statement, StringPart, SupertypeEntry, TypeParameter, TypePathSegment, TypeRef, ValueParameter,
    VariableKind, VisibilityModifier, WhenCondition, WhenEntry,
};

const PREC_ASSIGNMENT: u8 = 1;
const PREC_OR: u8 = 2;
const PREC_AND: u8 = 3;
const PREC_EQUALITY: u8 = 4;
const PREC_COMPARISON: u8 = 5;
const PREC_MEMBERSHIP: u8 = 6;
const PREC_ELVIS: u8 = 7;
const PREC_TO: u8 = 8;
const PREC_RANGE: u8 = 9;
const PREC_ADDITIVE: u8 = 10;
const PREC_MULTIPLICATIVE: u8 = 11;
const PREC_CAST: u8 = 12;
const PREC_PREFIX: u8 = 13;

pub(super) fn parse_file(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedFile, ParserInternalError> {
    let source = sources.source_text(lexed.source_id())?;
    validate_lexemes(sources, lexed, source.len())?;
    let lexical_recoveries = LexicalRecoveryIndex::new(source, lexed)?;
    let strict_trials = StrictCallTrialIndex::new(lexed)?;
    let lambda_headers = LambdaHeaderIndex::new(lexed, &lexical_recoveries.terminal_owner_events)?;
    let mut parser = Parser {
        sources,
        lexed,
        lexical_recoveries,
        strict_trials,
        lambda_headers,
        index: 0,
        next_terminal_recovery_event: 0,
        recursion_depth: 0,
        file_mode: true,
        ast: ExpressionAst::new(lexed.source_id()),
        diagnostics: Vec::new(),
        #[cfg(test)]
        declaration_recovery_raw_visits: 0,
        #[cfg(test)]
        declaration_recovery_event_queries_and_applications: 0,
        #[cfg(test)]
        block_dispatch_iterations: 0,
        #[cfg(test)]
        lambda_body_dispatch_iterations: 0,
        #[cfg(test)]
        significant_raw_visits: Cell::new(0),
    };
    let (package, imports) = parser.parse_file_header()?;
    let roots = parser.parse_file_roots()?;
    for root in roots.iter().copied() {
        parser.validate_item_context(root)?;
    }
    let mut diagnostics = lexed.diagnostics().to_vec();
    diagnostics.extend(parser.diagnostics);
    let diagnostics = ordered_diagnostics(sources, &diagnostics)?
        .into_iter()
        .cloned()
        .collect();
    Ok(ParsedFile {
        ast: parser.ast,
        package,
        imports,
        roots,
        diagnostics,
    })
}

pub(super) fn parse(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedExpression, ParserInternalError> {
    // 这一步同时校验 LexedFile 的 map-local source identity。
    let source = sources.source_text(lexed.source_id())?;
    let source_len = source.len();
    validate_lexemes(sources, lexed, source_len)?;
    let lexical_recoveries = LexicalRecoveryIndex::new(source, lexed)?;
    let strict_trials = StrictCallTrialIndex::new(lexed)?;
    let lambda_headers = LambdaHeaderIndex::new(lexed, &lexical_recoveries.terminal_owner_events)?;

    let mut parser = Parser {
        sources,
        lexed,
        lexical_recoveries,
        strict_trials,
        lambda_headers,
        index: 0,
        next_terminal_recovery_event: 0,
        recursion_depth: 0,
        file_mode: false,
        ast: ExpressionAst::new(lexed.source_id()),
        diagnostics: Vec::new(),
        #[cfg(test)]
        declaration_recovery_raw_visits: 0,
        #[cfg(test)]
        declaration_recovery_event_queries_and_applications: 0,
        #[cfg(test)]
        block_dispatch_iterations: 0,
        #[cfg(test)]
        lambda_body_dispatch_iterations: 0,
        #[cfg(test)]
        significant_raw_visits: Cell::new(0),
    };
    let root = parser.parse_expression_bp(0, Stops::ROOT)?;
    let root = parser.consume_expression_tail(root, Stops::ROOT)?;
    parser.validate_expression_context(root, false)?;

    let mut diagnostics = lexed.diagnostics().to_vec();
    diagnostics.extend(parser.diagnostics);
    let diagnostics = ordered_diagnostics(sources, &diagnostics)?
        .into_iter()
        .cloned()
        .collect();

    Ok(ParsedExpression {
        ast: parser.ast,
        root,
        diagnostics,
    })
}

pub(super) fn parse_declaration(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedDeclaration, ParserInternalError> {
    let source = sources.source_text(lexed.source_id())?;
    validate_lexemes(sources, lexed, source.len())?;
    let lexical_recoveries = LexicalRecoveryIndex::new(source, lexed)?;
    let strict_trials = StrictCallTrialIndex::new(lexed)?;
    let lambda_headers = LambdaHeaderIndex::new(lexed, &lexical_recoveries.terminal_owner_events)?;
    let mut parser = Parser {
        sources,
        lexed,
        lexical_recoveries,
        strict_trials,
        lambda_headers,
        index: 0,
        next_terminal_recovery_event: 0,
        recursion_depth: 0,
        file_mode: false,
        ast: ExpressionAst::new(lexed.source_id()),
        diagnostics: Vec::new(),
        #[cfg(test)]
        declaration_recovery_raw_visits: 0,
        #[cfg(test)]
        declaration_recovery_event_queries_and_applications: 0,
        #[cfg(test)]
        block_dispatch_iterations: 0,
        #[cfg(test)]
        lambda_body_dispatch_iterations: 0,
        #[cfg(test)]
        significant_raw_visits: Cell::new(0),
    };
    let root = parser.parse_declaration_root()?;
    parser.validate_item_context(root)?;

    let mut diagnostics = lexed.diagnostics().to_vec();
    diagnostics.extend(parser.diagnostics);
    let diagnostics = ordered_diagnostics(sources, &diagnostics)?
        .into_iter()
        .cloned()
        .collect();
    Ok(ParsedDeclaration {
        ast: parser.ast,
        root,
        diagnostics,
    })
}

pub(super) fn parse_block(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedBlock, ParserInternalError> {
    let source = sources.source_text(lexed.source_id())?;
    validate_lexemes(sources, lexed, source.len())?;
    let lexical_recoveries = LexicalRecoveryIndex::new(source, lexed)?;
    let strict_trials = StrictCallTrialIndex::new(lexed)?;
    let lambda_headers = LambdaHeaderIndex::new(lexed, &lexical_recoveries.terminal_owner_events)?;
    let mut parser = Parser {
        sources,
        lexed,
        lexical_recoveries,
        strict_trials,
        lambda_headers,
        index: 0,
        next_terminal_recovery_event: 0,
        recursion_depth: 0,
        file_mode: false,
        ast: ExpressionAst::new(lexed.source_id()),
        diagnostics: Vec::new(),
        #[cfg(test)]
        declaration_recovery_raw_visits: 0,
        #[cfg(test)]
        declaration_recovery_event_queries_and_applications: 0,
        #[cfg(test)]
        block_dispatch_iterations: 0,
        #[cfg(test)]
        lambda_body_dispatch_iterations: 0,
        #[cfg(test)]
        significant_raw_visits: Cell::new(0),
    };
    let root = parser.parse_block_root()?;
    parser.validate_statement_context(root, true)?;

    let mut diagnostics = lexed.diagnostics().to_vec();
    diagnostics.extend(parser.diagnostics);
    let diagnostics = ordered_diagnostics(sources, &diagnostics)?
        .into_iter()
        .cloned()
        .collect();
    Ok(ParsedBlock {
        ast: parser.ast,
        root,
        diagnostics,
    })
}

fn validate_lexemes(
    sources: &SourceMap,
    lexed: &LexedFile,
    source_len: usize,
) -> Result<(), ParserInternalError> {
    if lexed.lexemes().is_empty() {
        return Err(ParserInternalError::InvalidLexemeStream);
    }

    let mut next_start = 0;
    let mut saw_eof = false;
    for (index, lexeme) in lexed.lexemes().iter().enumerate() {
        let span = lexeme.span();
        sources.slice(span)?;
        if span.source_id() != lexed.source_id() || span.start() != next_start {
            return Err(ParserInternalError::InvalidLexemeStream);
        }
        if matches!(lexeme.kind(), LexemeKind::Eof) {
            if saw_eof
                || index + 1 != lexed.lexemes().len()
                || !span.is_empty()
                || span.start() != source_len
            {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            saw_eof = true;
        } else {
            if span.is_empty() {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            next_start = span.end();
        }
    }
    if !saw_eof || next_start != source_len {
        return Err(ParserInternalError::InvalidLexemeStream);
    }
    Ok(())
}

struct Parser<'source> {
    sources: &'source SourceMap,
    lexed: &'source LexedFile,
    lexical_recoveries: LexicalRecoveryIndex,
    strict_trials: StrictCallTrialIndex,
    lambda_headers: LambdaHeaderIndex,
    index: usize,
    // Parser cursor 只向前推进，因此恢复入口也可在整根内单调跳过已经越过的 terminal event。
    next_terminal_recovery_event: usize,
    recursion_depth: usize,
    file_mode: bool,
    ast: ExpressionAst,
    diagnostics: Vec<Diagnostic>,
    #[cfg(test)]
    declaration_recovery_raw_visits: usize,
    #[cfg(test)]
    declaration_recovery_event_queries_and_applications: usize,
    #[cfg(test)]
    block_dispatch_iterations: usize,
    #[cfg(test)]
    lambda_body_dispatch_iterations: usize,
    #[cfg(test)]
    significant_raw_visits: Cell<usize>,
}

#[derive(Clone, Copy)]
enum ContextWork {
    Item(ItemId),
    Statement {
        id: StatementId,
        expression_is_statement: bool,
    },
    ControlBody {
        id: StatementId,
        value_required: bool,
    },
    Expression {
        id: ExpressionId,
        statement_allowed: bool,
    },
}

#[derive(Clone, Copy)]
struct InfixRule {
    precedence: u8,
    right_precedence: u8,
    kind: InfixKind,
    non_associative_group: Option<NonAssociativeGroup>,
}

enum ParsedRight {
    Expression(ExpressionId),
    Type(TypeRefId),
}

impl InfixRule {
    const fn left(precedence: u8, kind: InfixKind) -> Self {
        Self {
            precedence,
            right_precedence: precedence + 1,
            kind,
            non_associative_group: None,
        }
    }

    const fn right(precedence: u8, kind: InfixKind) -> Self {
        Self {
            precedence,
            right_precedence: precedence,
            kind,
            non_associative_group: None,
        }
    }

    const fn non_associative(precedence: u8, kind: InfixKind, group: NonAssociativeGroup) -> Self {
        Self {
            precedence,
            right_precedence: precedence + 1,
            kind,
            non_associative_group: Some(group),
        }
    }
}

#[derive(Clone, Copy)]
enum InfixKind {
    Binary(BinaryOperator),
    Cast(CastOperator),
    TypeTest { negated: bool },
    Assignment(AssignmentOperator),
}

#[derive(Clone, Copy)]
enum NonAssociativeGroup {
    Range = 0,
    Membership = 1,
    Comparison = 2,
    Equality = 3,
}

fn marker_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
    }
}

fn parameter_mode_span(marker: ParameterModeMarker) -> Span {
    match marker {
        ParameterModeMarker::Borrow(span) | ParameterModeMarker::Inout(span) => span,
    }
}

#[derive(Clone, Copy)]
enum RecoveryOwner {
    String { opener: usize },
    Interpolation { opener: usize },
}

impl RecoveryOwner {
    const fn kind(self) -> TerminalOwnerKind {
        match self {
            Self::String { .. } => TerminalOwnerKind::String,
            Self::Interpolation { .. } => TerminalOwnerKind::Interpolation,
        }
    }

    const fn opener(self) -> usize {
        match self {
            Self::String { opener } | Self::Interpolation { opener } => opener,
        }
    }
}
