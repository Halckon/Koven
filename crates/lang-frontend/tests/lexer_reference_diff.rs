//! SPEC-0482 (F3b): Independent Guide v0.47 reference lexer machine differential tests.
//!
//! This test builds an independent state-machine tokenizer directly from Guide v0.47 §01
//! (`docs/guide/01-lexical.md`) without sharing `scanner.rs`, then compares both the
//! complete lexeme stream (kinds + UTF-8 byte spans) and diagnostic spans against
//! production `lang_frontend::lexer::lex`.

use lang_frontend::{
    lexer::{
        FloatLiteralSuffix, IntegerLiteralSuffix, InvalidKind, Keyword, LexedFile, LexemeKind,
        ReservedWord, Symbol, TokenKind, TriviaKind, lex,
    },
    source::SourceMap,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RefLexeme {
    kind: LexemeKind,
    start: usize,
    end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefDiagKind {
    UnexpectedCharacter,
    ReservedWord,
    UnterminatedBlockComment,
    UnterminatedString,
    UnterminatedInterpolation,
    InvalidStringEscape,
    InvalidCharLiteral,
    InvalidNumericLiteral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RefDiag {
    #[allow(dead_code)]
    kind: RefDiagKind,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LexProjection {
    lexemes: Vec<RefLexeme>,
    diag_spans: Vec<(usize, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Normal,
    String { start: usize },
    Interpolation { start: usize, brace_depth: usize },
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn classify_word(word: &str) -> TokenKind {
    match word {
        "class" => TokenKind::Keyword(Keyword::Class),
        "companion" => TokenKind::Keyword(Keyword::Companion),
        "const" => TokenKind::Keyword(Keyword::Const),
        "enum" => TokenKind::Keyword(Keyword::Enum),
        "extern" => TokenKind::Keyword(Keyword::Extern),
        "fun" => TokenKind::Keyword(Keyword::Fun),
        "import" => TokenKind::Keyword(Keyword::Import),
        "interface" => TokenKind::Keyword(Keyword::Interface),
        "object" => TokenKind::Keyword(Keyword::Object),
        "package" => TokenKind::Keyword(Keyword::Package),
        "typealias" => TokenKind::Keyword(Keyword::Typealias),
        "val" => TokenKind::Keyword(Keyword::Val),
        "var" => TokenKind::Keyword(Keyword::Var),
        "vararg" => TokenKind::Keyword(Keyword::Vararg),
        "break" => TokenKind::Keyword(Keyword::Break),
        "continue" => TokenKind::Keyword(Keyword::Continue),
        "else" => TokenKind::Keyword(Keyword::Else),
        "for" => TokenKind::Keyword(Keyword::For),
        "if" => TokenKind::Keyword(Keyword::If),
        "in" => TokenKind::Keyword(Keyword::In),
        "is" => TokenKind::Keyword(Keyword::Is),
        "return" => TokenKind::Keyword(Keyword::Return),
        "when" => TokenKind::Keyword(Keyword::When),
        "while" => TokenKind::Keyword(Keyword::While),
        "unsafe" => TokenKind::Keyword(Keyword::Unsafe),
        "internal" => TokenKind::Keyword(Keyword::Internal),
        "private" => TokenKind::Keyword(Keyword::Private),
        "public" => TokenKind::Keyword(Keyword::Public),
        "as" => TokenKind::Keyword(Keyword::As),
        "false" => TokenKind::Keyword(Keyword::False),
        "null" => TokenKind::Keyword(Keyword::Null),
        "operator" => TokenKind::Keyword(Keyword::Operator),
        "override" => TokenKind::Keyword(Keyword::Override),
        "super" => TokenKind::Keyword(Keyword::Super),
        "this" => TokenKind::Keyword(Keyword::This),
        "true" => TokenKind::Keyword(Keyword::True),
        "async" => TokenKind::ReservedWord(ReservedWord::Async),
        "await" => TokenKind::ReservedWord(ReservedWord::Await),
        "suspend" => TokenKind::ReservedWord(ReservedWord::Suspend),
        "actor" => TokenKind::ReservedWord(ReservedWord::Actor),
        "spawn" => TokenKind::ReservedWord(ReservedWord::Spawn),
        "sealed" => TokenKind::ReservedWord(ReservedWord::Sealed),
        "dyn" => TokenKind::ReservedWord(ReservedWord::Dyn),
        "where" => TokenKind::ReservedWord(ReservedWord::Where),
        "yield" => TokenKind::ReservedWord(ReservedWord::Yield),
        "macro" => TokenKind::ReservedWord(ReservedWord::Macro),
        "reify" => TokenKind::ReservedWord(ReservedWord::Reify),
        _ => TokenKind::Identifier,
    }
}

fn valid_radix_digits(s: &str, predicate: fn(u8) -> bool) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes[0] == b'_' || bytes[bytes.len() - 1] == b'_' {
        return false;
    }
    let mut prev_underscore = false;
    for &b in bytes {
        if b == b'_' {
            if prev_underscore {
                return false;
            }
            prev_underscore = true;
        } else if predicate(b) {
            prev_underscore = false;
        } else {
            return false;
        }
    }
    true
}

fn parse_int_suffix(suffix: &str) -> Option<IntegerLiteralSuffix> {
    match suffix {
        "" => Some(IntegerLiteralSuffix::None),
        "L" => Some(IntegerLiteralSuffix::Long),
        "u" | "U" => Some(IntegerLiteralSuffix::Unsigned),
        "uL" | "UL" => Some(IntegerLiteralSuffix::UnsignedLong),
        _ => None,
    }
}

fn scan_number(source: &str, start: usize) -> (LexemeKind, usize, Option<RefDiagKind>) {
    let bytes = source.as_bytes();
    let len = bytes.len();

    // Hex or binary integer literal: 0x... / 0X... / 0b... / 0B...
    if bytes[start] == b'0' && start + 1 < len {
        let prefix = bytes[start + 1];
        if matches!(prefix, b'x' | b'X' | b'b' | b'B') {
            let is_hex = matches!(prefix, b'x' | b'X');
            let mut end = start + 2;
            while end < len && is_ident_continue(bytes[end]) {
                end += 1;
            }
            let body = &source[start + 2..end];
            let digit_pred: fn(u8) -> bool = if is_hex {
                |b| b.is_ascii_hexdigit()
            } else {
                |b| matches!(b, b'0' | b'1')
            };
            let candidates = ["uL", "UL", "L", "u", "U", ""];
            for suf in candidates {
                if let Some(digits) = body.strip_suffix(suf)
                    && valid_radix_digits(digits, digit_pred)
                {
                    let suffix = parse_int_suffix(suf).unwrap();
                    return (
                        LexemeKind::Token(TokenKind::IntegerLiteral(suffix)),
                        end,
                        None,
                    );
                }
            }
            return (
                LexemeKind::Invalid(InvalidKind::InvalidNumericLiteral),
                end,
                Some(RefDiagKind::InvalidNumericLiteral),
            );
        }
    }

    // Decimal integer or float.
    let mut pos = start;
    while pos < len && (bytes[pos].is_ascii_digit() || bytes[pos] == b'_') {
        pos += 1;
    }
    let int_part = &source[start..pos];

    // Check for fractional part: '.' followed by optional '_' and an ASCII digit
    // (stops before `..` / `..<` and ordinary member access like `1.toString()`).
    let mut is_float = false;
    let mut frac_part = "";
    if pos + 1 < len && bytes[pos] == b'.' {
        let mut probe = pos + 1;
        while probe < len && bytes[probe] == b'_' {
            probe += 1;
        }
        if probe < len && bytes[probe].is_ascii_digit() {
            is_float = true;
            let frac_start = pos + 1;
            pos = frac_start;
            while pos < len && (bytes[pos].is_ascii_digit() || bytes[pos] == b'_') {
                pos += 1;
            }
            frac_part = &source[frac_start..pos];
        }
    }

    // Absorb any contiguous ASCII [A-Za-z0-9_]* suffix.
    let suffix_start = pos;
    while pos < len && is_ident_continue(bytes[pos]) {
        pos += 1;
    }
    let suffix = &source[suffix_start..pos];

    let dec_pred: fn(u8) -> bool = |b| b.is_ascii_digit();
    if is_float {
        let valid_parts =
            valid_radix_digits(int_part, dec_pred) && valid_radix_digits(frac_part, dec_pred);
        let float_suffix = match suffix {
            "" => Some(FloatLiteralSuffix::None),
            "f" | "F" => Some(FloatLiteralSuffix::Float),
            _ => None,
        };
        if valid_parts && let Some(fs) = float_suffix {
            return (LexemeKind::Token(TokenKind::FloatLiteral(fs)), pos, None);
        }
        return (
            LexemeKind::Invalid(InvalidKind::InvalidNumericLiteral),
            pos,
            Some(RefDiagKind::InvalidNumericLiteral),
        );
    }

    if valid_radix_digits(int_part, dec_pred) {
        if matches!(suffix, "f" | "F") {
            return (
                LexemeKind::Token(TokenKind::FloatLiteral(FloatLiteralSuffix::Float)),
                pos,
                None,
            );
        }
        if let Some(is) = parse_int_suffix(suffix) {
            return (LexemeKind::Token(TokenKind::IntegerLiteral(is)), pos, None);
        }
    }

    (
        LexemeKind::Invalid(InvalidKind::InvalidNumericLiteral),
        pos,
        Some(RefDiagKind::InvalidNumericLiteral),
    )
}

fn scan_char_literal(source: &str, start: usize) -> (LexemeKind, usize, Option<RefDiagKind>) {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut pos = start + 1;
    let mut valid_scalars = 0usize;
    let mut bad = false;

    while pos < len {
        let b = bytes[pos];
        if b == b'\'' {
            pos += 1;
            if !bad && valid_scalars == 1 {
                return (LexemeKind::Token(TokenKind::CharLiteral), pos, None);
            }
            return (
                LexemeKind::Invalid(InvalidKind::InvalidCharLiteral),
                pos,
                Some(RefDiagKind::InvalidCharLiteral),
            );
        }
        if b == b'\n' || b == b'\r' {
            return (
                LexemeKind::Invalid(InvalidKind::InvalidCharLiteral),
                pos,
                Some(RefDiagKind::InvalidCharLiteral),
            );
        }
        if b == b'\\' {
            pos += 1;
            if pos >= len || bytes[pos] == b'\n' || bytes[pos] == b'\r' {
                return (
                    LexemeKind::Invalid(InvalidKind::InvalidCharLiteral),
                    pos,
                    Some(RefDiagKind::InvalidCharLiteral),
                );
            }
            let esc = source[pos..].chars().next().unwrap();
            if !matches!(esc, '\\' | '\'' | '"' | 'n' | 'r' | 't' | '0') {
                bad = true;
            }
            pos += esc.len_utf8();
            valid_scalars += 1;
        } else {
            let ch = source[pos..].chars().next().unwrap();
            pos += ch.len_utf8();
            valid_scalars += 1;
        }
    }

    (
        LexemeKind::Invalid(InvalidKind::InvalidCharLiteral),
        pos,
        Some(RefDiagKind::InvalidCharLiteral),
    )
}

fn scan_symbol(source: &str, pos: usize) -> Option<(Symbol, usize)> {
    let rest = &source[pos..];
    let bytes = rest.as_bytes();
    if rest.starts_with("..<") {
        return Some((Symbol::DotDotLess, pos + 3));
    }
    if rest.starts_with("!in") && (bytes.len() == 3 || !is_ident_continue(bytes[3])) {
        return Some((Symbol::BangIn, pos + 3));
    }
    if rest.starts_with("!is") && (bytes.len() == 3 || !is_ident_continue(bytes[3])) {
        return Some((Symbol::BangIs, pos + 3));
    }
    let two = [
        ("?.", Symbol::QuestionDot),
        ("?:", Symbol::QuestionColon),
        ("!!", Symbol::BangBang),
        ("::", Symbol::ColonColon),
        ("->", Symbol::Arrow),
        ("..", Symbol::DotDot),
        ("<=", Symbol::LessEqual),
        (">=", Symbol::GreaterEqual),
        ("==", Symbol::EqualEqual),
        ("!=", Symbol::BangEqual),
        ("&&", Symbol::AndAnd),
        ("||", Symbol::OrOr),
        ("+=", Symbol::PlusEqual),
        ("-=", Symbol::MinusEqual),
        ("*=", Symbol::StarEqual),
        ("/=", Symbol::SlashEqual),
        ("%=", Symbol::PercentEqual),
    ];
    for (lit, sym) in two {
        if rest.starts_with(lit) {
            return Some((sym, pos + 2));
        }
    }
    let one = match bytes[0] {
        b'(' => Symbol::LeftParen,
        b')' => Symbol::RightParen,
        b'[' => Symbol::LeftBracket,
        b']' => Symbol::RightBracket,
        b'{' => Symbol::LeftBrace,
        b'}' => Symbol::RightBrace,
        b',' => Symbol::Comma,
        b':' => Symbol::Colon,
        b';' => Symbol::Semicolon,
        b'@' => Symbol::At,
        b'.' => Symbol::Dot,
        b'?' => Symbol::Question,
        b'!' => Symbol::Bang,
        b'*' => Symbol::Star,
        b'/' => Symbol::Slash,
        b'%' => Symbol::Percent,
        b'+' => Symbol::Plus,
        b'-' => Symbol::Minus,
        b'<' => Symbol::Less,
        b'>' => Symbol::Greater,
        b'&' => Symbol::Ampersand,
        b'=' => Symbol::Equal,
        _ => return None,
    };
    Some((one, pos + 1))
}

fn reference_lex(source: &str) -> (Vec<RefLexeme>, Vec<RefDiag>) {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;
    let mut modes = vec![Mode::Normal];
    let mut lexemes = Vec::new();
    let mut diags = Vec::new();
    let mut suppressed_eof_error = false;

    while pos < len {
        match *modes.last().unwrap() {
            Mode::String { start: str_start } => {
                let b = bytes[pos];
                if b == b'"' {
                    lexemes.push(RefLexeme {
                        kind: LexemeKind::Token(TokenKind::StringEnd),
                        start: pos,
                        end: pos + 1,
                    });
                    pos += 1;
                    modes.pop();
                } else if b == b'\n' || b == b'\r' {
                    diags.push(RefDiag {
                        kind: RefDiagKind::UnterminatedString,
                        start: str_start,
                        end: pos,
                    });
                    modes.pop();
                } else if b == b'$' && pos + 1 < len && bytes[pos + 1] == b'{' {
                    lexemes.push(RefLexeme {
                        kind: LexemeKind::Token(TokenKind::InterpolationStart),
                        start: pos,
                        end: pos + 2,
                    });
                    modes.push(Mode::Interpolation {
                        start: pos,
                        brace_depth: 0,
                    });
                    pos += 2;
                } else {
                    let text_start = pos;
                    let mut invalid_esc: Option<(usize, usize, bool)> = None;
                    while pos < len {
                        let cur = bytes[pos];
                        if cur == b'"'
                            || cur == b'\n'
                            || cur == b'\r'
                            || (cur == b'$' && pos + 1 < len && bytes[pos + 1] == b'{')
                        {
                            break;
                        }
                        if cur == b'\\' {
                            if pos + 1 >= len || bytes[pos + 1] == b'\n' || bytes[pos + 1] == b'\r'
                            {
                                invalid_esc = Some((pos, pos + 1, true));
                                break;
                            }
                            let esc = source[pos + 1..].chars().next().unwrap();
                            if matches!(esc, '\\' | '\'' | '"' | 'n' | 'r' | 't' | '0' | '$') {
                                pos += 1 + esc.len_utf8();
                            } else {
                                invalid_esc = Some((pos, pos + 1 + esc.len_utf8(), false));
                                break;
                            }
                        } else {
                            let ch = source[pos..].chars().next().unwrap();
                            pos += ch.len_utf8();
                        }
                    }
                    if pos > text_start {
                        lexemes.push(RefLexeme {
                            kind: LexemeKind::Token(TokenKind::StringText),
                            start: text_start,
                            end: pos,
                        });
                    }
                    if let Some((esc_start, esc_end, ends_string)) = invalid_esc {
                        lexemes.push(RefLexeme {
                            kind: LexemeKind::Invalid(InvalidKind::InvalidStringEscape),
                            start: esc_start,
                            end: esc_end,
                        });
                        diags.push(RefDiag {
                            kind: RefDiagKind::InvalidStringEscape,
                            start: esc_start,
                            end: esc_end,
                        });
                        pos = esc_end;
                        if ends_string {
                            modes.pop();
                            if pos == len {
                                suppressed_eof_error = true;
                            }
                        }
                    }
                }
            }
            Mode::Normal | Mode::Interpolation { .. } => {
                let b = bytes[pos];
                if b == b' ' || b == b'\t' {
                    let start = pos;
                    while pos < len && matches!(bytes[pos], b' ' | b'\t') {
                        pos += 1;
                    }
                    lexemes.push(RefLexeme {
                        kind: LexemeKind::Trivia(TriviaKind::Whitespace),
                        start,
                        end: pos,
                    });
                } else if b == b'\n' {
                    lexemes.push(RefLexeme {
                        kind: LexemeKind::Trivia(TriviaKind::Newline),
                        start: pos,
                        end: pos + 1,
                    });
                    pos += 1;
                } else if b == b'\r' && pos + 1 < len && bytes[pos + 1] == b'\n' {
                    lexemes.push(RefLexeme {
                        kind: LexemeKind::Trivia(TriviaKind::Newline),
                        start: pos,
                        end: pos + 2,
                    });
                    pos += 2;
                } else if source[pos..].starts_with("//") {
                    let start = pos;
                    pos += 2;
                    while pos < len && !matches!(bytes[pos], b'\n' | b'\r') {
                        pos += 1;
                    }
                    lexemes.push(RefLexeme {
                        kind: LexemeKind::Trivia(TriviaKind::LineComment),
                        start,
                        end: pos,
                    });
                } else if source[pos..].starts_with("/*") {
                    let start = pos;
                    if let Some(rel_close) = source[pos + 2..].find("*/") {
                        let end = pos + 2 + rel_close + 2;
                        lexemes.push(RefLexeme {
                            kind: LexemeKind::Trivia(TriviaKind::BlockComment),
                            start,
                            end,
                        });
                        pos = end;
                    } else {
                        lexemes.push(RefLexeme {
                            kind: LexemeKind::Invalid(InvalidKind::UnterminatedBlockComment),
                            start,
                            end: len,
                        });
                        diags.push(RefDiag {
                            kind: RefDiagKind::UnterminatedBlockComment,
                            start,
                            end: len,
                        });
                        pos = len;
                        suppressed_eof_error = true;
                    }
                } else if b == b'"' {
                    lexemes.push(RefLexeme {
                        kind: LexemeKind::Token(TokenKind::StringStart),
                        start: pos,
                        end: pos + 1,
                    });
                    modes.push(Mode::String { start: pos });
                    pos += 1;
                } else if b == b'\'' {
                    let (kind, end, diag) = scan_char_literal(source, pos);
                    lexemes.push(RefLexeme {
                        kind,
                        start: pos,
                        end,
                    });
                    if let Some(dk) = diag {
                        diags.push(RefDiag {
                            kind: dk,
                            start: pos,
                            end,
                        });
                    }
                    pos = end;
                } else if b.is_ascii_digit() {
                    let (kind, end, diag) = scan_number(source, pos);
                    lexemes.push(RefLexeme {
                        kind,
                        start: pos,
                        end,
                    });
                    if let Some(dk) = diag {
                        diags.push(RefDiag {
                            kind: dk,
                            start: pos,
                            end,
                        });
                    }
                    pos = end;
                } else if is_ident_start(b) {
                    let start = pos;
                    while pos < len && is_ident_continue(bytes[pos]) {
                        pos += 1;
                    }
                    let word = &source[start..pos];
                    if word == "as" && pos < len && bytes[pos] == b'?' {
                        pos += 1;
                        lexemes.push(RefLexeme {
                            kind: LexemeKind::Token(TokenKind::Symbol(Symbol::AsQuestion)),
                            start,
                            end: pos,
                        });
                    } else {
                        let tk = classify_word(word);
                        lexemes.push(RefLexeme {
                            kind: LexemeKind::Token(tk),
                            start,
                            end: pos,
                        });
                        if matches!(tk, TokenKind::ReservedWord(_)) {
                            diags.push(RefDiag {
                                kind: RefDiagKind::ReservedWord,
                                start,
                                end: pos,
                            });
                        }
                    }
                } else if b == b'{' {
                    if let Some(Mode::Interpolation { brace_depth, .. }) = modes.last_mut() {
                        *brace_depth += 1;
                    }
                    lexemes.push(RefLexeme {
                        kind: LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)),
                        start: pos,
                        end: pos + 1,
                    });
                    pos += 1;
                } else if b == b'}' {
                    match modes.last_mut().unwrap() {
                        Mode::Interpolation { brace_depth, .. } if *brace_depth == 0 => {
                            lexemes.push(RefLexeme {
                                kind: LexemeKind::Token(TokenKind::InterpolationEnd),
                                start: pos,
                                end: pos + 1,
                            });
                            modes.pop();
                        }
                        Mode::Interpolation { brace_depth, .. } => {
                            *brace_depth -= 1;
                            lexemes.push(RefLexeme {
                                kind: LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)),
                                start: pos,
                                end: pos + 1,
                            });
                        }
                        Mode::Normal | Mode::String { .. } => {
                            lexemes.push(RefLexeme {
                                kind: LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)),
                                start: pos,
                                end: pos + 1,
                            });
                        }
                    }
                    pos += 1;
                } else if let Some((sym, end)) = scan_symbol(source, pos) {
                    lexemes.push(RefLexeme {
                        kind: LexemeKind::Token(TokenKind::Symbol(sym)),
                        start: pos,
                        end,
                    });
                    pos = end;
                } else {
                    let ch = source[pos..].chars().next().unwrap();
                    let end = pos + ch.len_utf8();
                    lexemes.push(RefLexeme {
                        kind: LexemeKind::Invalid(InvalidKind::UnexpectedCharacter),
                        start: pos,
                        end,
                    });
                    diags.push(RefDiag {
                        kind: RefDiagKind::UnexpectedCharacter,
                        start: pos,
                        end,
                    });
                    pos = end;
                }
            }
        }
    }

    if !suppressed_eof_error {
        match *modes.last().unwrap() {
            Mode::Normal => {}
            Mode::String { start } => diags.push(RefDiag {
                kind: RefDiagKind::UnterminatedString,
                start,
                end: len,
            }),
            Mode::Interpolation { start, .. } => diags.push(RefDiag {
                kind: RefDiagKind::UnterminatedInterpolation,
                start,
                end: len,
            }),
        }
    }

    lexemes.push(RefLexeme {
        kind: LexemeKind::Eof,
        start: len,
        end: len,
    });
    (lexemes, diags)
}

