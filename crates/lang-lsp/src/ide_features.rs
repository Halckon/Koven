//! LSP IDE 语义查询：Hover、Inlay Hints 与 Rename 校验/准备。

use lang_frontend::{
    lexer::{Keyword, LexemeKind, Symbol, TokenKind, lex},
    name_resolution::{CompilationUnitNames, SourceUnitId, SymbolKind, UnitSymbolId},
    source::{SourceId, SourceMap, Span},
    type_checking::{CompilationUnitTypes, ParameterMode, UnitTypeId, UnitTypeKind},
};
use lsp_types::{
    Hover, HoverContents, InlayHint, InlayHintKind, InlayHintLabel, MarkupContent, MarkupKind,
    Position, PrepareRenameResponse, Range,
};

use crate::{
    position_adapter::{PositionMappingError, span_range},
    rename::{is_valid_rename_identifier, span_slice},
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

/// 在给定位置计算 `textDocument/hover`。
pub(crate) fn hover_at(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    offset: usize,
) -> Result<Option<Hover>, PositionMappingError> {
    let Some((ref_span, targets)) = snapshot.definitions().entry_at(source_unit, offset) else {
        return Ok(None);
    };
    let Some(target) = targets.first().copied() else {
        return Ok(None);
    };
    let sources = snapshot.name_snapshot().sources();
    let names = snapshot.name_snapshot().names();
    let typed = snapshot.typed();

    let Some(target_source) = names
        .index()
        .source_units()
        .iter()
        .find(|u| u.id() == target.source_unit)
    else {
        return Ok(None);
    };
    let target_source_id = target_source.source_id();
    let Ok(target_text) = sources.source_text(target_source_id) else {
        return Ok(None);
    };

    let (signature, doc_comment) =
        describe_target(names, typed, target.source_unit, target.span, target_text);
    let Some(signature) = signature else {
        return Ok(None);
    };

    let pkg_line = names
        .index()
        .packages()
        .get(target_source.package().index())
        .and_then(|pkg| pkg.name())
        .filter(|name| !name.is_default())
        .map(|name| format!("`package {}`", name.segments().join(".")));

    let mut markdown = String::new();
    if let Some(pkg) = pkg_line {
        markdown.push_str(&pkg);
        markdown.push_str("\n\n");
    }
    markdown.push_str("```koven\n");
    markdown.push_str(&signature);
    markdown.push_str("\n```");
    if let Some(doc) = doc_comment {
        markdown.push_str("\n\n---\n\n");
        markdown.push_str(&doc);
    }

    let range = span_range(sources, ref_span)?;
    Ok(Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: Some(range),
    }))
}

/// 为指定文档的可见范围生成局部绑定推断类型 Inlay Hints。
pub(crate) fn inlay_hints_for_source(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    range: Range,
) -> Result<Vec<InlayHint>, PositionMappingError> {
    let Some(typed) = snapshot.typed() else {
        return Ok(Vec::new());
    };
    let sources = snapshot.name_snapshot().sources();
    let names = snapshot.name_snapshot().names();
    let Ok(source_text) = sources.source_text(source_id) else {
        return Ok(Vec::new());
    };
    let Some(unit_names) = names
        .source_units()
        .iter()
        .find(|u| u.source_unit() == source_unit)
    else {
        return Ok(Vec::new());
    };

    let mut hints = Vec::new();
    for symbol in unit_names.resolution().symbols() {
        if !matches!(
            symbol.kind(),
            SymbolKind::Variable | SymbolKind::ForBinding | SymbolKind::LambdaParameter
        ) {
            continue;
        }
        let span = symbol.span();
        if span.source_id() != source_id || span.is_empty() || span.end() > source_text.len() {
            continue;
        }
        if has_explicit_type_annotation(source_text, span.end()) {
            continue;
        }
        let unit_symbol = UnitSymbolId::new(source_unit, symbol.id());
        let Some(ty_id) = typed.symbol_type(unit_symbol) else {
            continue;
        };
        let Some(ty_text) = format_unit_type(ty_id, typed, names) else {
            continue;
        };
        let lsp_range = span_range(sources, span)?;
        let pos = lsp_range.end;
        if !position_in_range(pos, range) {
            continue;
        }
        hints.push(InlayHint {
            position: pos,
            label: InlayHintLabel::String(format!(": {ty_text}")),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(false),
            padding_right: Some(false),
            data: None,
        });
    }
    hints.sort_by_key(|h| (h.position.line, h.position.character));
    Ok(hints)
}

