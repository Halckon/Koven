use crate::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    diagnostic::{Diagnostic, DiagnosticCode, Severity, codes, ordered_diagnostics},
    lexer::{Keyword, LexedFile, Lexeme, LexemeKind, Symbol, TokenKind},
    source::{SourceMap, Span},
};

#[cfg(test)]
use std::cell::Cell;

use super::lambda_trial::{LambdaHeaderIndex, LambdaHeaderTrial};
use super::trial::{CallTrial, StrictCallTrialIndex};
use super::{
    AssignmentOperator, BinaryOperator, CallArgument, CastOperator, ClassField, ClassifierBody,
    ClassifierDeclaration, ClassifierKind, CompanionObject, DeclarationModifiers, DelegationClause,
    EnumVariant, EnumVariantParameter, Expression, ExpressionAst, ForBinding, FunctionBody,
    FunctionForm, FunctionTypeParameter, ImportAlias, ImportDirective, Item, LiteralKind,
    MAX_RECURSION_DEPTH, NameMarker, NamedArgumentPrefix, PackageDirective, ParameterModeMarker,
    ParsedBlock, ParsedDeclaration, ParsedExpression, ParsedFile, ParserInternalError,
    PrefixOperator, PrimaryConstructor, QualifiedNameSegment, Statement, StringPart,
    SupertypeEntry, TypeParameter, TypePathSegment, TypeRef, ValueParameter, VariableKind,
    VisibilityModifier, WhenCondition, WhenEntry,
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

struct LexicalRecoveryIndex {
    source_len: usize,
    string_owner_ends: Vec<(usize, usize)>,
    string_recoveries: Vec<(usize, usize)>,
    lexical_poison_string_recoveries: Vec<(usize, usize)>,
    unterminated_interpolation_starts: Vec<usize>,
    terminal_error_at_eof: bool,
    terminal_owner_events: Vec<TerminalOwnerEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TerminalOwnerKind {
    String,
    Interpolation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TerminalOwnerEvent {
    pub(super) offset: usize,
    pub(super) kind: TerminalOwnerKind,
    pub(super) opener: usize,
}

impl LexicalRecoveryIndex {
    fn new(source: &str, lexed: &LexedFile) -> Result<Self, ParserInternalError> {
        let catalog = codes::catalog()?;
        let unterminated_string = catalog.resolve(codes::UNTERMINATED_STRING)?;
        let unterminated_interpolation = catalog.resolve(codes::UNTERMINATED_INTERPOLATION)?;
        let invalid_string_escape = catalog.resolve(codes::INVALID_STRING_ESCAPE)?;
        let invalid_char_literal = catalog.resolve(codes::INVALID_CHAR_LITERAL)?;
        let unterminated_block_comment = catalog.resolve(codes::UNTERMINATED_BLOCK_COMMENT)?;

        let mut string_recoveries = Vec::new();
        let mut string_exit_events = Vec::new();
        let mut invalid_string_escape_spans = Vec::new();
        let mut terminal_escape_spans = Vec::new();
        let mut terminal_other_spans = Vec::new();
        let mut unterminated_interpolation_starts = Vec::new();
        let mut terminal_error_at_eof = false;
        let mut terminal_owner_events = Vec::new();

        for diagnostic in lexed.diagnostics() {
            let span = diagnostic.primary_span();
            let code = diagnostic.code();
            if code == unterminated_string {
                string_recoveries.push((span.start(), span.end()));
                string_exit_events.push((span.end(), span.start()));
                terminal_error_at_eof |= span.end() == source.len();
                terminal_owner_events.push(TerminalOwnerEvent {
                    offset: span.end(),
                    kind: TerminalOwnerKind::String,
                    opener: span.start(),
                });
                continue;
            }
            if code == unterminated_interpolation {
                unterminated_interpolation_starts.push(span.start());
                terminal_error_at_eof |= span.end() == source.len();
                terminal_owner_events.push(TerminalOwnerEvent {
                    offset: span.end(),
                    kind: TerminalOwnerKind::Interpolation,
                    opener: span.start(),
                });
                continue;
            }
            if code == invalid_string_escape {
                invalid_string_escape_spans.push((span.start(), span.end()));
                if span.end() == span.start() + 1
                    && (span.end() == source.len()
                        || source[span.end()..].starts_with(['\r', '\n']))
                {
                    terminal_escape_spans.push((span.start(), span.end()));
                    terminal_error_at_eof |= span.end() == source.len();
                }
                continue;
            }
            let terminal_other = (code == unterminated_block_comment && span.end() == source.len())
                || (code == invalid_char_literal && unterminated_char_at_eof(source, span));
            if terminal_other {
                terminal_other_spans.push((span.start(), span.end()));
                terminal_error_at_eof |= span.end() == source.len();
            }
        }

        string_exit_events.sort_unstable();
        invalid_string_escape_spans.sort_unstable();
        terminal_escape_spans.sort_unstable();
        terminal_other_spans.sort_unstable();
        unterminated_interpolation_starts.sort_unstable();

        terminal_owner_events.sort_unstable_by(|left, right| {
            left.offset
                .cmp(&right.offset)
                .then_with(|| right.opener.cmp(&left.opener))
        });
        let mut verified_events = Vec::with_capacity(terminal_owner_events.len());
        let mut active_owners = Vec::new();
        let mut next_terminal_event = 0usize;
        for lexeme in lexed.lexemes() {
            while terminal_owner_events
                .get(next_terminal_event)
                .is_some_and(|event| event.offset <= lexeme.span().start())
            {
                let event = terminal_owner_events[next_terminal_event];
                if active_owners.last().copied() != Some((event.kind, event.opener)) {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                active_owners.pop();
                verified_events.push(event);
                next_terminal_event += 1;
            }

            if matches!(lexeme.kind(), LexemeKind::Eof) && !active_owners.is_empty() {
                let has_terminal_root = verified_events
                    .last()
                    .is_some_and(|event| event.offset == source.len());
                if !has_terminal_root {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                while let Some((kind, opener)) = active_owners.pop() {
                    verified_events.push(TerminalOwnerEvent {
                        offset: source.len(),
                        kind,
                        opener,
                    });
                }
            }

            match lexeme.kind() {
                LexemeKind::Token(TokenKind::StringStart) => {
                    active_owners.push((TerminalOwnerKind::String, lexeme.span().start()))
                }
                LexemeKind::Token(TokenKind::InterpolationStart) => {
                    active_owners.push((TerminalOwnerKind::Interpolation, lexeme.span().start()))
                }
                LexemeKind::Token(TokenKind::StringEnd) => {
                    if !matches!(active_owners.pop(), Some((TerminalOwnerKind::String, _))) {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    }
                }
                LexemeKind::Token(TokenKind::InterpolationEnd) => {
                    if !matches!(
                        active_owners.pop(),
                        Some((TerminalOwnerKind::Interpolation, _))
                    ) {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    }
                }
                LexemeKind::Invalid(_)
                    if terminal_escape_spans
                        .binary_search(&(lexeme.span().start(), lexeme.span().end()))
                        .is_ok() =>
                {
                    let Some((TerminalOwnerKind::String, opener)) = active_owners.last().copied()
                    else {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    };
                    let event = TerminalOwnerEvent {
                        offset: lexeme.span().end(),
                        kind: TerminalOwnerKind::String,
                        opener,
                    };
                    active_owners.pop();
                    verified_events.push(event);
                }
                _ => {}
            }
        }
        if next_terminal_event != terminal_owner_events.len() {
            return Err(ParserInternalError::InvalidLexemeStream);
        }
        let mut active_strings: Vec<ActiveString> = Vec::new();
        let mut string_owner_ends = Vec::new();
        let mut lexical_poison_string_recoveries = Vec::new();
        let mut next_exit = 0usize;
        for lexeme in lexed.lexemes() {
            while string_exit_events
                .get(next_exit)
                .is_some_and(|(end, _)| *end <= lexeme.span().start())
            {
                let (_, owner) = string_exit_events[next_exit];
                for active_owner in &mut active_strings {
                    active_owner.lexical_poison = true;
                }
                if lexeme.span().start() == source.len() {
                    for active_owner in &active_strings {
                        string_recoveries.push((active_owner.opener, source.len()));
                    }
                }
                pop_string_owner(&mut active_strings, owner)?;
                next_exit += 1;
            }

            match lexeme.kind() {
                LexemeKind::Token(TokenKind::StringStart) => {
                    active_strings.push(ActiveString {
                        opener: lexeme.span().start(),
                        lexical_poison: false,
                    });
                }
                LexemeKind::Token(TokenKind::StringEnd) => {
                    let owner = active_strings
                        .pop()
                        .ok_or(ParserInternalError::InvalidLexemeStream)?;
                    string_owner_ends.push((owner.opener, lexeme.span().end()));
                    if owner.lexical_poison {
                        lexical_poison_string_recoveries.push((owner.opener, lexeme.span().end()));
                    }
                }
                LexemeKind::Token(TokenKind::InterpolationStart)
                    if unterminated_interpolation_starts
                        .binary_search(&lexeme.span().start())
                        .is_ok() =>
                {
                    for owner in &active_strings {
                        string_recoveries.push((owner.opener, source.len()));
                    }
                }
                LexemeKind::Invalid(_)
                    if invalid_string_escape_spans
                        .binary_search(&(lexeme.span().start(), lexeme.span().end()))
                        .is_ok() =>
                {
                    if active_strings.is_empty() {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    }
                    for owner in &mut active_strings {
                        owner.lexical_poison = true;
                    }
                    if terminal_escape_spans
                        .binary_search(&(lexeme.span().start(), lexeme.span().end()))
                        .is_err()
                    {
                        continue;
                    }
                    let end = lexeme.span().end();
                    if end == source.len() {
                        for owner in &active_strings {
                            string_recoveries.push((owner.opener, end));
                        }
                    } else if let Some(owner) = active_strings.last() {
                        string_recoveries.push((owner.opener, end));
                    }
                    if let Some(owner) = active_strings.last().copied() {
                        pop_string_owner(&mut active_strings, owner.opener)?;
                    }
                }
                LexemeKind::Invalid(_)
                    if terminal_other_spans
                        .binary_search(&(lexeme.span().start(), lexeme.span().end()))
                        .is_ok() =>
                {
                    for owner in &active_strings {
                        string_recoveries.push((owner.opener, source.len()));
                    }
                }
                _ => {}
            }
        }

        string_recoveries.sort_unstable();
        string_recoveries.dedup_by_key(|(start, _)| *start);
        string_owner_ends.sort_unstable();
        string_owner_ends.dedup_by_key(|(start, _)| *start);
        lexical_poison_string_recoveries.sort_unstable();
        lexical_poison_string_recoveries.dedup_by_key(|(start, _)| *start);
        Ok(Self {
            source_len: source.len(),
            string_owner_ends,
            string_recoveries,
            lexical_poison_string_recoveries,
            unterminated_interpolation_starts,
            terminal_error_at_eof,
            terminal_owner_events: verified_events,
        })
    }

    fn string_recovery_end(&self, start: usize) -> Option<usize> {
        self.string_recoveries
            .binary_search_by_key(&start, |(owner, _)| *owner)
            .ok()
            .map(|index| self.string_recoveries[index].1)
    }

    fn string_owner_end(&self, start: usize) -> Option<usize> {
        self.string_owner_ends
            .binary_search_by_key(&start, |(owner, _)| *owner)
            .ok()
            .map(|index| self.string_owner_ends[index].1)
    }

    fn lexical_poison_string_recovery_end(&self, start: usize) -> Option<usize> {
        self.lexical_poison_string_recoveries
            .binary_search_by_key(&start, |(owner, _)| *owner)
            .ok()
            .map(|index| self.lexical_poison_string_recoveries[index].1)
    }
}

#[derive(Clone, Copy)]
struct ActiveString {
    opener: usize,
    lexical_poison: bool,
}

fn pop_string_owner(
    active_strings: &mut Vec<ActiveString>,
    owner: usize,
) -> Result<(), ParserInternalError> {
    if active_strings.last().map(|active| active.opener) != Some(owner) {
        return Err(ParserInternalError::InvalidLexemeStream);
    }
    active_strings.pop();
    Ok(())
}

fn unterminated_char_at_eof(source: &str, span: Span) -> bool {
    if span.end() != source.len() {
        return false;
    }
    let mut chars = source[span.start()..span.end()].chars();
    if chars.next() != Some('\'') {
        return false;
    }
    while let Some(character) = chars.next() {
        match character {
            '\'' => return false,
            '\r' | '\n' => return false,
            '\\' => match chars.next() {
                Some('\r' | '\n') => return false,
                Some(_) => {}
                None => return true,
            },
            _ => {}
        }
    }
    true
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

impl Parser<'_> {
    fn root_expression_stops(&self) -> Stops {
        if self.file_mode {
            Stops::FILE
        } else {
            Stops::ROOT
        }
    }

    fn root_type_stops(&self) -> TypeStops {
        if self.file_mode {
            TypeStops::empty().with(TypeStops::FILE)
        } else {
            TypeStops::empty()
        }
    }

    fn root_declaration_stops(&self) -> DeclarationStops {
        if self.file_mode {
            DeclarationStops::FILE
        } else {
            DeclarationStops::EMPTY
        }
    }

    fn is_file_declaration_boundary(&self, lexeme: Lexeme) -> bool {
        self.file_mode
            && (file_construct_start_kind(lexeme.kind())
                || matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
                ))
    }

    fn file_separator_region_has_line_break(&self) -> Result<bool, ParserInternalError> {
        let start = self.previous_significant_end();
        let end = self.current()?.span().start();
        self.gap_has_line_break(start, end)
    }

    fn gap_has_line_break(&self, start: usize, end: usize) -> Result<bool, ParserInternalError> {
        Ok(self.sources.slice(self.span(start, end)?)?.contains('\n'))
    }

    fn parse_block_root(&mut self) -> Result<StatementId, ParserInternalError> {
        if !self.current_is_symbol(Symbol::LeftBrace) {
            return self.parse_missing_block_root();
        }
        let root = self.parse_block_statement(Stops::ROOT)?;
        if !matches!(self.current()?.kind(), LexemeKind::Eof) {
            let first = self.current()?.span();
            self.emit(
                codes::UNEXPECTED_TRAILING_TOKEN,
                "unexpected trailing token",
                first,
            )?;
            self.recover_declaration_region(DeclarationStops::EMPTY)?;
        }
        Ok(root)
    }

    fn parse_missing_block_root(&mut self) -> Result<StatementId, ParserInternalError> {
        let current = self.current()?;
        let start = current.span().start();
        if matches!(current.kind(), LexemeKind::Eof) {
            let span = self.empty_at(start)?;
            self.emit(codes::EXPECTED_BLOCK, "expected block", span)?;
            return self.add_statement(span, Statement::Error);
        }
        if !self.is_poison_kind(current.kind()) {
            self.emit(codes::EXPECTED_BLOCK, "expected block", current.span())?;
        }

        // 独立入口一旦缺 opener，整个剩余输入属于同一个 error root。这里按 raw lexeme 前进，
        // 因而 trailing trivia 也被消费，但节点范围只止于最后一个实际 token / invalid。
        let mut end = current.span().end();
        while self.index < self.lexed.lexemes().len() {
            let lexeme = self.lexed.lexemes()[self.index];
            self.index += 1;
            if !matches!(lexeme.kind(), LexemeKind::Trivia(_) | LexemeKind::Eof) {
                end = lexeme.span().end();
            }
            if matches!(lexeme.kind(), LexemeKind::Eof) {
                break;
            }
        }
        self.add_statement(self.span(start, end)?, Statement::Error)
    }

    fn parse_block_statement(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_block_statement_inner(outer_stops);
        self.recursion_depth -= 1;
        result
    }

    fn parse_block_statement_inner(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let opener = self.bump()?.span();
        let mut elements = Vec::new();

        while !self.current_is_symbol(Symbol::RightBrace)
            && !outer_stops.contains_hard(self.current()?)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            #[cfg(test)]
            {
                self.block_dispatch_iterations += 1;
            }
            let before = self.index;
            let element = self.parse_block_element(outer_stops)?;
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            elements.push(element);
        }

        let end = if self.current_is_symbol(Symbol::RightBrace) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !self.lexical_recoveries.terminal_error_at_eof {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
            self.previous_significant_end().max(opener.end())
        };
        self.add_statement(
            self.span(opener.start(), end)?,
            Statement::Block { elements },
        )
    }

    fn parse_block_element(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        if self.current_is_symbol(Symbol::LeftBrace) {
            return self.parse_block_statement(outer_stops);
        }
        let expression_stops = Stops::block_expression(outer_stops);
        if self.local_destructuring_start(Keyword::Val) {
            let val_span = self.bump()?.span();
            return self.parse_local_destructuring(val_span, expression_stops);
        }
        if self.local_destructuring_start(Keyword::Var) || self.const_local_destructuring_start() {
            return self.parse_unsupported_local_destructuring(expression_stops);
        }
        if self.current_is_keyword(Keyword::While)
            || self.current_is_keyword(Keyword::For)
            || self.current_is_keyword(Keyword::Loop)
        {
            return self.parse_loop_statement(outer_stops);
        }
        if self.current_is_keyword(Keyword::Val) || self.current_is_keyword(Keyword::Var) {
            let keyword = self.bump()?;
            let kind = match keyword.kind() {
                LexemeKind::Token(TokenKind::Keyword(Keyword::Val)) => VariableKind::Val,
                LexemeKind::Token(TokenKind::Keyword(Keyword::Var)) => VariableKind::Var,
                _ => return Err(ParserInternalError::InvalidLexemeStream),
            };
            let declaration =
                self.parse_local_variable_declaration(keyword.span(), kind, expression_stops)?;
            let span = self.ast.items().get(declaration)?.span();
            return self.add_statement(span, Statement::LocalVariable { declaration });
        }
        if self.is_unsupported_block_element() {
            return self.parse_unsupported_block_element();
        }

        let current = self.current()?;
        if self.is_poison_kind(current.kind()) {
            let span = self.bump()?.span();
            return self.add_statement(span, Statement::Error);
        }
        if self.can_start_expression(current) {
            let stops = Stops::block_expression(outer_stops);
            let expression = self.parse_expression_bp(0, stops)?;
            let expression = if self.control_expression_line_boundary(expression)? {
                expression
            } else {
                self.consume_expression_tail(expression, stops)?
            };
            let span = self.expression_span(expression)?;
            return self.add_statement(span, Statement::Expression { expression });
        }

        let span = self.bump()?.span();
        self.emit(
            codes::EXPECTED_BLOCK_ELEMENT,
            "expected block element",
            span,
        )?;
        self.add_statement(span, Statement::Error)
    }

    fn parse_loop_statement(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        if self.current_is_keyword(Keyword::While) {
            let keyword_span = self.bump()?.span();
            let condition = self.parse_parenthesized_condition(keyword_span, outer_stops)?;
            let body = self.parse_required_loop_body(outer_stops)?;
            let end = self
                .statement_span(body)?
                .end()
                .max(self.expression_span(condition)?.end());
            return self.add_statement(
                self.span(keyword_span.start(), end)?,
                Statement::While {
                    keyword_span,
                    condition,
                    body,
                },
            );
        }
        if self.current_is_keyword(Keyword::Loop) {
            let keyword_span = self.bump()?.span();
            let body = self.parse_required_loop_body(outer_stops)?;
            let end = self.statement_span(body)?.end();
            return self.add_statement(
                self.span(keyword_span.start(), end)?,
                Statement::Loop { keyword_span, body },
            );
        }

        let keyword_span = self.bump()?.span();
        let opener = if self.current_is_symbol(Symbol::LeftParen) {
            Some(self.bump()?.span())
        } else {
            let span = self.boundary_span(self.current()?, outer_stops)?;
            self.emit(codes::EXPECTED_FOR_BINDING, "expected for binding", span)?;
            None
        };
        let binding = self.parse_for_binding(outer_stops)?;
        let in_span = if self.current_is_keyword(Keyword::In) {
            self.bump()?.span()
        } else {
            let span = self.boundary_span(self.current()?, outer_stops)?;
            self.emit(codes::EXPECTED_FOR_IN, "expected in", span)?;
            self.empty_at(self.previous_significant_end())?
        };
        let source = if self.can_start_expression(self.current()?) {
            self.parse_expression_bp(0, outer_stops.with(Stops::RIGHT_PAREN))?
        } else {
            let span = self.boundary_span(self.current()?, outer_stops.with(Stops::RIGHT_PAREN))?;
            self.emit(codes::EXPECTED_CONDITION, "expected condition", span)?;
            self.add_expression(span, Expression::Error)?
        };
        if self.current_is_symbol(Symbol::RightParen) {
            self.bump()?;
        } else if let Some(opener) = opener
            && !self.is_poison()
        {
            self.emit_closing(self.boundary_span(self.current()?, outer_stops)?, opener)?;
        }
        let body = self.parse_required_loop_body(outer_stops)?;
        let end = self
            .statement_span(body)?
            .end()
            .max(self.expression_span(source)?.end());
        self.add_statement(
            self.span(keyword_span.start(), end)?,
            Statement::For {
                keyword_span,
                binding,
                in_span,
                source,
                body,
            },
        )
    }

    fn parse_for_binding(&mut self, outer_stops: Stops) -> Result<ForBinding, ParserInternalError> {
        if self.current_is_identifier() {
            return Ok(ForBinding::Name(NameMarker::Present(self.bump()?.span())));
        }
        if !self.current_is_symbol(Symbol::LeftParen) {
            let current = self.current()?;
            let span = self.boundary_span(current, outer_stops.with(Stops::RIGHT_PAREN))?;
            self.emit(codes::EXPECTED_FOR_BINDING, "expected for binding", span)?;
            return Ok(ForBinding::Name(NameMarker::Missing(span)));
        }
        let left_paren_span = self.bump()?.span();
        let mut names = Vec::new();
        while !self.current_is_symbol(Symbol::RightParen)
            && !self.current_is_keyword(Keyword::In)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            if self.current_is_identifier() {
                names.push(NameMarker::Present(self.bump()?.span()));
            } else {
                let span = self.current()?.span();
                if !self.is_poison() {
                    self.emit(codes::EXPECTED_FOR_BINDING, "expected for binding", span)?;
                }
                names.push(NameMarker::Error(self.bump()?.span()));
            }
            if self.current_is_symbol(Symbol::Comma) {
                self.bump()?;
                continue;
            }
            break;
        }
        if names.is_empty() {
            let span = self.empty_at(self.current()?.span().start())?;
            self.emit(codes::EXPECTED_FOR_BINDING, "expected for binding", span)?;
            names.push(NameMarker::Missing(span));
        }
        let right_paren_span = if self.current_is_symbol(Symbol::RightParen) {
            Some(self.bump()?.span())
        } else {
            None
        };
        Ok(ForBinding::Destructuring {
            left_paren_span,
            names,
            right_paren_span,
        })
    }

    fn parse_required_loop_body(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        if self.current_is_symbol(Symbol::LeftBrace) {
            return self.parse_block_statement(outer_stops);
        }
        let span = self.boundary_span(self.current()?, outer_stops)?;
        if !self.is_poison() {
            self.emit(codes::EXPECTED_LOOP_BODY, "expected loop body", span)?;
        }
        self.add_statement(span, Statement::Error)
    }

    fn parse_unsupported_block_element(&mut self) -> Result<StatementId, ParserInternalError> {
        let first_lexeme = self.bump()?;
        let first = first_lexeme.span();
        let mut end = first.end();
        if self.current_is_keyword(Keyword::Val)
            && matches!(
                first_lexeme.kind(),
                LexemeKind::Token(TokenKind::Keyword(Keyword::Const))
            )
        {
            end = self.bump()?.span().end();
        }
        let span = self.span(first.start(), end)?;
        self.emit(
            codes::UNSUPPORTED_BLOCK_ELEMENT,
            "unsupported block element",
            span,
        )?;
        self.add_statement(span, Statement::Error)
    }

    fn is_unsupported_block_element(&self) -> bool {
        self.peek(0)
            .is_some_and(|lexeme| unsupported_block_element_kind(lexeme.kind()))
    }

    fn parse_declaration_root(&mut self) -> Result<ItemId, ParserInternalError> {
        let root = self.parse_declaration_item()?;
        self.consume_declaration_tail(root)
    }

    fn parse_file_header(
        &mut self,
    ) -> Result<(Option<PackageDirective>, Vec<ImportDirective>), ParserInternalError> {
        let package = if self.current_is_keyword(Keyword::Package) {
            let directive = self.parse_package_directive()?;
            self.consume_file_header_separator()?;
            Some(directive)
        } else {
            None
        };

        let mut imports = Vec::new();
        while self.current_is_keyword(Keyword::Import) {
            imports.push(self.parse_import_directive()?);
            self.consume_file_header_separator()?;
        }

        Ok((package, imports))
    }

    fn consume_file_header_separator(&mut self) -> Result<(), ParserInternalError> {
        if matches!(self.current()?.kind(), LexemeKind::Eof) {
            return Ok(());
        }
        if self.file_separator_region_has_line_break()? {
            return Ok(());
        }
        if self.current_is_symbol(Symbol::Semicolon) {
            self.bump()?;
            return Ok(());
        }
        self.emit(
            codes::EXPECTED_FILE_HEADER_SEPARATOR,
            "expected file header separator",
            self.current()?.span(),
        )
    }

    fn parse_package_directive(&mut self) -> Result<PackageDirective, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let (segments, _) = self.parse_qualified_name(
            codes::EXPECTED_PACKAGE_NAME,
            "expected package name",
            false,
        )?;
        let end = self.previous_significant_end().max(keyword_span.end());
        Ok(PackageDirective {
            span: self.span(keyword_span.start(), end)?,
            keyword_span,
            segments,
        })
    }

    fn parse_import_directive(&mut self) -> Result<ImportDirective, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let (segments, wildcard_span) = self.parse_import_target()?;
        let mut alias = None;
        let mut end = self.previous_significant_end().max(keyword_span.end());

        if self.current_is_keyword(Keyword::As) {
            let as_span = self.bump()?.span();
            if wildcard_span.is_some() {
                self.emit(
                    codes::WILDCARD_IMPORT_ALIAS,
                    "wildcard import cannot have an alias",
                    as_span,
                )?;
            }
            let name_span = if self.current_is_identifier() {
                self.bump()?.span()
            } else {
                let current = self.current()?;
                let boundary = matches!(current.kind(), LexemeKind::Eof)
                    || self.is_file_declaration_boundary(current);
                let span = if boundary {
                    self.empty_at(current.span().start())?
                } else {
                    current.span()
                };
                if !self.is_poison_kind(current.kind()) {
                    self.emit(codes::EXPECTED_IMPORT_ALIAS, "expected import alias", span)?;
                }
                if !boundary {
                    self.recover_declaration_region(DeclarationStops::FILE)?;
                    end = self.previous_significant_end().max(end);
                }
                span
            };
            end = end.max(name_span.end()).max(as_span.end());
            if wildcard_span.is_none() {
                alias = Some(ImportAlias { as_span, name_span });
            }
        }

        Ok(ImportDirective {
            span: self.span(keyword_span.start(), end)?,
            keyword_span,
            segments,
            wildcard_span,
            alias,
        })
    }

    fn parse_import_target(
        &mut self,
    ) -> Result<(Vec<QualifiedNameSegment>, Option<Span>), ParserInternalError> {
        let (segments, wildcard_prefix) = self.parse_qualified_name(
            codes::EXPECTED_IMPORT_TARGET,
            "expected import target",
            true,
        )?;
        let wildcard_span = if wildcard_prefix && self.current_is_symbol(Symbol::Star) {
            Some(self.bump()?.span())
        } else {
            None
        };
        Ok((segments, wildcard_span))
    }

    fn parse_qualified_name(
        &mut self,
        code: &str,
        message: &'static str,
        allow_wildcard: bool,
    ) -> Result<(Vec<QualifiedNameSegment>, bool), ParserInternalError> {
        let mut segments = Vec::new();
        if !self.current_is_identifier() {
            let current = self.current()?;
            let boundary = matches!(current.kind(), LexemeKind::Eof)
                || self.is_file_declaration_boundary(current);
            let span = if boundary {
                self.empty_at(current.span().start())?
            } else {
                current.span()
            };
            if !self.is_poison_kind(current.kind()) {
                self.emit(code, message, span)?;
            }
            if !boundary {
                self.recover_declaration_region(DeclarationStops::FILE)?;
            }
            return Ok((segments, false));
        }

        segments.push(QualifiedNameSegment {
            span: self.bump()?.span(),
        });
        while self.current_is_symbol(Symbol::Dot) {
            self.bump()?;
            if allow_wildcard && self.current_is_symbol(Symbol::Star) {
                return Ok((segments, true));
            }
            if self.current_is_identifier() {
                segments.push(QualifiedNameSegment {
                    span: self.bump()?.span(),
                });
                continue;
            }
            let current = self.current()?;
            let boundary = matches!(current.kind(), LexemeKind::Eof)
                || self.is_file_declaration_boundary(current);
            let span = if boundary {
                self.empty_at(current.span().start())?
            } else {
                current.span()
            };
            if !self.is_poison_kind(current.kind()) {
                self.emit(code, message, span)?;
            }
            if !boundary {
                self.recover_declaration_region(DeclarationStops::FILE)?;
            }
            break;
        }
        Ok((segments, false))
    }

    fn parse_file_roots(&mut self) -> Result<Vec<ItemId>, ParserInternalError> {
        let mut roots = Vec::new();
        while !matches!(self.current()?.kind(), LexemeKind::Eof) {
            if self.current_is_symbol(Symbol::Semicolon) {
                let span = self.bump()?.span();
                self.emit(codes::EXPECTED_DECLARATION, "expected declaration", span)?;
                roots.push(self.add_item(span, Item::Error)?);
                continue;
            }

            if self.current_is_keyword(Keyword::Package) || self.current_is_keyword(Keyword::Import)
            {
                let is_package = self.current_is_keyword(Keyword::Package);
                let start = self.current()?.span().start();
                let (directive_end, primary) = if is_package {
                    let directive = self.parse_package_directive()?;
                    (directive.span.end(), directive.keyword_span)
                } else {
                    let directive = self.parse_import_directive()?;
                    (directive.span.end(), directive.keyword_span)
                };
                let span = self.span(start, directive_end)?;
                self.emit(
                    if is_package {
                        codes::MISPLACED_PACKAGE_DIRECTIVE
                    } else {
                        codes::MISPLACED_IMPORT_DIRECTIVE
                    },
                    if is_package {
                        "misplaced package directive"
                    } else {
                        "misplaced import directive"
                    },
                    primary,
                )?;
                roots.push(self.add_item(span, Item::Error)?);
                self.consume_file_header_separator()?;
                continue;
            }

            let started_as_declaration = simple_declaration_start_kind(self.current()?.kind());
            let before = self.index;
            let root = self.parse_declaration_item()?;
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            roots.push(root);

            if matches!(self.current()?.kind(), LexemeKind::Eof) {
                break;
            }
            let separated_by_line_break = self.file_separator_region_has_line_break()?;
            if self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
                continue;
            }
            if started_as_declaration
                && self.is_file_declaration_boundary(self.current()?)
                && !separated_by_line_break
            {
                self.emit(
                    if self.current_is_keyword(Keyword::Package)
                        || self.current_is_keyword(Keyword::Import)
                    {
                        codes::EXPECTED_FILE_HEADER_SEPARATOR
                    } else {
                        codes::EXPECTED_DECLARATION_SEPARATOR
                    },
                    if self.current_is_keyword(Keyword::Package)
                        || self.current_is_keyword(Keyword::Import)
                    {
                        "expected file header separator"
                    } else {
                        "expected declaration separator"
                    },
                    self.current()?.span(),
                )?;
            }
        }
        Ok(roots)
    }

    fn parse_declaration_item(&mut self) -> Result<ItemId, ParserInternalError> {
        let modifiers = self.parse_declaration_modifiers(false)?;
        let declaration = self.parse_unmodified_declaration_item()?;
        self.wrap_modified_item(modifiers, declaration)
    }

    fn parse_unmodified_declaration_item(&mut self) -> Result<ItemId, ParserInternalError> {
        if self.current_identifier_is("nocopy")?
            && self
                .peek(1)
                .is_some_and(|next| classifier_declaration_start_kind(next.kind()))
        {
            let primary = self.bump()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            return self.parse_classifier_declaration();
        }
        Ok(
            if self.local_destructuring_start(Keyword::Val)
                || self.local_destructuring_start(Keyword::Var)
                || self.const_local_destructuring_start()
            {
                self.parse_unsupported_destructuring_context()?
            } else if self.current_is_keyword(Keyword::Val) {
                let keyword = self.bump()?.span();
                self.parse_variable_declaration(keyword, VariableKind::Val)?
            } else if self.current_is_keyword(Keyword::Var) {
                let keyword = self.bump()?.span();
                self.parse_variable_declaration(keyword, VariableKind::Var)?
            } else if self.current_is_keyword(Keyword::Const) {
                self.parse_constant_declaration()?
            } else if self.current_is_keyword(Keyword::Fun) {
                self.parse_function_declaration()?
            } else if classifier_declaration_start_kind(self.current()?.kind()) {
                self.parse_classifier_declaration()?
            } else {
                let current = self.current()?;
                let span = if self.file_mode {
                    let start = current.span().start();
                    let end = self.recover_declaration_region(DeclarationStops::FILE)?;
                    self.span(start, end.max(current.span().end()))?
                } else if matches!(current.kind(), LexemeKind::Eof) {
                    self.empty_at(current.span().start())?
                } else {
                    self.bump()?.span()
                };
                if !self.is_poison_kind(current.kind()) {
                    self.emit(codes::EXPECTED_DECLARATION, "expected declaration", span)?;
                }
                self.add_item(span, Item::Error)?
            },
        )
    }

    fn parse_declaration_modifiers(
        &mut self,
        allow_override: bool,
    ) -> Result<DeclarationModifiers, ParserInternalError> {
        let mut modifiers = DeclarationModifiers::default();
        let mut saw_override = false;
        loop {
            let visibility = if self.current_is_keyword(Keyword::Public) {
                Some(VisibilityModifier::Public(self.current()?.span()))
            } else if self.current_is_keyword(Keyword::Internal) {
                Some(VisibilityModifier::Internal(self.current()?.span()))
            } else if self.current_is_keyword(Keyword::Private) {
                Some(VisibilityModifier::Private(self.current()?.span()))
            } else {
                None
            };
            if let Some(visibility) = visibility {
                let span = self.bump()?.span();
                if modifiers.visibility.is_some() || saw_override {
                    self.emit(
                        codes::INVALID_DECLARATION_MODIFIER,
                        "invalid declaration modifier",
                        span,
                    )?;
                } else {
                    modifiers.visibility = Some(visibility);
                }
                continue;
            }
            if self.current_is_keyword(Keyword::Override) {
                let span = self.bump()?.span();
                if !allow_override || saw_override {
                    self.emit(
                        codes::INVALID_DECLARATION_MODIFIER,
                        "invalid declaration modifier",
                        span,
                    )?;
                } else {
                    modifiers.override_span = Some(span);
                }
                saw_override = true;
                continue;
            }
            break;
        }
        Ok(modifiers)
    }

    fn wrap_modified_item(
        &mut self,
        modifiers: DeclarationModifiers,
        declaration: ItemId,
    ) -> Result<ItemId, ParserInternalError> {
        let Some(start) = declaration_modifier_start(modifiers) else {
            return Ok(declaration);
        };
        let child_span = self.ast.items().get(declaration)?.span();
        self.add_item(
            self.span(start, child_span.end().max(start))?,
            Item::Modified {
                modifiers,
                declaration,
            },
        )
    }

    fn parse_variable_declaration(
        &mut self,
        keyword: Span,
        kind: VariableKind,
    ) -> Result<ItemId, ParserInternalError> {
        let name = self.parse_name_marker(
            codes::EXPECTED_DECLARATION_NAME,
            "expected declaration name",
            NameContext::Declaration,
        )?;
        let (colon_span, type_ref) = self.parse_optional_type_annotation()?;
        let (equals_span, initializer) = self.parse_required_initializer()?;
        let end = self.expression_span(initializer)?.end().max(keyword.end());
        self.add_item(
            self.span(keyword.start(), end)?,
            Item::Variable {
                kind,
                name,
                colon_span,
                type_ref,
                equals_span,
                initializer,
            },
        )
    }

    fn parse_classifier_declaration(&mut self) -> Result<ItemId, ParserInternalError> {
        let kind = if self.current_is_keyword(Keyword::Value) {
            let value_span = self.bump()?.span();
            let class_span = if self.current_is_keyword(Keyword::Class) {
                self.bump()?.span()
            } else {
                let primary = self.current()?.span();
                self.emit(
                    codes::EXPECTED_CLASS_KEYWORD,
                    "expected 'class' keyword",
                    primary,
                )?;
                self.empty_at(primary.start())?
            };
            ClassifierKind::ValueClass {
                value_span,
                class_span,
            }
        } else if self.current_is_keyword(Keyword::Class) {
            ClassifierKind::Class {
                class_span: self.bump()?.span(),
            }
        } else if self.current_is_keyword(Keyword::Interface) {
            ClassifierKind::Interface {
                interface_span: self.bump()?.span(),
            }
        } else if self.current_is_keyword(Keyword::Enum) {
            let enum_span = self.bump()?.span();
            let class_span = if self.current_is_keyword(Keyword::Class) {
                self.bump()?.span()
            } else {
                let primary = self.current()?.span();
                self.emit(
                    codes::EXPECTED_CLASS_KEYWORD,
                    "expected 'class' keyword",
                    primary,
                )?;
                self.empty_at(primary.start())?
            };
            ClassifierKind::EnumClass {
                enum_span,
                class_span,
            }
        } else {
            ClassifierKind::Object {
                object_span: self.bump()?.span(),
            }
        };
        let start = classifier_keyword_start(kind);
        let name = self.parse_classifier_name()?;
        let (type_parameters, type_parameter_list_span) = self.parse_type_parameters()?;
        if let (ClassifierKind::Object { .. }, Some(list_span)) = (kind, type_parameter_list_span) {
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                list_span,
            )?;
        }

        let supports_constructor = matches!(
            kind,
            ClassifierKind::ValueClass { .. } | ClassifierKind::Class { .. }
        );
        let primary_constructor = if self.current_is_symbol(Symbol::LeftParen) {
            let constructor =
                self.parse_primary_constructor(matches!(kind, ClassifierKind::ValueClass { .. }))?;
            if !supports_constructor {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    constructor.left_paren_span,
                )?;
            }
            supports_constructor.then_some(constructor)
        } else {
            if matches!(kind, ClassifierKind::ValueClass { .. }) {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    self.empty_at(self.current()?.span().start())?,
                )?;
            }
            None
        };

        let (supertype_colon_span, supertypes) = self.parse_supertype_list(kind)?;
        let body = if self.current_is_symbol(Symbol::LeftBrace) {
            Some(self.parse_classifier_body(kind)?)
        } else {
            if matches!(kind, ClassifierKind::EnumClass { .. }) {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    self.empty_at(self.current()?.span().start())?,
                )?;
            }
            None
        };
        let end = body
            .as_ref()
            .and_then(|body| body.right_brace_span)
            .map(Span::end)
            .or_else(|| {
                supertypes
                    .last()
                    .map(|entry| entry.span.end())
                    .or_else(|| {
                        primary_constructor.as_ref().map(|constructor| {
                            constructor
                                .right_paren_span
                                .map(Span::end)
                                .unwrap_or_else(|| {
                                    constructor
                                        .fields
                                        .last()
                                        .map(|field| field.span.end())
                                        .unwrap_or(constructor.left_paren_span.end())
                                })
                        })
                    })
                    .or_else(|| type_parameter_list_span.map(Span::end))
            })
            .unwrap_or(marker_span(name).end().max(start));
        self.add_item(
            self.span(start, end.max(start))?,
            Item::Classifier(Box::new(ClassifierDeclaration {
                kind,
                name,
                type_parameters,
                type_parameter_list_span,
                primary_constructor,
                supertype_colon_span,
                supertypes,
                body,
            })),
        )
    }

    fn parse_classifier_name(&mut self) -> Result<NameMarker, ParserInternalError> {
        if self.current_is_identifier() {
            return Ok(NameMarker::Present(self.bump()?.span()));
        }
        let current = self.current()?;
        let is_boundary = matches!(current.kind(), LexemeKind::Eof)
            || matches!(
                current.kind(),
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::Less | Symbol::LeftParen | Symbol::Colon | Symbol::LeftBrace
                ))
            )
            || self.is_file_declaration_boundary(current);
        let primary = if is_boundary {
            self.empty_at(current.span().start())?
        } else {
            current.span()
        };
        if !self.is_poison_kind(current.kind()) {
            self.emit(
                codes::EXPECTED_CLASSIFIER_NAME,
                "expected classifier name",
                primary,
            )?;
        }
        if is_boundary {
            Ok(NameMarker::Missing(primary))
        } else {
            self.bump()?;
            Ok(NameMarker::Error(primary))
        }
    }

    fn parse_primary_constructor(
        &mut self,
        require_nonempty: bool,
    ) -> Result<PrimaryConstructor, ParserInternalError> {
        let left_paren_span = self.bump()?.span();
        let mut fields = Vec::new();
        if self.current_is_symbol(Symbol::RightParen) && require_nonempty {
            self.emit(
                codes::EXPECTED_CONSTRUCTOR_FIELD,
                "expected constructor field",
                self.current()?.span(),
            )?;
        }
        while !self.current_is_symbol(Symbol::RightParen)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                self.emit(
                    codes::EXPECTED_CONSTRUCTOR_FIELD,
                    "expected constructor field",
                    comma,
                )?;
                continue;
            }
            if let Some(field) = self.parse_class_field()? {
                fields.push(field);
            }
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                if self.current_is_symbol(Symbol::RightParen) {
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        comma,
                    )?;
                }
                continue;
            }
            if self.current_is_symbol(Symbol::RightParen) {
                break;
            }
            self.emit(
                codes::EXPECTED_CONSTRUCTOR_SEPARATOR,
                "expected constructor separator",
                self.current()?.span(),
            )?;
            if class_field_start_kind(self.current()?.kind()) {
                continue;
            }
            self.recover_declaration_region(
                DeclarationStops::EMPTY
                    .with(DeclarationStops::COMMA)
                    .with(DeclarationStops::RIGHT_PAREN),
            )?;
        }
        let right_paren_span = if self.current_is_symbol(Symbol::RightParen) {
            Some(self.bump()?.span())
        } else {
            self.emit_closing(
                self.empty_at(self.current()?.span().start())?,
                left_paren_span,
            )?;
            None
        };
        Ok(PrimaryConstructor {
            left_paren_span,
            fields,
            right_paren_span,
        })
    }

    fn parse_class_field(&mut self) -> Result<Option<ClassField>, ParserInternalError> {
        let modifiers = self.parse_declaration_modifiers(false)?;
        let visibility = modifiers.visibility;
        let current_start = self.current()?.span().start();
        let start = declaration_modifier_start(modifiers).unwrap_or(current_start);
        if matches!(
            self.current()?.kind(),
            LexemeKind::Token(TokenKind::Keyword(
                Keyword::Borrow | Keyword::Inout | Keyword::Own | Keyword::Vararg
            ))
        ) {
            let primary = self.bump()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
        }
        let (kind, keyword_span) = if self.current_is_keyword(Keyword::Val) {
            (VariableKind::Val, self.bump()?.span())
        } else if self.current_is_keyword(Keyword::Var) {
            (VariableKind::Var, self.bump()?.span())
        } else {
            let primary = self.current()?.span();
            if !self.is_poison() {
                self.emit(
                    codes::EXPECTED_CONSTRUCTOR_FIELD,
                    "expected constructor field",
                    primary,
                )?;
            }
            self.recover_declaration_region(
                DeclarationStops::EMPTY
                    .with(DeclarationStops::COMMA)
                    .with(DeclarationStops::RIGHT_PAREN),
            )?;
            return Ok(None);
        };
        let name = self.parse_name_marker(
            codes::EXPECTED_PARAMETER_NAME,
            "expected parameter name",
            NameContext::ValueParameter,
        )?;
        let colon_span = if self.current_is_symbol(Symbol::Colon) {
            self.bump()?.span()
        } else {
            let primary = self.empty_at(self.current()?.span().start())?;
            self.emit(
                codes::EXPECTED_PARAMETER_COLON,
                "expected parameter colon",
                primary,
            )?;
            primary
        };
        let type_ref = self.parse_type_ref(
            TypeStops::empty()
                .with(TypeStops::COMMA)
                .with(TypeStops::RIGHT_PAREN)
                .with(TypeStops::EQUAL),
        )?;
        if self.current_is_symbol(Symbol::Equal) {
            let primary = self.bump()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            self.recover_declaration_region(
                DeclarationStops::EMPTY
                    .with(DeclarationStops::COMMA)
                    .with(DeclarationStops::RIGHT_PAREN),
            )?;
        }
        let end = self.type_span(type_ref)?.end().max(keyword_span.end());
        Ok(Some(ClassField {
            span: self.span(start, end.max(start))?,
            visibility,
            kind,
            keyword_span,
            name,
            colon_span,
            type_ref,
        }))
    }

    fn parse_supertype_list(
        &mut self,
        kind: ClassifierKind,
    ) -> Result<(Option<Span>, Vec<SupertypeEntry>), ParserInternalError> {
        if !self.current_is_symbol(Symbol::Colon) {
            return Ok((None, Vec::new()));
        }
        let colon_span = self.bump()?.span();
        let mut entries = Vec::new();
        loop {
            let current = self.current()?;
            if self.can_start_type_ref(current) {
                let type_ref = self.parse_type_ref(
                    TypeStops::empty()
                        .with(TypeStops::COMMA)
                        .with(TypeStops::LEFT_BRACE)
                        .with(TypeStops::FILE),
                )?;
                let type_span = self.type_span(type_ref)?;
                let delegation = if self.current_identifier_is("by")? {
                    let by_span = self.bump()?.span();
                    if !matches!(kind, ClassifierKind::Class { .. }) {
                        self.emit(
                            codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                            "unsupported class-family form",
                            by_span,
                        )?;
                    }
                    let current = self.current()?;
                    let target_boundary = self.current_is_symbol(Symbol::Comma)
                        || self.current_is_symbol(Symbol::LeftBrace)
                        || matches!(current.kind(), LexemeKind::Eof)
                        || self.is_file_declaration_boundary(current);
                    let target = if self.current_is_identifier() {
                        NameMarker::Present(self.bump()?.span())
                    } else {
                        let primary = if target_boundary {
                            self.empty_at(current.span().start())?
                        } else {
                            current.span()
                        };
                        if !self.is_poison_kind(current.kind()) {
                            self.emit(
                                codes::EXPECTED_DELEGATION_TARGET,
                                "expected delegation target",
                                primary,
                            )?;
                        }
                        if target_boundary {
                            NameMarker::Missing(primary)
                        } else {
                            self.bump()?;
                            NameMarker::Error(primary)
                        }
                    };
                    let target_span = marker_span(target);
                    let delegation = DelegationClause {
                        span: self.span(
                            by_span.start(),
                            if target_span.is_empty() {
                                by_span.end()
                            } else {
                                target_span.end().max(by_span.end())
                            },
                        )?,
                        by_span,
                        target,
                    };
                    let current = self.current()?;
                    let at_entry_boundary = self.current_is_symbol(Symbol::Comma)
                        || self.current_is_symbol(Symbol::LeftBrace)
                        || matches!(current.kind(), LexemeKind::Eof)
                        || self.is_file_declaration_boundary(current);
                    if !at_entry_boundary {
                        if !self.is_poison_kind(current.kind()) {
                            self.emit(
                                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                                "unsupported class-family form",
                                current.span(),
                            )?;
                        }
                        self.recover_declaration_region(
                            DeclarationStops::EMPTY
                                .with(DeclarationStops::COMMA)
                                .with(DeclarationStops::LEFT_BRACE)
                                .union(self.root_declaration_stops()),
                        )?;
                    }
                    Some(delegation)
                } else {
                    None
                };
                entries.push(SupertypeEntry {
                    span: self.span(
                        type_span.start(),
                        delegation
                            .map(|clause| clause.span.end())
                            .unwrap_or(type_span.end()),
                    )?,
                    type_ref,
                    delegation,
                });
                if self.current_is_symbol(Symbol::LeftParen) {
                    let primary = self.current()?.span();
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        primary,
                    )?;
                    self.recover_declaration_region(
                        DeclarationStops::EMPTY
                            .with(DeclarationStops::COMMA)
                            .with(DeclarationStops::LEFT_BRACE)
                            .union(self.root_declaration_stops()),
                    )?;
                }
            } else {
                let primary = if self.current_is_symbol(Symbol::LeftBrace)
                    || matches!(current.kind(), LexemeKind::Eof)
                    || self.is_file_declaration_boundary(current)
                {
                    self.empty_at(current.span().start())?
                } else {
                    current.span()
                };
                if !self.is_poison_kind(current.kind()) {
                    self.emit(codes::EXPECTED_SUPERTYPE, "expected supertype", primary)?;
                }
                if !primary.is_empty() {
                    self.recover_declaration_region(
                        DeclarationStops::EMPTY
                            .with(DeclarationStops::COMMA)
                            .with(DeclarationStops::LEFT_BRACE)
                            .union(self.root_declaration_stops()),
                    )?;
                }
            }
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                if self.current_is_symbol(Symbol::LeftBrace)
                    || matches!(self.current()?.kind(), LexemeKind::Eof)
                {
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        comma,
                    )?;
                    break;
                }
                continue;
            }
            break;
        }
        Ok((Some(colon_span), entries))
    }

    fn parse_classifier_body(
        &mut self,
        kind: ClassifierKind,
    ) -> Result<ClassifierBody, ParserInternalError> {
        if matches!(kind, ClassifierKind::EnumClass { .. }) {
            self.parse_enum_body()
        } else {
            let context = match kind {
                ClassifierKind::ValueClass { .. } => ClassMemberContext::ValueClass,
                ClassifierKind::Class { .. } => ClassMemberContext::Class,
                ClassifierKind::Interface { .. } => ClassMemberContext::Interface,
                ClassifierKind::Object { .. } => ClassMemberContext::Object,
                ClassifierKind::EnumClass { .. } => ClassMemberContext::Enum,
            };
            self.parse_ordinary_classifier_body(context)
        }
    }

    fn parse_ordinary_classifier_body(
        &mut self,
        context: ClassMemberContext,
    ) -> Result<ClassifierBody, ParserInternalError> {
        let left_brace_span = self.bump()?.span();
        let members = self.parse_classifier_members(context)?;
        let right_brace_span = self.finish_classifier_body(left_brace_span)?;
        Ok(ClassifierBody {
            left_brace_span,
            variants: Vec::new(),
            enum_member_delimiter_span: None,
            members,
            right_brace_span,
        })
    }

    fn parse_classifier_members(
        &mut self,
        context: ClassMemberContext,
    ) -> Result<Vec<ItemId>, ParserInternalError> {
        let mut members = Vec::new();
        while !self.current_is_symbol(Symbol::RightBrace)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            if self.current_is_symbol(Symbol::Semicolon) {
                let primary = self.bump()?.span();
                self.emit(codes::EXPECTED_MEMBER, "expected member", primary)?;
                continue;
            }
            let before = self.index;
            let member = self.parse_classifier_member(context)?;
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            let member_end = self.ast.items().get(member)?.span().end();
            members.push(member);
            if self.current_is_symbol(Symbol::RightBrace)
                || matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                break;
            }
            if self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
                continue;
            }
            if self.gap_has_line_break(member_end, self.current()?.span().start())? {
                continue;
            }
            self.emit(
                codes::EXPECTED_MEMBER_SEPARATOR,
                "expected member separator",
                self.current()?.span(),
            )?;
            if class_member_start_kind(self.current()?.kind()) {
                continue;
            }
            self.recover_declaration_region(
                DeclarationStops::EMPTY
                    .with(DeclarationStops::RIGHT_BRACE)
                    .with(DeclarationStops::SEMICOLON)
                    .with(DeclarationStops::CLASS_MEMBER),
            )?;
            if self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
            }
        }
        Ok(members)
    }

    fn parse_classifier_member(
        &mut self,
        context: ClassMemberContext,
    ) -> Result<ItemId, ParserInternalError> {
        let modifiers = self.parse_declaration_modifiers(context.allows_override())?;
        let primary = self.current()?.span();
        if matches!(context, ClassMemberContext::Interface)
            && self.current_is_keyword(Keyword::Fun)
            && let Some(
                visibility @ (VisibilityModifier::Internal(_) | VisibilityModifier::Private(_)),
            ) = modifiers.visibility
        {
            self.emit(
                codes::INVALID_DECLARATION_MODIFIER,
                "invalid declaration modifier",
                visibility_span(visibility),
            )?;
        }
        let declaration = if self.current_is_keyword(Keyword::Companion) {
            if !context.allows_companion() {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    primary,
                )?;
            }
            self.parse_companion_object()?
        } else if self.current_is_keyword(Keyword::Fun) {
            self.parse_function_declaration()?
        } else if self.current_is_keyword(Keyword::Const) {
            if !context.allows_constant() {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    primary,
                )?;
            }
            self.parse_constant_declaration()?
        } else if self.current_is_keyword(Keyword::Val) || self.current_is_keyword(Keyword::Var) {
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            let kind = if self.current_is_keyword(Keyword::Var) {
                VariableKind::Var
            } else {
                VariableKind::Val
            };
            let keyword = self.bump()?.span();
            self.parse_variable_declaration(keyword, kind)?
        } else if classifier_declaration_start_kind(self.current()?.kind()) {
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            self.parse_classifier_declaration()?
        } else {
            let boundary = self.current_is_symbol(Symbol::RightBrace)
                || matches!(self.current()?.kind(), LexemeKind::Eof);
            let primary = if boundary {
                self.empty_at(primary.start())?
            } else {
                primary
            };
            let start = primary.start();
            if !self.is_poison() {
                if self.current_identifier_is("constructor")?
                    || self.current_identifier_is("init")?
                    || self.current_identifier_is("by")?
                {
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        primary,
                    )?;
                } else {
                    self.emit(codes::EXPECTED_MEMBER, "expected member", primary)?;
                }
            }
            let end = self.recover_declaration_region(
                DeclarationStops::EMPTY
                    .with(DeclarationStops::RIGHT_BRACE)
                    .with(DeclarationStops::SEMICOLON)
                    .with(DeclarationStops::CLASS_MEMBER),
            )?;
            self.add_item(self.span(start, end.max(primary.end()))?, Item::Error)?
        };
        self.wrap_modified_item(modifiers, declaration)
    }

    fn parse_companion_object(&mut self) -> Result<ItemId, ParserInternalError> {
        let companion_span = self.bump()?.span();
        let object_span = if self.current_is_keyword(Keyword::Object) {
            self.bump()?.span()
        } else {
            let primary = self.current()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            self.empty_at(primary.start())?
        };
        let body = if self.current_is_symbol(Symbol::LeftBrace) {
            self.parse_ordinary_classifier_body(ClassMemberContext::Companion)?
        } else {
            let primary = self.current()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
            ClassifierBody {
                left_brace_span: self.empty_at(primary.start())?,
                variants: Vec::new(),
                enum_member_delimiter_span: None,
                members: Vec::new(),
                right_brace_span: None,
            }
        };
        let end = body
            .right_brace_span
            .map(Span::end)
            .unwrap_or_else(|| object_span.end().max(companion_span.end()));
        self.add_item(
            self.span(companion_span.start(), end)?,
            Item::Companion(Box::new(CompanionObject {
                companion_span,
                object_span,
                body,
            })),
        )
    }

    fn parse_enum_body(&mut self) -> Result<ClassifierBody, ParserInternalError> {
        let left_brace_span = self.bump()?.span();
        let mut variants = Vec::new();
        let mut delimiter = None;
        if self.current_is_symbol(Symbol::RightBrace) {
            self.emit(
                codes::EXPECTED_ENUM_VARIANT,
                "expected enum variant",
                self.current()?.span(),
            )?;
        }
        while !self.current_is_symbol(Symbol::RightBrace)
            && !self.current_is_symbol(Symbol::Semicolon)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            if !self.current_is_identifier() {
                if class_member_start_kind(self.current()?.kind()) {
                    self.emit(
                        codes::EXPECTED_ENUM_MEMBER_DELIMITER,
                        "expected enum member delimiter",
                        self.current()?.span(),
                    )?;
                    break;
                }
                let primary = self.current()?.span();
                if !self.is_poison() {
                    self.emit(
                        codes::EXPECTED_ENUM_VARIANT,
                        "expected enum variant",
                        primary,
                    )?;
                }
                self.recover_declaration_region(
                    DeclarationStops::EMPTY
                        .with(DeclarationStops::COMMA)
                        .with(DeclarationStops::RIGHT_BRACE)
                        .with(DeclarationStops::SEMICOLON)
                        .with(DeclarationStops::ENUM_VARIANT),
                )?;
            } else {
                variants.push(self.parse_enum_variant()?);
            }
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                if self.current_is_symbol(Symbol::RightBrace)
                    || self.current_is_symbol(Symbol::Semicolon)
                {
                    self.emit(
                        codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                        "unsupported class-family form",
                        comma,
                    )?;
                }
                continue;
            }
            if self.current_is_symbol(Symbol::RightBrace)
                || self.current_is_symbol(Symbol::Semicolon)
                || matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                break;
            }
            if class_member_start_kind(self.current()?.kind()) {
                self.emit(
                    codes::EXPECTED_ENUM_MEMBER_DELIMITER,
                    "expected enum member delimiter",
                    self.current()?.span(),
                )?;
                break;
            }
            self.emit(
                codes::EXPECTED_ENUM_VARIANT_SEPARATOR,
                "expected enum variant separator",
                self.current()?.span(),
            )?;
            if self.current_is_identifier() {
                continue;
            }
        }
        let members = if self.current_is_symbol(Symbol::Semicolon) {
            let separator = self.bump()?.span();
            delimiter = Some(separator);
            let members = self.parse_classifier_members(ClassMemberContext::Enum)?;
            if members.is_empty() {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    separator,
                )?;
            }
            members
        } else if class_member_start_kind(self.current()?.kind()) {
            self.parse_classifier_members(ClassMemberContext::Enum)?
        } else {
            Vec::new()
        };
        let right_brace_span = self.finish_classifier_body(left_brace_span)?;
        Ok(ClassifierBody {
            left_brace_span,
            variants,
            enum_member_delimiter_span: delimiter,
            members,
            right_brace_span,
        })
    }

    fn parse_enum_variant(&mut self) -> Result<EnumVariant, ParserInternalError> {
        let name = NameMarker::Present(self.bump()?.span());
        let start = marker_span(name).start();
        let mut parameters = Vec::new();
        let mut left_paren_span = None;
        let mut right_paren_span = None;
        if self.current_is_symbol(Symbol::LeftParen) {
            let opener = self.bump()?.span();
            left_paren_span = Some(opener);
            if self.current_is_symbol(Symbol::RightParen) {
                self.emit(
                    codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                    "unsupported class-family form",
                    self.current()?.span(),
                )?;
            }
            while !self.current_is_symbol(Symbol::RightParen)
                && !matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                if self.current_is_symbol(Symbol::Comma) {
                    let primary = self.bump()?.span();
                    self.emit(
                        codes::EXPECTED_CONSTRUCTOR_FIELD,
                        "expected constructor field",
                        primary,
                    )?;
                    continue;
                }
                parameters.push(self.parse_enum_variant_parameter()?);
                if self.current_is_symbol(Symbol::Comma) {
                    let comma = self.bump()?.span();
                    if self.current_is_symbol(Symbol::RightParen) {
                        self.emit(
                            codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                            "unsupported class-family form",
                            comma,
                        )?;
                    }
                    continue;
                }
                if !self.current_is_symbol(Symbol::RightParen) {
                    self.emit(
                        codes::EXPECTED_CONSTRUCTOR_SEPARATOR,
                        "expected constructor separator",
                        self.current()?.span(),
                    )?;
                    if self.current_is_identifier() {
                        continue;
                    }
                    self.recover_declaration_region(
                        DeclarationStops::EMPTY
                            .with(DeclarationStops::COMMA)
                            .with(DeclarationStops::RIGHT_PAREN),
                    )?;
                }
            }
            if self.current_is_symbol(Symbol::RightParen) {
                right_paren_span = Some(self.bump()?.span());
            } else {
                self.emit_closing(self.empty_at(self.current()?.span().start())?, opener)?;
            }
        }
        let end = right_paren_span
            .map(Span::end)
            .or_else(|| parameters.last().map(|parameter| parameter.span.end()))
            .unwrap_or(marker_span(name).end());
        Ok(EnumVariant {
            span: self.span(start, end)?,
            name,
            left_paren_span,
            parameters,
            right_paren_span,
        })
    }

    fn parse_enum_variant_parameter(
        &mut self,
    ) -> Result<EnumVariantParameter, ParserInternalError> {
        if matches!(
            self.current()?.kind(),
            LexemeKind::Token(TokenKind::Keyword(
                Keyword::Val
                    | Keyword::Var
                    | Keyword::Borrow
                    | Keyword::Inout
                    | Keyword::Own
                    | Keyword::Vararg
            ))
        ) {
            let primary = self.bump()?.span();
            self.emit(
                codes::UNSUPPORTED_CLASS_FAMILY_FORM,
                "unsupported class-family form",
                primary,
            )?;
        }
        let name = self.parse_name_marker(
            codes::EXPECTED_PARAMETER_NAME,
            "expected parameter name",
            NameContext::ValueParameter,
        )?;
        let colon_span = if self.current_is_symbol(Symbol::Colon) {
            self.bump()?.span()
        } else {
            let primary = self.empty_at(self.current()?.span().start())?;
            self.emit(
                codes::EXPECTED_PARAMETER_COLON,
                "expected parameter colon",
                primary,
            )?;
            primary
        };
        let type_ref = self.parse_type_ref(
            TypeStops::empty()
                .with(TypeStops::COMMA)
                .with(TypeStops::RIGHT_PAREN),
        )?;
        let end = self.type_span(type_ref)?.end().max(marker_span(name).end());
        Ok(EnumVariantParameter {
            span: self.span(marker_span(name).start(), end)?,
            name,
            colon_span,
            type_ref,
        })
    }

    fn finish_classifier_body(
        &mut self,
        opener: Span,
    ) -> Result<Option<Span>, ParserInternalError> {
        if self.current_is_symbol(Symbol::RightBrace) {
            Ok(Some(self.bump()?.span()))
        } else {
            let current = self.current()?;
            if !self.is_poison_kind(current.kind()) {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
            Ok(None)
        }
    }

    fn parse_local_variable_declaration(
        &mut self,
        keyword: Span,
        kind: VariableKind,
        initializer_stops: Stops,
    ) -> Result<ItemId, ParserInternalError> {
        let name = self.parse_name_marker_with_stops(
            codes::EXPECTED_DECLARATION_NAME,
            "expected declaration name",
            NameContext::LocalDeclaration,
            DeclarationStops::from_expression_hard(initializer_stops),
        )?;
        let (colon_span, type_ref) = if self.current_is_symbol(Symbol::Colon) {
            let colon = self.bump()?.span();
            let type_ref = self.parse_type_ref(
                TypeStops::from_expression(initializer_stops)
                    .with(TypeStops::LEFT_BRACE)
                    .with(TypeStops::EQUAL),
            )?;
            (Some(colon), Some(type_ref))
        } else {
            (None, None)
        };
        let (equals_span, initializer) =
            self.parse_required_block_initializer(initializer_stops)?;
        let end = self
            .expression_span(initializer)?
            .end()
            .max(marker_span(name).end())
            .max(
                type_ref
                    .map(|id| self.type_span(id).map(Span::end))
                    .transpose()?
                    .unwrap_or(keyword.end()),
            );
        self.add_item(
            self.span(keyword.start(), end)?,
            Item::Variable {
                kind,
                name,
                colon_span,
                type_ref,
                equals_span,
                initializer,
            },
        )
    }

    fn local_destructuring_start(&self, keyword: Keyword) -> bool {
        self.current_is_keyword(keyword)
            && self.peek(1).is_some_and(|lexeme| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen))
                )
            })
    }

    fn const_local_destructuring_start(&self) -> bool {
        self.current_is_keyword(Keyword::Const)
            && self.peek(1).is_some_and(|lexeme| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Keyword(Keyword::Val))
                )
            })
            && self.peek(2).is_some_and(|lexeme| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen))
                )
            })
    }

    fn parse_unsupported_destructuring_context(&mut self) -> Result<ItemId, ParserInternalError> {
        let first = self.bump()?.span();
        if self.current_is_keyword(Keyword::Val) {
            self.bump()?;
        }
        let opener = self.bump()?.span();
        self.emit(
            codes::UNSUPPORTED_DESTRUCTURING_CONTEXT,
            "unsupported destructuring context",
            opener,
        )?;
        let end = self.recover_declaration_region(self.root_declaration_stops())?;
        self.add_item(
            self.span(first.start(), end.max(opener.end()))?,
            Item::Error,
        )
    }

    fn parse_unsupported_local_destructuring(
        &mut self,
        element_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let first = self.bump()?.span();
        let mut primary_end = first.end();
        if self.current_is_keyword(Keyword::Val) {
            primary_end = self.bump()?.span().end();
        }
        let primary = self.span(first.start(), primary_end)?;
        self.emit(
            codes::UNSUPPORTED_DESTRUCTURING_FORM,
            "unsupported destructuring form",
            primary,
        )?;
        let end = self.recover_declaration_region(
            DeclarationStops::from_expression_hard(element_stops)
                .with(DeclarationStops::BLOCK_ELEMENT),
        )?;
        self.add_statement(
            self.span(first.start(), end.max(primary_end))?,
            Statement::Error,
        )
    }

    fn parse_local_destructuring(
        &mut self,
        val_span: Span,
        initializer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let left_paren_span = self.bump()?.span();
        let mut bindings = Vec::new();
        let mut comma_already_reported_as_empty = false;
        let mut needs_binding = true;

        loop {
            let current = self.current()?;
            if self.current_is_symbol(Symbol::RightParen) {
                if bindings.is_empty() {
                    let missing = self.empty_at(current.span().start())?;
                    self.emit(
                        codes::EXPECTED_DESTRUCTURING_BINDING,
                        "expected destructuring binding",
                        missing,
                    )?;
                    bindings.push(NameMarker::Missing(missing));
                }
                break;
            }
            if self.current_is_symbol(Symbol::Equal)
                || initializer_stops.contains(current)
                || matches!(current.kind(), LexemeKind::Eof)
            {
                if needs_binding {
                    let missing = self.empty_at(current.span().start())?;
                    if !comma_already_reported_as_empty {
                        self.emit(
                            codes::EXPECTED_DESTRUCTURING_BINDING,
                            "expected destructuring binding",
                            missing,
                        )?;
                        bindings.push(NameMarker::Missing(missing));
                    }
                }
                break;
            }
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                self.emit(
                    codes::EXPECTED_DESTRUCTURING_BINDING,
                    "expected destructuring binding",
                    comma,
                )?;
                bindings.push(NameMarker::Missing(self.empty_at(comma.start())?));
                comma_already_reported_as_empty = true;
                needs_binding = true;
                if self.current_is_symbol(Symbol::RightParen) {
                    break;
                }
                continue;
            }

            let binding = if self.current_is_identifier() {
                let span = self.bump()?.span();
                if self.sources.slice(span)? == "_" {
                    self.emit(
                        codes::UNSUPPORTED_DESTRUCTURING_FORM,
                        "unsupported destructuring form",
                        span,
                    )?;
                    NameMarker::Error(span)
                } else {
                    NameMarker::Present(span)
                }
            } else if self.is_declaration_trailing_poison(current) {
                let first = current.span();
                let end = self.recover_destructuring_region(initializer_stops)?;
                NameMarker::Error(self.span(first.start(), end.max(first.end()))?)
            } else {
                let first = current.span();
                self.emit(
                    codes::UNSUPPORTED_DESTRUCTURING_FORM,
                    "unsupported destructuring form",
                    first,
                )?;
                let end = self.recover_destructuring_region(initializer_stops)?;
                NameMarker::Error(self.span(first.start(), end.max(first.end()))?)
            };
            bindings.push(binding);
            comma_already_reported_as_empty = false;
            needs_binding = false;

            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                if self.current_is_symbol(Symbol::RightParen) {
                    self.emit(
                        codes::UNSUPPORTED_DESTRUCTURING_TRAILING_COMMA,
                        "unsupported destructuring trailing comma",
                        comma,
                    )?;
                    break;
                }
                needs_binding = true;
                continue;
            }
            let current = self.current()?;
            if self.current_is_symbol(Symbol::RightParen)
                || self.current_is_symbol(Symbol::Equal)
                || initializer_stops.contains(current)
                || matches!(current.kind(), LexemeKind::Eof)
            {
                break;
            }
            if self.current_is_identifier() {
                let missing = self.empty_at(current.span().start())?;
                self.emit(
                    codes::EXPECTED_DESTRUCTURING_SEPARATOR,
                    "expected destructuring separator",
                    missing,
                )?;
                needs_binding = true;
                continue;
            }
            if self.current_is_symbol(Symbol::Colon) || self.current_is_symbol(Symbol::LeftParen) {
                self.emit(
                    codes::UNSUPPORTED_DESTRUCTURING_FORM,
                    "unsupported destructuring form",
                    current.span(),
                )?;
                self.recover_destructuring_region(initializer_stops)?;
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    needs_binding = true;
                }
                continue;
            }
            if !self.is_declaration_trailing_poison(current) {
                self.emit(
                    codes::EXPECTED_DESTRUCTURING_SEPARATOR,
                    "expected destructuring separator",
                    current.span(),
                )?;
            }
            self.recover_destructuring_region(initializer_stops)?;
            if self.current_is_symbol(Symbol::Comma) {
                self.bump()?;
                needs_binding = true;
            }
        }

        let right_paren_span = if self.current_is_symbol(Symbol::RightParen) {
            Some(self.bump()?.span())
        } else {
            let current = self.current()?;
            let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                && self.lexical_recoveries.terminal_error_at_eof;
            if !terminal_lexer_root && !self.is_poison() {
                self.emit_closing(self.empty_at(current.span().start())?, left_paren_span)?;
            }
            None
        };

        if self.current_is_symbol(Symbol::Colon) {
            let colon = self.current()?.span();
            self.emit(
                codes::UNSUPPORTED_DESTRUCTURING_FORM,
                "unsupported destructuring form",
                colon,
            )?;
            self.recover_destructuring_region(initializer_stops)?;
        }

        let suppress_missing_separator =
            right_paren_span.is_none() && !self.current_is_symbol(Symbol::Equal);
        let (equals_span, initializer) =
            self.parse_destructuring_initializer(initializer_stops, suppress_missing_separator)?;
        let initializer_span = self.expression_span(initializer)?;
        let end = if initializer_span.is_empty() {
            self.previous_significant_end().max(val_span.end())
        } else {
            initializer_span.end()
        };
        self.add_statement(
            self.span(val_span.start(), end)?,
            Statement::LocalDestructuring {
                val_span,
                left_paren_span,
                bindings,
                right_paren_span,
                equals_span,
                initializer,
            },
        )
    }

    fn recover_destructuring_region(
        &mut self,
        outer_stops: Stops,
    ) -> Result<usize, ParserInternalError> {
        self.recover_declaration_region(
            DeclarationStops::from_expression_hard(outer_stops)
                .with(DeclarationStops::COMMA)
                .with(DeclarationStops::RIGHT_PAREN)
                .with(DeclarationStops::EQUAL),
        )
    }

    fn parse_destructuring_initializer(
        &mut self,
        stops: Stops,
        suppress_missing_separator: bool,
    ) -> Result<(Option<Span>, ExpressionId), ParserInternalError> {
        if self.current_is_symbol(Symbol::Equal) {
            let equals = self.bump()?.span();
            let current = self.current()?;
            if stops.contains(current) || matches!(current.kind(), LexemeKind::Eof) {
                let empty = self.empty_at(current.span().start())?;
                let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                    && self.lexical_recoveries.terminal_error_at_eof;
                if !terminal_lexer_root && !self.is_poison() {
                    self.emit(
                        codes::EXPECTED_DESTRUCTURING_INITIALIZER,
                        "expected destructuring initializer",
                        empty,
                    )?;
                }
                return Ok((Some(equals), self.add_expression(empty, Expression::Error)?));
            }
            if self.can_start_expression(current) {
                let initializer = self.parse_expression_bp(0, stops)?;
                let initializer = self.consume_expression_tail(initializer, stops)?;
                return Ok((Some(equals), initializer));
            }
            if self.is_poison() {
                let span = self.bump()?.span();
                return Ok((Some(equals), self.add_expression(span, Expression::Error)?));
            }
            self.emit(
                codes::EXPECTED_DESTRUCTURING_INITIALIZER,
                "expected destructuring initializer",
                current.span(),
            )?;
            let start = current.span().start();
            let end = self.recover_declaration_region(
                DeclarationStops::from_expression_hard(stops).with(DeclarationStops::BLOCK_ELEMENT),
            )?;
            let span = self.span(start, end.max(current.span().end()))?;
            return Ok((Some(equals), self.add_expression(span, Expression::Error)?));
        }

        let current = self.current()?;
        let boundary = stops.contains(current) || matches!(current.kind(), LexemeKind::Eof);
        let primary = if boundary || self.can_start_expression(current) {
            self.empty_at(current.span().start())?
        } else {
            current.span()
        };
        let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
            && self.lexical_recoveries.terminal_error_at_eof;
        if !suppress_missing_separator && !self.is_poison() && !terminal_lexer_root {
            self.emit(
                codes::EXPECTED_DESTRUCTURING_INITIALIZER_SEPARATOR,
                "expected destructuring initializer separator",
                primary,
            )?;
        }
        if self.can_start_expression(current) {
            let initializer = self.parse_expression_bp(0, stops)?;
            let initializer = self.consume_expression_tail(initializer, stops)?;
            return Ok((None, initializer));
        }
        if boundary {
            return Ok((None, self.add_expression(primary, Expression::Error)?));
        }
        if self.is_poison() {
            let span = self.bump()?.span();
            return Ok((None, self.add_expression(span, Expression::Error)?));
        }
        let start = current.span().start();
        let end = self.recover_declaration_region(
            DeclarationStops::from_expression_hard(stops).with(DeclarationStops::BLOCK_ELEMENT),
        )?;
        let span = self.span(start, end.max(current.span().end()))?;
        Ok((None, self.add_expression(span, Expression::Error)?))
    }

    fn parse_required_block_initializer(
        &mut self,
        stops: Stops,
    ) -> Result<(Span, ExpressionId), ParserInternalError> {
        if self.current_is_symbol(Symbol::Equal) {
            let equals = self.bump()?.span();
            let initializer = self.parse_expression_bp(0, stops)?;
            let initializer = if self.lambda_initializer_line_boundary(initializer)? {
                initializer
            } else {
                self.consume_expression_tail(initializer, stops)?
            };
            return Ok((equals, initializer));
        }

        let current = self.current()?;
        let boundary = stops.contains(current);
        let primary = if boundary {
            self.empty_at(current.span().start())?
        } else {
            current.span()
        };
        let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
            && self.lexical_recoveries.terminal_error_at_eof;
        if !self.is_poison_kind(current.kind()) && !terminal_lexer_root {
            self.emit(codes::EXPECTED_INITIALIZER, "expected initializer", primary)?;
        }
        let equals = self.empty_at(current.span().start())?;
        if self.can_start_expression(current) {
            let initializer = self.parse_expression_bp(0, stops)?;
            let initializer = self.consume_expression_tail(initializer, stops)?;
            return Ok((equals, initializer));
        }
        if boundary {
            return Ok((equals, self.add_expression(primary, Expression::Error)?));
        }
        let span = self.bump()?.span();
        Ok((equals, self.add_expression(span, Expression::Error)?))
    }

    fn parse_constant_declaration(&mut self) -> Result<ItemId, ParserInternalError> {
        let const_span = self.bump()?.span();
        let val_marker = if self.current_is_keyword(Keyword::Val) {
            NameMarker::Present(self.bump()?.span())
        } else {
            let current = self.current()?;
            let primary = if matches!(current.kind(), LexemeKind::Eof)
                || self.is_file_declaration_boundary(current)
            {
                self.empty_at(current.span().start())?
            } else {
                current.span()
            };
            if !self.is_poison_kind(current.kind()) {
                self.emit(
                    codes::EXPECTED_VAL_AFTER_CONST,
                    "expected 'val' after 'const'",
                    primary,
                )?;
            }
            if self.file_mode && self.is_file_declaration_boundary(current) {
                NameMarker::Missing(self.empty_at(current.span().start())?)
            } else if self.current_is_keyword(Keyword::Var) {
                NameMarker::Error(self.bump()?.span())
            } else if self.current_is_identifier()
                || self.current_is_symbol(Symbol::Colon)
                || self.current_is_symbol(Symbol::Equal)
                || matches!(current.kind(), LexemeKind::Eof)
            {
                NameMarker::Missing(self.empty_at(current.span().start())?)
            } else {
                NameMarker::Error(self.bump()?.span())
            }
        };
        let name = self.parse_name_marker(
            codes::EXPECTED_DECLARATION_NAME,
            "expected declaration name",
            NameContext::Declaration,
        )?;
        let (colon_span, type_ref) = self.parse_optional_type_annotation()?;
        let (equals_span, initializer) = self.parse_required_initializer()?;
        let end = self
            .expression_span(initializer)?
            .end()
            .max(const_span.end());
        self.add_item(
            self.span(const_span.start(), end)?,
            Item::Constant {
                const_span,
                val_marker,
                name,
                colon_span,
                type_ref,
                equals_span,
                initializer,
            },
        )
    }

    fn parse_function_declaration(&mut self) -> Result<ItemId, ParserInternalError> {
        let fun_span = self.bump()?.span();
        let (type_parameters, type_parameter_list_span) = self.parse_type_parameters()?;
        let name = self.parse_name_marker(
            codes::EXPECTED_DECLARATION_NAME,
            "expected declaration name",
            NameContext::Declaration,
        )?;
        let parameters = self.parse_value_parameters()?;
        let parameter_end = self.previous_significant_end().max(fun_span.end());
        let (form, end) = self.parse_function_form(parameter_end)?;
        self.add_item(
            self.span(fun_span.start(), end)?,
            Item::Function {
                name,
                type_parameters,
                type_parameter_list_span,
                parameters,
                form,
            },
        )
    }

    fn parse_function_form(
        &mut self,
        parameter_end: usize,
    ) -> Result<(FunctionForm, usize), ParserInternalError> {
        if self.current_is_symbol(Symbol::Colon) {
            let colon_span = self.bump()?.span();
            let type_ref = self.parse_type_ref(
                self.root_type_stops()
                    .with(TypeStops::EQUAL)
                    .with(TypeStops::LEFT_BRACE),
            )?;
            return self.finish_explicit_function_form(colon_span, type_ref);
        }

        if self.current_is_symbol(Symbol::Equal) {
            let insertion = self.empty_at(self.current()?.span().start())?;
            self.emit(
                codes::EXPECTED_RETURN_TYPE,
                "expected explicit return type",
                insertion,
            )?;
            let type_ref = self.add_type_ref(insertion, TypeRef::Error)?;
            return self.finish_explicit_function_form(insertion, type_ref);
        }

        if self.current_is_symbol(Symbol::LeftBrace) {
            let block = self.parse_block_statement(Stops::ROOT)?;
            let end = self.statement_span(block)?.end();
            return Ok((FunctionForm::ImplicitUnitBlock(block), end));
        }

        let current = self.current()?;
        if self.can_start_type_ref(current) {
            let colon_span = self.empty_at(current.span().start())?;
            self.emit(
                codes::EXPECTED_RETURN_TYPE,
                "expected explicit return type",
                current.span(),
            )?;
            let type_ref = self.parse_type_ref(
                self.root_type_stops()
                    .with(TypeStops::EQUAL)
                    .with(TypeStops::LEFT_BRACE),
            )?;
            return self.finish_explicit_function_form(colon_span, type_ref);
        }

        Ok((FunctionForm::ImplicitUnitAbsent, parameter_end))
    }

    fn finish_explicit_function_form(
        &mut self,
        colon_span: Span,
        type_ref: TypeRefId,
    ) -> Result<(FunctionForm, usize), ParserInternalError> {
        let body = if self.current_is_symbol(Symbol::Equal) {
            let equals_span = self.bump()?.span();
            let expression = self.parse_expression_bp(0, self.root_expression_stops())?;
            FunctionBody::Expression {
                equals_span,
                expression,
            }
        } else if self.current_is_symbol(Symbol::LeftBrace) {
            FunctionBody::Block(self.parse_block_statement(Stops::ROOT)?)
        } else {
            FunctionBody::Absent
        };
        let end = match body {
            FunctionBody::Absent => self.previous_significant_end(),
            FunctionBody::Expression { expression, .. } => self.expression_span(expression)?.end(),
            FunctionBody::Block(statement) => self.statement_span(statement)?.end(),
        };
        Ok((
            FunctionForm::Explicit {
                colon_span,
                type_ref,
                body,
            },
            end,
        ))
    }

    fn parse_name_marker(
        &mut self,
        code: &str,
        message: &'static str,
        context: NameContext,
    ) -> Result<NameMarker, ParserInternalError> {
        let stops = if self.file_mode && matches!(context, NameContext::Declaration) {
            DeclarationStops::FILE
        } else {
            DeclarationStops::EMPTY
        };
        self.parse_name_marker_with_stops(code, message, context, stops)
    }

    fn parse_name_marker_with_stops(
        &mut self,
        code: &str,
        message: &'static str,
        context: NameContext,
        additional_stops: DeclarationStops,
    ) -> Result<NameMarker, ParserInternalError> {
        if self.current_is_identifier() {
            return Ok(NameMarker::Present(self.bump()?.span()));
        }
        let current = self.current()?;
        if self.is_poison() {
            return Ok(NameMarker::Error(self.bump()?.span()));
        }
        let symbol = match current.kind() {
            LexemeKind::Token(TokenKind::Symbol(symbol)) => Some(symbol),
            _ => None,
        };
        let boundary = matches!(current.kind(), LexemeKind::Eof)
            || context.is_stop(current)
            || additional_stops.contains_hard(current, symbol)
            || additional_stops.contains_soft(current, symbol);
        let primary = if boundary {
            self.empty_at(current.span().start())?
        } else {
            current.span()
        };
        self.emit(code, message, primary)?;
        if boundary {
            Ok(NameMarker::Missing(primary))
        } else {
            let start = current.span().start();
            let end =
                self.recover_declaration_region(context.recovery_stops().union(additional_stops))?;
            Ok(NameMarker::Error(self.span(start, end.max(start))?))
        }
    }

    fn parse_optional_type_annotation(
        &mut self,
    ) -> Result<(Option<Span>, Option<TypeRefId>), ParserInternalError> {
        if !self.current_is_symbol(Symbol::Colon) {
            return Ok((None, None));
        }
        let colon = self.bump()?.span();
        let type_ref = self.parse_type_ref(self.root_type_stops().with(TypeStops::EQUAL))?;
        Ok((Some(colon), Some(type_ref)))
    }

    fn parse_required_initializer(&mut self) -> Result<(Span, ExpressionId), ParserInternalError> {
        if self.current_is_symbol(Symbol::Equal) {
            let equals = self.bump()?.span();
            let initializer = self.parse_expression_bp(0, self.root_expression_stops())?;
            return Ok((equals, initializer));
        }
        let current = self.current()?;
        let file_boundary = self.is_file_declaration_boundary(current);
        let primary = if matches!(current.kind(), LexemeKind::Eof) || file_boundary {
            self.empty_at(current.span().start())?
        } else {
            current.span()
        };
        let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
            && self.lexical_recoveries.terminal_error_at_eof;
        if !self.is_poison_kind(current.kind()) && !terminal_lexer_root {
            self.emit(codes::EXPECTED_INITIALIZER, "expected initializer", primary)?;
        }
        let equals = self.empty_at(current.span().start())?;
        if self.can_start_expression(current) {
            let initializer = self.parse_expression_bp(0, self.root_expression_stops())?;
            return Ok((equals, initializer));
        }
        let error_span = if matches!(current.kind(), LexemeKind::Eof) || file_boundary {
            primary
        } else {
            let start = current.span().start();
            let end = self.recover_declaration_region(self.root_declaration_stops())?;
            self.span(start, end.max(start))?
        };
        let initializer = self.add_expression(error_span, Expression::Error)?;
        Ok((equals, initializer))
    }

    fn parse_type_parameters(
        &mut self,
    ) -> Result<(Vec<TypeParameter>, Option<Span>), ParserInternalError> {
        if !self.current_is_symbol(Symbol::Less) {
            return Ok((Vec::new(), None));
        }
        let opener = self.bump()?.span();
        let mut parameters = Vec::new();
        let stops = TypeStops::empty()
            .with(TypeStops::COMMA)
            .with(TypeStops::GREATER);
        if self.current_is_symbol(Symbol::Greater) {
            self.emit(
                codes::EXPECTED_LIST_ELEMENT,
                "expected list element",
                self.current()?.span(),
            )?;
        } else {
            loop {
                if self.current_is_symbol(Symbol::Comma) {
                    let comma = self.bump()?.span();
                    self.emit(codes::EXPECTED_LIST_ELEMENT, "expected list element", comma)?;
                    if self.current_is_symbol(Symbol::Greater) {
                        break;
                    }
                    continue;
                }
                let name = self.parse_name_marker(
                    codes::EXPECTED_PARAMETER_NAME,
                    "expected parameter name",
                    NameContext::TypeParameter,
                )?;
                let start = marker_span(name).start();
                let (colon_span, bound) = if self.current_is_symbol(Symbol::Colon) {
                    let colon = self.bump()?.span();
                    let bound = self.parse_type_ref(stops)?;
                    (Some(colon), Some(bound))
                } else {
                    (None, None)
                };
                let end = bound
                    .map(|id| self.type_span(id).map(Span::end))
                    .transpose()?
                    .unwrap_or(marker_span(name).end());
                parameters.push(TypeParameter {
                    span: self.span(start, end)?,
                    name,
                    colon_span,
                    bound,
                });
                if self.current_is_symbol(Symbol::Comma) {
                    let comma = self.bump()?.span();
                    if self.current_is_symbol(Symbol::Greater) {
                        self.emit(
                            codes::UNSUPPORTED_TRAILING_COMMA,
                            "unsupported trailing comma",
                            comma,
                        )?;
                        break;
                    }
                    continue;
                }
                if self.current_is_symbol(Symbol::Greater) {
                    break;
                }
                if self.current_is_identifier()
                    && matches!(
                        self.peek(1).map(Lexeme::kind),
                        Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)))
                    )
                {
                    let current = self.current()?;
                    self.emit_closing(self.empty_at(current.span().start())?, opener)?;
                    let end = parameters
                        .last()
                        .map(|parameter| parameter.span.end())
                        .unwrap_or(opener.end());
                    return Ok((parameters, Some(self.span(opener.start(), end)?)));
                }
                if self.current_is_identifier() {
                    self.emit(
                        codes::EXPECTED_LIST_SEPARATOR,
                        "expected list separator",
                        self.current()?.span(),
                    )?;
                    continue;
                }
                if matches!(self.current()?.kind(), LexemeKind::Eof) {
                    break;
                }
                if self.is_poison() {
                    self.bump()?;
                    if self.current_is_identifier() {
                        self.emit(
                            codes::EXPECTED_LIST_SEPARATOR,
                            "expected list separator",
                            self.current()?.span(),
                        )?;
                        continue;
                    }
                    if matches!(self.current()?.kind(), LexemeKind::Eof)
                        || self.current_is_symbol(Symbol::Greater)
                    {
                        break;
                    }
                }
                let primary = self.current()?.span();
                self.emit(
                    codes::EXPECTED_LIST_SEPARATOR,
                    "expected list separator",
                    primary,
                )?;
                self.recover_declaration_region(
                    DeclarationStops::EMPTY
                        .with(DeclarationStops::COMMA)
                        .with(DeclarationStops::GREATER),
                )?;
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                break;
            }
        }
        let end = if self.current_is_symbol(Symbol::Greater) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            parameters
                .last()
                .map(|parameter| parameter.span.end())
                .unwrap_or(opener.end())
        };
        Ok((parameters, Some(self.span(opener.start(), end)?)))
    }

    fn parse_parameter_mode_marker(
        &mut self,
    ) -> Result<Option<ParameterModeMarker>, ParserInternalError> {
        let marker = if self.current_is_keyword(Keyword::Borrow) {
            Some(ParameterModeMarker::Borrow(self.bump()?.span()))
        } else if self.current_is_keyword(Keyword::Inout) {
            Some(ParameterModeMarker::Inout(self.bump()?.span()))
        } else {
            None
        };
        if marker.is_none() {
            return Ok(None);
        }
        while self.current_is_keyword(Keyword::Borrow) || self.current_is_keyword(Keyword::Inout) {
            let duplicate = self.bump()?.span();
            self.emit(
                codes::DUPLICATE_PARAMETER_MODE,
                "duplicate parameter mode",
                duplicate,
            )?;
        }
        Ok(marker)
    }

    fn parse_value_parameters(&mut self) -> Result<Vec<ValueParameter>, ParserInternalError> {
        if !self.current_is_symbol(Symbol::LeftParen) {
            let current = self.current()?;
            let span = self.empty_at(current.span().start())?;
            self.emit_closing(span, span)?;
            return Ok(Vec::new());
        }
        let opener = self.bump()?.span();
        let mut parameters = Vec::new();
        let stops = TypeStops::empty()
            .with(TypeStops::COMMA)
            .with(TypeStops::RIGHT_PAREN);
        while !self.current_is_symbol(Symbol::RightParen)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                self.emit(codes::EXPECTED_LIST_ELEMENT, "expected list element", comma)?;
                continue;
            }
            let mode_marker = self.parse_parameter_mode_marker()?;
            let name = self.parse_name_marker(
                codes::EXPECTED_PARAMETER_NAME,
                "expected parameter name",
                NameContext::ValueParameter,
            )?;
            let start = mode_marker
                .map(parameter_mode_span)
                .unwrap_or_else(|| marker_span(name))
                .start();
            let mut last_consumed_end = mode_marker.map(|marker| {
                self.previous_significant_end()
                    .max(parameter_mode_span(marker).end())
            });
            let name_span = marker_span(name);
            if !name_span.is_empty() {
                last_consumed_end = Some(last_consumed_end.unwrap_or(0).max(name_span.end()));
            }
            let colon_span = if self.current_is_symbol(Symbol::Colon) {
                self.bump()?.span()
            } else {
                let current = self.current()?;
                let insertion = self.empty_at(current.span().start())?;
                let primary =
                    if self.can_start_type_ref(current) || self.current_is_symbol(Symbol::Equal) {
                        current.span()
                    } else if self.current_is_symbol(Symbol::Comma)
                        || self.current_is_symbol(Symbol::RightParen)
                        || matches!(current.kind(), LexemeKind::Eof)
                    {
                        insertion
                    } else {
                        current.span()
                    };
                if !self.is_poison_kind(current.kind()) {
                    self.emit(
                        codes::EXPECTED_PARAMETER_COLON,
                        "expected ':' after parameter name",
                        primary,
                    )?;
                }
                insertion
            };
            if !colon_span.is_empty() {
                last_consumed_end = Some(last_consumed_end.unwrap_or(0).max(colon_span.end()));
            }
            let missing_colon_default =
                self.current_is_symbol(Symbol::Equal) && colon_span.is_empty();
            let type_ref = if self.current_is_symbol(Symbol::Equal)
                || self.current_is_symbol(Symbol::Comma)
                || self.current_is_symbol(Symbol::RightParen)
                || matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                self.add_type_ref(
                    self.empty_at(self.current()?.span().start())?,
                    TypeRef::Error,
                )?
            } else if self.can_start_type_ref(self.current()?) {
                self.parse_type_ref(stops)?
            } else {
                let start = self.current()?.span().start();
                let end = self.recover_declaration_region(
                    DeclarationStops::EMPTY
                        .with(DeclarationStops::COMMA)
                        .with(DeclarationStops::RIGHT_PAREN)
                        .with(DeclarationStops::LEFT_BRACE),
                )?;
                self.add_type_ref(self.span(start, end.max(start))?, TypeRef::Error)?
            };
            let type_span = self.type_span(type_ref)?;
            if !type_span.is_empty() {
                last_consumed_end = Some(last_consumed_end.unwrap_or(0).max(type_span.end()));
            }
            let end = last_consumed_end.unwrap_or(type_span.end());
            parameters.push(ValueParameter {
                span: self.span(start, end)?,
                mode_marker,
                name,
                colon_span,
                type_ref,
            });
            if self.current_is_symbol(Symbol::Equal) {
                let equals = self.current()?.span();
                let end = self.recover_declaration_region(
                    DeclarationStops::EMPTY
                        .with(DeclarationStops::COMMA)
                        .with(DeclarationStops::RIGHT_PAREN),
                )?;
                if !missing_colon_default {
                    self.emit(
                        codes::UNSUPPORTED_PARAMETER_DEFAULT,
                        "unsupported parameter default",
                        self.span(equals.start(), end.max(equals.end()))?,
                    )?;
                }
            }
            if self.current_is_symbol(Symbol::Comma) {
                let comma = self.bump()?.span();
                if self.current_is_symbol(Symbol::RightParen) {
                    self.emit(
                        codes::UNSUPPORTED_TRAILING_COMMA,
                        "unsupported trailing comma",
                        comma,
                    )?;
                    break;
                }
                continue;
            }
            if self.current_is_identifier() {
                self.emit(
                    codes::EXPECTED_LIST_SEPARATOR,
                    "expected list separator",
                    self.current()?.span(),
                )?;
                continue;
            }
            if self.is_poison() {
                self.bump()?;
                if self.current_is_identifier() {
                    self.emit(
                        codes::EXPECTED_LIST_SEPARATOR,
                        "expected list separator",
                        self.current()?.span(),
                    )?;
                    continue;
                }
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                if self.current_is_symbol(Symbol::RightParen)
                    || matches!(self.current()?.kind(), LexemeKind::Eof)
                {
                    break;
                }
            }
            if !self.current_is_symbol(Symbol::RightParen)
                && !matches!(self.current()?.kind(), LexemeKind::Eof)
            {
                self.emit(
                    codes::EXPECTED_LIST_SEPARATOR,
                    "expected list separator",
                    self.current()?.span(),
                )?;
                self.recover_declaration_region(
                    DeclarationStops::EMPTY
                        .with(DeclarationStops::COMMA)
                        .with(DeclarationStops::RIGHT_PAREN),
                )?;
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
            }
            break;
        }
        if self.current_is_symbol(Symbol::RightParen) {
            self.bump()?;
        } else {
            let current = self.current()?;
            let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                && self.lexical_recoveries.terminal_error_at_eof;
            if !terminal_lexer_root {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
        }
        Ok(parameters)
    }

    /// 消费声明恢复区，保留调用方拥有的 hard closer，并只在局部 delimiter / owner
    /// 回到 baseline 后识别逗号 soft stop。
    fn recover_declaration_region(
        &mut self,
        stops: DeclarationStops,
    ) -> Result<usize, ParserInternalError> {
        let mut delimiters = Vec::new();
        let mut owners: Vec<(RecoveryOwner, usize)> = Vec::new();
        let first = self
            .significant(0)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        let recovery_start = first.1.span().start();
        while let Some(event) = self
            .lexical_recoveries
            .terminal_owner_events
            .get(self.next_terminal_recovery_event)
        {
            #[cfg(test)]
            {
                self.declaration_recovery_event_queries_and_applications += 1;
            }
            if event.offset > recovery_start {
                break;
            }
            self.next_terminal_recovery_event += 1;
        }
        let mut next_terminal_event = self.next_terminal_recovery_event;
        let mut end = recovery_start;
        let mut next_significant = Some(first);

        'recovery: loop {
            let (raw_index, current) = if let Some(first) = next_significant.take() {
                first
            } else {
                self.significant(0)
                    .ok_or(ParserInternalError::InvalidLexemeStream)?
            };
            #[cfg(test)]
            {
                self.declaration_recovery_raw_visits += raw_index - self.index + 1;
            }
            self.index = raw_index;

            while let Some(event) = {
                #[cfg(test)]
                {
                    self.declaration_recovery_event_queries_and_applications += 1;
                }
                self.lexical_recoveries
                    .terminal_owner_events
                    .get(next_terminal_event)
                    .copied()
            } {
                if event.offset > current.span().start() {
                    break;
                }
                #[cfg(test)]
                {
                    self.declaration_recovery_event_queries_and_applications += 1;
                }
                let Some((owner, delimiter_baseline)) = owners.last().copied() else {
                    // 恢复可以从一个由调用方拥有的 string / interpolation
                    // 内部开始。其 terminal event 是该调用方的 hard boundary，
                    // 不是当前错误区域缺少局部 owner。
                    break 'recovery;
                };
                if owner.kind() != event.kind || owner.opener() != event.opener {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                owners.pop();
                delimiters.truncate(delimiter_baseline);
                next_terminal_event += 1;
            }
            if matches!(current.kind(), LexemeKind::Eof) {
                break;
            }

            let symbol = match current.kind() {
                LexemeKind::Token(TokenKind::Symbol(symbol)) => Some(symbol),
                _ => None,
            };
            if let Some(expected) = delimiters.last().copied()
                && symbol == Some(expected)
            {
                self.index = raw_index + 1;
                end = current.span().end();
                delimiters.pop();
                continue;
            }
            let outside_owner = owners.is_empty();
            let hard_stop = outside_owner && stops.contains_hard(current, symbol);
            if hard_stop
                || (outside_owner && delimiters.is_empty() && stops.contains_soft(current, symbol))
            {
                break;
            }

            match current.kind() {
                LexemeKind::Token(TokenKind::StringStart) => {
                    let opener = current.span();
                    owners.push((
                        RecoveryOwner::String {
                            opener: opener.start(),
                        },
                        delimiters.len(),
                    ))
                }
                LexemeKind::Token(TokenKind::StringEnd) => {
                    let Some((RecoveryOwner::String { .. }, delimiter_baseline)) = owners.pop()
                    else {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    };
                    delimiters.truncate(delimiter_baseline);
                }
                LexemeKind::Token(TokenKind::InterpolationStart) => owners.push((
                    RecoveryOwner::Interpolation {
                        opener: current.span().start(),
                    },
                    delimiters.len(),
                )),
                LexemeKind::Token(TokenKind::InterpolationEnd) => {
                    let Some((RecoveryOwner::Interpolation { .. }, delimiter_baseline)) =
                        owners.pop()
                    else {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    };
                    delimiters.truncate(delimiter_baseline);
                }
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => {
                    delimiters.push(Symbol::RightParen)
                }
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBracket)) => {
                    delimiters.push(Symbol::RightBracket)
                }
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) => {
                    delimiters.push(Symbol::RightBrace)
                }
                _ => {}
            }
            self.index = raw_index + 1;
            end = current.span().end();
        }
        self.next_terminal_recovery_event = next_terminal_event;
        Ok(end)
    }

    fn consume_declaration_tail(&mut self, item: ItemId) -> Result<ItemId, ParserInternalError> {
        if matches!(self.current()?.kind(), LexemeKind::Eof) {
            return Ok(item);
        }
        let current = self.current()?;
        let implicit_function = matches!(
            self.ast.items().get(item)?.payload(),
            Item::Function {
                form: FunctionForm::ImplicitUnitAbsent,
                ..
            }
        );
        if !implicit_function || !self.is_declaration_trailing_poison(current) {
            self.emit(
                codes::UNEXPECTED_TRAILING_TOKEN,
                "unexpected trailing token",
                current.span(),
            )?;
        }
        self.recover_declaration_region(DeclarationStops::EMPTY)?;
        Ok(item)
    }

    fn parse_expression_bp(
        &mut self,
        minimum_precedence: u8,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_expression_bp_inner(minimum_precedence, stops);
        self.recursion_depth -= 1;
        result
    }

    fn parse_expression_bp_inner(
        &mut self,
        minimum_precedence: u8,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let mut left = self.parse_prefix(stops)?;
        let mut seen_non_associative = [false; 4];

        loop {
            let current = self.current()?;
            if stops.contains(current) {
                break;
            }
            if stops.when_entry_body
                && matches!(
                    current.kind(),
                    LexemeKind::Token(TokenKind::Keyword(Keyword::Is | Keyword::In))
                        | LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs | Symbol::BangIn))
                )
                && self
                    .gap_has_line_break(self.expression_span(left)?.end(), current.span().start())?
            {
                break;
            }

            if let Some(combination) = self.unsupported_operator()? {
                let left_span = self.expression_span(left)?;
                self.emit(
                    codes::UNSUPPORTED_OPERATOR,
                    "unsupported operator",
                    combination,
                )?;
                self.bump()?;
                self.bump()?;
                let end = if self.can_start_expression(self.current()?) {
                    // 只消费当前组合紧邻的一个 prefix/postfix operand；外层 Pratt loop 继续
                    // 处理后续组合，避免平坦错误序列被实现成递归链。
                    let right = self.parse_prefix(stops)?;
                    self.expression_span(right)?.end()
                } else {
                    combination.end()
                };
                left =
                    self.add_expression(self.span(left_span.start(), end)?, Expression::Error)?;
                continue;
            }

            let Some(rule) = self.infix_rule(current)? else {
                break;
            };
            if rule.precedence < minimum_precedence {
                break;
            }

            if let Some(group) = rule.non_associative_group {
                let slot = group as usize;
                if seen_non_associative[slot] {
                    let operator = self.bump()?;
                    self.emit(
                        codes::NON_ASSOCIATIVE_CHAIN,
                        "non-associative operator chain",
                        operator.span(),
                    )?;
                    let right = self.consume_rule_right(rule, stops)?;
                    let right_end = match right {
                        ParsedRight::Expression(id) => self.expression_span(id)?.end(),
                        ParsedRight::Type(id) => self.type_span(id)?.end(),
                    };
                    let error_span = self.span(operator.span().start(), right_end)?;
                    self.add_expression(error_span, Expression::Error)?;
                    let left_span = self.expression_span(left)?;
                    left = self.add_expression(
                        self.span(left_span.start(), right_end)?,
                        Expression::Error,
                    )?;
                    continue;
                }
                seen_non_associative[slot] = true;
            }

            let operator = self.bump()?;
            left = self.build_infix(left, operator.span(), rule, stops)?;
        }
        Ok(left)
    }

    fn parse_prefix(&mut self, stops: Stops) -> Result<ExpressionId, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_prefix_inner(stops);
        self.recursion_depth -= 1;
        result
    }

    fn parse_prefix_inner(&mut self, stops: Stops) -> Result<ExpressionId, ParserInternalError> {
        if let Some(combination) = self.unsupported_operator()? {
            self.emit(
                codes::UNSUPPORTED_OPERATOR,
                "unsupported operator",
                combination,
            )?;
            self.bump()?;
            self.bump()?;
            let end = if self.can_start_expression(self.current()?) {
                let operand = self.parse_expression_bp(PREC_PREFIX, stops)?;
                self.expression_span(operand)?.end()
            } else {
                combination.end()
            };
            return self.add_expression(self.span(combination.start(), end)?, Expression::Error);
        }

        let current = self.current()?;
        let operator = match current.kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::Bang)) => Some(PrefixOperator::Not),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)) => Some(PrefixOperator::Plus),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)) => Some(PrefixOperator::Minus),
            _ => None,
        };
        if let Some(operator) = operator {
            let operator_span = self.bump()?.span();
            let operand = self.parse_expression_bp(PREC_PREFIX, stops)?;
            let operand_span = self.expression_span(operand)?;
            return self.add_expression(
                self.span(operator_span.start(), operand_span.end())?,
                Expression::Prefix {
                    operator,
                    operator_span,
                    operand,
                },
            );
        }

        let primary = self.parse_primary(stops)?;
        self.parse_postfix(primary, stops)
    }

    fn parse_primary(&mut self, stops: Stops) -> Result<ExpressionId, ParserInternalError> {
        let current = self.current()?;
        if matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
        ) {
            return self.parse_lambda(None, stops);
        }
        if matches!(
            current.kind(),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Move))
        ) && self.peek(1).is_some_and(|next| {
            matches!(
                next.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
            )
        }) {
            let move_span = self.bump()?.span();
            return self.parse_lambda(Some(move_span), stops);
        }
        if stops.contains(current) && !control_expression_start_kind(current.kind()) {
            let span = self.empty_at(current.span().start())?;
            self.emit(codes::EXPECTED_EXPRESSION, "expected expression", span)?;
            return self.add_expression(span, Expression::Error);
        }

        match current.kind() {
            LexemeKind::Token(TokenKind::Keyword(Keyword::If)) => self.parse_if(stops),
            LexemeKind::Token(TokenKind::Keyword(Keyword::When)) => self.parse_when(stops),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Return)) => self.parse_return(stops),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Break)) => {
                let keyword_span = self.bump()?.span();
                self.add_expression(keyword_span, Expression::Break { keyword_span })
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::Continue)) => {
                let keyword_span = self.bump()?.span();
                self.add_expression(keyword_span, Expression::Continue { keyword_span })
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::Super)) => self.parse_super(stops),
            LexemeKind::Token(TokenKind::Identifier) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Name)
            }
            LexemeKind::Token(TokenKind::IntegerLiteral) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Literal(LiteralKind::Integer))
            }
            LexemeKind::Token(TokenKind::FloatLiteral) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Literal(LiteralKind::Float))
            }
            LexemeKind::Token(TokenKind::CharLiteral) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Literal(LiteralKind::Char))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::True)) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Literal(LiteralKind::Boolean(true)))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::False)) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Literal(LiteralKind::Boolean(false)))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::Null)) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Literal(LiteralKind::Null))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::This)) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::This)
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => self.parse_group(stops),
            LexemeKind::Token(TokenKind::Symbol(Symbol::ColonColon)) => {
                self.parse_unbound_reference(stops)
            }
            LexemeKind::Token(TokenKind::StringStart) => self.parse_string(),
            LexemeKind::Invalid(_) | LexemeKind::Token(TokenKind::ReservedWord(_)) => {
                let span = self.bump()?.span();
                self.add_expression(span, Expression::Error)
            }
            _ => {
                let span = self.bump()?.span();
                self.emit(codes::EXPECTED_EXPRESSION, "expected expression", span)?;
                self.add_expression(span, Expression::Error)
            }
        }
    }

    fn parse_if(&mut self, outer_stops: Stops) -> Result<ExpressionId, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let condition = self.parse_parenthesized_condition(keyword_span, outer_stops)?;
        let then_branch = self.parse_control_body(outer_stops.with(Stops::ELSE))?;
        let (else_span, else_branch) = if self.current_is_keyword(Keyword::Else) {
            let else_span = self.bump()?.span();
            let branch = self.parse_control_body(outer_stops)?;
            (Some(else_span), Some(branch))
        } else {
            (None, None)
        };
        let end = else_branch
            .map(|branch| self.statement_span(branch).map(Span::end))
            .transpose()?
            .unwrap_or(self.statement_span(then_branch)?.end());
        self.add_expression(
            self.span(keyword_span.start(), end)?,
            Expression::If {
                keyword_span,
                condition,
                then_branch,
                else_span,
                else_branch,
            },
        )
    }

    fn parse_parenthesized_condition(
        &mut self,
        keyword_span: Span,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        if !self.current_is_symbol(Symbol::LeftParen) {
            let current = self.current()?;
            let span = self.boundary_span(current, outer_stops)?;
            self.emit(codes::EXPECTED_CONDITION, "expected condition", span)?;
            return self.add_expression(self.empty_at(keyword_span.end())?, Expression::Error);
        }
        let opener = self.bump()?.span();
        let condition = if self.current_is_symbol(Symbol::RightParen) {
            let empty = self.empty_at(self.current()?.span().start())?;
            self.emit(codes::EXPECTED_CONDITION, "expected condition", empty)?;
            self.add_expression(empty, Expression::Error)?
        } else {
            self.parse_expression_bp(
                0,
                outer_stops
                    .without_lambda_body_soft_stops()
                    .with(Stops::RIGHT_PAREN),
            )?
        };
        if self.current_is_symbol(Symbol::RightParen) {
            self.bump()?;
        } else if !self.is_poison() {
            self.emit_closing(self.boundary_span(self.current()?, outer_stops)?, opener)?;
        }
        Ok(condition)
    }

    fn parse_control_body(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        if self.current_is_symbol(Symbol::LeftBrace) {
            return self.parse_control_block(outer_stops);
        }
        let current = self.current()?;
        if self.can_start_expression(current) {
            let expression = self.parse_expression_bp(0, outer_stops)?;
            let span = self.expression_span(expression)?;
            return self.add_statement(span, Statement::Expression { expression });
        }
        let span = self.boundary_span(current, outer_stops)?;
        if !self.is_poison() {
            self.emit(codes::EXPECTED_CONTROL_BODY, "expected control body", span)?;
        }
        self.add_statement(span, Statement::Error)
    }

    fn parse_control_block(
        &mut self,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let opener = self.bump()?.span();
        let mut elements = Vec::new();
        while !self.current_is_symbol(Symbol::RightBrace)
            && !outer_stops.contains_hard(self.current()?)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            let before = self.index;
            elements.push(self.parse_block_element(outer_stops)?);
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
        }
        let end = if self.current_is_symbol(Symbol::RightBrace) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !self.lexical_recoveries.terminal_error_at_eof && !self.is_poison() {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
            self.previous_significant_end().max(opener.end())
        };
        self.add_statement(
            self.span(opener.start(), end)?,
            Statement::ControlBody { elements },
        )
    }

    fn parse_when(&mut self, outer_stops: Stops) -> Result<ExpressionId, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let subject = if self.current_is_symbol(Symbol::LeftParen) {
            let opener = self.bump()?.span();
            let subject = if self.current_is_symbol(Symbol::RightParen) {
                let empty = self.empty_at(self.current()?.span().start())?;
                self.emit(codes::EXPECTED_CONDITION, "expected condition", empty)?;
                self.add_expression(empty, Expression::Error)?
            } else {
                self.parse_expression_bp(0, Stops::ROOT.with(Stops::RIGHT_PAREN))?
            };
            if self.current_is_symbol(Symbol::RightParen) {
                self.bump()?;
            } else if !self.is_poison() {
                self.emit_closing(self.boundary_span(self.current()?, outer_stops)?, opener)?;
            }
            Some(subject)
        } else {
            None
        };
        if !self.current_is_symbol(Symbol::LeftBrace) {
            let current = self.current()?;
            let span = self.boundary_span(current, outer_stops)?;
            self.emit(codes::EXPECTED_CONTROL_BODY, "expected control body", span)?;
            return self.add_expression(
                self.span(keyword_span.start(), self.previous_significant_end())?,
                Expression::When {
                    keyword_span,
                    subject,
                    entries: Vec::new(),
                },
            );
        }
        let opener = self.bump()?.span();
        let mut entries = Vec::new();
        while !self.current_is_symbol(Symbol::RightBrace)
            && !outer_stops.contains_hard(self.current()?)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            let before = self.index;
            entries.push(self.parse_when_entry(outer_stops)?);
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            if self.current_is_symbol(Symbol::RightBrace) {
                break;
            }
            if self.current_is_symbol(Symbol::Semicolon) {
                self.bump()?;
            } else if !self.gap_has_line_break(
                self.previous_significant_end(),
                self.current()?.span().start(),
            )? {
                self.emit(
                    codes::EXPECTED_WHEN_ENTRY_SEPARATOR,
                    "expected when entry separator",
                    self.current()?.span(),
                )?;
            }
        }
        let end = if self.current_is_symbol(Symbol::RightBrace) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !self.lexical_recoveries.terminal_error_at_eof && !self.is_poison() {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
            self.previous_significant_end().max(opener.end())
        };
        self.add_expression(
            self.span(keyword_span.start(), end)?,
            Expression::When {
                keyword_span,
                subject,
                entries,
            },
        )
    }

    fn parse_when_entry(&mut self, outer_stops: Stops) -> Result<WhenEntry, ParserInternalError> {
        let start = self.current()?.span().start();
        let (conditions, else_span) = if self.current_is_keyword(Keyword::Else) {
            (Vec::new(), Some(self.bump()?.span()))
        } else {
            let mut conditions = Vec::new();
            loop {
                conditions.push(self.parse_when_condition(outer_stops)?);
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                break;
            }
            (conditions, None)
        };
        let arrow_span = if self.current_is_symbol(Symbol::Arrow) {
            self.bump()?.span()
        } else {
            let span = self.boundary_span(self.current()?, outer_stops.with(Stops::RIGHT_BRACE))?;
            self.emit(codes::EXPECTED_WHEN_ARROW, "expected when arrow", span)?;
            self.empty_at(self.previous_significant_end())?
        };
        let body =
            self.parse_control_body(outer_stops.with(Stops::RIGHT_BRACE).as_when_entry_body())?;
        let end = self.statement_span(body)?.end().max(arrow_span.end());
        Ok(WhenEntry {
            span: self.span(start, end)?,
            conditions,
            else_span,
            arrow_span,
            body,
        })
    }

    fn parse_when_condition(
        &mut self,
        outer_stops: Stops,
    ) -> Result<WhenCondition, ParserInternalError> {
        let condition_stops = outer_stops
            .without_lambda_body_soft_stops()
            .with(Stops::COMMA)
            .with(Stops::ARROW)
            .with(Stops::RIGHT_BRACE);
        if self.current_is_keyword(Keyword::Is) || self.current_is_symbol(Symbol::BangIs) {
            let operator = self.bump()?;
            let negated = matches!(
                operator.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs))
            );
            let type_ref = self.parse_type_ref(TypeStops::from_expression(condition_stops))?;
            return Ok(WhenCondition::TypeTest {
                operator_span: operator.span(),
                negated,
                type_ref,
            });
        }
        if self.current_is_keyword(Keyword::In) || self.current_is_symbol(Symbol::BangIn) {
            let operator = self.bump()?;
            let negated = matches!(
                operator.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::BangIn))
            );
            let expression = if self.can_start_expression(self.current()?) {
                self.parse_expression_bp(0, condition_stops)?
            } else {
                let span = self.boundary_span(self.current()?, condition_stops)?;
                self.emit(codes::EXPECTED_CONDITION, "expected condition", span)?;
                self.add_expression(span, Expression::Error)?
            };
            return Ok(WhenCondition::Contains {
                operator_span: operator.span(),
                negated,
                expression,
            });
        }
        if !self.can_start_expression(self.current()?) {
            let span = self.boundary_span(self.current()?, condition_stops)?;
            self.emit(codes::EXPECTED_WHEN_ENTRY, "expected when entry", span)?;
            if !condition_stops.contains(self.current()?) {
                self.bump()?;
            }
            let expression = self.add_expression(span, Expression::Error)?;
            return Ok(WhenCondition::Expression(expression));
        }
        Ok(WhenCondition::Expression(
            self.parse_expression_bp(0, condition_stops)?,
        ))
    }

    fn parse_return(&mut self, stops: Stops) -> Result<ExpressionId, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let current = self.current()?;
        let value = if self.can_start_expression(current)
            && !self.gap_has_line_break(keyword_span.end(), current.span().start())?
            && !stops.contains(current)
        {
            Some(self.parse_expression_bp(0, stops)?)
        } else {
            None
        };
        let end = value
            .map(|value| self.expression_span(value).map(Span::end))
            .transpose()?
            .unwrap_or(keyword_span.end());
        self.add_expression(
            self.span(keyword_span.start(), end)?,
            Expression::Return {
                keyword_span,
                value,
            },
        )
    }

    fn parse_super(&mut self, outer_stops: Stops) -> Result<ExpressionId, ParserInternalError> {
        let keyword_span = self.bump()?.span();
        let interface = if self.current_is_symbol(Symbol::Less) {
            let opener = self.bump()?.span();
            let type_ref = self.parse_type_ref(TypeStops::empty().with(TypeStops::GREATER))?;
            if self.current_is_symbol(Symbol::Greater) {
                self.bump()?;
            } else if !self.is_poison() {
                self.emit_closing(
                    self.type_boundary_span(self.current()?, TypeStops::empty())?,
                    opener,
                )?;
            }
            type_ref
        } else {
            let span = self.boundary_span(self.current()?, outer_stops)?;
            self.emit(
                codes::EXPECTED_SUPER_INTERFACE,
                "expected super interface",
                span,
            )?;
            self.add_type_ref(span, TypeRef::Error)?
        };
        let dot_span = if self.current_is_symbol(Symbol::Dot) {
            self.bump()?.span()
        } else {
            let span = self.boundary_span(self.current()?, outer_stops)?;
            self.emit(
                codes::EXPECTED_SUPER_MEMBER_SEPARATOR,
                "expected super member separator",
                span,
            )?;
            self.empty_at(self.previous_significant_end())?
        };
        let name_span = if self.current_is_identifier() {
            self.bump()?.span()
        } else {
            let span = self.boundary_span(self.current()?, outer_stops)?;
            self.emit(
                codes::EXPECTED_MEMBER_NAME,
                "expected member or reference name",
                span,
            )?;
            self.empty_at(self.previous_significant_end())?
        };
        let end = name_span
            .end()
            .max(dot_span.end())
            .max(self.type_span(interface)?.end())
            .max(keyword_span.end());
        self.add_expression(
            self.span(keyword_span.start(), end)?,
            Expression::SuperMember {
                keyword_span,
                interface,
                dot_span,
                name_span,
            },
        )
    }

    fn parse_lambda(
        &mut self,
        move_span: Option<Span>,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_lambda_inner(move_span, outer_stops);
        self.recursion_depth -= 1;
        result
    }

    fn parse_lambda_inner(
        &mut self,
        move_span: Option<Span>,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let opener_raw = self.current_raw()?;
        let header = self.lambda_headers.query(opener_raw)?.clone();
        let opener = self.bump()?.span();
        let start = move_span.unwrap_or(opener).start();

        let (parameters, arrow_span) = match header {
            LambdaHeaderTrial::NoHeader => (Vec::new(), None),
            LambdaHeaderTrial::Header {
                parameter_raw,
                arrow_raw,
            } => {
                let mut parameters = Vec::with_capacity(parameter_raw.len());
                for (ordinal, expected_raw) in parameter_raw.iter().copied().enumerate() {
                    if self.current_raw()? != expected_raw || !self.current_is_identifier() {
                        return Err(ParserInternalError::InvalidLexemeStream);
                    }
                    parameters.push(self.bump()?.span());
                    if ordinal + 1 < parameter_raw.len() {
                        if !self.current_is_symbol(Symbol::Comma) {
                            return Err(ParserInternalError::InvalidLexemeStream);
                        }
                        self.bump()?;
                    }
                }
                if self.current_raw()? != arrow_raw || !self.current_is_symbol(Symbol::Arrow) {
                    return Err(ParserInternalError::InvalidLexemeStream);
                }
                let arrow = self.bump()?.span();
                (parameters, Some(arrow))
            }
        };

        let body = self.parse_lambda_body(opener, outer_stops)?;
        let end = self.statement_span(body)?.end();
        self.add_expression(
            self.span(start, end)?,
            Expression::Lambda {
                move_span,
                parameters,
                arrow_span,
                body,
            },
        )
    }

    fn parse_lambda_body(
        &mut self,
        opener: Span,
        outer_stops: Stops,
    ) -> Result<StatementId, ParserInternalError> {
        let mut elements = Vec::new();
        let expression_stops = Stops::lambda_expression(outer_stops);

        while !self.current_is_symbol(Symbol::RightBrace)
            && !outer_stops.contains_hard(self.current()?)
            && !matches!(self.current()?.kind(), LexemeKind::Eof)
        {
            #[cfg(test)]
            {
                self.lambda_body_dispatch_iterations += 1;
            }
            let before = self.index;
            let element = if self.current_is_symbol(Symbol::LeftBrace) {
                self.parse_block_statement(outer_stops)?
            } else if self.local_destructuring_start(Keyword::Val) {
                let val_span = self.bump()?.span();
                self.parse_local_destructuring(val_span, expression_stops)?
            } else if self.local_destructuring_start(Keyword::Var)
                || self.const_local_destructuring_start()
            {
                self.parse_unsupported_local_destructuring(expression_stops)?
            } else if self.current_is_keyword(Keyword::While)
                || self.current_is_keyword(Keyword::For)
                || self.current_is_keyword(Keyword::Loop)
            {
                self.parse_loop_statement(outer_stops)?
            } else if self.current_is_keyword(Keyword::Val) || self.current_is_keyword(Keyword::Var)
            {
                let keyword = self.bump()?;
                let kind = match keyword.kind() {
                    LexemeKind::Token(TokenKind::Keyword(Keyword::Val)) => VariableKind::Val,
                    LexemeKind::Token(TokenKind::Keyword(Keyword::Var)) => VariableKind::Var,
                    _ => return Err(ParserInternalError::InvalidLexemeStream),
                };
                let declaration =
                    self.parse_local_variable_declaration(keyword.span(), kind, expression_stops)?;
                let span = self.ast.items().get(declaration)?.span();
                self.add_statement(span, Statement::LocalVariable { declaration })?
            } else if self.is_unsupported_block_element() {
                self.parse_unsupported_lambda_body_form()?
            } else {
                let current = self.current()?;
                if self.is_poison_kind(current.kind()) {
                    let span = self.bump()?.span();
                    self.add_statement(span, Statement::Error)?
                } else if self.current_is_symbol(Symbol::Comma)
                    || self.current_is_symbol(Symbol::Arrow)
                {
                    self.parse_unsupported_lambda_body_form()?
                } else if self.can_start_expression(current) {
                    let expression = self.parse_expression_bp(0, expression_stops)?;
                    let expression = if self.control_expression_line_boundary(expression)? {
                        expression
                    } else {
                        self.consume_expression_tail(expression, expression_stops)?
                    };
                    let span = self.expression_span(expression)?;
                    self.add_statement(span, Statement::Expression { expression })?
                } else {
                    let span = self.bump()?.span();
                    self.emit(
                        codes::EXPECTED_LAMBDA_BODY_ELEMENT,
                        "expected lambda body element",
                        span,
                    )?;
                    self.add_statement(span, Statement::Error)?
                }
            };
            if self.index <= before {
                return Err(ParserInternalError::InvalidLexemeStream);
            }
            elements.push(element);
        }

        let end = if self.current_is_symbol(Symbol::RightBrace) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !self.lexical_recoveries.terminal_error_at_eof && !self.is_poison() {
                self.emit_closing(self.empty_at(current.span().start())?, opener)?;
            }
            self.previous_significant_end().max(opener.end())
        };
        self.add_statement(
            self.span(opener.start(), end)?,
            Statement::LambdaBody { elements },
        )
    }

    fn parse_unsupported_lambda_body_form(&mut self) -> Result<StatementId, ParserInternalError> {
        let first_lexeme = self.bump()?;
        let first = first_lexeme.span();
        let mut end = first.end();
        if self.current_is_keyword(Keyword::Val)
            && matches!(
                first_lexeme.kind(),
                LexemeKind::Token(TokenKind::Keyword(Keyword::Const))
            )
        {
            end = self.bump()?.span().end();
        }
        let span = self.span(first.start(), end)?;
        self.emit(
            codes::UNSUPPORTED_LAMBDA_BODY_FORM,
            "unsupported lambda body form",
            span,
        )?;
        self.add_statement(span, Statement::Error)
    }

    fn parse_group(&mut self, outer_stops: Stops) -> Result<ExpressionId, ParserInternalError> {
        let outer_stops = outer_stops.without_file_declaration_stop();
        let opener = self.bump()?.span();
        let mut inner = self.parse_expression_bp(
            0,
            outer_stops
                .without_lambda_body_soft_stops()
                .with(Stops::RIGHT_PAREN),
        )?;
        let current = self.current()?;
        if !self.current_is_symbol(Symbol::RightParen)
            && (matches!(
                current.kind(),
                LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon))
            ) || (self.file_mode && file_construct_start_kind(current.kind())))
        {
            self.emit(
                codes::UNEXPECTED_TRAILING_TOKEN,
                "unexpected trailing token",
                current.span(),
            )?;
            let start = self.expression_span(inner)?.start();
            let end = self.recover_declaration_region(
                DeclarationStops::from_expression_hard(outer_stops)
                    .with(DeclarationStops::RIGHT_PAREN),
            )?;
            inner = self.add_expression(self.span(start, end)?, Expression::Error)?;
        }
        let inner_span = self.expression_span(inner)?;
        let end = if self.current_is_symbol(Symbol::RightParen) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !self.is_poison() {
                self.emit_closing(self.boundary_span(current, outer_stops)?, opener)?;
            }
            inner_span.end().max(opener.end())
        };
        self.add_expression(
            self.span(opener.start(), end)?,
            Expression::Group { expression: inner },
        )
    }

    fn parse_unbound_reference(
        &mut self,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let operator_span = self.bump()?.span();
        if self.current_is_identifier() {
            let name_span = self.bump()?.span();
            return self.add_expression(
                self.span(operator_span.start(), name_span.end())?,
                Expression::CallableReference {
                    receiver: None,
                    operator_span,
                    name_span,
                },
            );
        }

        let current = self.current()?;
        let end = if self.is_poison() {
            self.bump()?.span().end()
        } else {
            let primary = self.boundary_span(current, stops)?;
            self.emit(
                codes::EXPECTED_MEMBER_NAME,
                "expected member or reference name",
                primary,
            )?;
            operator_span.end()
        };
        self.add_expression(self.span(operator_span.start(), end)?, Expression::Error)
    }

    fn parse_string(&mut self) -> Result<ExpressionId, ParserInternalError> {
        let opener = self.bump()?.span();
        let mut parts = Vec::new();
        let mut end = opener.end();
        loop {
            let current = self.current()?;
            match current.kind() {
                LexemeKind::Token(TokenKind::StringText) => {
                    let span = self.bump()?.span();
                    end = span.end();
                    parts.push(StringPart::Text(span));
                }
                LexemeKind::Token(TokenKind::InterpolationStart) => {
                    let interpolation_start = self.bump()?.span();
                    let expression =
                        self.parse_expression_bp(0, Stops::ROOT.with(Stops::INTERPOLATION_END))?;
                    let expression = self.consume_expression_tail(
                        expression,
                        Stops::ROOT.with(Stops::INTERPOLATION_END),
                    )?;
                    let expression_span = self.expression_span(expression)?;
                    let interpolation_end = if matches!(
                        self.current()?.kind(),
                        LexemeKind::Token(TokenKind::InterpolationEnd)
                    ) {
                        self.bump()?.span().end()
                    } else {
                        let boundary = self.current()?.span();
                        let lexer_owns_boundary = self
                            .lexical_recoveries
                            .unterminated_interpolation_starts
                            .binary_search(&interpolation_start.start())
                            .is_ok()
                            || (boundary.end() == self.lexical_recoveries.source_len
                                && self.lexical_recoveries.terminal_error_at_eof);
                        if !lexer_owns_boundary {
                            self.emit_closing(boundary, interpolation_start)?;
                        }
                        expression_span.end().max(interpolation_start.end())
                    };
                    let span = self.span(interpolation_start.start(), interpolation_end)?;
                    end = span.end();
                    parts.push(StringPart::Interpolation { span, expression });
                }
                LexemeKind::Token(TokenKind::StringEnd) => {
                    end = self.bump()?.span().end();
                    break;
                }
                LexemeKind::Invalid(_) => {
                    let span = self.bump()?.span();
                    end = span.end();
                    parts.push(StringPart::Error(span));
                    if self
                        .lexical_recoveries
                        .string_recovery_end(opener.start())
                        .is_some_and(|recovery_end| span.end() >= recovery_end)
                    {
                        break;
                    }
                }
                _ => {
                    let lexer_owns_boundary = self
                        .lexical_recoveries
                        .string_recovery_end(opener.start())
                        .is_some()
                        || (current.span().end() == self.lexical_recoveries.source_len
                            && self.lexical_recoveries.terminal_error_at_eof);
                    if !lexer_owns_boundary {
                        self.emit_closing(self.boundary_span(current, Stops::ROOT)?, opener)?;
                    }
                    break;
                }
            }
        }
        self.add_expression(
            self.span(opener.start(), end)?,
            Expression::String { parts },
        )
    }

    fn parse_postfix(
        &mut self,
        mut receiver: ExpressionId,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        loop {
            if matches!(
                self.ast.expressions().get(receiver)?.payload(),
                Expression::Error
            ) && self.is_postfix_start()
            {
                receiver = self.consume_error_postfix(receiver, stops)?;
                continue;
            }
            receiver = if self.current_is_symbol(Symbol::Dot)
                || self.current_is_symbol(Symbol::QuestionDot)
            {
                self.parse_member(receiver, stops)?
            } else if self.current_is_symbol(Symbol::LeftParen) {
                self.parse_call(receiver, Vec::new(), None, stops)?
            } else if self.current_is_symbol(Symbol::Less) {
                let checkpoint = self.index;
                if let Some((type_arguments, type_arguments_span)) =
                    self.try_parse_call_type_arguments()?
                {
                    self.parse_call(receiver, type_arguments, Some(type_arguments_span), stops)?
                } else {
                    self.index = checkpoint;
                    break;
                }
            } else if self.current_is_symbol(Symbol::LeftBracket) {
                self.parse_index(receiver, stops)?
            } else if matches!(
                self.peek(0).map(Lexeme::kind),
                Some(LexemeKind::Token(TokenKind::Symbol(
                    Symbol::BangBang | Symbol::Question
                )))
            ) {
                let operator = self.bump()?;
                let operator_span = operator.span();
                let receiver_span = self.expression_span(receiver)?;
                let payload = match operator.kind() {
                    LexemeKind::Token(TokenKind::Symbol(Symbol::BangBang)) => {
                        Expression::NonNullAssert {
                            operand: receiver,
                            operator_span,
                        }
                    }
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Question)) => {
                        Expression::Propagate {
                            value: receiver,
                            question_span: operator_span,
                        }
                    }
                    _ => return Err(ParserInternalError::InvalidLexemeStream),
                };
                self.add_expression(
                    self.span(receiver_span.start(), operator_span.end())?,
                    payload,
                )?
            } else if self.current_is_symbol(Symbol::ColonColon) {
                self.parse_bound_reference(receiver, stops)?
            } else {
                break;
            };
        }
        Ok(receiver)
    }

    fn is_postfix_start(&self) -> bool {
        matches!(
            self.peek(0).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Symbol(
                Symbol::Dot
                    | Symbol::QuestionDot
                    | Symbol::LeftParen
                    | Symbol::LeftBracket
                    | Symbol::BangBang
                    | Symbol::Question
                    | Symbol::ColonColon
            )))
        )
    }

    fn consume_error_postfix(
        &mut self,
        receiver: ExpressionId,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let receiver_span = self.expression_span(receiver)?;
        let first = self.bump()?;
        let mut end = first.span().end();
        match first.kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => {
                end = self.consume_balanced_suffix(
                    Symbol::LeftParen,
                    Symbol::RightParen,
                    end,
                    stops,
                )?;
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBracket)) => {
                end = self.consume_balanced_suffix(
                    Symbol::LeftBracket,
                    Symbol::RightBracket,
                    end,
                    stops,
                )?;
            }
            LexemeKind::Token(TokenKind::Symbol(
                Symbol::Dot | Symbol::QuestionDot | Symbol::ColonColon,
            )) if !matches!(self.current()?.kind(), LexemeKind::Eof) => {
                end = self.bump()?.span().end();
            }
            _ => {}
        }
        self.add_expression(self.span(receiver_span.start(), end)?, Expression::Error)
    }

    fn consume_balanced_suffix(
        &mut self,
        opener: Symbol,
        closer: Symbol,
        mut end: usize,
        stops: Stops,
    ) -> Result<usize, ParserInternalError> {
        let stops = stops.without_lambda_body_soft_stops();
        let mut depth = 1usize;
        while depth > 0 {
            let current = self.current()?;
            if matches!(current.kind(), LexemeKind::Eof) || stops.contains(current) {
                break;
            }
            let lexeme = self.bump()?;
            end = lexeme.span().end();
            match lexeme.kind() {
                LexemeKind::Token(TokenKind::Symbol(symbol)) if symbol == opener => depth += 1,
                LexemeKind::Token(TokenKind::Symbol(symbol)) if symbol == closer => depth -= 1,
                _ => {}
            }
        }
        Ok(end)
    }

    fn parse_member(
        &mut self,
        receiver: ExpressionId,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let operator = self.bump()?;
        let safe = matches!(
            operator.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::QuestionDot))
        );
        let operator_span = operator.span();
        if !self.current_is_identifier() {
            let current = self.current()?;
            let end = if self.is_poison() {
                self.bump()?.span().end()
            } else {
                let primary = self.boundary_span(current, stops)?;
                self.emit(
                    codes::EXPECTED_MEMBER_NAME,
                    "expected member or reference name",
                    primary,
                )?;
                operator_span.end()
            };
            let receiver_span = self.expression_span(receiver)?;
            return self.add_expression(self.span(receiver_span.start(), end)?, Expression::Error);
        }
        let name_span = self.bump()?.span();
        let receiver_span = self.expression_span(receiver)?;
        self.add_expression(
            self.span(receiver_span.start(), name_span.end())?,
            Expression::Member {
                receiver,
                operator_span,
                name_span,
                safe,
            },
        )
    }

    fn parse_bound_reference(
        &mut self,
        receiver: ExpressionId,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let operator_span = self.bump()?.span();
        if !self.current_is_identifier() {
            let current = self.current()?;
            let end = if self.is_poison() {
                self.bump()?.span().end()
            } else {
                let primary = self.boundary_span(current, stops)?;
                self.emit(
                    codes::EXPECTED_MEMBER_NAME,
                    "expected member or reference name",
                    primary,
                )?;
                operator_span.end()
            };
            let receiver_span = self.expression_span(receiver)?;
            return self.add_expression(self.span(receiver_span.start(), end)?, Expression::Error);
        }
        let name_span = self.bump()?.span();
        let receiver_span = self.expression_span(receiver)?;
        self.add_expression(
            self.span(receiver_span.start(), name_span.end())?,
            Expression::CallableReference {
                receiver: Some(receiver),
                operator_span,
                name_span,
            },
        )
    }

    fn parse_call(
        &mut self,
        callee: ExpressionId,
        type_arguments: Vec<TypeRefId>,
        type_arguments_span: Option<Span>,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let outer_stops = outer_stops.without_file_declaration_stop();
        let opener = self.bump()?.span();
        let callee_span = self.expression_span(callee)?;
        let argument_stops = outer_stops
            .without_lambda_body_soft_stops()
            .with(Stops::COMMA)
            .with(Stops::RIGHT_PAREN);
        let mut arguments = Vec::new();
        let mut last_consumed_end = opener.end();

        if !self.current_is_symbol(Symbol::RightParen) {
            loop {
                if self.current_is_symbol(Symbol::Comma) {
                    let comma = self.bump()?.span();
                    last_consumed_end = last_consumed_end.max(comma.end());
                    self.emit(
                        codes::UNSUPPORTED_ARGUMENT_EMPTY_ELEMENT,
                        "unsupported argument empty element",
                        comma,
                    )?;
                    let empty = self.empty_at(comma.start())?;
                    let value = self.add_expression(empty, Expression::Error)?;
                    arguments.push(CallArgument {
                        span: empty,
                        named_prefix: None,
                        mode_marker: None,
                        value,
                    });
                    if self.current_is_symbol(Symbol::RightParen) {
                        break;
                    }
                    continue;
                }

                let (argument, consumed_separator) =
                    self.parse_call_argument(argument_stops, outer_stops)?;
                if !argument.span.is_empty() {
                    last_consumed_end = argument.span.end().max(last_consumed_end);
                }
                arguments.push(argument);
                if consumed_separator {
                    last_consumed_end = self.previous_significant_end().max(last_consumed_end);
                    if self.current_is_symbol(Symbol::RightParen) {
                        break;
                    }
                    continue;
                }
                if self.current_is_symbol(Symbol::Comma) {
                    let comma = self.bump()?.span();
                    last_consumed_end = last_consumed_end.max(comma.end());
                    if self.current_is_symbol(Symbol::RightParen) {
                        let span = self.empty_at(self.current()?.span().start())?;
                        self.emit(
                            codes::UNSUPPORTED_ARGUMENT_TRAILING_COMMA,
                            "unsupported argument trailing comma",
                            comma,
                        )?;
                        let value = self.add_expression(span, Expression::Error)?;
                        arguments.push(CallArgument {
                            span,
                            named_prefix: None,
                            mode_marker: None,
                            value,
                        });
                        break;
                    }
                    continue;
                }
                let current = self.current()?;
                if self.call_argument_boundary(current, outer_stops) {
                    break;
                }
                if self.can_start_call_argument(current) {
                    self.emit(
                        codes::EXPECTED_ARGUMENT_SEPARATOR,
                        "expected argument separator",
                        self.empty_at(current.span().start())?,
                    )?;
                    continue;
                }
                let diagnostic_span = current.span();
                let recovery_end = self.recover_call_region(outer_stops)?;
                self.emit(
                    codes::EXPECTED_ARGUMENT_SEPARATOR,
                    "expected argument separator",
                    diagnostic_span,
                )?;
                last_consumed_end = recovery_end.max(last_consumed_end);
                if self.current_is_symbol(Symbol::Comma) {
                    last_consumed_end = self.bump()?.span().end().max(last_consumed_end);
                    if self.current_is_symbol(Symbol::RightParen) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        let end = if self.current_is_symbol(Symbol::RightParen) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !self.is_poison() {
                self.emit_closing(self.boundary_span(current, outer_stops)?, opener)?;
            }
            last_consumed_end
        };
        self.add_expression(
            self.span(callee_span.start(), end)?,
            Expression::Call {
                callee,
                type_arguments,
                type_arguments_span,
                arguments,
            },
        )
    }

    fn parse_call_argument(
        &mut self,
        argument_stops: Stops,
        outer_stops: Stops,
    ) -> Result<(CallArgument, bool), ParserInternalError> {
        let named_prefix = if self.current_is_identifier()
            && self.peek(1).is_some_and(|lexeme| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Equal))
                )
            }) {
            let name_span = self.bump()?.span();
            let equals_span = self.bump()?.span();
            Some(NamedArgumentPrefix {
                name_span,
                equals_span,
            })
        } else {
            None
        };
        let mut first_start = named_prefix.map(|prefix| prefix.name_span.start());
        let mut last_consumed_end = named_prefix.map(|prefix| prefix.equals_span.end());

        let mode_marker = self.parse_argument_mode_marker()?;
        if let Some(marker) = mode_marker {
            let span = parameter_mode_span(marker);
            first_start.get_or_insert(span.start());
            last_consumed_end = Some(self.previous_significant_end().max(span.end()));
        }

        if mode_marker.is_some()
            && self.current_is_identifier()
            && self.peek(1).is_some_and(|lexeme| {
                matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Equal))
                )
            })
        {
            self.bump()?;
            let equals = self.bump()?.span();
            self.emit(
                codes::INVALID_ARGUMENT_MODE_ORDERING,
                "invalid argument mode ordering",
                equals,
            )?;
            last_consumed_end = Some(equals.end());
        }

        let current = self.current()?;
        let (value, consumed_separator) = if self.call_argument_boundary(current, outer_stops) {
            let empty = self.empty_at(current.span().start())?;
            self.emit(
                codes::EXPECTED_ARGUMENT_VALUE,
                "expected argument value",
                empty,
            )?;
            let value = self.add_expression(empty, Expression::Error)?;
            let consumed_separator = self.current_is_symbol(Symbol::Comma);
            if consumed_separator {
                self.bump()?;
            }
            (value, consumed_separator)
        } else if self.can_start_expression(current) {
            (self.parse_expression_bp(0, argument_stops)?, false)
        } else {
            let error_start = current.span().start();
            let diagnostic_span = current.span();
            let poison = self.is_poison();
            let error_end = self.recover_call_region(outer_stops)?;
            let error_span = self.span(error_start, error_end.max(error_start))?;
            if !poison {
                self.emit(
                    codes::EXPECTED_ARGUMENT_VALUE,
                    "expected argument value",
                    diagnostic_span,
                )?;
            }
            let value = self.add_expression(error_span, Expression::Error)?;
            let consumed_separator = self.current_is_symbol(Symbol::Comma);
            if consumed_separator {
                self.bump()?;
            }
            (value, consumed_separator)
        };
        let value_span = self.expression_span(value)?;
        let start = first_start.unwrap_or(value_span.start());
        if !value_span.is_empty() {
            last_consumed_end = Some(last_consumed_end.unwrap_or(0).max(value_span.end()));
        }
        let end = last_consumed_end.unwrap_or(value_span.end());
        Ok((
            CallArgument {
                span: self.span(start, end)?,
                named_prefix,
                mode_marker,
                value,
            },
            consumed_separator,
        ))
    }

    fn parse_argument_mode_marker(
        &mut self,
    ) -> Result<Option<ParameterModeMarker>, ParserInternalError> {
        let marker = if self.current_is_keyword(Keyword::Borrow) {
            Some(ParameterModeMarker::Borrow(self.bump()?.span()))
        } else if self.current_is_symbol(Symbol::Ampersand) {
            Some(ParameterModeMarker::Inout(self.bump()?.span()))
        } else {
            None
        };
        if marker.is_none() {
            return Ok(None);
        }
        while self.current_is_keyword(Keyword::Borrow) || self.current_is_symbol(Symbol::Ampersand)
        {
            let duplicate = self.bump()?.span();
            self.emit(
                codes::DUPLICATE_ARGUMENT_MODE,
                "duplicate argument mode",
                duplicate,
            )?;
        }
        Ok(marker)
    }

    fn can_start_call_argument(&self, lexeme: Lexeme) -> bool {
        self.can_start_expression(lexeme)
            || self.current_is_keyword(Keyword::Borrow)
            || self.current_is_symbol(Symbol::Ampersand)
    }

    fn call_argument_boundary(&self, lexeme: Lexeme, outer_stops: Stops) -> bool {
        matches!(lexeme.kind(), LexemeKind::Eof)
            || self.current_is_symbol(Symbol::Comma)
            || self.current_is_symbol(Symbol::RightParen)
            || (outer_stops.contains(lexeme)
                && !self.current_is_symbol(Symbol::Comma)
                && !self.current_is_symbol(Symbol::RightParen))
    }

    fn recover_call_region(&mut self, outer_stops: Stops) -> Result<usize, ParserInternalError> {
        self.recover_declaration_region(
            DeclarationStops::from_expression_hard(outer_stops)
                .with(DeclarationStops::COMMA)
                .with(DeclarationStops::RIGHT_PAREN),
        )
    }

    /// 以只读严格识别器判断 `<...>(`，成功后才使用正式 TypeRef parser提交节点。
    fn try_parse_call_type_arguments(
        &mut self,
    ) -> Result<Option<(Vec<TypeRefId>, Span)>, ParserInternalError> {
        let trial_start = self
            .significant(0)
            .map(|(raw, _)| raw)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        let ast_type_len = self.ast.type_refs().len();
        let diagnostic_len = self.diagnostics.len();
        let closing_raw = match self
            .strict_trials
            .query(trial_start, self.recursion_depth)?
        {
            CallTrial::Match { closing_raw, .. } => closing_raw,
            CallTrial::NoMatch { .. } => return Ok(None),
        };

        let opener = self.bump()?.span();
        let mut arguments = Vec::new();
        let stops = TypeStops::empty()
            .with(TypeStops::COMMA)
            .with(TypeStops::GREATER);
        loop {
            arguments.push(self.parse_type_ref(stops)?);
            if self.current_is_symbol(Symbol::Comma) {
                self.bump()?;
                continue;
            }
            break;
        }
        let closer = self.bump()?.span();
        if self.index <= closing_raw
            || !self.current_is_symbol(Symbol::LeftParen)
            || self.ast.type_refs().len() <= ast_type_len
            || self.diagnostics.len() != diagnostic_len
        {
            return Err(ParserInternalError::InvalidLexemeStream);
        }
        Ok(Some((arguments, self.span(opener.start(), closer.end())?)))
    }

    fn parse_index(
        &mut self,
        receiver: ExpressionId,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let outer_stops = outer_stops.without_file_declaration_stop();
        let opener = self.bump()?.span();
        let receiver_span = self.expression_span(receiver)?;
        let inner_stops = outer_stops
            .without_lambda_body_soft_stops()
            .with(Stops::RIGHT_BRACKET)
            .with(Stops::COMMA);
        let index = self.parse_expression_bp(0, inner_stops)?;
        let index_span = self.expression_span(index)?;
        let mut last_consumed_end = index_span.end().max(opener.end());
        let mut reported_missing_closer = false;

        if self.current_is_symbol(Symbol::Comma) {
            let comma = self.bump()?.span();
            self.emit_closing(comma, opener)?;
            reported_missing_closer = true;
            last_consumed_end = comma.end();
            while !self.current_is_symbol(Symbol::RightBracket)
                && !outer_stops.contains(self.current()?)
            {
                last_consumed_end = self.bump()?.span().end();
            }
        }

        let end = if self.current_is_symbol(Symbol::RightBracket) {
            self.bump()?.span().end()
        } else {
            let current = self.current()?;
            if !reported_missing_closer && !self.is_poison() {
                self.emit_closing(self.boundary_span(current, outer_stops)?, opener)?;
            }
            last_consumed_end
        };
        self.add_expression(
            self.span(receiver_span.start(), end)?,
            Expression::Index { receiver, index },
        )
    }

    fn unsupported_operator(&self) -> Result<Option<Span>, ParserInternalError> {
        let Some(first) = self.peek(0) else {
            return Ok(None);
        };
        let Some(second) = self.peek(1) else {
            return Ok(None);
        };
        if first.span().end() != second.span().start() {
            return Ok(None);
        }
        let first_symbol = match first.kind() {
            LexemeKind::Token(TokenKind::Symbol(symbol)) => symbol,
            _ => return Ok(None),
        };
        let second_symbol = match second.kind() {
            LexemeKind::Token(TokenKind::Symbol(symbol)) => symbol,
            _ => return Ok(None),
        };
        let unsupported = matches!(
            (first_symbol, second_symbol),
            (Symbol::Plus, Symbol::Plus)
                | (Symbol::Minus, Symbol::Minus)
                | (Symbol::Less, Symbol::Less)
                | (Symbol::Greater, Symbol::Greater)
                | (Symbol::DotDot, Symbol::Dot)
        );
        if !unsupported {
            return Ok(None);
        }
        Ok(Some(self.span(first.span().start(), second.span().end())?))
    }

    fn can_start_expression(&self, lexeme: Lexeme) -> bool {
        matches!(
            lexeme.kind(),
            LexemeKind::Token(
                TokenKind::Identifier
                    | TokenKind::IntegerLiteral
                    | TokenKind::FloatLiteral
                    | TokenKind::CharLiteral
                    | TokenKind::StringStart
            ) | LexemeKind::Token(TokenKind::Keyword(
                Keyword::True
                    | Keyword::False
                    | Keyword::Null
                    | Keyword::This
                    | Keyword::Move
                    | Keyword::If
                    | Keyword::When
                    | Keyword::Return
                    | Keyword::Break
                    | Keyword::Continue
                    | Keyword::Super
            )) | LexemeKind::Token(TokenKind::Symbol(
                Symbol::LeftParen
                    | Symbol::LeftBrace
                    | Symbol::ColonColon
                    | Symbol::Bang
                    | Symbol::Plus
                    | Symbol::Minus
            )) | LexemeKind::Invalid(_)
                | LexemeKind::Token(TokenKind::ReservedWord(_))
        )
    }

    fn can_start_type_ref(&self, lexeme: Lexeme) -> bool {
        matches!(
            lexeme.kind(),
            LexemeKind::Token(TokenKind::Identifier)
                | LexemeKind::Token(TokenKind::Keyword(Keyword::Move))
                | LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen))
        )
    }

    fn is_poison_kind(&self, kind: LexemeKind) -> bool {
        matches!(
            kind,
            LexemeKind::Invalid(_) | LexemeKind::Token(TokenKind::ReservedWord(_))
        )
    }

    /// 声明尾随恢复必须让 Lexer 已拥有的错误区域保持唯一根因。
    fn is_declaration_trailing_poison(&self, lexeme: Lexeme) -> bool {
        self.is_poison_kind(lexeme.kind())
            || (matches!(lexeme.kind(), LexemeKind::Token(TokenKind::StringStart))
                && self
                    .lexical_recoveries
                    .string_recovery_end(lexeme.span().start())
                    .is_some())
            || (matches!(lexeme.kind(), LexemeKind::Token(TokenKind::StringStart))
                && self
                    .lexical_recoveries
                    .lexical_poison_string_recovery_end(lexeme.span().start())
                    .is_some())
    }

    fn infix_rule(&self, lexeme: Lexeme) -> Result<Option<InfixRule>, ParserInternalError> {
        let rule = match lexeme.kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::Star)) => InfixRule::left(
                PREC_MULTIPLICATIVE,
                InfixKind::Binary(BinaryOperator::Multiply),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Slash)) => InfixRule::left(
                PREC_MULTIPLICATIVE,
                InfixKind::Binary(BinaryOperator::Divide),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Percent)) => InfixRule::left(
                PREC_MULTIPLICATIVE,
                InfixKind::Binary(BinaryOperator::Remainder),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Plus)) => {
                InfixRule::left(PREC_ADDITIVE, InfixKind::Binary(BinaryOperator::Add))
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)) => {
                InfixRule::left(PREC_ADDITIVE, InfixKind::Binary(BinaryOperator::Subtract))
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::DotDot)) => InfixRule::non_associative(
                PREC_RANGE,
                InfixKind::Binary(BinaryOperator::InclusiveRange),
                NonAssociativeGroup::Range,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::DotDotLess)) => InfixRule::non_associative(
                PREC_RANGE,
                InfixKind::Binary(BinaryOperator::ExclusiveRange),
                NonAssociativeGroup::Range,
            ),
            LexemeKind::Token(TokenKind::Identifier)
                if self.sources.slice(lexeme.span())? == "to" =>
            {
                InfixRule::left(PREC_TO, InfixKind::Binary(BinaryOperator::To))
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::QuestionColon)) => {
                InfixRule::right(PREC_ELVIS, InfixKind::Binary(BinaryOperator::Elvis))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::In)) => InfixRule::non_associative(
                PREC_MEMBERSHIP,
                InfixKind::Binary(BinaryOperator::In),
                NonAssociativeGroup::Membership,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangIn)) => InfixRule::non_associative(
                PREC_MEMBERSHIP,
                InfixKind::Binary(BinaryOperator::NotIn),
                NonAssociativeGroup::Membership,
            ),
            LexemeKind::Token(TokenKind::Keyword(Keyword::Is)) => InfixRule::non_associative(
                PREC_MEMBERSHIP,
                InfixKind::TypeTest { negated: false },
                NonAssociativeGroup::Membership,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs)) => InfixRule::non_associative(
                PREC_MEMBERSHIP,
                InfixKind::TypeTest { negated: true },
                NonAssociativeGroup::Membership,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Less)) => InfixRule::non_associative(
                PREC_COMPARISON,
                InfixKind::Binary(BinaryOperator::Less),
                NonAssociativeGroup::Comparison,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)) => InfixRule::non_associative(
                PREC_COMPARISON,
                InfixKind::Binary(BinaryOperator::Greater),
                NonAssociativeGroup::Comparison,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::LessEqual)) => InfixRule::non_associative(
                PREC_COMPARISON,
                InfixKind::Binary(BinaryOperator::LessEqual),
                NonAssociativeGroup::Comparison,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::GreaterEqual)) => {
                InfixRule::non_associative(
                    PREC_COMPARISON,
                    InfixKind::Binary(BinaryOperator::GreaterEqual),
                    NonAssociativeGroup::Comparison,
                )
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::EqualEqual)) => InfixRule::non_associative(
                PREC_EQUALITY,
                InfixKind::Binary(BinaryOperator::Equal),
                NonAssociativeGroup::Equality,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangEqual)) => InfixRule::non_associative(
                PREC_EQUALITY,
                InfixKind::Binary(BinaryOperator::NotEqual),
                NonAssociativeGroup::Equality,
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::AndAnd)) => {
                InfixRule::left(PREC_AND, InfixKind::Binary(BinaryOperator::LogicalAnd))
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::OrOr)) => {
                InfixRule::left(PREC_OR, InfixKind::Binary(BinaryOperator::LogicalOr))
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::As)) => {
                InfixRule::left(PREC_CAST, InfixKind::Cast(CastOperator::As))
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::AsQuestion)) => {
                InfixRule::left(PREC_CAST, InfixKind::Cast(CastOperator::SafeAs))
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::Assign),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::PlusEqual)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::AddAssign),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::MinusEqual)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::SubtractAssign),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::StarEqual)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::MultiplyAssign),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::SlashEqual)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::DivideAssign),
            ),
            LexemeKind::Token(TokenKind::Symbol(Symbol::PercentEqual)) => InfixRule::right(
                PREC_ASSIGNMENT,
                InfixKind::Assignment(AssignmentOperator::RemainderAssign),
            ),
            _ => return Ok(None),
        };
        Ok(Some(rule))
    }

    fn build_infix(
        &mut self,
        left: ExpressionId,
        operator_span: Span,
        rule: InfixRule,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let left_span = self.expression_span(left)?;
        match rule.kind {
            InfixKind::Cast(operator) => {
                let type_ref = self.parse_type_ref(TypeStops::from_expression(stops))?;
                let type_span = self.type_span(type_ref)?;
                self.add_expression(
                    self.span(left_span.start(), type_span.end())?,
                    Expression::Cast {
                        expression: left,
                        operator,
                        operator_span,
                        type_ref,
                    },
                )
            }
            InfixKind::TypeTest { negated } => {
                let type_ref = self.parse_type_ref(TypeStops::from_expression(stops))?;
                let type_span = self.type_span(type_ref)?;
                self.add_expression(
                    self.span(left_span.start(), type_span.end())?,
                    Expression::TypeTest {
                        expression: left,
                        negated,
                        operator_span,
                        type_ref,
                    },
                )
            }
            InfixKind::Binary(operator) => {
                let right = self.parse_expression_bp(rule.right_precedence, stops)?;
                let right_span = self.expression_span(right)?;
                self.add_expression(
                    self.span(left_span.start(), right_span.end())?,
                    Expression::Binary {
                        left,
                        operator,
                        operator_span,
                        right,
                    },
                )
            }
            InfixKind::Assignment(operator) => {
                let right = self.parse_expression_bp(rule.right_precedence, stops)?;
                let right_span = self.expression_span(right)?;
                self.add_expression(
                    self.span(left_span.start(), right_span.end())?,
                    Expression::Assignment {
                        target: left,
                        operator,
                        operator_span,
                        value: right,
                    },
                )
            }
        }
    }

    fn consume_rule_right(
        &mut self,
        rule: InfixRule,
        stops: Stops,
    ) -> Result<ParsedRight, ParserInternalError> {
        let parsed = match rule.kind {
            InfixKind::Cast(_) | InfixKind::TypeTest { .. } => {
                ParsedRight::Type(self.parse_type_ref(TypeStops::from_expression(stops))?)
            }
            InfixKind::Binary(_) | InfixKind::Assignment(_) => {
                ParsedRight::Expression(self.parse_expression_bp(rule.right_precedence, stops)?)
            }
        };
        Ok(parsed)
    }

    fn parse_type_ref(&mut self, stops: TypeStops) -> Result<TypeRefId, ParserInternalError> {
        self.enter_recursion()?;
        let result = self.parse_type_ref_inner(stops);
        self.recursion_depth -= 1;
        result
    }

    fn parse_type_ref_inner(&mut self, stops: TypeStops) -> Result<TypeRefId, ParserInternalError> {
        if self.current_is_keyword(Keyword::Move) || self.current_is_symbol(Symbol::LeftParen) {
            return self.parse_function_type(stops);
        }
        let current = self.current()?;
        let string_start = matches!(current.kind(), LexemeKind::Token(TokenKind::StringStart));
        if string_start {
            let opener = current.span().start();
            let lexical_poison_end =
                self.lexical_recoveries
                    .string_recovery_end(opener)
                    .or_else(|| {
                        self.lexical_recoveries
                            .lexical_poison_string_recovery_end(opener)
                    });
            if let Some(recovery_end) = lexical_poison_end {
                return self.consume_segmented_string_type_poison(recovery_end);
            }
            if let Some(owner_end) = self.lexical_recoveries.string_owner_end(opener) {
                let span = current.span();
                self.emit(
                    codes::EXPECTED_TYPE_REFERENCE,
                    "expected type reference",
                    span,
                )?;
                return self.consume_segmented_string_type_poison(owner_end);
            }
        }
        if !self.current_is_identifier() {
            if stops.contains(current) || matches!(current.kind(), LexemeKind::Eof) {
                let span = self.empty_at(current.span().start())?;
                let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                    && self.lexical_recoveries.terminal_error_at_eof;
                if !terminal_lexer_root {
                    self.emit(
                        codes::EXPECTED_TYPE_REFERENCE,
                        "expected type reference",
                        span,
                    )?;
                }
                return self.add_type_ref(span, TypeRef::Error);
            }
            let span = self.bump()?.span();
            if !matches!(
                current.kind(),
                LexemeKind::Invalid(_) | LexemeKind::Token(TokenKind::ReservedWord(_))
            ) {
                self.emit(
                    codes::EXPECTED_TYPE_REFERENCE,
                    "expected type reference",
                    span,
                )?;
            }
            return self.add_type_ref(span, TypeRef::Error);
        }

        self.parse_qualified_type(stops)
    }

    fn consume_segmented_string_type_poison(
        &mut self,
        recovery_end: usize,
    ) -> Result<TypeRefId, ParserInternalError> {
        let opener = self.bump()?.span();
        let start = opener.start();
        let mut end = opener.end();

        while end < recovery_end {
            let current = self.current()?;
            if matches!(current.kind(), LexemeKind::Eof) {
                break;
            }
            end = self.bump()?.span().end();
        }

        self.add_type_ref(self.span(start, end)?, TypeRef::Error)
    }

    fn parse_qualified_type(&mut self, stops: TypeStops) -> Result<TypeRefId, ParserInternalError> {
        let first = self.bump()?.span();
        let mut segments = vec![TypePathSegment {
            name_span: first,
            arguments: Vec::new(),
        }];
        let mut end = first.end();

        while self.current_is_symbol(Symbol::Dot) {
            let dot = self.bump()?.span();
            if !self.current_is_identifier() {
                let current = self.current()?;
                let error_end = if self.is_poison() {
                    self.bump()?.span().end()
                } else {
                    let primary = self.type_boundary_span(current, stops)?;
                    self.emit(
                        codes::EXPECTED_TYPE_REFERENCE,
                        "expected type reference",
                        primary,
                    )?;
                    dot.end()
                };
                return self.add_type_ref(self.span(first.start(), error_end)?, TypeRef::Error);
            }
            let name_span = self.bump()?.span();
            end = name_span.end();
            segments.push(TypePathSegment {
                name_span,
                arguments: Vec::new(),
            });
        }

        if self.current_is_symbol(Symbol::Less) {
            let opener = self.bump()?.span();
            let mut arguments = Vec::new();
            let argument_stops = stops
                .without_block_elements()
                .with(TypeStops::COMMA)
                .with(TypeStops::GREATER);
            loop {
                arguments.push(self.parse_type_ref(argument_stops)?);
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                break;
            }
            if self.current_is_symbol(Symbol::Greater) {
                end = self.bump()?.span().end();
            } else {
                let current = self.current()?;
                if !self.is_poison() {
                    self.emit_closing(self.type_boundary_span(current, stops)?, opener)?;
                }
                if let Some(last) = arguments.last() {
                    end = self.type_span(*last)?.end();
                }
            }
            if let Some(last) = segments.last_mut() {
                last.arguments = arguments;
            }
        }

        let nullable_span = if self.current_is_symbol(Symbol::Question) {
            let span = self.bump()?.span();
            end = span.end();
            Some(span)
        } else {
            None
        };
        self.add_type_ref(
            self.span(first.start(), end)?,
            TypeRef::Qualified {
                segments,
                nullable_span,
            },
        )
    }

    fn parse_function_type(
        &mut self,
        outer_stops: TypeStops,
    ) -> Result<TypeRefId, ParserInternalError> {
        let move_span = if self.current_is_keyword(Keyword::Move) {
            Some(self.bump()?.span())
        } else {
            None
        };
        if !self.current_is_symbol(Symbol::LeftParen) {
            let current = self.current()?;
            let poison = self.is_poison();
            let start = move_span
                .map(Span::start)
                .unwrap_or_else(|| current.span().start());
            let error_span =
                if outer_stops.contains(current) || matches!(current.kind(), LexemeKind::Eof) {
                    match move_span {
                        Some(span) => span,
                        None => self.empty_at(current.span().start())?,
                    }
                } else {
                    let consumed = self.bump()?.span();
                    self.span(start, consumed.end())?
                };
            if !poison {
                let diagnostic_span = self.type_boundary_span(current, outer_stops)?;
                self.emit(
                    codes::EXPECTED_TYPE_REFERENCE,
                    "expected type reference",
                    diagnostic_span,
                )?;
            }
            return self.add_type_ref(error_span, TypeRef::Error);
        }
        let opener = self.bump()?.span();
        let start = move_span.unwrap_or(opener).start();
        let mut parameters = Vec::new();
        let parameter_stops = outer_stops
            .without_block_elements()
            .with(TypeStops::COMMA)
            .with(TypeStops::RIGHT_PAREN);
        if !self.current_is_symbol(Symbol::RightParen) {
            loop {
                let mode_marker = self.parse_parameter_mode_marker()?;
                let parameter_start = mode_marker
                    .map(parameter_mode_span)
                    .map(Span::start)
                    .unwrap_or(self.current()?.span().start());
                let type_ref = self.parse_type_ref(parameter_stops)?;
                let type_span = self.type_span(type_ref)?;
                let marker_end = mode_marker.map(|marker| {
                    self.previous_significant_end()
                        .max(parameter_mode_span(marker).end())
                });
                let parameter_end = if type_span.is_empty() {
                    marker_end.unwrap_or(type_span.end())
                } else {
                    marker_end.unwrap_or(0).max(type_span.end())
                };
                parameters.push(FunctionTypeParameter {
                    span: self.span(parameter_start, parameter_end)?,
                    mode_marker,
                    type_ref,
                });
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                break;
            }
        }
        let mut last_end = parameters
            .last()
            .map(|parameter| parameter.span.end())
            .unwrap_or(opener.end());
        if self.current_is_symbol(Symbol::RightParen) {
            last_end = self.bump()?.span().end();
        } else {
            let current = self.current()?;
            if !self.is_poison() {
                self.emit_closing(self.type_boundary_span(current, outer_stops)?, opener)?;
            }
        }

        let (arrow_span, return_type) = if self.current_is_symbol(Symbol::Arrow) {
            let span = self.bump()?.span();
            last_end = span.end();
            let return_type = self.parse_type_ref(outer_stops)?;
            (span, return_type)
        } else {
            let current = self.current()?;
            let primary = self.type_boundary_span(current, outer_stops)?;
            let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
                && self.lexical_recoveries.terminal_error_at_eof;
            if !self.is_poison() && !terminal_lexer_root {
                self.emit(
                    codes::EXPECTED_TYPE_REFERENCE,
                    "expected type reference",
                    primary,
                )?;
            }
            (
                self.span(last_end, last_end)?,
                self.add_type_ref(primary, TypeRef::Error)?,
            )
        };
        let return_span = self.type_span(return_type)?;
        self.add_type_ref(
            self.span(start, return_span.end().max(last_end))?,
            TypeRef::Function {
                move_span,
                parameters,
                arrow_span,
                return_type,
            },
        )
    }

    fn significant(&self, ordinal: usize) -> Option<(usize, Lexeme)> {
        let mut remaining = ordinal;
        for (index, lexeme) in self.lexed.lexemes()[self.index..].iter().enumerate() {
            #[cfg(test)]
            self.significant_raw_visits
                .set(self.significant_raw_visits.get() + 1);
            if matches!(lexeme.kind(), LexemeKind::Trivia(_)) {
                continue;
            }
            if remaining == 0 {
                return Some((self.index + index, *lexeme));
            }
            remaining -= 1;
        }
        None
    }

    fn enter_recursion(&mut self) -> Result<(), ParserInternalError> {
        if self.recursion_depth >= MAX_RECURSION_DEPTH {
            return Err(ParserInternalError::NestingLimitExceeded {
                limit: MAX_RECURSION_DEPTH,
            });
        }
        self.recursion_depth += 1;
        Ok(())
    }

    fn current(&self) -> Result<Lexeme, ParserInternalError> {
        self.significant(0)
            .map(|(_, lexeme)| lexeme)
            .ok_or(ParserInternalError::InvalidLexemeStream)
    }

    fn current_raw(&self) -> Result<usize, ParserInternalError> {
        self.significant(0)
            .map(|(raw, _)| raw)
            .ok_or(ParserInternalError::InvalidLexemeStream)
    }

    fn peek(&self, ordinal: usize) -> Option<Lexeme> {
        self.significant(ordinal).map(|(_, lexeme)| lexeme)
    }

    fn bump(&mut self) -> Result<Lexeme, ParserInternalError> {
        let (index, lexeme) = self
            .significant(0)
            .ok_or(ParserInternalError::InvalidLexemeStream)?;
        self.index = index + 1;
        Ok(lexeme)
    }

    fn current_is_symbol(&self, symbol: Symbol) -> bool {
        matches!(
            self.peek(0).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Symbol(actual))) if actual == symbol
        )
    }

    fn current_is_keyword(&self, keyword: Keyword) -> bool {
        matches!(
            self.peek(0).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Keyword(actual))) if actual == keyword
        )
    }

    fn current_is_identifier(&self) -> bool {
        matches!(
            self.peek(0).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Identifier))
        )
    }

    fn current_identifier_is(&self, expected: &str) -> Result<bool, ParserInternalError> {
        if !self.current_is_identifier() {
            return Ok(false);
        }
        Ok(self.sources.slice(self.current()?.span())? == expected)
    }

    fn is_poison(&self) -> bool {
        matches!(
            self.peek(0).map(Lexeme::kind),
            Some(LexemeKind::Invalid(_)) | Some(LexemeKind::Token(TokenKind::ReservedWord(_)))
        )
    }

    fn span(&self, start: usize, end: usize) -> Result<Span, ParserInternalError> {
        Ok(self.sources.span(self.lexed.source_id(), start, end)?)
    }

    fn empty_at(&self, offset: usize) -> Result<Span, ParserInternalError> {
        self.span(offset, offset)
    }

    fn boundary_span(&self, current: Lexeme, stops: Stops) -> Result<Span, ParserInternalError> {
        if stops.contains(current) || matches!(current.kind(), LexemeKind::Eof) {
            self.empty_at(current.span().start())
        } else {
            Ok(current.span())
        }
    }

    fn type_boundary_span(
        &self,
        current: Lexeme,
        stops: TypeStops,
    ) -> Result<Span, ParserInternalError> {
        if stops.contains(current) || matches!(current.kind(), LexemeKind::Eof) {
            self.empty_at(current.span().start())
        } else {
            Ok(current.span())
        }
    }

    fn previous_significant_end(&self) -> usize {
        self.lexed.lexemes()[..self.index]
            .iter()
            .rev()
            .find(|lexeme| !matches!(lexeme.kind(), LexemeKind::Trivia(_)))
            .map(|lexeme| lexeme.span().end())
            .unwrap_or(0)
    }

    fn expression_span(&self, id: ExpressionId) -> Result<Span, ParserInternalError> {
        Ok(self.ast.expressions().get(id)?.span())
    }

    fn validate_item_context(&mut self, id: ItemId) -> Result<(), ParserInternalError> {
        self.validate_context_work(vec![ContextWork::Item(id)])
    }

    fn control_expression_line_boundary(
        &self,
        id: ExpressionId,
    ) -> Result<bool, ParserInternalError> {
        if !matches!(
            self.ast.expressions().get(id)?.payload(),
            Expression::If { .. }
                | Expression::When { .. }
                | Expression::Return { .. }
                | Expression::Break { .. }
                | Expression::Continue { .. }
        ) {
            return Ok(false);
        }
        self.gap_has_line_break(
            self.expression_span(id)?.end(),
            self.current()?.span().start(),
        )
    }

    fn lambda_initializer_line_boundary(
        &self,
        id: ExpressionId,
    ) -> Result<bool, ParserInternalError> {
        if !matches!(
            self.ast.expressions().get(id)?.payload(),
            Expression::Lambda { .. }
        ) {
            return Ok(false);
        }
        self.gap_has_line_break(
            self.expression_span(id)?.end(),
            self.current()?.span().start(),
        )
    }

    fn validate_statement_context(
        &mut self,
        id: StatementId,
        expression_is_statement: bool,
    ) -> Result<(), ParserInternalError> {
        self.validate_context_work(vec![ContextWork::Statement {
            id,
            expression_is_statement,
        }])
    }

    fn validate_expression_context(
        &mut self,
        id: ExpressionId,
        statement_allowed: bool,
    ) -> Result<(), ParserInternalError> {
        self.validate_context_work(vec![ContextWork::Expression {
            id,
            statement_allowed,
        }])
    }

    fn validate_context_work(
        &mut self,
        mut work: Vec<ContextWork>,
    ) -> Result<(), ParserInternalError> {
        while let Some(current) = work.pop() {
            match current {
                ContextWork::Item(id) => {
                    let item = self.ast.items().get(id)?.payload().clone();
                    match item {
                        Item::Error => {}
                        Item::Modified { declaration, .. } => {
                            work.push(ContextWork::Item(declaration));
                        }
                        Item::Variable { initializer, .. } | Item::Constant { initializer, .. } => {
                            work.push(ContextWork::Expression {
                                id: initializer,
                                statement_allowed: false,
                            });
                        }
                        Item::Function { form, .. } => match form {
                            FunctionForm::ImplicitUnitAbsent => {}
                            FunctionForm::ImplicitUnitBlock(body) => {
                                work.push(ContextWork::Statement {
                                    id: body,
                                    expression_is_statement: true,
                                });
                            }
                            FunctionForm::Explicit { body, .. } => match body {
                                FunctionBody::Absent => {}
                                FunctionBody::Expression { expression, .. } => {
                                    work.push(ContextWork::Expression {
                                        id: expression,
                                        statement_allowed: false,
                                    });
                                }
                                FunctionBody::Block(body) => {
                                    work.push(ContextWork::Statement {
                                        id: body,
                                        expression_is_statement: true,
                                    });
                                }
                            },
                        },
                        Item::Classifier(classifier) => {
                            if let Some(body) = classifier.body {
                                for member in body.members.into_iter().rev() {
                                    work.push(ContextWork::Item(member));
                                }
                            }
                        }
                        Item::Companion(companion) => {
                            for member in companion.body.members.into_iter().rev() {
                                work.push(ContextWork::Item(member));
                            }
                        }
                    }
                }
                ContextWork::Statement {
                    id,
                    expression_is_statement,
                } => {
                    let statement = self.ast.statements().get(id)?.payload().clone();
                    match statement {
                        Statement::Error => {}
                        Statement::Block { elements } | Statement::ControlBody { elements } => {
                            for element in elements.into_iter().rev() {
                                work.push(ContextWork::Statement {
                                    id: element,
                                    expression_is_statement: true,
                                });
                            }
                        }
                        Statement::LambdaBody { elements } => {
                            let last = elements.len().saturating_sub(1);
                            for (index, element) in elements.into_iter().enumerate().rev() {
                                let is_tail_expression = index == last
                                    && matches!(
                                        self.ast.statements().get(element)?.payload(),
                                        Statement::Expression { .. }
                                    );
                                work.push(ContextWork::Statement {
                                    id: element,
                                    expression_is_statement: !is_tail_expression,
                                });
                            }
                        }
                        Statement::LocalVariable { declaration } => {
                            work.push(ContextWork::Item(declaration));
                        }
                        Statement::LocalDestructuring { initializer, .. } => {
                            work.push(ContextWork::Expression {
                                id: initializer,
                                statement_allowed: false,
                            });
                        }
                        Statement::While {
                            condition, body, ..
                        } => {
                            work.push(ContextWork::Statement {
                                id: body,
                                expression_is_statement: true,
                            });
                            work.push(ContextWork::Expression {
                                id: condition,
                                statement_allowed: false,
                            });
                        }
                        Statement::For { source, body, .. } => {
                            work.push(ContextWork::Statement {
                                id: body,
                                expression_is_statement: true,
                            });
                            work.push(ContextWork::Expression {
                                id: source,
                                statement_allowed: false,
                            });
                        }
                        Statement::Loop { body, .. } => {
                            work.push(ContextWork::Statement {
                                id: body,
                                expression_is_statement: true,
                            });
                        }
                        Statement::Expression { expression } => {
                            work.push(ContextWork::Expression {
                                id: expression,
                                statement_allowed: expression_is_statement,
                            });
                        }
                    }
                }
                ContextWork::ControlBody { id, value_required } => {
                    let statement = self.ast.statements().get(id)?.payload().clone();
                    match statement {
                        Statement::ControlBody { elements } => {
                            let last = elements.len().saturating_sub(1);
                            for (index, element) in elements.into_iter().enumerate().rev() {
                                let is_tail_expression = value_required
                                    && index == last
                                    && matches!(
                                        self.ast.statements().get(element)?.payload(),
                                        Statement::Expression { .. }
                                    );
                                work.push(ContextWork::Statement {
                                    id: element,
                                    expression_is_statement: !is_tail_expression,
                                });
                            }
                        }
                        Statement::Expression { expression } => {
                            work.push(ContextWork::Expression {
                                id: expression,
                                statement_allowed: !value_required,
                            });
                        }
                        _ => work.push(ContextWork::Statement {
                            id,
                            expression_is_statement: true,
                        }),
                    }
                }
                ContextWork::Expression {
                    id,
                    statement_allowed,
                } => {
                    let expression = self.ast.expressions().get(id)?.payload().clone();
                    match expression {
                        Expression::Error
                        | Expression::Name
                        | Expression::This
                        | Expression::Literal(_)
                        | Expression::Break { .. }
                        | Expression::Continue { .. }
                        | Expression::SuperMember { .. } => {}
                        Expression::Group { expression }
                        | Expression::Prefix {
                            operand: expression,
                            ..
                        }
                        | Expression::Cast { expression, .. }
                        | Expression::TypeTest { expression, .. }
                        | Expression::Member {
                            receiver: expression,
                            ..
                        }
                        | Expression::NonNullAssert {
                            operand: expression,
                            ..
                        }
                        | Expression::Propagate {
                            value: expression, ..
                        } => work.push(ContextWork::Expression {
                            id: expression,
                            statement_allowed: false,
                        }),
                        Expression::String { parts } => {
                            for part in parts.into_iter().rev() {
                                if let StringPart::Interpolation { expression, .. } = part {
                                    work.push(ContextWork::Expression {
                                        id: expression,
                                        statement_allowed: false,
                                    });
                                }
                            }
                        }
                        Expression::Lambda { body, .. } => {
                            work.push(ContextWork::Statement {
                                id: body,
                                expression_is_statement: true,
                            });
                        }
                        Expression::If {
                            then_branch,
                            else_branch,
                            condition,
                            ..
                        } => {
                            if else_branch.is_none() && !statement_allowed {
                                let span =
                                    self.empty_at(self.statement_span(then_branch)?.end())?;
                                self.emit(
                                    codes::EXPECTED_ELSE_BRANCH,
                                    "expected else branch",
                                    span,
                                )?;
                            }
                            if let Some(branch) = else_branch {
                                work.push(ContextWork::ControlBody {
                                    id: branch,
                                    value_required: !statement_allowed,
                                });
                            }
                            work.push(ContextWork::ControlBody {
                                id: then_branch,
                                value_required: !statement_allowed,
                            });
                            work.push(ContextWork::Expression {
                                id: condition,
                                statement_allowed: false,
                            });
                        }
                        Expression::When {
                            subject, entries, ..
                        } => {
                            for entry in entries.into_iter().rev() {
                                work.push(ContextWork::ControlBody {
                                    id: entry.body,
                                    value_required: !statement_allowed,
                                });
                                for condition in entry.conditions.into_iter().rev() {
                                    match condition {
                                        WhenCondition::Expression(expression)
                                        | WhenCondition::Contains { expression, .. } => {
                                            work.push(ContextWork::Expression {
                                                id: expression,
                                                statement_allowed: false,
                                            });
                                        }
                                        WhenCondition::TypeTest { .. } => {}
                                    }
                                }
                            }
                            if let Some(subject) = subject {
                                work.push(ContextWork::Expression {
                                    id: subject,
                                    statement_allowed: false,
                                });
                            }
                        }
                        Expression::Return { value, .. } => {
                            if let Some(value) = value {
                                work.push(ContextWork::Expression {
                                    id: value,
                                    statement_allowed: false,
                                });
                            }
                        }
                        Expression::Binary { left, right, .. }
                        | Expression::Assignment {
                            target: left,
                            value: right,
                            ..
                        }
                        | Expression::Index {
                            receiver: left,
                            index: right,
                        } => {
                            work.push(ContextWork::Expression {
                                id: right,
                                statement_allowed: false,
                            });
                            work.push(ContextWork::Expression {
                                id: left,
                                statement_allowed: false,
                            });
                        }
                        Expression::Call {
                            callee, arguments, ..
                        } => {
                            for argument in arguments.into_iter().rev() {
                                work.push(ContextWork::Expression {
                                    id: argument.value,
                                    statement_allowed: false,
                                });
                            }
                            work.push(ContextWork::Expression {
                                id: callee,
                                statement_allowed: false,
                            });
                        }
                        Expression::CallableReference { receiver, .. } => {
                            if let Some(receiver) = receiver {
                                work.push(ContextWork::Expression {
                                    id: receiver,
                                    statement_allowed: false,
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn statement_span(&self, id: StatementId) -> Result<Span, ParserInternalError> {
        Ok(self.ast.statements().get(id)?.span())
    }

    fn type_span(&self, id: TypeRefId) -> Result<Span, ParserInternalError> {
        Ok(self.ast.type_refs().get(id)?.span())
    }

    fn add_expression(
        &mut self,
        span: Span,
        payload: Expression,
    ) -> Result<ExpressionId, ParserInternalError> {
        Ok(self.ast.add_expression(span, payload)?)
    }

    fn add_statement(
        &mut self,
        span: Span,
        payload: Statement,
    ) -> Result<StatementId, ParserInternalError> {
        Ok(self.ast.add_statement(span, payload)?)
    }

    fn add_item(&mut self, span: Span, item: Item) -> Result<ItemId, ParserInternalError> {
        Ok(self.ast.add_item(span, item)?)
    }

    fn add_type_ref(
        &mut self,
        span: Span,
        payload: TypeRef,
    ) -> Result<TypeRefId, ParserInternalError> {
        Ok(self.ast.add_type_ref(span, payload)?)
    }

    fn code(&self, raw: &str) -> Result<DiagnosticCode, ParserInternalError> {
        Ok(codes::catalog()?.resolve(raw)?)
    }

    fn emit(
        &mut self,
        raw_code: &str,
        message: &'static str,
        span: Span,
    ) -> Result<(), ParserInternalError> {
        let code = self.code(raw_code)?;
        self.diagnostics.push(Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            message,
            span,
        )?);
        Ok(())
    }

    fn emit_closing(&mut self, primary: Span, opener: Span) -> Result<(), ParserInternalError> {
        let code = self.code(codes::EXPECTED_CLOSING_DELIMITER)?;
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            "expected closing delimiter",
            primary,
        )?;
        diagnostic.add_label(self.sources, opener, "opening delimiter is here")?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn consume_expression_tail(
        &mut self,
        expression: ExpressionId,
        stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let start = self.expression_span(expression)?.start();
        let mut consumed_end = None;

        while self.is_poison() {
            let span = self.bump()?.span();
            self.add_expression(span, Expression::Error)?;
            consumed_end = Some(span.end());
        }

        if !stops.contains(self.current()?) {
            let first = self.current()?.span();
            self.emit(
                codes::UNEXPECTED_TRAILING_TOKEN,
                "unexpected trailing token",
                first,
            )?;
            let error_start = first.start();
            let mut error_end = first.end();
            while !stops.contains(self.current()?) {
                error_end = self.bump()?.span().end();
            }
            self.add_expression(self.span(error_start, error_end)?, Expression::Error)?;
            consumed_end = Some(error_end);
        }

        if let Some(end) = consumed_end {
            return self.add_expression(self.span(start, end)?, Expression::Error);
        }
        Ok(expression)
    }
}

fn unsupported_block_element_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Const
                | Keyword::Fun
                | Keyword::For
                | Keyword::While
                | Keyword::Loop
                | Keyword::Value
                | Keyword::Class
                | Keyword::Interface
                | Keyword::Enum
                | Keyword::Object
                | Keyword::Companion
        ))
    )
}

fn control_expression_start_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::If
                | Keyword::When
                | Keyword::Return
                | Keyword::Break
                | Keyword::Continue
                | Keyword::Super
        ))
    )
}