fn project_production(sources: &SourceMap, lexed: &LexedFile) -> LexProjection {
    let text = sources.source_text(lexed.source_id()).unwrap();
    let mut cursor = 0usize;
    let mut eof_count = 0usize;
    let mut lexemes = Vec::with_capacity(lexed.lexemes().len());

    for lexeme in lexed.lexemes() {
        let span = lexeme.span();
        assert_eq!(span.source_id(), lexed.source_id());
        assert_eq!(
            span.start(),
            cursor,
            "lexeme stream gap or overlap at byte {cursor}"
        );
        if lexeme.kind() == LexemeKind::Eof {
            eof_count += 1;
            assert_eq!(span.start(), text.len());
            assert_eq!(span.end(), text.len());
        } else {
            assert!(
                span.start() < span.end(),
                "non-EOF lexeme must be non-empty"
            );
            assert!(
                sources.slice(span).is_ok(),
                "lexeme span must slice valid UTF-8"
            );
        }
        cursor = span.end();
        lexemes.push(RefLexeme {
            kind: lexeme.kind(),
            start: span.start(),
            end: span.end(),
        });
    }
    assert_eq!(eof_count, 1, "must emit exactly one EOF");
    assert_eq!(lexemes.last().unwrap().kind, LexemeKind::Eof);
    assert_eq!(cursor, text.len(), "lexeme stream must cover entire source");

    let diag_spans = lexed
        .diagnostics()
        .iter()
        .map(|d| (d.primary_span().start(), d.primary_span().end()))
        .collect();
    LexProjection {
        lexemes,
        diag_spans,
    }
}

