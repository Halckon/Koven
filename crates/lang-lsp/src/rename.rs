//! LSP 安全重命名：Lexer 标识符校验、绑定不变性验证与带版本 WorkspaceEdit 构造。

use std::{collections::BTreeMap, fmt};

use lang_frontend::{
    analysis::{UnitSourceDescriptor, analyze_unit_names},
    lexer::{LexemeKind, TokenKind, lex},
    name_resolution::SourceUnitId,
    source::{SourceMap, Span},
    type_checking::{check_compilation_unit_types, standard_environments},
};
use lsp_types::{
    DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier, Range, TextDocumentEdit,
    TextEdit, Uri, WorkspaceEdit,
};

use crate::{
    definition::{RenamePlan, UnitDefinitionIndex},
    position_adapter::PositionMappingError,
};

/// 重命名请求的可预见拒绝或内部失败。
#[derive(Debug)]
pub(crate) enum RenameError {
    InvalidIdentifier(String),
    SharedOverloadReference,
    AliasReference,
    BindingConflict,
    StaleClosedFile(String),
    Position(PositionMappingError),
    Internal(String),
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier(name) => {
                write!(f, "invalid Koven identifier for rename: {name:?}")
            }
            Self::SharedOverloadReference => f.write_str(
                "cannot safely rename a single overload referenced through a shared import or overload set",
            ),
            Self::AliasReference => {
                f.write_str("cannot rename declaration through an explicit import alias")
            }
            Self::BindingConflict => {
                f.write_str("rename would cause a name conflict or alter symbol bindings")
            }
            Self::StaleClosedFile(uri) => write!(
                f,
                "cannot safely rename across closed file {uri:?} because its on-disk content changed since the workspace snapshot"
            ),
            Self::Position(error) => write!(f, "rename position mapping failed: {error}"),
            Self::Internal(message) => f.write_str(message),
        }
    }
}

impl From<PositionMappingError> for RenameError {
    fn from(error: PositionMappingError) -> Self {
        Self::Position(error)
    }
}

/// 复用生产 Lexer 判断候选名称是否是合法的 Koven 标识符（自动拒绝全部硬关键字与未来保留字，
/// 同时允许 `loop` 等上下文软关键字）。
pub(crate) fn is_valid_rename_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut sources = SourceMap::new();
    let Ok(source_id) = sources.add_source("<rename>", name) else {
        return false;
    };
    let Ok(lexed) = lex(&sources, source_id) else {
        return false;
    };
    if !lexed.diagnostics().is_empty() {
        return false;
    }
    matches!(
        lexed.lexemes(),
        [token, eof]
            if token.kind() == LexemeKind::Token(TokenKind::Identifier)
                && token.span().start() == 0
                && token.span().end() == name.len()
                && eof.kind() == LexemeKind::Eof
    )
}

/// 参与绑定不变性校验的一份源码输入。
pub(crate) struct RenameSourceInput<'a> {
    pub(crate) source_unit: SourceUnitId,
    pub(crate) name: &'a str,
    pub(crate) root: Option<&'a str>,
    pub(crate) logical_path: Option<&'a str>,
    pub(crate) text: &'a str,
}