fn simple_declaration_start_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Val
                | Keyword::Var
                | Keyword::Const
                | Keyword::Fun
                | Keyword::Value
                | Keyword::Class
                | Keyword::Interface
                | Keyword::Enum
                | Keyword::Object
                | Keyword::Public
                | Keyword::Internal
                | Keyword::Private
                | Keyword::Override
        ))
    )
}

fn classifier_declaration_start_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Value | Keyword::Class | Keyword::Interface | Keyword::Enum | Keyword::Object
        ))
    )
}

fn class_field_start_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Keyword(
            Keyword::Val
                | Keyword::Var
                | Keyword::Public
                | Keyword::Internal
                | Keyword::Private
                | Keyword::Override
        ))
    )
}

fn class_member_start_kind(kind: LexemeKind) -> bool {
    classifier_declaration_start_kind(kind)
        || matches!(
            kind,
            LexemeKind::Token(TokenKind::Keyword(
                Keyword::Fun
                    | Keyword::Const
                    | Keyword::Val
                    | Keyword::Var
                    | Keyword::Companion
                    | Keyword::Public
                    | Keyword::Internal
                    | Keyword::Private
                    | Keyword::Override
            ))
        )
}

fn visibility_span(visibility: VisibilityModifier) -> Span {
    match visibility {
        VisibilityModifier::Public(span)
        | VisibilityModifier::Internal(span)
        | VisibilityModifier::Private(span) => span,
    }
}

