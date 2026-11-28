//! `textDocument/semanticTokens/full` 实现：基于前端名称/类型事实与 O(1) 目标索引生成确定性 UTF-16 语义高亮。

use std::collections::BTreeMap;

use lang_frontend::{
    analysis::DISPLAY_PATH,
    name_resolution::{
        Namespace, SourceUnitId, SymbolKind as FrontSymbolKind, UnitDeclaration,
        UnitReferenceTarget,
    },
    parser::{ClassifierKind, Item, NameMarker, ParsedFile, VariableKind},
    source::{SourceId, Span},
    type_checking::{CompilationUnitTypes, NominalKind},
};
use lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensDelta,
    SemanticTokensEdit, SemanticTokensFullDeltaResult, SemanticTokensLegend,
};

use crate::{
    navigation_extras::collect_assignment_write_spans,
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

type TargetKey = (SourceUnitId, usize, usize);

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

    let mutable_spans = collect_mutable_spans(parsed_files);
    let assignment_writes = collect_assignment_write_spans(parsed_file);

    // Pre-index all target classifications once in O(D + M + L) so reference lookup is O(log N).
    let embedded_units: Vec<SourceUnitId> = names
        .index()
        .source_units()
        .iter()
        .filter(|su| {
            su.key()
                .logical_path()
                .is_some_and(|p| p.as_str() == DISPLAY_PATH)
                && sources
                    .source_name(su.source_id())
                    .is_ok_and(|n| n.starts_with("koven-std/"))
        })
        .map(|su| su.id())
        .collect();
    let target_index =
        build_target_index(names, typed, parsed_files, &mutable_spans, &embedded_units);

    // Map `(line, start_col_utf16) -> (length_utf16, token_type, token_modifiers)`.
    let mut raw: BTreeMap<(u32, u32), (u32, u32, u32)> = BTreeMap::new();

    // 1. Package header and import package segments -> NAMESPACE.
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
        let key = target_key(decl.source_unit(), decl.name_span());
        if let Some(&(tt, mods)) = target_index.get(&key) {
            record_span(
                sources,
                decl.name_span(),
                tt,
                mods | MOD_DECLARATION,
                &mut raw,
            )?;
        }

        if let Some(typed) = typed
            && let Some(nominal) = typed
                .signatures()
                .declaration(decl.id())
                .and_then(|s| s.nominal())
        {
            for field in nominal.fields() {
                if field.span().source_id() == source_id {
                    let fmod = if mutable_spans.contains(&field.span()) {
                        MOD_DECLARATION
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

    // 4. All resolved references in this file via O(log N) target index; set MOD_MODIFICATION only on actual writes.
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
        if let Some(&(tt, mut mods)) =
            target_index.get(&target_key(target.source_unit, target.span))
        {
            if assignment_writes.contains(&(ref_span.start(), ref_span.end())) {
                mods |= MOD_MODIFICATION;
            }
            record_span(sources, ref_span, tt, mods, &mut raw)?;
        }
    }

    // 5. Frontend-resolved compiler builtins (`External` / `ExternalOverloadSet`) and embedded declarations (`Display`).
    let decl_by_id: BTreeMap<lang_frontend::name_resolution::DeclarationId, TargetKey> = names
        .index()
        .declarations()
        .iter()
        .map(|d| (d.id(), target_key(d.source_unit(), d.name_span())))
        .collect();
    for reference in names.references() {
        if reference.source_unit() != source_unit || reference.span().source_id() != source_id {
            continue;
        }
        match reference.target() {
            UnitReferenceTarget::External(_) | UnitReferenceTarget::ExternalOverloadSet(_) => {
                let tt = match reference.namespace() {
                    Some(Namespace::Type) => TT_TYPE,
                    Some(Namespace::Value) => TT_FUNCTION,
                    None => continue,
                };
                record_span(sources, reference.span(), tt, MOD_DEFAULT_LIBRARY, &mut raw)?;
            }
            UnitReferenceTarget::Declaration(decl_id) => {
                if let Some(key) = decl_by_id.get(decl_id)
                    && let Some(&(tt, mods)) = target_index.get(key)
                {
                    record_span(sources, reference.span(), tt, mods, &mut raw)?;
                }
            }
            _ => {}
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

/// 按文档 URI 缓存最近一次 `SemanticTokens` 结果以支持 `textDocument/semanticTokens/full/delta`。
#[derive(Default)]
pub(crate) struct SemanticTokenCache {
    next_id: u64,
    by_uri: BTreeMap<String, (String, Vec<SemanticToken>)>,
}

impl SemanticTokenCache {
    /// 登记一次完整语义高亮并返回带单调 `result_id` 的 `SemanticTokens`。
    pub(crate) fn store_full(&mut self, uri: &str, mut tokens: SemanticTokens) -> SemanticTokens {
        self.next_id += 1;
        let id = self.next_id.to_string();
        tokens.result_id = Some(id.clone());
        self.by_uri
            .insert(uri.to_owned(), (id, tokens.data.clone()));
        tokens
    }

    /// 计算与 `previous_result_id` 之间的最小增量 `SemanticTokensDelta`；缓存未命中时回退完整 `Tokens`。
    pub(crate) fn compute_delta(
        &mut self,
        uri: &str,
        previous_result_id: &str,
        new_tokens: SemanticTokens,
    ) -> SemanticTokensFullDeltaResult {
        let prev = self
            .by_uri
            .get(uri)
            .filter(|(id, _)| id == previous_result_id)
            .map(|(_, data)| data.clone());
        let stored = self.store_full(uri, new_tokens);
        let Some(prev_data) = prev else {
            return SemanticTokensFullDeltaResult::Tokens(stored);
        };
        let edits = diff_semantic_tokens(&prev_data, &stored.data);
        SemanticTokensFullDeltaResult::TokensDelta(SemanticTokensDelta {
            result_id: stored.result_id,
            edits,
        })
    }

    /// 文档关闭或移除时清理对应缓存。
    pub(crate) fn remove(&mut self, uri: &str) {
        self.by_uri.remove(uri);
    }
}

/// 计算前后两版 `SemanticToken` 序列的最小单一连续区间编辑（以 u32 槽位计数，每个 token 占 5 个 u32）。
pub(crate) fn diff_semantic_tokens(
    prev: &[SemanticToken],
    next: &[SemanticToken],
) -> Vec<SemanticTokensEdit> {
    if prev == next {
        return Vec::new();
    }
    let prefix = prev
        .iter()
        .zip(next.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let max_suffix = (prev.len() - prefix).min(next.len() - prefix);
    let suffix = prev[prefix..]
        .iter()
        .rev()
        .zip(next[prefix..].iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    let delete_tokens = prev.len() - prefix - suffix;
    let insert_slice = &next[prefix..next.len() - suffix];
    vec![SemanticTokensEdit {
        start: (prefix * 5) as u32,
        delete_count: (delete_tokens * 5) as u32,
        data: (!insert_slice.is_empty()).then(|| insert_slice.to_vec()),
    }]
}

fn target_key(su: SourceUnitId, span: Span) -> TargetKey {
    (su, span.start(), span.end())
}

fn build_target_index(
    names: &lang_frontend::name_resolution::CompilationUnitNames,
    typed: Option<&CompilationUnitTypes>,
    parsed_files: &[ParsedFile],
    mutable_spans: &[Span],
    embedded_units: &[SourceUnitId],
) -> BTreeMap<TargetKey, (u32, u32)> {
    let mut index: BTreeMap<TargetKey, (u32, u32)> = BTreeMap::new();

    // 1. Local symbols across all source units.
    for unit_names in names.source_units() {
        let su = unit_names.source_unit();
        for sym in unit_names.resolution().symbols() {
            if !sym.is_synthetic()
                && let Some(entry) = classify_local_symbol(sym.kind(), sym.span(), mutable_spans)
            {
                index.insert(target_key(su, sym.span()), entry);
            }
        }
    }

    // 2. Nominal fields, enum cases, instance methods, companion methods.
    if let Some(typed) = typed {
        for decl in names.index().declarations() {
            let Some(nominal) = typed
                .signatures()
                .declaration(decl.id())
                .and_then(|s| s.nominal())
            else {
                continue;
            };
            let su = decl.source_unit();
            let extra = if embedded_units.contains(&su) {
                MOD_DEFAULT_LIBRARY
            } else {
                0
            };
            for field in nominal.fields() {
                let fmod = if mutable_spans.contains(&field.span()) {
                    extra
                } else {
                    MOD_READONLY | extra
                };
                index.insert(target_key(su, field.span()), (TT_PROPERTY, fmod));
            }
            for case in nominal.enum_cases() {
                index.insert(
                    target_key(su, case.name_span()),
                    (TT_ENUM_MEMBER, MOD_STATIC | extra),
                );
            }
            for member in nominal.members() {
                index.insert(target_key(su, member.name_span()), (TT_METHOD, extra));
            }
            for member in nominal.companion_members() {
                index.insert(
                    target_key(su, member.name_span()),
                    (TT_METHOD, MOD_STATIC | extra),
                );
            }
        }
    }

    // 3. Top-level declarations (prefer Namespace::Type for objects).
    let mut seen_type_decl = BTreeMap::<TargetKey, bool>::new();
    for decl in names.index().declarations() {
        let key = target_key(decl.source_unit(), decl.name_span());
        if seen_type_decl.get(&key).copied().unwrap_or(false) {
            continue;
        }
        let (tt, mut mods) = classify_declaration(decl, typed, parsed_files, mutable_spans);
        if embedded_units.contains(&decl.source_unit()) {
            mods |= MOD_DEFAULT_LIBRARY;
        }
        index.insert(key, (tt, mods));
        if decl.namespace() == Namespace::Type {
            seen_type_decl.insert(key, true);
        }
    }

    index
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
                (TT_VARIABLE, 0)
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
                0
            } else {
                MOD_READONLY
            };
            Some((TT_VARIABLE, m))
        }
        _ => None,
    }
}