/// 检查光标处符号是否支持安全重命名，并返回范围与占位名称。
pub(crate) fn prepare_rename_at(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    offset: usize,
) -> Result<Option<PrepareRenameResponse>, PositionMappingError> {
    let sources = snapshot.name_snapshot().sources();
    let Some((ref_span, _)) = snapshot.definitions().entry_at(source_unit, offset) else {
        return Ok(None);
    };
    let Ok(Some(_)) = snapshot
        .definitions()
        .plan_rename_at(source_unit, offset, sources)
    else {
        return Ok(None);
    };
    let Some(placeholder) = span_slice(sources, ref_span) else {
        return Ok(None);
    };
    if !is_valid_rename_identifier(placeholder) {
        return Ok(None);
    }
    let range = span_range(sources, ref_span)?;
    Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
        range,
        placeholder: placeholder.to_owned(),
    }))
}

fn describe_target(
    names: &CompilationUnitNames,
    typed: Option<&CompilationUnitTypes>,
    target_unit: SourceUnitId,
    target_span: Span,
    target_text: &str,
) -> (Option<String>, Option<String>) {
    // 1. Top-level or member declaration
    if let Some(decl) = names
        .index()
        .declarations()
        .iter()
        .find(|d| d.source_unit() == target_unit && d.name_span() == target_span)
    {
        let header = extract_declaration_header(target_text, target_span.start());
        let docs = extract_leading_doc_comment(target_text, target_span.start());
        let fallback = if header.is_empty() {
            decl.name().to_owned()
        } else {
            header
        };
        return (Some(fallback), docs);
    }

    // 2. Local symbol (val/var/parameter/field/type param/etc.)
    if let Some(unit_names) = names
        .source_units()
        .iter()
        .find(|u| u.source_unit() == target_unit)
        && let Some(symbol) = unit_names
            .resolution()
            .symbols()
            .iter()
            .find(|s| s.span() == target_span)
    {
        let unit_symbol = UnitSymbolId::new(target_unit, symbol.id());
        let ty_suffix = typed
            .and_then(|t| {
                t.symbol_type(unit_symbol)
                    .and_then(|ty| format_unit_type(ty, t, names))
            })
            .map(|ty| format!(": {ty}"))
            .unwrap_or_default();
        let prefix = match symbol.kind() {
            SymbolKind::Variable => {
                let line_start = target_text[..target_span.start()]
                    .rfind('\n')
                    .map_or(0, |i| i + 1);
                let before_name = target_text[line_start..target_span.start()].trim();
                if before_name.split_whitespace().any(|w| w == "var") {
                    "var"
                } else {
                    "val"
                }
            }
            SymbolKind::Constant => "const val",
            SymbolKind::Function => "fun",
            SymbolKind::Field => "field",
            SymbolKind::ValueParameter => "param",
            SymbolKind::LambdaParameter => "lambda param",
            SymbolKind::ForBinding => "for",
            SymbolKind::TypeParameter => "type param",
            SymbolKind::Classifier => "type",
            SymbolKind::EnumVariant => "enum case",
            _ => "symbol",
        };
        let docs = extract_leading_doc_comment(target_text, target_span.start());
        return (Some(format!("{prefix} {}{ty_suffix}", symbol.name())), docs);
    }

    (None, None)
}

fn has_explicit_type_annotation(text: &str, after_name_offset: usize) -> bool {
    let rest = &text[after_name_offset..];
    let mut map = SourceMap::new();
    let Ok(sid) = map.add_source("<inlay>", rest) else {
        return false;
    };
    let Ok(lexed) = lex(&map, sid) else {
        return false;
    };
    for lexeme in lexed.lexemes() {
        match lexeme.kind() {
            LexemeKind::Trivia(_) => continue,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Colon)) => return true,
            _ => return false,
        }
    }
    false
}

fn position_in_range(pos: Position, range: Range) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
        && (pos.line < range.end.line
            || (pos.line == range.end.line && pos.character <= range.end.character))
}

fn extract_declaration_header(text: &str, name_offset: usize) -> String {
    let line_start = text[..name_offset].rfind('\n').map_or(0, |i| i + 1);
    let rest = &text[line_start..];
    let mut map = SourceMap::new();
    let Ok(sid) = map.add_source("<hover>", rest) else {
        return String::new();
    };
    let Ok(lexed) = lex(&map, sid) else {
        return String::new();
    };
    let mut paren = 0usize;
    let mut angle = 0usize;
    let mut is_value_decl = false;
    let mut seen_decl_kw = false;
    let mut end = rest.len();
    for lexeme in lexed.lexemes() {
        match lexeme.kind() {
            LexemeKind::Token(TokenKind::Keyword(kw)) if !seen_decl_kw => match kw {
                Keyword::Val | Keyword::Var | Keyword::Const | Keyword::Typealias => {
                    is_value_decl = true;
                    seen_decl_kw = true;
                }
                Keyword::Fun
                | Keyword::Class
                | Keyword::Interface
                | Keyword::Object
                | Keyword::Enum => {
                    seen_decl_kw = true;
                }
                _ => {}
            },
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => paren += 1,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                paren = paren.saturating_sub(1);
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Less)) => angle += 1,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)) => {
                angle = angle.saturating_sub(1);
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) if paren == 0 && angle == 0 => {
                end = lexeme.span().start();
                break;
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Equal))
                if !is_value_decl && paren == 0 && angle == 0 =>
            {
                end = lexeme.span().start();
                break;
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)) if paren == 0 && angle == 0 => {
                end = lexeme.span().start();
                break;
            }
            LexemeKind::Trivia(_)
                if is_value_decl
                    && paren == 0
                    && angle == 0
                    && rest[lexeme.span().start()..lexeme.span().end()].contains('\n') =>
            {
                end = lexeme.span().start();
                break;
            }
            _ => {}
        }
    }
    rest[..end].trim().to_owned()
}