fn declaration_modifier_start(modifiers: DeclarationModifiers) -> Option<usize> {
    modifiers
        .visibility
        .map(visibility_span)
        .map(Span::start)
        .into_iter()
        .chain(modifiers.override_span.map(Span::start))
        .min()
}

fn classifier_keyword_start(kind: ClassifierKind) -> usize {
    match kind {
        ClassifierKind::ValueClass { value_span, .. } => value_span.start(),
        ClassifierKind::Class { class_span } => class_span.start(),
        ClassifierKind::Interface { interface_span } => interface_span.start(),
        ClassifierKind::EnumClass { enum_span, .. } => enum_span.start(),
        ClassifierKind::Object { object_span } => object_span.start(),
    }
}

#[derive(Clone, Copy)]
enum ClassMemberContext {
    ValueClass,
    Class,
    Interface,
    Enum,
    Object,
    Companion,
}

impl ClassMemberContext {
    const fn allows_override(self) -> bool {
        matches!(
            self,
            Self::ValueClass | Self::Class | Self::Enum | Self::Object
        )
    }

    const fn allows_companion(self) -> bool {
        !matches!(self, Self::Object | Self::Companion)
    }

    const fn allows_constant(self) -> bool {
        matches!(self, Self::Object | Self::Companion)
    }
}