/// 在内存中模拟应用全部重命名编辑并重跑前端名称/类型解析，确认：
/// 1. 每一处被改写的 span 在重命名后仍然绑定到完全相同的目标声明集合（防止遮蔽/捕获）；
/// 2. 目标声明集合在重命名后不会意外捕获任何原本不相关的引用（防止反向捕获/同作用域冲突）。
pub(crate) fn verify_rename_preserves_bindings(
    inputs: &[RenameSourceInput<'_>],
    plan: &RenamePlan,
    new_name: &str,
) -> Result<(), RenameError> {
    let mut edits_by_unit: BTreeMap<SourceUnitId, Vec<(usize, usize)>> = BTreeMap::new();
    for occurrence in &plan.occurrences {
        edits_by_unit
            .entry(occurrence.source_unit)
            .or_default()
            .push((occurrence.span.start(), occurrence.span.end()));
    }
    for spans in edits_by_unit.values_mut() {
        spans.sort_unstable();
        spans.dedup();
    }

    let mut new_sources = SourceMap::new();
    let mut descriptors = Vec::with_capacity(inputs.len());
    let mut shifted_edits: BTreeMap<SourceUnitId, Vec<(usize, usize)>> = BTreeMap::new();
    let mut shifted_targets: Vec<(SourceUnitId, usize, usize)> = Vec::new();
    let mut source_id_by_unit: BTreeMap<SourceUnitId, lang_frontend::source::SourceId> =
        BTreeMap::new();

    for input in inputs {
        let unit_edits = edits_by_unit
            .get(&input.source_unit)
            .map_or(&[][..], Vec::as_slice);
        let (rewritten, mapping) = apply_edits_in_memory(input.text, unit_edits, new_name)?;
        let source_id = new_sources
            .add_source(input.name, rewritten)
            .map_err(|e| RenameError::Internal(e.to_string()))?;
        source_id_by_unit.insert(input.source_unit, source_id);
        descriptors.push(match (input.root, input.logical_path) {
            (Some(root), Some(path)) => UnitSourceDescriptor::new(root, path, source_id),
            _ => UnitSourceDescriptor::memory_document(source_id),
        });
        for &(old_start, old_end) in unit_edits {
            let new_range = mapping
                .get(&(old_start, old_end))
                .copied()
                .ok_or_else(|| RenameError::Internal("missing shifted edit span".to_owned()))?;
            shifted_edits
                .entry(input.source_unit)
                .or_default()
                .push(new_range);
        }
        for &(target_unit, t_start, t_end) in &plan.target_keys {
            if target_unit == input.source_unit {
                let (ns, ne) = mapping
                    .get(&(t_start, t_end))
                    .copied()
                    .ok_or(RenameError::BindingConflict)?;
                shifted_targets.push((target_unit, ns, ne));
            }
        }
    }
    shifted_targets.sort_unstable();
    shifted_targets.dedup();

    let (name_env, type_env) = standard_environments();
    let name_snapshot = analyze_unit_names(new_sources, descriptors, name_env)
        .map_err(|e| RenameError::Internal(e.to_string()))?;
    let typed = if let Some(validated) = name_snapshot.validated_names() {
        let unit_inputs = name_snapshot.inputs();
        Some(
            check_compilation_unit_types(
                name_snapshot.sources(),
                &unit_inputs,
                validated,
                &type_env,
            )
            .map_err(|e| RenameError::Internal(e.to_string()))?,
        )
    } else {
        None
    };
    let post_index = UnitDefinitionIndex::build(
        name_snapshot.parsed_files(),
        name_snapshot.names(),
        typed.as_ref(),
    )
    .map_err(|e| RenameError::Internal(e.to_string()))?;

    // Build mapping from old SourceUnitId -> new SourceUnitId via SourceId identity.
    let mut old_to_new_unit: BTreeMap<SourceUnitId, SourceUnitId> = BTreeMap::new();
    for (&old_unit, &sid) in &source_id_by_unit {
        let new_unit = name_snapshot
            .names()
            .index()
            .source_units()
            .iter()
            .find(|u| u.source_id() == sid)
            .ok_or_else(|| RenameError::Internal("missing post-rename source unit".to_owned()))?
            .id();
        old_to_new_unit.insert(old_unit, new_unit);
    }

    let mut expected_targets: Vec<(SourceUnitId, usize, usize)> = Vec::new();
    for (old_unit, ns, ne) in shifted_targets {
        let new_unit = *old_to_new_unit
            .get(&old_unit)
            .ok_or_else(|| RenameError::Internal("missing mapped target unit".to_owned()))?;
        expected_targets.push((new_unit, ns, ne));
    }
    expected_targets.sort_unstable();
    expected_targets.dedup();

    let mut expected_edited_keys: Vec<(SourceUnitId, usize, usize)> = Vec::new();
    for (old_unit, spans) in &shifted_edits {
        let new_unit = *old_to_new_unit
            .get(old_unit)
            .ok_or_else(|| RenameError::Internal("missing mapped edit unit".to_owned()))?;
        for &(ns, ne) in spans {
            expected_edited_keys.push((new_unit, ns, ne));
            let Some((found_span, targets)) = post_index.entry_at(new_unit, ns) else {
                return Err(RenameError::BindingConflict);
            };
            if found_span.start() != ns || found_span.end() != ne {
                return Err(RenameError::BindingConflict);
            }
            let actual_targets: Vec<(SourceUnitId, usize, usize)> = targets
                .iter()
                .map(|t| (t.source_unit, t.span.start(), t.span.end()))
                .collect();
            if actual_targets != expected_targets {
                return Err(RenameError::BindingConflict);
            }
        }
    }
    expected_edited_keys.sort_unstable();
    expected_edited_keys.dedup();

    // Also verify no unrelated same-name occurrence was newly captured by the renamed declaration.
    if let Some(&(first_unit, first_start, _)) = expected_targets.first() {
        let post_refs = post_index.references_at(first_unit, first_start, true);
        let mut actual_same_name_refs: Vec<(SourceUnitId, usize, usize)> = Vec::new();
        for item in post_refs {
            if let Ok(text) = name_snapshot.sources().source_text(item.span.source_id())
                && item.span.end() <= text.len()
                && &text[item.span.start()..item.span.end()] == new_name
            {
                actual_same_name_refs.push((item.source_unit, item.span.start(), item.span.end()));
            }
        }
        actual_same_name_refs.sort_unstable();
        actual_same_name_refs.dedup();
        if actual_same_name_refs != expected_edited_keys {
            return Err(RenameError::BindingConflict);
        }
    }

    Ok(())
}

type ByteRange = (usize, usize);
type SpanShiftMap = BTreeMap<ByteRange, ByteRange>;

fn apply_edits_in_memory(
    text: &str,
    sorted_edits: &[ByteRange],
    new_name: &str,
) -> Result<(String, SpanShiftMap), RenameError> {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut mapping = BTreeMap::new();
    for &(start, end) in sorted_edits {
        if start < cursor || end > text.len() || start > end {
            return Err(RenameError::Internal(
                "overlapping rename edit spans".to_owned(),
            ));
        }
        out.push_str(&text[cursor..start]);
        let new_start = out.len();
        out.push_str(new_name);
        let new_end = out.len();
        mapping.insert((start, end), (new_start, new_end));
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    Ok((out, mapping))
}

/// 将按文件分组的编辑构造成携带可选文档版本的 `WorkspaceEdit` (`documentChanges`)。
pub(crate) fn build_versioned_workspace_edit(
    edits_by_doc: Vec<(Uri, Option<i32>, Range)>,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    if edits_by_doc.is_empty() {
        return None;
    }
    let mut grouped: BTreeMap<String, (Uri, Option<i32>, Vec<TextEdit>)> = BTreeMap::new();
    for (uri, version, range) in edits_by_doc {
        let entry = grouped
            .entry(uri.as_str().to_owned())
            .or_insert_with(|| (uri, version, Vec::new()));
        entry.2.push(TextEdit {
            range,
            new_text: new_name.to_owned(),
        });
    }
    let mut doc_edits = Vec::with_capacity(grouped.len());
    for (_, (uri, version, mut edits)) in grouped {
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
        doc_edits.push(TextDocumentEdit {
            text_document: OptionalVersionedTextDocumentIdentifier { uri, version },
            edits: edits.into_iter().map(OneOf::Left).collect(),
        });
    }
    Some(WorkspaceEdit {
        changes: None,
        document_changes: Some(DocumentChanges::Edits(doc_edits)),
        change_annotations: None,
    })
}

/// 辅助检查：`span` 在 `sources` 中的切片文本。
pub(crate) fn span_slice(sources: &SourceMap, span: Span) -> Option<&str> {
    let text = sources.source_text(span.source_id()).ok()?;
    if span.start() <= span.end() && span.end() <= text.len() {
        Some(&text[span.start()..span.end()])
    } else {
        None
    }
}
