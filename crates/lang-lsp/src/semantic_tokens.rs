//! `textDocument/semanticTokens/full` 实现：基于前端名称与类型事实生成确定性 UTF-16 语义高亮。

use std::collections::BTreeMap;

use lang_frontend::{
    lexer::{LexemeKind, TokenKind, lex},
    name_resolution::{
        CompilationUnitNames, Namespace, SourceUnitId, SymbolKind as FrontSymbolKind,
        UnitDeclaration,
    },
    parser::{ClassifierKind, Item, NameMarker, ParsedFile, VariableKind},
    source::{SourceId, Span},
    type_checking::{BuiltinType, CompilationUnitTypes, NominalKind},
};
use lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensLegend,
};

use crate::{
    position_adapter::{PositionMappingError, span_range},
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

const TT_NAMESPACE: u32 = 0;
const TT_CLASS: u32 = 1;
const TT_INTERFACE: u32 = 2;
const TT_ENUM: u32 = 3;
const TT_ENUM_MEMBER: u32 = 4;
const TT_TYPE_PARAMETER: u32 = 5;
const TT_FUNCTION: u32 = 6;
const TT_METHOD: u32 = 7;
const TT_PROPERTY: u32 = 8;
const TT_PARAMETER: u32 = 9;
const TT_VARIABLE: u32 = 10;
const TT_TYPE: u32 = 11;

const MOD_DECLARATION: u32 = 1 << 0;
const MOD_READONLY: u32 = 1 << 1;
const MOD_MODIFICATION: u32 = 1 << 2;
const MOD_STATIC: u32 = 1 << 3;
const MOD_DEFAULT_LIBRARY: u32 = 1 << 4;

const BUILTIN_EXTRAS: &[&str] = &[
    "Array",
    "Box",
    "Copyable",
    "Hashable",
    "List",
    "Map",
    "MutableList",
    "MutableMap",
    "Rc",
    "Transferable",
    "View",
];

const BUILTIN_FUNCS: &[&str] = &[
    "println",
    "error",
    "listOf",
    "mutableListOf",
    "mapOf",
    "mutableMapOf",
    "arrayOf",
    "replace",
    "swap",
    "rangeView",
    "textByteLength",
    "textByteAt",
    "textUtf8ErrorOffset",
    "textCopyBytes",
    "textCopySlice",
];

/// 返回 `ServerCapabilities` 中声明的语义高亮图例。
pub(crate) fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::NAMESPACE,
            SemanticTokenType::CLASS,
            SemanticTokenType::INTERFACE,
            SemanticTokenType::ENUM,
            SemanticTokenType::ENUM_MEMBER,
            SemanticTokenType::TYPE_PARAMETER,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::METHOD,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::TYPE,
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::READONLY,
            SemanticTokenModifier::MODIFICATION,
            SemanticTokenModifier::STATIC,
            SemanticTokenModifier::DEFAULT_LIBRARY,
        ],
    }
}