fn file_construct_start_kind(kind: LexemeKind) -> bool {
    simple_declaration_start_kind(kind)
        || matches!(
            kind,
            LexemeKind::Token(TokenKind::Keyword(Keyword::Package | Keyword::Import))
        )
}

#[derive(Clone, Copy)]
struct Stops {
    delimiters: u16,
    block_elements: bool,
    when_entry_body: bool,
}

impl Stops {
    const ROOT: Self = Self {
        delimiters: 0,
        block_elements: false,
        when_entry_body: false,
    };
    const FILE: Self = Self {
        delimiters: Self::FILE_DECLARATION,
        block_elements: false,
        when_entry_body: false,
    };
    const RIGHT_PAREN: u16 = 1 << 0;
    const RIGHT_BRACKET: u16 = 1 << 1;
    const COMMA: u16 = 1 << 2;
    const INTERPOLATION_END: u16 = 1 << 3;
    const RIGHT_BRACE: u16 = 1 << 4;
    const ARROW: u16 = 1 << 5;
    const LAMBDA_COMMA: u16 = 1 << 6;
    const FILE_DECLARATION: u16 = 1 << 7;
    const ELSE: u16 = 1 << 8;
    const HARD_DELIMITERS: u16 = Self::RIGHT_PAREN | Self::RIGHT_BRACKET | Self::INTERPOLATION_END;

