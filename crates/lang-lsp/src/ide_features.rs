//! LSP IDE 语义查询：Hover、Inlay Hints 与 Rename 校验/准备。

use std::collections::{BTreeMap, HashMap};

use lang_frontend::{
    name_resolution::{CompilationUnitNames, SourceUnitId, SymbolKind, UnitSymbolId},
    source::{SourceId, Span},
    type_checking::{CompilationUnitTypes, ParameterMode, UnitTypeId, UnitTypeKind},
};
use lsp_types::{
    Hover, HoverContents, InlayHint, InlayHintKind, InlayHintLabel, Location, MarkupContent,
    MarkupKind, Position, PrepareRenameResponse, Range, TextEdit, Uri, WorkspaceEdit,
};

use crate::{
    position_adapter::{PositionMappingError, span_range},
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

const HARD_KEYWORDS: &[&str] = &[
    "as",
    "break",
    "class",
    "const",
    "continue",
    "deinit",
    "else",
    "enum",
    "extern",
    "false",
    "for",
    "fun",
    "if",
    "import",
    "in",
    "interface",
    "internal",
    "is",
    "loop",
    "null",
    "object",
    "override",
    "package",
    "private",
    "public",
    "return",
    "super",
    "this",
    "true",
    "val",
    "var",
    "when",
    "while",
];

/// 判断候选标识符是否是可安全重命名的 Koven 标识符。
pub(crate) fn is_valid_rename_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        return false;
    }
    !HARD_KEYWORDS.contains(&name)
}

/// 将一组已排序的引用位置构造成 `WorkspaceEdit`。
pub(crate) fn build_workspace_edit(
    locations: Vec<Location>,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    if locations.is_empty() {
        return None;
    }
    let mut grouped: BTreeMap<String, (Uri, Vec<TextEdit>)> = BTreeMap::new();
    for location in locations {
        let entry = grouped
            .entry(location.uri.as_str().to_owned())
            .or_insert_with(|| (location.uri, Vec::new()));
        entry.1.push(TextEdit {
            range: location.range,
            new_text: new_name.to_owned(),
        });
    }
    // lsp_types::WorkspaceEdit::changes 固定使用 HashMap<Uri, Vec<TextEdit>>。
    #[allow(clippy::mutable_key_type)]
    let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::with_capacity(grouped.len());
    for (_, (uri, mut edits)) in grouped {
        edits.sort_by_key(|e| {
            (
                e.range.start.line,
                e.range.start.character,
                e.range.end.line,
                e.range.end.character,
            )
        });
        edits.dedup_by_key(|e| {
            (
                e.range.start.line,
                e.range.start.character,
                e.range.end.line,
                e.range.end.character,
            )
        });
        changes.insert(uri, edits);
    }
    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

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
    let Some((ref_span, targets)) = snapshot.definitions().entry_at(source_unit, offset) else {
        return Ok(None);
    };
    if targets.is_empty() {
        return Ok(None);
    }
    let sources = snapshot.name_snapshot().sources();
    let Ok(text) = sources.source_text(ref_span.source_id()) else {
        return Ok(None);
    };
    if ref_span.end() > text.len() || ref_span.start() >= ref_span.end() {
        return Ok(None);
    }
    let placeholder = &text[ref_span.start()..ref_span.end()];
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
    for ch in rest.chars() {
        if ch == ' ' || ch == '\t' {
            continue;
        }
        return ch == ':';
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
    let line_end = text[name_offset..]
        .find('\n')
        .map_or(text.len(), |i| name_offset + i);
    let line = text[line_start..line_end].trim();
    line.trim_end_matches('{').trim_end().to_owned()
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