/// 为单个源码文件构造确定性、无重叠的 `textDocument/semanticTokens/full` 数据。
pub(crate) fn semantic_tokens_full(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
) -> Result<SemanticTokens, PositionMappingError> {
    let sources = snapshot.name_snapshot().sources();
    let names = snapshot.name_snapshot().names();
    let typed = snapshot.typed();
    let parsed_files = snapshot.name_snapshot().parsed_files();
    let parsed_file = parsed_files.iter().find(|p| p.source_id() == source_id);

    // Pre-collect mutable (`var`) name spans from AST across the unit for READONLY vs MODIFICATION.
    let mutable_spans = collect_mutable_spans(parsed_files);

    // Map `(line, start_col_utf16) -> (length_utf16, token_type, token_modifiers)`.
    let mut raw: BTreeMap<(u32, u32), (u32, u32, u32)> = BTreeMap::new();

    // 1. Package header segments -> NAMESPACE.
    if let Some(parsed) = parsed_file {
        if let Some(pkg) = parsed.package() {
            for seg in &pkg.segments {
                record_span(sources, seg.span, TT_NAMESPACE, 0, &mut raw)?;
            }
        }
        for import in parsed.imports() {
            let last_is_pkg = import.wildcard_span.is_some();
            let count = import.segments.len();
            for (idx, seg) in import.segments.iter().enumerate() {
                if last_is_pkg || idx + 1 < count {
                    record_span(sources, seg.span, TT_NAMESPACE, 0, &mut raw)?;
                }
            }
        }
    }

    // 2. Top-level declarations & nominal members in this file (DECLARATION modifier).
    for decl in names.index().declarations() {
        if decl.source_unit() != source_unit || decl.name_span().source_id() != source_id {
            continue;
        }
        let (tt, mods) = classify_declaration(decl, typed, parsed_files, &mutable_spans);
        record_span(
            sources,
            decl.name_span(),
            tt,
            mods | MOD_DECLARATION,
            &mut raw,
        )?;

        if let Some(typed) = typed
            && let Some(nominal) = typed
                .signatures()
                .declaration(decl.id())
                .and_then(|s| s.nominal())
        {
            for field in nominal.fields() {
                if field.span().source_id() == source_id {
                    let fmod = if mutable_spans.contains(&field.span()) {
                        MOD_DECLARATION | MOD_MODIFICATION
                    } else {
                        MOD_DECLARATION | MOD_READONLY
                    };
                    record_span(sources, field.span(), TT_PROPERTY, fmod, &mut raw)?;
                }
            }
            for case in nominal.enum_cases() {
                if case.name_span().source_id() == source_id {
                    record_span(
                        sources,
                        case.name_span(),
                        TT_ENUM_MEMBER,
                        MOD_DECLARATION | MOD_STATIC,
                        &mut raw,
                    )?;
                }
            }
            for member in nominal.members() {
                if member.name_span().source_id() == source_id {
                    record_span(
                        sources,
                        member.name_span(),
                        TT_METHOD,
                        MOD_DECLARATION,
                        &mut raw,
                    )?;
                }
            }
            for member in nominal.companion_members() {
                if member.name_span().source_id() == source_id {
                    record_span(
                        sources,
                        member.name_span(),
                        TT_METHOD,
                        MOD_DECLARATION | MOD_STATIC,
                        &mut raw,
                    )?;
                }
            }
        }
    }

    // 3. Local symbols in this file (parameters, type params, variables, constants, for-bindings).
    if let Some(unit_names) = names
        .source_units()
        .iter()
        .find(|u| u.source_unit() == source_unit)
    {
        for sym in unit_names.resolution().symbols() {
            if sym.is_synthetic() || sym.span().source_id() != source_id {
                continue;
            }
            if let Some((tt, mods)) = classify_local_symbol(sym.kind(), sym.span(), &mutable_spans)
            {
                record_span(sources, sym.span(), tt, mods | MOD_DECLARATION, &mut raw)?;
            }
        }
    }

    // 4. All resolved references in this file (without DECLARATION modifier).
    for (ref_span, targets) in snapshot.definitions().entries_in_unit(source_unit) {
        if ref_span.source_id() != source_id {
            continue;
        }
        let Some(target) = targets.first().copied() else {
            continue;
        };
        if target.source_unit == source_unit && target.span == ref_span {
            continue;
        }
        if let Some((tt, mods)) = classify_target(
            names,
            typed,
            parsed_files,
            &mutable_spans,
            target.source_unit,
            target.span,
        ) {
            record_span(sources, ref_span, tt, mods, &mut raw)?;
        }
    }

    // 5. Unshadowed builtin types and functions via lexer scan.
    if let Ok(lexed) = lex(sources, source_id)
        && let Ok(text) = sources.source_text(source_id)
    {
        for lexeme in lexed.lexemes() {
            if !matches!(lexeme.kind(), LexemeKind::Token(TokenKind::Identifier)) {
                continue;
            }
            let span = lexeme.span();
            if span.end() > text.len() {
                continue;
            }
            let ident = &text[span.start()..span.end()];
            if is_builtin_type(ident) {
                record_span(sources, span, TT_TYPE, MOD_DEFAULT_LIBRARY, &mut raw)?;
            } else if BUILTIN_FUNCS.contains(&ident) {
                record_span(sources, span, TT_FUNCTION, MOD_DEFAULT_LIBRARY, &mut raw)?;
            }
        }
    }

    // 6. Delta-encode in strictly increasing `(line, col)` order, dropping any overlapping spans.
    let mut data = Vec::with_capacity(raw.len());
    let mut prev_line = 0u32;
    let mut prev_col = 0u32;
    let mut prev_end = 0u32;
    let mut has_prev = false;
    for ((line, col), (length, token_type, token_modifiers_bitset)) in raw {
        if length == 0 {
            continue;
        }
        if has_prev && line == prev_line && col < prev_end {
            continue;
        }
        let delta_line = if has_prev { line - prev_line } else { line };
        let delta_start = if has_prev && delta_line == 0 {
            col - prev_col
        } else {
            col
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset,
        });
        prev_line = line;
        prev_col = col;
        prev_end = col + length;
        has_prev = true;
    }

    Ok(SemanticTokens {
        result_id: None,
        data,
    })
}