    const fn block_expression(outer_stops: Self) -> Self {
        Self {
            delimiters: (outer_stops.delimiters & Self::HARD_DELIMITERS) | Self::RIGHT_BRACE,
            block_elements: true,
            when_entry_body: false,
        }
    }

    const fn lambda_expression(outer_stops: Self) -> Self {
        Self {
            delimiters: (outer_stops.delimiters & Self::HARD_DELIMITERS)
                | Self::RIGHT_BRACE
                | Self::LAMBDA_COMMA
                | Self::ARROW,
            block_elements: true,
            when_entry_body: false,
        }
    }

    const fn with(self, flag: u16) -> Self {
        Self {
            delimiters: self.delimiters | flag,
            block_elements: self.block_elements,
            when_entry_body: self.when_entry_body,
        }
    }

    const fn as_when_entry_body(self) -> Self {
        Self {
            when_entry_body: true,
            ..self
        }
    }

    const fn without_lambda_body_soft_stops(self) -> Self {
        Self {
            delimiters: self.delimiters
                & !(Self::LAMBDA_COMMA | Self::ARROW | Self::FILE_DECLARATION),
            block_elements: false,
            when_entry_body: self.when_entry_body,
        }
    }

    const fn without_file_declaration_stop(self) -> Self {
        Self {
            delimiters: self.delimiters & !Self::FILE_DECLARATION,
            block_elements: self.block_elements,
            when_entry_body: self.when_entry_body,
        }
    }