fn project_reference(source: &str) -> LexProjection {
    let (lexemes, diags) = reference_lex(source);
    LexProjection {
        lexemes,
        diag_spans: diags.into_iter().map(|d| (d.start, d.end)).collect(),
    }
}

fn compare_projections(prod: &LexProjection, reference: &LexProjection) -> Result<(), String> {
    if prod == reference {
        Ok(())
    } else {
        Err(format!(
            "lexer differential mismatch:\n  production: {prod:?}\n  reference:  {reference:?}"
        ))
    }
}

fn assert_diff_matches(label: &str, source: &str) {
    let mut sources = SourceMap::new();
    let id = sources.add_source(format!("{label}.ko"), source).unwrap();
    let lexed = lex(&sources, id).expect("production lex must not fail internally");
    let prod = project_production(&sources, &lexed);
    let reference = project_reference(source);
    compare_projections(&prod, &reference)
        .unwrap_or_else(|err| panic!("{label} ({source:?}): {err}"));
}

#[test]
fn fault_controls_detect_kind_span_and_diagnostic_drift() {
    let sample = "val x = 1_000L + \"a${b}\" // 雪\n";
    let mut sources = SourceMap::new();
    let id = sources.add_source("control.ko", sample).unwrap();
    let lexed = lex(&sources, id).unwrap();
    let baseline = project_production(&sources, &lexed);
    let reference = project_reference(sample);
    assert_eq!(compare_projections(&baseline, &reference), Ok(()));

    let mut mutated = reference.clone();
    mutated.lexemes[0].kind = LexemeKind::Token(TokenKind::Identifier);
    assert!(compare_projections(&baseline, &mutated).is_err());

    let mut mutated = reference.clone();
    mutated.lexemes[0].end -= 1;
    assert!(compare_projections(&baseline, &mutated).is_err());

    let mut mutated = reference.clone();
    mutated.diag_spans.push((0, 1));
    assert!(compare_projections(&baseline, &mutated).is_err());

    let mut mutated = reference;
    mutated.lexemes.pop();
    assert!(compare_projections(&baseline, &mutated).is_err());
}