fn record_span(
    sources: &lang_frontend::source::SourceMap,
    span: Span,
    token_type: u32,
    modifiers: u32,
    out: &mut BTreeMap<(u32, u32), (u32, u32, u32)>,
) -> Result<(), PositionMappingError> {
    if span.is_empty() {
        return Ok(());
    }
    let range = span_range(sources, span)?;
    if range.start.line != range.end.line {
        return Ok(());
    }
    let len = range.end.character.saturating_sub(range.start.character);
    if len == 0 {
        return Ok(());
    }
    out.entry((range.start.line, range.start.character))
        .or_insert((len, token_type, modifiers));
    Ok(())
}

fn collect_mutable_spans(parsed_files: &[ParsedFile]) -> Vec<Span> {
    let mut out = Vec::new();
    for parsed in parsed_files {
        for (_, node) in parsed.ast().items().iter() {
            match node.payload() {
                Item::Variable {
                    kind: VariableKind::Var,
                    name: NameMarker::Present(s),
                    ..
                } => {
                    out.push(*s);
                }
                Item::Classifier(classifier) => {
                    if let Some(ctor) = &classifier.primary_constructor {
                        for field in &ctor.fields {
                            if field.kind == VariableKind::Var
                                && let NameMarker::Present(s) = field.name
                            {
                                out.push(s);
                                out.push(field.span);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn classify_declaration(
    decl: &UnitDeclaration,
    typed: Option<&CompilationUnitTypes>,
    parsed_files: &[ParsedFile],
    mutable_spans: &[Span],
) -> (u32, u32) {
    match decl.kind() {
        FrontSymbolKind::Function => (TT_FUNCTION, 0),
        FrontSymbolKind::Constant => (TT_VARIABLE, MOD_READONLY),
        FrontSymbolKind::Variable => {
            if mutable_spans.contains(&decl.name_span()) {
                (TT_VARIABLE, MOD_MODIFICATION)
            } else {
                (TT_VARIABLE, MOD_READONLY)
            }
        }
        FrontSymbolKind::Classifier => {
            if let Some(nominal) = typed
                .and_then(|t| t.signatures().declaration(decl.id()))
                .and_then(|s| s.nominal())
            {
                return match nominal.kind() {
                    NominalKind::Interface => (TT_INTERFACE, 0),
                    NominalKind::EnumClass => (TT_ENUM, 0),
                    NominalKind::Class | NominalKind::ValueClass | NominalKind::Object => {
                        (TT_CLASS, 0)
                    }
                };
            }
            if let Some(parsed) = parsed_files
                .iter()
                .find(|p| p.source_id() == decl.name_span().source_id())
                && let Ok(node) = parsed.ast().items().get(decl.root())
            {
                let payload = match node.payload() {
                    Item::Modified { declaration, .. } => parsed
                        .ast()
                        .items()
                        .get(*declaration)
                        .ok()
                        .map(|n| n.payload()),
                    other => Some(other),
                };
                if let Some(Item::Classifier(c)) = payload {
                    return match c.kind {
                        ClassifierKind::Interface { .. } => (TT_INTERFACE, 0),
                        ClassifierKind::EnumClass { .. } => (TT_ENUM, 0),
                        _ => (TT_CLASS, 0),
                    };
                }
            }
            (TT_CLASS, 0)
        }
        FrontSymbolKind::EnumVariant => (TT_ENUM_MEMBER, MOD_STATIC),
        FrontSymbolKind::Field => (TT_PROPERTY, 0),
        FrontSymbolKind::TypeParameter => (TT_TYPE_PARAMETER, 0),
        _ => (TT_VARIABLE, 0),
    }
}

fn classify_local_symbol(
    kind: FrontSymbolKind,
    span: Span,
    mutable_spans: &[Span],
) -> Option<(u32, u32)> {
    match kind {
        FrontSymbolKind::ValueParameter | FrontSymbolKind::LambdaParameter => {
            Some((TT_PARAMETER, 0))
        }
        FrontSymbolKind::TypeParameter => Some((TT_TYPE_PARAMETER, 0)),
        FrontSymbolKind::ForBinding | FrontSymbolKind::Constant => {
            Some((TT_VARIABLE, MOD_READONLY))
        }
        FrontSymbolKind::Variable => {
            let m = if mutable_spans.contains(&span) {
                MOD_MODIFICATION
            } else {
                MOD_READONLY
            };
            Some((TT_VARIABLE, m))
        }
        _ => None,
    }
}

fn classify_target(
    names: &CompilationUnitNames,
    typed: Option<&CompilationUnitTypes>,
    parsed_files: &[ParsedFile],
    mutable_spans: &[Span],
    target_unit: SourceUnitId,
    target_span: Span,
) -> Option<(u32, u32)> {
    // 1. Top-level declaration (prefer Namespace::Type for objects).
    let mut matched_decl: Option<&UnitDeclaration> = None;
    for decl in names.index().declarations() {
        if decl.source_unit() == target_unit
            && decl.name_span() == target_span
            && (matched_decl.is_none() || decl.namespace() == Namespace::Type)
        {
            matched_decl = Some(decl);
        }
    }
    if let Some(decl) = matched_decl {
        return Some(classify_declaration(
            decl,
            typed,
            parsed_files,
            mutable_spans,
        ));
    }

    // 2. Nominal fields, methods, companion methods, enum cases.
    if let Some(typed) = typed {
        for sig in typed.signatures().declarations() {
            let Some(nominal) = sig.nominal() else {
                continue;
            };
            if let Some(field) = nominal.fields().iter().find(|f| f.span() == target_span) {
                let fmod = if mutable_spans.contains(&field.span()) {
                    MOD_MODIFICATION
                } else {
                    MOD_READONLY
                };
                return Some((TT_PROPERTY, fmod));
            }
            if nominal
                .enum_cases()
                .iter()
                .any(|c| c.name_span() == target_span)
            {
                return Some((TT_ENUM_MEMBER, MOD_STATIC));
            }
            if nominal
                .members()
                .iter()
                .any(|m| m.name_span() == target_span)
            {
                return Some((TT_METHOD, 0));
            }
            if nominal
                .companion_members()
                .iter()
                .any(|m| m.name_span() == target_span)
            {
                return Some((TT_METHOD, MOD_STATIC));
            }
        }
    }

    // 3. Local symbols in target_unit.
    if let Some(unit_names) = names
        .source_units()
        .iter()
        .find(|u| u.source_unit() == target_unit)
        && let Some(sym) = unit_names
            .resolution()
            .symbols()
            .iter()
            .find(|s| s.span() == target_span)
    {
        return classify_local_symbol(sym.kind(), sym.span(), mutable_spans);
    }

    None
}

fn is_builtin_type(ident: &str) -> bool {
    BuiltinType::ALL.iter().any(|b| b.name() == ident) || BUILTIN_EXTRAS.contains(&ident)
}