    fn contains_hard(self, lexeme: Lexeme) -> bool {
        match lexeme.kind() {
            LexemeKind::Eof => true,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                self.delimiters & Self::RIGHT_PAREN != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => {
                self.delimiters & Self::RIGHT_BRACKET != 0
            }
            LexemeKind::Token(TokenKind::InterpolationEnd) => {
                self.delimiters & Self::INTERPOLATION_END != 0
            }
            _ => false,
        }
    }

    fn contains(self, lexeme: Lexeme) -> bool {
        match lexeme.kind() {
            LexemeKind::Eof => true,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                self.delimiters & Self::RIGHT_PAREN != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => {
                self.delimiters & Self::RIGHT_BRACKET != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)) => {
                self.delimiters & Self::RIGHT_BRACE != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)) => {
                self.delimiters & (Self::COMMA | Self::LAMBDA_COMMA) != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)) => {
                self.delimiters & Self::FILE_DECLARATION != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow)) => {
                self.delimiters & Self::ARROW != 0
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::Else)) => {
                self.delimiters & Self::ELSE != 0
            }
            LexemeKind::Token(TokenKind::InterpolationEnd) => {
                self.delimiters & Self::INTERPOLATION_END != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace))
            | LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
                if self.block_elements =>
            {
                true
            }
            kind if self.block_elements && unsupported_block_element_kind(kind) => true,
            kind if self.block_elements && control_expression_start_kind(kind) => true,
            kind if self.delimiters & Self::FILE_DECLARATION != 0
                && file_construct_start_kind(kind) =>
            {
                true
            }
            _ => false,
        }
    }
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