#[test]
fn f3b_trivia_line_break_and_comment_witnesses_match_reference() {
    let cases = [
        ("empty", ""),
        ("lf", "a\nb"),
        ("crlf", "a\r\nb"),
        ("bare-cr", "a\rb"),
        ("form-feed", "a\x0Cb"),
        ("line-comment-lf", "a//雪\nb"),
        ("line-comment-crlf", "a//雪\r\nb"),
        ("line-comment-bare-cr", "a//雪\rb"),
        ("block-comment-lf", "a/*雪\nc*/b"),
        ("block-comment-crlf", "a/*雪\r\nc*/b"),
        ("block-comment-bare-cr", "a/*雪\rc*/b"),
        ("block-comment-mixed-nl", "a/*雪\n\r\nc*/b"),
        ("block-comment-non-nested", "/* outer /* inner */ tail"),
        ("block-comment-unterminated", "a/*雪\n"),
        ("bom-and-unicode-ws", "\u{FEFF}a\u{00A0}b"),
        ("shebang-rejected", "#!/usr/bin/env koven\nval a = 1"),
    ];

    for (label, source) in cases {
        assert_diff_matches(label, source);
    }
}

#[test]
fn keywords_reserved_words_soft_keywords_and_symbols_match_reference() {
    let hard_keywords = "\
class companion const enum extern fun import interface object package typealias val var vararg \
break continue else for if in is return when while unsafe internal private public \
as false null operator override super this true";
    assert_diff_matches("all-36-hard-keywords", hard_keywords);

    let reserved_words = "async await suspend actor spawn sealed dyn where yield macro reify";
    assert_diff_matches("all-11-reserved-words", reserved_words);

    let soft_and_boundary = "\
value loop block own borrow inout move to by infix and or xor shl shr ushr \
error get set Copyable _ className classβ asyncName";
    assert_diff_matches("soft-keywords-and-ident-boundaries", soft_and_boundary);

    let symbols_and_longest_match = "\
( ) [ ] { } , : ; @ . ?. ? ?: !! ! :: -> * / % + - .. ..< < > <= >= == != & && || \
+= -= *= /= %= = as? as ? !in ! in !inside !is ! is !island \
++ -- << >> ... | ^ ~ #";
    assert_diff_matches(
        "all-44-symbols-and-longest-match",
        symbols_and_longest_match,
    );
}

