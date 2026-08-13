use crate::{
    ast::{ExpressionId, ItemId, TypeRefId},
    diagnostic::{Diagnostic, DiagnosticCode, Severity, codes, ordered_diagnostics},
    lexer::{Keyword, LexedFile, Lexeme, LexemeKind, Symbol, TokenKind},
    source::{SourceMap, Span},
};

use super::trial::{CallTrial, StrictCallTrialIndex};
use super::{
    AssignmentOperator, BinaryOperator, CastOperator, Expression, ExpressionAst, Item, LiteralKind,
    MAX_RECURSION_DEPTH, NameMarker, ParsedDeclaration, ParsedExpression, ParserInternalError,
    PrefixOperator, StringPart, TypeParameter, TypePathSegment, TypeRef, ValueParameter,
    VariableKind,
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

    let mut parser = Parser {
        sources,
        lexed,
        lexical_recoveries,
        strict_trials,
        index: 0,
        recursion_depth: 0,
        ast: ExpressionAst::new(lexed.source_id()),
        diagnostics: Vec::new(),
        #[cfg(test)]
        declaration_recovery_raw_visits: 0,
        #[cfg(test)]
        declaration_recovery_event_queries_and_applications: 0,
    };
    let root = parser.parse_expression_bp(0, Stops::ROOT)?;
    let root = parser.consume_expression_tail(root, Stops::ROOT)?;

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
    let mut parser = Parser {
        sources,
        lexed,
        lexical_recoveries,
        strict_trials,
        index: 0,
        recursion_depth: 0,
        ast: ExpressionAst::new(lexed.source_id()),
        diagnostics: Vec::new(),
        #[cfg(test)]
        declaration_recovery_raw_visits: 0,
        #[cfg(test)]
        declaration_recovery_event_queries_and_applications: 0,
    };
    let root = parser.parse_declaration_root()?;

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
    string_recoveries: Vec<(usize, usize)>,
    terminal_string_ends: Vec<usize>,
    unterminated_interpolation_starts: Vec<usize>,
    terminal_error_at_eof: bool,
    terminal_owner_events: Vec<TerminalOwnerEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalOwnerKind {
    String,
    Interpolation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalOwnerEvent {
    offset: usize,
    kind: TerminalOwnerKind,
    opener: usize,
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
            if code == invalid_string_escape
                && span.end() == span.start() + 1
                && (span.end() == source.len() || source[span.end()..].starts_with(['\r', '\n']))
            {
                terminal_escape_spans.push((span.start(), span.end()));
                terminal_error_at_eof |= span.end() == source.len();
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
        let mut active_strings = Vec::new();
        let mut next_exit = 0usize;
        for lexeme in lexed.lexemes() {
            while string_exit_events
                .get(next_exit)
                .is_some_and(|(end, _)| *end <= lexeme.span().start())
            {
                let (_, owner) = string_exit_events[next_exit];
                if lexeme.span().start() == source.len() {
                    for active_owner in active_strings.iter().copied() {
                        string_recoveries.push((active_owner, source.len()));
                    }
                }
                pop_string_owner(&mut active_strings, owner)?;
                next_exit += 1;
            }

            match lexeme.kind() {
                LexemeKind::Token(TokenKind::StringStart) => {
                    active_strings.push(lexeme.span().start());
                }
                LexemeKind::Token(TokenKind::StringEnd) => {
                    active_strings
                        .pop()
                        .ok_or(ParserInternalError::InvalidLexemeStream)?;
                }
                LexemeKind::Token(TokenKind::InterpolationStart)
                    if unterminated_interpolation_starts
                        .binary_search(&lexeme.span().start())
                        .is_ok() =>
                {
                    for owner in active_strings.iter().copied() {
                        string_recoveries.push((owner, source.len()));
                    }
                }
                LexemeKind::Invalid(_)
                    if terminal_escape_spans
                        .binary_search(&(lexeme.span().start(), lexeme.span().end()))
                        .is_ok() =>
                {
                    let end = lexeme.span().end();
                    if end == source.len() {
                        for owner in active_strings.iter().copied() {
                            string_recoveries.push((owner, end));
                        }
                    } else if let Some(owner) = active_strings.last().copied() {
                        string_recoveries.push((owner, end));
                    }
                    if let Some(owner) = active_strings.last().copied() {
                        pop_string_owner(&mut active_strings, owner)?;
                    }
                }
                LexemeKind::Invalid(_)
                    if terminal_other_spans
                        .binary_search(&(lexeme.span().start(), lexeme.span().end()))
                        .is_ok() =>
                {
                    for owner in active_strings.iter().copied() {
                        string_recoveries.push((owner, source.len()));
                    }
                }
                _ => {}
            }
        }

        string_recoveries.sort_unstable();
        string_recoveries.dedup_by_key(|(start, _)| *start);
        let mut terminal_string_ends = string_recoveries
            .iter()
            .map(|(_, end)| *end)
            .collect::<Vec<_>>();
        terminal_string_ends.sort_unstable();
        terminal_string_ends.dedup();

        Ok(Self {
            source_len: source.len(),
            string_recoveries,
            terminal_string_ends,
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
}

fn pop_string_owner(
    active_strings: &mut Vec<usize>,
    owner: usize,
) -> Result<(), ParserInternalError> {
    if active_strings.last().copied() != Some(owner) {
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
    index: usize,
    recursion_depth: usize,
    ast: ExpressionAst,
    diagnostics: Vec<Diagnostic>,
    #[cfg(test)]
    declaration_recovery_raw_visits: usize,
    #[cfg(test)]
    declaration_recovery_event_queries_and_applications: usize,
}

impl Parser<'_> {
    fn parse_declaration_root(&mut self) -> Result<ItemId, ParserInternalError> {
        let root = if self.current_is_keyword(Keyword::Val) {
            let keyword = self.bump()?.span();
            self.parse_variable_declaration(keyword, VariableKind::Val)?
        } else if self.current_is_keyword(Keyword::Var) {
            let keyword = self.bump()?.span();
            self.parse_variable_declaration(keyword, VariableKind::Var)?
        } else if self.current_is_keyword(Keyword::Const) {
            self.parse_constant_declaration()?
        } else if self.current_is_keyword(Keyword::Fun) {
            self.parse_function_declaration()?
        } else {
            let current = self.current()?;
            let span = if matches!(current.kind(), LexemeKind::Eof) {
                self.empty_at(current.span().start())?
            } else {
                self.bump()?.span()
            };
            if !self.is_poison_kind(current.kind()) {
                self.emit(codes::EXPECTED_DECLARATION, "expected declaration", span)?;
            }
            self.add_item(span, Item::Error)?
        };
        self.consume_declaration_tail(root)
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

    fn parse_constant_declaration(&mut self) -> Result<ItemId, ParserInternalError> {
        let const_span = self.bump()?.span();
        let val_marker = if self.current_is_keyword(Keyword::Val) {
            NameMarker::Present(self.bump()?.span())
        } else {
            let current = self.current()?;
            let primary = if matches!(current.kind(), LexemeKind::Eof) {
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
            if self.current_is_keyword(Keyword::Var) {
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
        let (return_colon_span, return_type) = self.parse_required_return_type()?;
        let (body_equals_span, body) = if self.current_is_symbol(Symbol::Equal) {
            let equals = self.bump()?.span();
            let expression = self.parse_expression_bp(0, Stops::ROOT)?;
            (Some(equals), Some(expression))
        } else {
            (None, None)
        };
        let end = body
            .map(|id| self.expression_span(id).map(Span::end))
            .transpose()?
            .unwrap_or(self.type_span(return_type)?.end())
            .max(fun_span.end());
        self.add_item(
            self.span(fun_span.start(), end)?,
            Item::Function {
                name,
                type_parameters,
                type_parameter_list_span,
                parameters,
                return_colon_span,
                return_type,
                body_equals_span,
                body,
            },
        )
    }

    fn parse_name_marker(
        &mut self,
        code: &str,
        message: &'static str,
        context: NameContext,
    ) -> Result<NameMarker, ParserInternalError> {
        if self.current_is_identifier() {
            return Ok(NameMarker::Present(self.bump()?.span()));
        }
        let current = self.current()?;
        if self.is_poison() {
            return Ok(NameMarker::Error(self.bump()?.span()));
        }
        let boundary = matches!(current.kind(), LexemeKind::Eof) || context.is_stop(current);
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
            let end = self.recover_declaration_region(context.recovery_stops())?;
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
        let type_ref = self.parse_type_ref(TypeStops::empty().with(TypeStops::EQUAL))?;
        Ok((Some(colon), Some(type_ref)))
    }

    fn parse_required_initializer(&mut self) -> Result<(Span, ExpressionId), ParserInternalError> {
        if self.current_is_symbol(Symbol::Equal) {
            let equals = self.bump()?.span();
            let initializer = self.parse_expression_bp(0, Stops::ROOT)?;
            return Ok((equals, initializer));
        }
        let current = self.current()?;
        let primary = if matches!(current.kind(), LexemeKind::Eof) {
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
            let initializer = self.parse_expression_bp(0, Stops::ROOT)?;
            return Ok((equals, initializer));
        }
        let error_span = if matches!(current.kind(), LexemeKind::Eof) {
            primary
        } else {
            let start = current.span().start();
            let end = self.recover_declaration_region(DeclarationStops::EMPTY)?;
            self.span(start, end.max(start))?
        };
        let initializer = self.add_expression(error_span, Expression::Error)?;
        Ok((equals, initializer))
    }

    fn parse_required_return_type(&mut self) -> Result<(Span, TypeRefId), ParserInternalError> {
        if self.current_is_symbol(Symbol::Colon) {
            let colon = self.bump()?.span();
            let type_ref = self.parse_type_ref(TypeStops::empty().with(TypeStops::EQUAL))?;
            return Ok((colon, type_ref));
        }
        let current = self.current()?;
        let primary = if matches!(current.kind(), LexemeKind::Eof)
            || self.current_is_symbol(Symbol::Equal)
            || self.current_is_symbol(Symbol::LeftBrace)
        {
            self.empty_at(current.span().start())?
        } else {
            current.span()
        };
        let terminal_lexer_root = matches!(current.kind(), LexemeKind::Eof)
            && self.lexical_recoveries.terminal_error_at_eof;
        if !self.is_poison_kind(current.kind()) && !terminal_lexer_root {
            self.emit(
                codes::EXPECTED_RETURN_TYPE,
                "expected explicit return type",
                primary,
            )?;
        }
        let colon = self.empty_at(current.span().start())?;
        let type_ref = if self.can_start_type_ref(current) {
            self.parse_type_ref(TypeStops::empty().with(TypeStops::EQUAL))?
        } else {
            let span = if matches!(current.kind(), LexemeKind::Eof)
                || self.current_is_symbol(Symbol::Equal)
                || self.current_is_symbol(Symbol::LeftBrace)
            {
                primary
            } else {
                let start = current.span().start();
                let end = self.recover_declaration_region(
                    DeclarationStops::EMPTY
                        .with(DeclarationStops::EQUAL)
                        .with(DeclarationStops::LEFT_BRACE),
                )?;
                self.span(start, end.max(start))?
            };
            self.add_type_ref(span, TypeRef::Error)?
        };
        Ok((colon, type_ref))
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
            let name = self.parse_name_marker(
                codes::EXPECTED_PARAMETER_NAME,
                "expected parameter name",
                NameContext::ValueParameter,
            )?;
            let start = marker_span(name).start();
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
            let end = self.type_span(type_ref)?.end();
            parameters.push(ValueParameter {
                span: self.span(start, end)?,
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
        let mut next_terminal_event = self
            .lexical_recoveries
            .terminal_owner_events
            .partition_point(|event| event.offset <= recovery_start);
        #[cfg(test)]
        {
            // `partition_point` 是一次有界起点定位；其库内比较次数不参与逐 lexeme 计数。
            self.declaration_recovery_event_queries_and_applications += 1;
        }
        let mut end = recovery_start;
        let mut next_significant = Some(first);

        loop {
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
                    return Err(ParserInternalError::InvalidLexemeStream);
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
            let hard_stop = outside_owner && stops.contains_hard(symbol);
            if hard_stop || (outside_owner && delimiters.is_empty() && stops.contains_soft(symbol))
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
        Ok(end)
    }

    fn consume_declaration_tail(&mut self, item: ItemId) -> Result<ItemId, ParserInternalError> {
        if matches!(self.current()?.kind(), LexemeKind::Eof) {
            return Ok(item);
        }
        let first = self.current()?.span();
        self.emit(
            codes::UNEXPECTED_TRAILING_TOKEN,
            "unexpected trailing token",
            first,
        )?;
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
        if stops.contains(current) {
            let span = self.empty_at(current.span().start())?;
            self.emit(codes::EXPECTED_EXPRESSION, "expected expression", span)?;
            return self.add_expression(span, Expression::Error);
        }

        match current.kind() {
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

    fn parse_group(&mut self, outer_stops: Stops) -> Result<ExpressionId, ParserInternalError> {
        let opener = self.bump()?.span();
        let inner = self.parse_expression_bp(0, outer_stops.with(Stops::RIGHT_PAREN))?;
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
            } else if self.current_is_symbol(Symbol::BangBang) {
                let operator_span = self.bump()?.span();
                let receiver_span = self.expression_span(receiver)?;
                self.add_expression(
                    self.span(receiver_span.start(), operator_span.end())?,
                    Expression::NonNullAssert {
                        operand: receiver,
                        operator_span,
                    },
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
        self.current_is_symbol(Symbol::Dot)
            || self.current_is_symbol(Symbol::QuestionDot)
            || self.current_is_symbol(Symbol::LeftParen)
            || self.current_is_symbol(Symbol::LeftBracket)
            || self.current_is_symbol(Symbol::BangBang)
            || self.current_is_symbol(Symbol::ColonColon)
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
        let opener = self.bump()?.span();
        let callee_span = self.expression_span(callee)?;
        let argument_stops = outer_stops.with(Stops::COMMA).with(Stops::RIGHT_PAREN);
        let mut arguments = Vec::new();
        let mut last_consumed_end = opener.end();

        if !self.current_is_symbol(Symbol::RightParen) {
            loop {
                if self.argument_form_start()? {
                    self.consume_unsupported_argument(argument_stops)?;
                    last_consumed_end = self.previous_significant_end().max(last_consumed_end);
                } else {
                    let argument = self.parse_expression_bp(0, argument_stops)?;
                    last_consumed_end = self.expression_span(argument)?.end();
                    arguments.push(argument);
                }

                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    if self.current_is_symbol(Symbol::RightParen) {
                        let span = self.empty_at(self.current()?.span().start())?;
                        self.emit(codes::EXPECTED_EXPRESSION, "expected expression", span)?;
                        arguments.push(self.add_expression(span, Expression::Error)?);
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
            arguments
                .last()
                .map(|id| self.expression_span(*id).map(Span::end))
                .transpose()?
                .unwrap_or(last_consumed_end)
                .max(last_consumed_end)
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

    fn argument_form_start(&self) -> Result<bool, ParserInternalError> {
        if self.current_is_keyword(Keyword::Own)
            || self.current_is_keyword(Keyword::Inout)
            || self.current_is_keyword(Keyword::Borrow)
        {
            return Ok(true);
        }
        if !self.current_is_identifier() {
            return Ok(false);
        }
        Ok(matches!(
            self.peek(1).map(Lexeme::kind),
            Some(LexemeKind::Token(TokenKind::Symbol(Symbol::Equal)))
        ))
    }

    fn consume_unsupported_argument(&mut self, stops: Stops) -> Result<(), ParserInternalError> {
        let first = self.bump()?;
        let diagnostic_span = if matches!(first.kind(), LexemeKind::Token(TokenKind::Identifier))
            && self.current_is_symbol(Symbol::Equal)
        {
            self.bump()?.span()
        } else {
            first.span()
        };
        self.emit(
            codes::UNSUPPORTED_ARGUMENT_FORM,
            "unsupported argument form",
            diagnostic_span,
        )?;

        let mut delimiters = Vec::new();
        let mut string_depth = 0usize;
        let mut interpolation_depth = 0usize;
        let recovery_start = self.current()?.span().start();
        let mut next_string_recovery = self
            .lexical_recoveries
            .terminal_string_ends
            .partition_point(|end| *end <= recovery_start);
        loop {
            let current = self.current()?;
            let current_offset = current.span().start();
            let recovered_string_boundary = string_depth > 0
                && self
                    .lexical_recoveries
                    .terminal_string_ends
                    .get(next_string_recovery)
                    .is_some_and(|end| *end <= current_offset);
            if recovered_string_boundary {
                string_depth -= 1;
                next_string_recovery += 1;
            }
            if matches!(current.kind(), LexemeKind::Eof) {
                break;
            }
            if stops.contains(current) && string_depth == 0 {
                let closes_owned_delimiter = matches!(
                    current.kind(),
                    LexemeKind::Token(TokenKind::Symbol(symbol))
                        if delimiters.last().is_some_and(|closer| *closer == symbol)
                );
                if closes_owned_delimiter {
                    delimiters.pop();
                    self.bump()?;
                    continue;
                }
                let owner_hard_boundary = matches!(
                    current.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen))
                        | LexemeKind::Token(TokenKind::InterpolationEnd)
                );
                if owner_hard_boundary {
                    break;
                }
                let nested_comma = self.current_is_symbol(Symbol::Comma)
                    && (!delimiters.is_empty() || interpolation_depth > 0);
                if !nested_comma {
                    break;
                }
            }
            match current.kind() {
                LexemeKind::Token(TokenKind::StringStart) => string_depth += 1,
                LexemeKind::Token(TokenKind::StringEnd) if string_depth > 0 => string_depth -= 1,
                LexemeKind::Token(TokenKind::InterpolationStart) => interpolation_depth += 1,
                LexemeKind::Token(TokenKind::InterpolationEnd) if interpolation_depth > 0 => {
                    interpolation_depth -= 1;
                }
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => {
                    delimiters.push(Symbol::RightParen);
                }
                LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBracket)) => {
                    delimiters.push(Symbol::RightBracket);
                }
                LexemeKind::Token(TokenKind::Symbol(symbol))
                    if delimiters.last().is_some_and(|closer| *closer == symbol) =>
                {
                    delimiters.pop();
                }
                _ => {}
            }
            self.bump()?;
        }
        Ok(())
    }

    fn parse_index(
        &mut self,
        receiver: ExpressionId,
        outer_stops: Stops,
    ) -> Result<ExpressionId, ParserInternalError> {
        let opener = self.bump()?.span();
        let receiver_span = self.expression_span(receiver)?;
        let inner_stops = outer_stops.with(Stops::RIGHT_BRACKET).with(Stops::COMMA);
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
                Keyword::True | Keyword::False | Keyword::Null | Keyword::This
            )) | LexemeKind::Token(TokenKind::Symbol(
                Symbol::LeftParen
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
        if string_start
            && let Some(recovery_end) = self
                .lexical_recoveries
                .string_recovery_end(current.span().start())
        {
            return self.consume_segmented_string_type_poison(recovery_end);
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
            let argument_stops = stops.with(TypeStops::COMMA).with(TypeStops::GREATER);
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
            .with(TypeStops::COMMA)
            .with(TypeStops::RIGHT_PAREN);
        if !self.current_is_symbol(Symbol::RightParen) {
            loop {
                parameters.push(self.parse_type_ref(parameter_stops)?);
                if self.current_is_symbol(Symbol::Comma) {
                    self.bump()?;
                    continue;
                }
                break;
            }
        }
        let mut last_end = parameters
            .last()
            .map(|id| self.type_span(*id).map(Span::end))
            .transpose()?
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

#[derive(Clone, Copy)]
struct Stops(u8);

impl Stops {
    const ROOT: Self = Self(0);
    const RIGHT_PAREN: u8 = 1 << 0;
    const RIGHT_BRACKET: u8 = 1 << 1;
    const COMMA: u8 = 1 << 2;
    const INTERPOLATION_END: u8 = 1 << 3;

    const fn with(self, flag: u8) -> Self {
        Self(self.0 | flag)
    }

    fn contains(self, lexeme: Lexeme) -> bool {
        match lexeme.kind() {
            LexemeKind::Eof => true,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                self.0 & Self::RIGHT_PAREN != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => {
                self.0 & Self::RIGHT_BRACKET != 0
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma)) => self.0 & Self::COMMA != 0,
            LexemeKind::Token(TokenKind::InterpolationEnd) => self.0 & Self::INTERPOLATION_END != 0,
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
struct TypeStops(u8);

impl TypeStops {
    const EQUAL: u8 = 1 << 5;

    const fn empty() -> Self {
        Self(0)
    }

    const COMMA: u8 = 1 << 0;
    const GREATER: u8 = 1 << 1;
    const RIGHT_PAREN: u8 = 1 << 2;
    const RIGHT_BRACKET: u8 = 1 << 3;
    const INTERPOLATION_END: u8 = 1 << 4;

    const fn from_expression(stops: Stops) -> Self {
        let mut bits = 0;
        if stops.0 & Stops::COMMA != 0 {
            bits |= Self::COMMA;
        }
        if stops.0 & Stops::RIGHT_PAREN != 0 {
            bits |= Self::RIGHT_PAREN;
        }
        if stops.0 & Stops::RIGHT_BRACKET != 0 {
            bits |= Self::RIGHT_BRACKET;
        }
        if stops.0 & Stops::INTERPOLATION_END != 0 {
            bits |= Self::INTERPOLATION_END;
        }
        Self(bits)
    }

    const fn with(self, flag: u8) -> Self {
        Self(self.0 | flag)
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
            _ => false,
        }
    }
}

#[derive(Clone, Copy)]
enum NameContext {
    Declaration,
    TypeParameter,
    ValueParameter,
}

impl NameContext {
    fn is_stop(self, lexeme: Lexeme) -> bool {
        matches!(
            (self, lexeme.kind()),
            (
                Self::Declaration,
                LexemeKind::Token(TokenKind::Symbol(
                    Symbol::Colon | Symbol::Equal | Symbol::LeftParen,
                )),
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

    const fn with(self, flag: u16) -> Self {
        Self(self.0 | flag)
    }

    fn contains_hard(self, symbol: Option<Symbol>) -> bool {
        matches!(
            symbol,
            Some(Symbol::RightParen) if self.0 & Self::RIGHT_PAREN != 0
        ) || matches!(
            symbol,
            Some(Symbol::Greater) if self.0 & Self::GREATER != 0
        ) || matches!(
            symbol,
            Some(Symbol::LeftBrace) if self.0 & Self::LEFT_BRACE != 0
        )
    }

    fn contains_soft(self, symbol: Option<Symbol>) -> bool {
        matches!(symbol, Some(Symbol::Comma) if self.0 & Self::COMMA != 0)
            || matches!(symbol, Some(Symbol::Colon) if self.0 & Self::COLON != 0)
            || matches!(symbol, Some(Symbol::Equal) if self.0 & Self::EQUAL != 0)
            || matches!(
                symbol,
                Some(Symbol::LeftParen) if self.0 & Self::LEFT_PAREN != 0
            )
    }
}

fn marker_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
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
        let mut parser = Parser {
            sources: &sources,
            lexed: &lexed,
            lexical_recoveries,
            strict_trials,
            index: 0,
            recursion_depth: 0,
            ast: ExpressionAst::new(source_id),
            diagnostics: Vec::new(),
            declaration_recovery_raw_visits: 0,
            declaration_recovery_event_queries_and_applications: 0,
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