#[derive(Clone, Copy)]
struct TypeStops(u16);

impl TypeStops {
    const EQUAL: u16 = 1 << 5;
    const LEFT_BRACE: u16 = 1 << 6;
    const RIGHT_BRACE: u16 = 1 << 7;
    const BLOCK_ELEMENT: u16 = 1 << 8;
    const fn empty() -> Self {
        Self(0)
    }

    const COMMA: u16 = 1 << 0;
    const GREATER: u16 = 1 << 1;
    const RIGHT_PAREN: u16 = 1 << 2;
    const RIGHT_BRACKET: u16 = 1 << 3;
    const INTERPOLATION_END: u16 = 1 << 4;
    const FILE: u16 = 1 << 9;
    const ARROW: u16 = 1 << 10;

    const fn from_expression(stops: Stops) -> Self {
        let mut bits = 0;
        if stops.delimiters & (Stops::COMMA | Stops::LAMBDA_COMMA) != 0 {
            bits |= Self::COMMA;
        }
        if stops.delimiters & Stops::RIGHT_PAREN != 0 {
            bits |= Self::RIGHT_PAREN;
        }
        if stops.delimiters & Stops::RIGHT_BRACKET != 0 {
            bits |= Self::RIGHT_BRACKET;
        }
        if stops.delimiters & Stops::INTERPOLATION_END != 0 {
            bits |= Self::INTERPOLATION_END;
        }
        if stops.delimiters & Stops::ARROW != 0 {
            bits |= Self::ARROW;
        }
        if stops.delimiters & Stops::RIGHT_BRACE != 0 {
            bits |= Self::RIGHT_BRACE;
        }
        if stops.block_elements {
            bits |= Self::BLOCK_ELEMENT | Self::LEFT_BRACE;
        }
        if stops.delimiters & Stops::FILE_DECLARATION != 0 {
            bits |= Self::FILE;
        }
        Self(bits)
    }

    const fn with(self, flag: u16) -> Self {
        Self(self.0 | flag)
    }

    const fn without_block_elements(self) -> Self {
        Self(self.0 & !(Self::BLOCK_ELEMENT | Self::LEFT_BRACE | Self::FILE))
    }

    fn contains(self, lexeme: Lexeme) -> bool {
        match lexeme.kind() {
            LexemeKind::Eof => true,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)) => self.0 & Self::COMMA != 0,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)) => self.0 & Self::GREATER != 0,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                self.0 & Self::RIGHT_PAREN != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => {
                self.0 & Self::RIGHT_BRACKET != 0
            }
            LexemeKind::Token(TokenKind::InterpolationEnd) => self.0 & Self::INTERPOLATION_END != 0,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)) => self.0 & Self::EQUAL != 0,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Arrow)) => self.0 & Self::ARROW != 0,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) => {
                self.0 & Self::LEFT_BRACE != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)) => {
                self.0 & Self::RIGHT_BRACE != 0
            }
            LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
                if self.0 & Self::BLOCK_ELEMENT != 0 =>
            {
                true
            }
            kind if self.0 & Self::BLOCK_ELEMENT != 0 && unsupported_block_element_kind(kind) => {
                true
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)) if self.0 & Self::FILE != 0 => {
                true
            }
            kind if self.0 & Self::FILE != 0 && file_construct_start_kind(kind) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy)]
enum NameContext {
    Declaration,
    LocalDeclaration,
    TypeParameter,
    ValueParameter,
}

impl NameContext {
    fn is_stop(self, lexeme: Lexeme) -> bool {
        if matches!(self, Self::LocalDeclaration) && unsupported_block_element_kind(lexeme.kind()) {
            return true;
        }
        matches!(
            (self, lexeme.kind()),
            (
                Self::Declaration | Self::LocalDeclaration,
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::Colon | Symbol::Equal | Symbol::LeftParen,
                )),
            ) | (
                Self::LocalDeclaration,
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace | Symbol::RightBrace,)),
            ) | (
                Self::LocalDeclaration,
                LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var)),
            ) | (
                Self::TypeParameter,
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::Colon | Symbol::Comma | Symbol::Greater,
                )),
            ) | (
                Self::ValueParameter,
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::Colon | Symbol::Comma | Symbol::RightParen,
                )),
            )
        )
    }

    const fn recovery_stops(self) -> DeclarationStops {
        match self {
            Self::Declaration => DeclarationStops::EMPTY
                .with(DeclarationStops::COLON)
                .with(DeclarationStops::EQUAL)
                .with(DeclarationStops::LEFT_PAREN),
            Self::LocalDeclaration => DeclarationStops::EMPTY
                .with(DeclarationStops::COLON)
                .with(DeclarationStops::EQUAL)
                .with(DeclarationStops::LEFT_PAREN)
                .with(DeclarationStops::LEFT_BRACE)
                .with(DeclarationStops::RIGHT_BRACE)
                .with(DeclarationStops::BLOCK_ELEMENT),
            Self::TypeParameter => DeclarationStops::EMPTY
                .with(DeclarationStops::COLON)
                .with(DeclarationStops::COMMA)
                .with(DeclarationStops::GREATER),
            Self::ValueParameter => DeclarationStops::EMPTY
                .with(DeclarationStops::COLON)
                .with(DeclarationStops::COMMA)
                .with(DeclarationStops::RIGHT_PAREN),
        }
    }
}