#[test]
fn numeric_and_char_literals_match_reference() {
    let cases = [
        ("dec-ints", "0 42 1_000_000 007 1L 2u 3U 4uL 5UL"),
        (
            "hex-ints",
            "0x0 0xFF_AA_00 0Xdead_BEEFL 0x10u 0x20U 0x30uL 0x40UL",
        ),
        (
            "bin-ints",
            "0b0 0b1010_0110 0B1111_0000L 0b1u 0b10U 0b11uL 0b100UL",
        ),
        ("floats", "0.0 1.5 1_000.25_5 1f 1F 1.5f 2.5F"),
        ("range-stop", "1..2 1..<2 1.0..2.0 1.toString() .5 1."),
        (
            "invalid-numbers",
            "0x_1 0b_1 100_ 1__0 1_.0 1._0 1e3 1.0e3 1l 1LU 1.0L 0x 0b 0b2 0xG",
        ),
        (
            "valid-chars",
            r#"'a' '雪' '\\' '\'' '\"' '\n' '\r' '\t' '\0'"#,
        ),
        (
            "invalid-chars",
            "'' 'ab' '\\q' '\\' 'a\nnext 'a\rnext 'unterminated",
        ),
    ];

    for (label, source) in cases {
        assert_diff_matches(label, source);
    }
}