fn extract_leading_doc_comment(text: &str, name_offset: usize) -> Option<String> {
    let line_start = text[..name_offset].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[..line_start];
    let mut doc_lines = Vec::new();
    for raw in before.lines().rev() {
        let trimmed = raw.trim();
        if let Some(rest) = trimmed.strip_prefix("///") {
            doc_lines.push(rest.trim_start().to_owned());
        } else {
            break;
        }
    }
    if doc_lines.is_empty() {
        None
    } else {
        doc_lines.reverse();
        Some(doc_lines.join("\n"))
    }
}

/// 将 `UnitTypeId` 格式化为可读的 Koven 类型文本；错误/未决类型返回 `None`。
pub(crate) fn format_unit_type(
    ty: UnitTypeId,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
) -> Option<String> {
    format_unit_type_depth(ty, typed, names, 0)
}

fn format_unit_type_depth(
    ty: UnitTypeId,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    depth: usize,
) -> Option<String> {
    if depth > 16 {
        return Some("...".to_owned());
    }
    match typed.types().get(ty)? {
        UnitTypeKind::Builtin(b) => Some(format!("{b:?}")),
        UnitTypeKind::Nullable(inner) => Some(format!(
            "{}?",
            format_unit_type_depth(*inner, typed, names, depth + 1)?
        )),
        UnitTypeKind::Function {
            move_only,
            parameters,
            return_type,
        } => {
            let mut params = Vec::with_capacity(parameters.len());
            for p in parameters {
                let pt = format_unit_type_depth(p.ty(), typed, names, depth + 1)?;
                let mode = match p.mode() {
                    ParameterMode::Value => "own ",
                    ParameterMode::Borrow => "",
                    ParameterMode::Inout => "inout ",
                };
                params.push(format!("{mode}{pt}"));
            }
            let ret = format_unit_type_depth(*return_type, typed, names, depth + 1)?;
            let prefix = if *move_only { "move " } else { "" };
            Some(format!("{prefix}({}) -> {ret}", params.join(", ")))
        }
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            let base = names
                .index()
                .declarations()
                .iter()
                .find(|d| d.id() == *declaration)
                .map_or("?", |d| d.name());
            if arguments.is_empty() {
                Some(base.to_owned())
            } else {
                let mut args = Vec::with_capacity(arguments.len());
                for arg in arguments {
                    args.push(format_unit_type_depth(*arg, typed, names, depth + 1)?);
                }
                Some(format!("{base}<{}>", args.join(", ")))
            }
        }
        UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        } => {
            let base = format!("{constructor:?}");
            if arguments.is_empty() {
                Some(base)
            } else {
                let mut args = Vec::with_capacity(arguments.len());
                for arg in arguments {
                    args.push(format_unit_type_depth(*arg, typed, names, depth + 1)?);
                }
                Some(format!("{base}<{}>", args.join(", ")))
            }
        }
        UnitTypeKind::EnumCase { case, .. } => {
            let name = names
                .source_units()
                .iter()
                .find(|u| u.source_unit() == case.source_unit())
                .and_then(|u| {
                    u.resolution()
                        .symbols()
                        .iter()
                        .find(|s| s.id() == case.symbol())
                })
                .map_or("case", |s| s.name());
            Some(name.to_owned())
        }
        UnitTypeKind::TypeParameter(sym) => {
            let name = names
                .source_units()
                .iter()
                .find(|u| u.source_unit() == sym.source_unit())
                .and_then(|u| {
                    u.resolution()
                        .symbols()
                        .iter()
                        .find(|s| s.id() == sym.symbol())
                })
                .map_or("T", |s| s.name());
            Some(name.to_owned())
        }
        UnitTypeKind::StaticSelf(_) => Some("Self".to_owned()),
        UnitTypeKind::Capability(cap) => Some(format!("{cap:?}")),
        UnitTypeKind::IntegerLiteral(_) | UnitTypeKind::Deferred(_) | UnitTypeKind::Error => None,
    }
}