#[derive(Clone, Copy)]
struct DeclarationStops(u16);

impl DeclarationStops {
    const EMPTY: Self = Self(0);
    const COMMA: u16 = 1 << 0;
    const RIGHT_PAREN: u16 = 1 << 1;
    const GREATER: u16 = 1 << 2;
    const LEFT_BRACE: u16 = 1 << 3;
    const COLON: u16 = 1 << 4;
    const EQUAL: u16 = 1 << 5;
    const LEFT_PAREN: u16 = 1 << 6;
    const RIGHT_BRACE: u16 = 1 << 7;
    const BLOCK_ELEMENT: u16 = 1 << 8;
    const RIGHT_BRACKET: u16 = 1 << 9;
    const INTERPOLATION_END: u16 = 1 << 10;
    const FILE: Self = Self(1 << 11);
    const SEMICOLON: u16 = 1 << 12;
    const CLASS_MEMBER: u16 = 1 << 13;
    const ENUM_VARIANT: u16 = 1 << 14;

    const fn from_expression_hard(stops: Stops) -> Self {
        let mut bits = 0;
        if stops.delimiters & Stops::RIGHT_PAREN != 0 {
            bits |= Self::RIGHT_PAREN;
        }
        if stops.delimiters & Stops::RIGHT_BRACKET != 0 {
            bits |= Self::RIGHT_BRACKET;
        }
        if stops.delimiters & Stops::RIGHT_BRACE != 0 {
            bits |= Self::RIGHT_BRACE;
        }
        if stops.delimiters & Stops::INTERPOLATION_END != 0 {
            bits |= Self::INTERPOLATION_END;
        }
        Self(bits)
    }

    const fn with(self, flag: u16) -> Self {
        Self(self.0 | flag)
    }

    const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn contains_hard(self, lexeme: Lexeme, symbol: Option<Symbol>) -> bool {
        matches!(
            symbol,
            Some(Symbol::RightParen) if self.0 & Self::RIGHT_PAREN != 0
        ) || matches!(
            symbol,
            Some(Symbol::Greater) if self.0 & Self::GREATER != 0
        ) || matches!(
            symbol,
            Some(Symbol::LeftBrace) if self.0 & Self::LEFT_BRACE != 0
        ) || matches!(
            symbol,
            Some(Symbol::RightBrace) if self.0 & Self::RIGHT_BRACE != 0
        ) || matches!(
            symbol,
            Some(Symbol::RightBracket) if self.0 & Self::RIGHT_BRACKET != 0
        ) || (self.0 & Self::BLOCK_ELEMENT != 0
            && matches!(
                lexeme.kind(),
                LexemeKind::Token(TokenKind::Keyword(Keyword::Val | Keyword::Var))
            ))
            || (self.0 & Self::BLOCK_ELEMENT != 0 && unsupported_block_element_kind(lexeme.kind()))
            || (self.0 & Self::INTERPOLATION_END != 0
                && matches!(
                    lexeme.kind(),
                    LexemeKind::Token(TokenKind::InterpolationEnd)
                ))
    }

    fn contains_soft(self, lexeme: Lexeme, symbol: Option<Symbol>) -> bool {
        matches!(symbol, Some(Symbol::Comma) if self.0 & Self::COMMA != 0)
            || matches!(symbol, Some(Symbol::Colon) if self.0 & Self::COLON != 0)
            || matches!(symbol, Some(Symbol::Equal) if self.0 & Self::EQUAL != 0)
            || matches!(
                symbol,
                Some(Symbol::LeftParen) if self.0 & Self::LEFT_PAREN != 0
            )
            || matches!(symbol, Some(Symbol::Semicolon) if self.0 & Self::FILE.0 != 0)
            || matches!(symbol, Some(Symbol::Semicolon) if self.0 & Self::SEMICOLON != 0)
            || (self.0 & Self::CLASS_MEMBER != 0 && class_member_start_kind(lexeme.kind()))
            || (self.0 & Self::ENUM_VARIANT != 0
                && matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier)))
            || (self.0 & Self::FILE.0 != 0 && file_construct_start_kind(lexeme.kind()))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::lex, source::SourceMap};

    #[test]
    fn lexical_recovery_indexes_only_strings_that_own_invalid_escapes() {
        let text = r#""valid" "bad\q" "${"nested\z"}""#;
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("invalid-string-escape-owners.ko", text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let index = LexicalRecoveryIndex::new(text, &lexed).expect("recoveries must index");
        let openers = lexed
            .lexemes()
            .iter()
            .filter(|lexeme| matches!(lexeme.kind(), LexemeKind::Token(TokenKind::StringStart)))
            .map(|lexeme| lexeme.span().start())
            .collect::<Vec<_>>();
        let closers = lexed
            .lexemes()
            .iter()
            .filter(|lexeme| matches!(lexeme.kind(), LexemeKind::Token(TokenKind::StringEnd)))
            .map(|lexeme| lexeme.span().end())
            .collect::<Vec<_>>();

        assert_eq!(openers.len(), 4);
        assert_eq!(closers.len(), 4);
        assert_eq!(index.string_owner_end(openers[0]), Some(closers[0]));
        assert_eq!(index.string_owner_end(openers[1]), Some(closers[1]));
        assert_eq!(index.string_owner_end(openers[2]), Some(closers[3]));
        assert_eq!(index.string_owner_end(openers[3]), Some(closers[2]));
        assert_eq!(index.lexical_poison_string_recovery_end(openers[0]), None);
        assert_eq!(
            index.lexical_poison_string_recovery_end(openers[1]),
            Some(closers[1])
        );
        assert_eq!(
            index.lexical_poison_string_recovery_end(openers[2]),
            Some(closers[3])
        );
        assert_eq!(
            index.lexical_poison_string_recovery_end(openers[3]),
            Some(closers[2])
        );
    }

    #[test]
    fn function_suffix_transfers_complete_string_poison_to_the_lexer_owner() {
        for (text, expected) in [
            (r#"fun f() "a\q""#, vec!["L0006"]),
            (r#"fun f() "${"bad\q"}""#, vec!["L0006"]),
            ("fun f() \"${\"inner\n}tail\"", vec!["L0004"]),
            (r#"fun f(): "a\q""#, vec!["L0006"]),
            (r#"fun f(): "${"bad\q"}""#, vec!["L0006"]),
            ("fun f(): \"${\"inner\n}tail\"", vec!["L0004"]),
            (r#"fun f(): "ok""#, vec!["L0014"]),
        ] {
            let mut sources = SourceMap::new();
            let source_id = sources
                .add_source("function-string-suffix.ko", text)
                .expect("test source name must be unique");
            let lexed = lex(&sources, source_id).expect("test source must lex");
            let parsed = parse_declaration(&sources, &lexed)
                .expect("lexical poison must remain a user diagnostic");
            let actual = parsed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{text:?}: {:?}", parsed.diagnostics());

            let Item::Function { form, .. } = parsed
                .ast()
                .items()
                .get(parsed.root())
                .expect("function root")
                .payload()
            else {
                panic!("{text:?}: function root")
            };
            if text.starts_with("fun f():") {
                let FunctionForm::Explicit { type_ref, .. } = form else {
                    panic!("{text:?}: explicit form")
                };
                assert!(matches!(
                    parsed
                        .ast()
                        .type_refs()
                        .get(*type_ref)
                        .expect("return type")
                        .payload(),
                    TypeRef::Error
                ));
            } else {
                assert!(matches!(form, FunctionForm::ImplicitUnitAbsent));
            }
        }
    }

    fn recovery_metrics(regions: usize) -> (usize, usize, usize) {
        let region = " /* dense */ ( [ \"outer ${ [ \"bad\n next ] } tail\" ] ) // trivia\n ";
        let text = format!(
            "fun f(p: T = [ {} \"terminal ${{ value",
            region.repeat(regions)
        );

        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("declaration-recovery.ko", &text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let source = sources
            .source_text(source_id)
            .expect("test source must remain available");
        validate_lexemes(&sources, &lexed, source.len()).expect("lexer output must be valid");

        let lexical_recoveries =
            LexicalRecoveryIndex::new(source, &lexed).expect("recoveries must index");
        let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
        let lambda_headers =
            LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
                .expect("headers must index");
        let mut parser = Parser {
            sources: &sources,
            lexed: &lexed,
            lexical_recoveries,
            strict_trials,
            lambda_headers,
            index: 0,
            next_terminal_recovery_event: 0,
            recursion_depth: 0,
            file_mode: false,
            ast: ExpressionAst::new(source_id),
            diagnostics: Vec::new(),
            declaration_recovery_raw_visits: 0,
            declaration_recovery_event_queries_and_applications: 0,
            block_dispatch_iterations: 0,
            lambda_body_dispatch_iterations: 0,
            significant_raw_visits: Cell::new(0),
        };
        parser
            .parse_declaration_root()
            .expect("recovered declaration must parse");
        let unsupported_default = codes::catalog()
            .expect("diagnostic catalog must be valid")
            .resolve(codes::UNSUPPORTED_PARAMETER_DEFAULT)
            .expect("unsupported-default code must exist");
        assert_eq!(parser.diagnostics.len(), 1);
        assert_eq!(parser.diagnostics[0].code(), unsupported_default);

        (
            lexed.lexemes().len(),
            parser.declaration_recovery_raw_visits,
            parser.declaration_recovery_event_queries_and_applications,
        )
    }

    #[test]
    fn declaration_recovery_visits_raw_lexemes_and_terminal_events_linearly() {
        let (small_raw, small_visits, small_events) = recovery_metrics(16);
        let (large_raw, large_visits, large_events) = recovery_metrics(32);
        let small_work = small_visits + small_events;
        let large_work = large_visits + large_events;

        assert!(small_visits > 0);
        assert!(small_visits <= small_raw);
        assert!(large_visits <= large_raw);
        assert!(small_work <= small_raw * 3);
        assert!(large_work <= large_raw * 3);
        assert!(large_work <= small_work * 2 + 8);
    }

    fn parse_class_family_metrics(text: &str) -> (usize, usize, usize) {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("class-family-linear.ko", text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let source = sources
            .source_text(source_id)
            .expect("test source must remain available");
        validate_lexemes(&sources, &lexed, source.len()).expect("lexer output must be valid");
        let lexical_recoveries =
            LexicalRecoveryIndex::new(source, &lexed).expect("recoveries must index");
        let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
        let lambda_headers =
            LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
                .expect("headers must index");
        let mut parser = Parser {
            sources: &sources,
            lexed: &lexed,
            lexical_recoveries,
            strict_trials,
            lambda_headers,
            index: 0,
            next_terminal_recovery_event: 0,
            recursion_depth: 0,
            file_mode: false,
            ast: ExpressionAst::new(source_id),
            diagnostics: Vec::new(),
            declaration_recovery_raw_visits: 0,
            declaration_recovery_event_queries_and_applications: 0,
            block_dispatch_iterations: 0,
            lambda_body_dispatch_iterations: 0,
            significant_raw_visits: Cell::new(0),
        };
        parser
            .parse_declaration_root()
            .expect("class family must parse");
        assert!(parser.diagnostics.is_empty(), "{:?}", parser.diagnostics);
        (
            lexed.lexemes().len(),
            parser.significant_raw_visits.get(),
            parser.ast.items().len(),
        )
    }

    fn class_family_metrics(members: usize) -> (usize, usize, usize) {
        let text = format!("class C {{ {} }}", "fun f(): Unit; ".repeat(members));
        parse_class_family_metrics(&text)
    }

    #[test]
    fn class_family_member_dispatch_stays_linear_when_doubled() {
        let (small_raw, small_visits, small_items) = class_family_metrics(32);
        let (large_raw, large_visits, large_items) = class_family_metrics(64);
        assert_eq!(small_items, 33);
        assert_eq!(large_items, 65);
        assert!(small_visits <= small_raw * 24);
        assert!(large_visits <= large_raw * 24);
        assert!(large_visits <= small_visits * 2 + 64);
    }

    fn class_family_inline_sequence_metrics(variants: bool, elements: usize) -> (usize, usize) {
        let entries = (0..elements)
            .map(|index| {
                if variants {
                    format!("V{index}")
                } else {
                    format!("val f{index}: Int")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let text = if variants {
            format!("enum class E {{ {entries} }}")
        } else {
            format!("class C({entries})")
        };
        let (raw, visits, _) = parse_class_family_metrics(&text);
        (raw, visits)
    }

    #[test]
    fn class_family_field_and_variant_sequences_stay_linear_when_doubled() {
        for variants in [false, true] {
            let (small_raw, small_visits) = class_family_inline_sequence_metrics(variants, 32);
            let (large_raw, large_visits) = class_family_inline_sequence_metrics(variants, 64);
            assert!(small_visits <= small_raw * 24);
            assert!(large_visits <= large_raw * 24);
            assert!(large_visits <= small_visits * 2 + 64);
        }
    }

    #[test]
    fn interface_delegation_sequences_stay_linear_when_doubled() {
        let metrics = |entries| {
            let supertypes = (0..entries)
                .map(|index| format!("I{index} by delegate"))
                .collect::<Vec<_>>()
                .join(", ");
            let text = format!("class C(val delegate: Impl): {supertypes}");
            let (raw, visits, _) = parse_class_family_metrics(&text);
            (raw, visits)
        };
        let (small_raw, small_visits) = metrics(32);
        let (large_raw, large_visits) = metrics(64);
        assert!(small_visits <= small_raw * 24);
        assert!(large_visits <= large_raw * 24);
        assert!(large_visits <= small_visits * 2 + 64);
    }

    fn call_recovery_metrics(regions: usize) -> (usize, usize, usize, usize, usize) {
        let text = format!("f({}tail)", "@ \"bad\n, ".repeat(regions));
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("call-recovery.ko", &text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let source = sources
            .source_text(source_id)
            .expect("test source must remain available");
        validate_lexemes(&sources, &lexed, source.len()).expect("lexer output must be valid");

        let lexical_recoveries =
            LexicalRecoveryIndex::new(source, &lexed).expect("recoveries must index");
        let terminal_events = lexical_recoveries.terminal_owner_events.len();
        let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
        let lambda_headers =
            LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
                .expect("headers must index");
        let mut parser = Parser {
            sources: &sources,
            lexed: &lexed,
            lexical_recoveries,
            strict_trials,
            lambda_headers,
            index: 0,
            next_terminal_recovery_event: 0,
            recursion_depth: 0,
            file_mode: false,
            ast: ExpressionAst::new(source_id),
            diagnostics: Vec::new(),
            declaration_recovery_raw_visits: 0,
            declaration_recovery_event_queries_and_applications: 0,
            block_dispatch_iterations: 0,
            lambda_body_dispatch_iterations: 0,
            significant_raw_visits: Cell::new(0),
        };
        let root = parser
            .parse_expression_bp(0, Stops::ROOT)
            .expect("recovered call must parse");
        parser
            .consume_expression_tail(root, Stops::ROOT)
            .expect("recovered call tail must parse");

        assert_eq!(terminal_events, regions);
        assert_eq!(parser.next_terminal_recovery_event, terminal_events);
        assert_eq!(parser.diagnostics.len(), regions);
        assert!(
            parser
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code().to_string() == "L0033")
        );

        (
            lexed.lexemes().len(),
            parser.declaration_recovery_raw_visits,
            parser.declaration_recovery_event_queries_and_applications,
            terminal_events,
            parser.diagnostics.len(),
        )
    }

    #[test]
    fn call_recovery_with_many_terminal_owners_stays_linear() {
        let (small_raw, small_visits, small_event_work, small_events, small_diagnostics) =
            call_recovery_metrics(16);
        let (large_raw, large_visits, large_event_work, large_events, large_diagnostics) =
            call_recovery_metrics(32);
        let small_work = small_visits + small_event_work;
        let large_work = large_visits + large_event_work;

        assert_eq!(large_events, small_events * 2);
        assert_eq!(large_diagnostics, small_diagnostics * 2);
        assert!(small_visits <= small_raw);
        assert!(large_visits <= large_raw);
        assert!(small_work <= small_raw * 3);
        assert!(large_work <= large_raw * 3);
        assert!(large_work <= small_work * 2 + 8);
    }

    #[test]
    fn lambda_local_recovery_preserves_inherited_hard_closers() {
        for text in [
            "f({ val + )",
            "f({ val x: A<B )",
            "f({ { val + )",
            "a[{ val + ]",
        ] {
            let mut sources = SourceMap::new();
            let source_id = sources
                .add_source("lambda-local-recovery.ko", text)
                .expect("test source name must be unique");
            let lexed = lex(&sources, source_id).expect("test source must lex");
            let parsed = parse(&sources, &lexed).expect("recovery must remain a user diagnostic");
            assert!(
                matches!(
                    parsed
                        .ast()
                        .expressions()
                        .get(parsed.root())
                        .expect("root expression")
                        .payload(),
                    Expression::Call { .. } | Expression::Index { .. }
                ),
                "{text:?} must leave the inherited closer for its caller"
            );
            assert!(
                parsed.diagnostics().iter().all(|diagnostic| {
                    !matches!(diagnostic.code().to_string().as_str(), "L0013" | "L0029")
                }),
                "{text:?}: {:?}",
                parsed.diagnostics()
            );
        }
    }

    fn postfix_propagation_metrics(questions: usize) -> (usize, usize) {
        let text = format!("result{}", "?".repeat(questions));
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("postfix-propagation.ko", &text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let lexical_recoveries =
            LexicalRecoveryIndex::new(&text, &lexed).expect("recoveries must index");
        let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
        let lambda_headers =
            LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
                .expect("headers must index");
        let mut parser = Parser {
            sources: &sources,
            lexed: &lexed,
            lexical_recoveries,
            strict_trials,
            lambda_headers,
            index: 0,
            next_terminal_recovery_event: 0,
            recursion_depth: 0,
            file_mode: false,
            ast: ExpressionAst::new(source_id),
            diagnostics: Vec::new(),
            declaration_recovery_raw_visits: 0,
            declaration_recovery_event_queries_and_applications: 0,
            block_dispatch_iterations: 0,
            lambda_body_dispatch_iterations: 0,
            significant_raw_visits: Cell::new(0),
        };
        let root = parser
            .parse_expression_bp(0, Stops::ROOT)
            .expect("propagation chain must parse");
        parser
            .consume_expression_tail(root, Stops::ROOT)
            .expect("propagation tail must parse");
        assert!(parser.diagnostics.is_empty());
        (lexed.lexemes().len(), parser.significant_raw_visits.get())
    }

    #[test]
    fn postfix_propagation_significant_visits_stay_linear_when_doubled() {
        let (small_raw, small_visits) = postfix_propagation_metrics(256);
        let (large_raw, large_visits) = postfix_propagation_metrics(512);
        assert!(small_visits <= small_raw * 16);
        assert!(large_visits <= large_raw * 16);
        assert!(large_visits <= small_visits * 2 + 32);
    }

    fn block_dispatch_metrics(text: String) -> (usize, usize, usize, usize) {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("block-dispatch.ko", &text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let lexical_recoveries =
            LexicalRecoveryIndex::new(&text, &lexed).expect("recoveries must index");
        let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
        let lambda_headers =
            LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
                .expect("headers must index");
        let mut parser = Parser {
            sources: &sources,
            lexed: &lexed,
            lexical_recoveries,
            strict_trials,
            lambda_headers,
            index: 0,
            next_terminal_recovery_event: 0,
            recursion_depth: 0,
            file_mode: false,
            ast: ExpressionAst::new(source_id),
            diagnostics: Vec::new(),
            declaration_recovery_raw_visits: 0,
            declaration_recovery_event_queries_and_applications: 0,
            block_dispatch_iterations: 0,
            lambda_body_dispatch_iterations: 0,
            significant_raw_visits: Cell::new(0),
        };
        parser.parse_block_root().expect("block must parse");
        (
            lexed.lexemes().len(),
            parser.block_dispatch_iterations,
            parser.significant_raw_visits.get(),
            parser.diagnostics.len(),
        )
    }

    #[test]
    fn block_dispatch_legal_error_and_nested_families_stay_linear() {
        for (make, diagnostics_per_element) in [
            (|count| format!("{{ {} }}", "val x = 1 ".repeat(count)), 0),
            (|count| format!("{{ {} }}", "return ".repeat(count)), 0),
            (|count| format!("{{ {} }}", "@ ".repeat(count)), 1),
            (
                |count| format!("{}{}", "{".repeat(count), "}".repeat(count)),
                0,
            ),
        ] as [(fn(usize) -> String, usize); 4]
        {
            let (small_raw, small_iterations, small_visits, small_diagnostics) =
                block_dispatch_metrics(make(32));
            let (large_raw, large_iterations, large_visits, large_diagnostics) =
                block_dispatch_metrics(make(64));
            assert_eq!(small_diagnostics, diagnostics_per_element * 32);
            assert_eq!(large_diagnostics, diagnostics_per_element * 64);
            assert!(small_iterations <= small_raw);
            assert!(large_iterations <= large_raw);
            assert!(
                small_visits <= small_raw * 32,
                "{small_visits} > {small_raw} * 32"
            );
            assert!(
                large_visits <= large_raw * 32,
                "{large_visits} > {large_raw} * 32"
            );
            assert!(large_visits <= small_visits * 2 + 64);
        }
    }

    fn lambda_body_dispatch_metrics(text: String) -> (usize, usize, usize, usize) {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("lambda-body-dispatch.ko", &text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let lexical_diagnostics = lexed.diagnostics().len();
        let lexical_recoveries =
            LexicalRecoveryIndex::new(&text, &lexed).expect("recoveries must index");
        let strict_trials = StrictCallTrialIndex::new(&lexed).expect("trials must index");
        let lambda_headers =
            LambdaHeaderIndex::new(&lexed, &lexical_recoveries.terminal_owner_events)
                .expect("headers must index");
        let mut parser = Parser {
            sources: &sources,
            lexed: &lexed,
            lexical_recoveries,
            strict_trials,
            lambda_headers,
            index: 0,
            next_terminal_recovery_event: 0,
            recursion_depth: 0,
            file_mode: false,
            ast: ExpressionAst::new(source_id),
            diagnostics: Vec::new(),
            declaration_recovery_raw_visits: 0,
            declaration_recovery_event_queries_and_applications: 0,
            block_dispatch_iterations: 0,
            lambda_body_dispatch_iterations: 0,
            significant_raw_visits: Cell::new(0),
        };
        let root = parser
            .parse_expression_bp(0, Stops::ROOT)
            .expect("lambda must parse");
        parser
            .consume_expression_tail(root, Stops::ROOT)
            .expect("lambda tail must parse");

        (
            lexed.lexemes().len(),
            parser.lambda_body_dispatch_iterations,
            parser.significant_raw_visits.get(),
            lexical_diagnostics + parser.diagnostics.len(),
        )
    }

    #[test]
    fn lambda_body_legal_unsupported_and_poison_families_stay_linear() {
        for (make, diagnostics_per_element) in [
            (|count| format!("{{ {} }}", "{} ".repeat(count)), 0),
            (|count| format!("{{ {} }}", "return ".repeat(count)), 0),
            (|count| format!("{{ {} }}", "@ ".repeat(count)), 1),
        ] as [(fn(usize) -> String, usize); 3]
        {
            let (small_raw, small_iterations, small_visits, small_diagnostics) =
                lambda_body_dispatch_metrics(make(32));
            let (large_raw, large_iterations, large_visits, large_diagnostics) =
                lambda_body_dispatch_metrics(make(64));

            assert_eq!(small_iterations, 32);
            assert_eq!(large_iterations, 64);
            assert_eq!(small_diagnostics, diagnostics_per_element * 32);
            assert_eq!(large_diagnostics, diagnostics_per_element * 64);
            assert!(
                small_visits <= small_raw * 34,
                "{small_visits} > {small_raw} * 34"
            );
            assert!(
                large_visits <= large_raw * 34,
                "{large_visits} > {large_raw} * 34"
            );
            assert!(large_visits <= small_visits * 2 + 64);
        }
    }

    #[test]
    fn terminal_owner_events_preserve_boundary_order_and_exact_owner() {
        let text = concat!(
            "fun f(p: T = [ \"outer ${ \"bad\n next } tail\", ",
            "\"outer ${ \"bad\n next } tail\", ",
            "\"terminal ${ value"
        );
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("terminal-owner-events.ko", text)
            .expect("test source name must be unique");
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let index = LexicalRecoveryIndex::new(text, &lexed).expect("events must index");
        let bad_openers = text
            .match_indices("\"bad")
            .map(|(offset, _)| offset)
            .collect::<Vec<_>>();
        let terminal_interpolation = text.rfind("${").expect("terminal interpolation must exist");

        assert_eq!(index.terminal_owner_events.len(), 4);
        for (event, opener) in index.terminal_owner_events[..2].iter().zip(bad_openers) {
            assert_eq!(event.kind, TerminalOwnerKind::String);
            assert_eq!(event.opener, opener);
            assert_eq!(event.offset, text[opener..].find('\n').unwrap() + opener);
        }
        assert_eq!(
            index.terminal_owner_events[2],
            TerminalOwnerEvent {
                offset: text.len(),
                kind: TerminalOwnerKind::Interpolation,
                opener: terminal_interpolation,
            }
        );
        assert_eq!(
            index.terminal_owner_events[3],
            TerminalOwnerEvent {
                offset: text.len(),
                kind: TerminalOwnerKind::String,
                opener: text[..terminal_interpolation].rfind('"').unwrap(),
            }
        );

        let nested_eof_text = "fun f(x:T = \"${\"inner";
        let mut nested_sources = SourceMap::new();
        let nested_id = nested_sources
            .add_source("nested-terminal-owners.ko", nested_eof_text)
            .expect("test source name must be unique");
        let nested_lexed = lex(&nested_sources, nested_id).expect("test source must lex");
        let nested_index = LexicalRecoveryIndex::new(nested_eof_text, &nested_lexed)
            .expect("suppressed outer owners must index");
        let outer_string = nested_eof_text.find('"').unwrap();
        let interpolation = nested_eof_text.find("${").unwrap();
        let inner_string = nested_eof_text.rfind('"').unwrap();
        assert_eq!(
            nested_index.terminal_owner_events,
            [
                TerminalOwnerEvent {
                    offset: nested_eof_text.len(),
                    kind: TerminalOwnerKind::String,
                    opener: inner_string,
                },
                TerminalOwnerEvent {
                    offset: nested_eof_text.len(),
                    kind: TerminalOwnerKind::Interpolation,
                    opener: interpolation,
                },
                TerminalOwnerEvent {
                    offset: nested_eof_text.len(),
                    kind: TerminalOwnerKind::String,
                    opener: outer_string,
                },
            ]
        );

        let escape_text = "fun f(p: T = [ \"terminal\\";
        let mut escape_sources = SourceMap::new();
        let escape_id = escape_sources
            .add_source("terminal-escape-owner.ko", escape_text)
            .expect("test source name must be unique");
        let escape_lexed = lex(&escape_sources, escape_id).expect("test source must lex");
        let escape_index = LexicalRecoveryIndex::new(escape_text, &escape_lexed)
            .expect("terminal escape must index");
        assert_eq!(
            escape_index.terminal_owner_events,
            [TerminalOwnerEvent {
                offset: escape_text.len(),
                kind: TerminalOwnerKind::String,
                opener: escape_text.find('"').unwrap(),
            }]
        );
    }
}