#[test]
fn strings_nested_interpolations_and_recovery_match_reference() {
    let cases = [
        ("plain-string", r#""hello 雪" """#),
        (
            "string-escapes",
            r#""\\ \' \" \n \r \t \0 \$ \${not_interp} $plain""#,
        ),
        ("simple-interp", r#""val=${a + b}""#),
        (
            "nested-blocks-and-strings",
            r#""outer=${block { val s = "in=${1 + 2}"; s }}!""#,
        ),
        (
            "braces-in-comments-and-chars-inside-interp",
            "\"x=${ /* } */ '}' + \"}\" + a }\"",
        ),
        ("invalid-escape-continues", r#""a\qb\zc""#),
        ("unterminated-string-lf", "\"abc\nnext"),
        ("unterminated-string-crlf", "\"abc\r\nnext"),
        ("unterminated-inner-string-lf", "\"${ \"abc\nnext }\""),
        ("trailing-backslash-eof", "\"abc\\"),
        ("trailing-backslash-lf", "\"abc\\\nnext"),
        ("trailing-backslash-in-unclosed-interp", "\"${ \"abc\\"),
        ("unclosed-interp-eof", "\"${ a + b"),
        ("nested-unclosed-string-in-unclosed-interp", "\"${ \"abc"),
        ("unterminated-block-comment-in-interp", "\"${ /* unclosed"),
    ];

    for (label, source) in cases {
        assert_diff_matches(label, source);
    }
}

#[test]
fn combinatorial_corpus_matches_reference_machine() {
    let prefixes = ["", " ", "\t", "/*雪*/", "//雪\n", "/*\r\n*/"];
    let cores = [
        "val x = 1_000uL",
        "fun f(own a: Int) = a as? Int ?: 0",
        "a !in 1..<10 && b !is String",
        "\"v=${a + b}\"",
        "a++ --b << >> ...",
        "async + 0x_1 + '\\q'",
        "a/*雪\n*/-b",
    ];
    let suffixes = ["", "\n", "\r\n", "\r", " //尾\n", " /*未闭合"];

    let mut total = 0usize;
    for (pi, p) in prefixes.iter().enumerate() {
        for (ci, c) in cores.iter().enumerate() {
            for (si, s) in suffixes.iter().enumerate() {
                let sample = format!("{p}{c}{s}");
                assert_diff_matches(&format!("combo-{pi}-{ci}-{si}"), &sample);
                total += 1;
            }
        }
    }
    assert_eq!(total, 252);
}
