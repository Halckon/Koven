//! `textDocument/codeAction` 实现：未解析名称自动导入（QuickFix）与显式类型标注（Refactor）。

use std::collections::{BTreeSet, HashMap};

use lang_frontend::{
    name_resolution::{DeclarationVisibility, SourceUnitId, UnitSymbolId},
    parser::{Item, NameMarker, ParsedFile},
    source::SourceId,
};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, DocumentChangeOperation, DocumentChanges,
    OneOf, OptionalVersionedTextDocumentIdentifier, Position, Range, TextDocumentEdit, TextEdit,
    Uri, WorkspaceEdit,
};

use crate::{
    ide_features::format_unit_type,
    position_adapter::{PositionMappingError, span_range},
    rename::is_valid_rename_identifier,
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

const UNRESOLVED_NAME_CODE: &str = "L0080";

/// 为当前文档的选定范围生成快速修复与重构 CodeAction（自动导入、添加显式类型标注）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn code_actions(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    uri: &Uri,
    doc_version: Option<i32>,
    requested_range: Range,
    only: Option<&[CodeActionKind]>,
    supports_document_changes: bool,
) -> Result<Vec<CodeActionOrCommand>, PositionMappingError> {
    let sources = snapshot.name_snapshot().sources();
    let names = snapshot.name_snapshot().names();
    let Ok(text) = sources.source_text(source_id) else {
        return Ok(Vec::new());
    };
    let parsed_file = snapshot
        .name_snapshot()
        .parsed_files()
        .iter()
        .find(|p| p.source_id() == source_id);
    let mut actions = Vec::new();

    let allow_quickfix = kind_allowed(only, &CodeActionKind::QUICKFIX);
    let allow_refactor = kind_allowed(only, &CodeActionKind::REFACTOR_REWRITE);

    // 1. QuickFix: Auto-import for L0080 unresolved names overlapping `requested_range`.
    if allow_quickfix {
        let current_pkg = names
            .index()
            .source_units()
            .iter()
            .find(|u| u.id() == source_unit)
            .map(|u| u.package());
        let existing_imports = collect_existing_import_paths(parsed_file, text);
        let (insert_pos, insert_prefix, insert_suffix) =
            import_insertion_point(sources, parsed_file, text)?;

        let mut seen_fqns: BTreeSet<String> = BTreeSet::new();
        for diag in snapshot.diagnostics() {
            if diag.code().to_string() != UNRESOLVED_NAME_CODE
                || diag.primary_span().source_id() != source_id
            {
                continue;
            }
            let diag_range = span_range(sources, diag.primary_span())?;
            if !ranges_overlap(diag_range, requested_range) {
                continue;
            }
            let Ok(ident) = sources.slice(diag.primary_span()) else {
                continue;
            };
            if !is_valid_rename_identifier(ident) {
                continue;
            }

            // Collect all unique importable FQNs matching `ident` in other packages.
            let mut candidates: BTreeSet<String> = BTreeSet::new();
            for decl in names.index().declarations() {
                if decl.name() != ident || Some(decl.package()) == current_pkg {
                    continue;
                }
                if !matches!(
                    decl.visibility(),
                    DeclarationVisibility::Public | DeclarationVisibility::Internal
                ) {
                    continue;
                }
                let Some(pkg_name) = names
                    .index()
                    .packages()
                    .get(decl.package().index())
                    .and_then(|p| p.name())
                else {
                    continue;
                };
                let fqn = if pkg_name.is_default() {
                    decl.name().to_owned()
                } else {
                    format!("{}.{}", pkg_name.segments().join("."), decl.name())
                };
                if !existing_imports.contains(&fqn) {
                    candidates.insert(fqn);
                }
            }

            let is_preferred = candidates.len() == 1;
            for fqn in candidates {
                if !seen_fqns.insert(fqn.clone()) {
                    continue;
                }
                let new_text = format!("{insert_prefix}import {fqn}{insert_suffix}");
                let edit = build_single_file_edit(
                    uri,
                    doc_version,
                    TextEdit::new(Range::new(insert_pos, insert_pos), new_text),
                    supports_document_changes,
                );
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: format!("Import {fqn}"),
                    kind: Some(CodeActionKind::QUICKFIX),
                    is_preferred: is_preferred.then_some(true),
                    edit: Some(edit),
                    ..CodeAction::default()
                }));
            }
        }
    }

    // 2. Refactor: Add explicit type annotation `: T` on unannotated `val`/`var` intersecting `requested_range`.
    if allow_refactor
        && let Some(typed) = snapshot.typed()
        && let Some(parsed) = parsed_file
        && let Some(unit_names) = names
            .source_units()
            .iter()
            .find(|u| u.source_unit() == source_unit)
    {
        let mut seen_offsets: BTreeSet<usize> = BTreeSet::new();
        for (_, node) in parsed.ast().items().iter() {
            let Item::Variable {
                name: NameMarker::Present(name_span),
                type_ref: None,
                ..
            } = node.payload()
            else {
                continue;
            };
            if name_span.source_id() != source_id || !seen_offsets.insert(name_span.start()) {
                continue;
            }
            let item_range = span_range(sources, node.span())?;
            if !ranges_overlap(item_range, requested_range) {
                continue;
            }
            let Some(sym) = unit_names
                .resolution()
                .symbols()
                .iter()
                .find(|s| s.span() == *name_span)
            else {
                continue;
            };
            let Some(ty_id) = typed.symbol_type(UnitSymbolId::new(source_unit, sym.id())) else {
                continue;
            };
            let Some(ty_text) = format_unit_type(ty_id, typed, names) else {
                continue;
            };
            let name_end = span_range(sources, *name_span)?.end;
            let edit = build_single_file_edit(
                uri,
                doc_version,
                TextEdit::new(Range::new(name_end, name_end), format!(": {ty_text}")),
                supports_document_changes,
            );
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Add explicit type ': {ty_text}'"),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(edit),
                ..CodeAction::default()
            }));
        }
    }

    Ok(actions)
}

fn kind_allowed(only: Option<&[CodeActionKind]>, candidate: &CodeActionKind) -> bool {
    let Some(only) = only else {
        return true;
    };
    let cand_str = candidate.as_str();
    only.iter().any(|allowed| {
        let a = allowed.as_str();
        cand_str == a || cand_str.starts_with(&format!("{a}."))
    })
}

fn ranges_overlap(a: Range, b: Range) -> bool {
    !(a.end < b.start || b.end < a.start)
}

fn collect_existing_import_paths(parsed_file: Option<&ParsedFile>, text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(parsed) = parsed_file else {
        return out;
    };
    for import in parsed.imports() {
        if import.wildcard_span.is_some() {
            continue;
        }
        let segs: Vec<&str> = import
            .segments
            .iter()
            .filter_map(|s| {
                (s.span.start() <= s.span.end() && s.span.end() <= text.len())
                    .then_some(&text[s.span.start()..s.span.end()])
            })
            .collect();
        if !segs.is_empty() {
            out.insert(segs.join("."));
        }
    }
    out
}

/// 返回 `(insert_position, prefix, suffix)`，在现有 `import` 末尾、`package` 后空行或文件顶部插入导入。
fn import_insertion_point(
    sources: &lang_frontend::source::SourceMap,
    parsed_file: Option<&ParsedFile>,
    text: &str,
) -> Result<(Position, &'static str, &'static str), PositionMappingError> {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    if let Some(parsed) = parsed_file {
        if let Some(last_import) = parsed.imports().last() {
            let line = span_range(sources, last_import.span)?.end.line;
            return Ok((Position::new(line + 1, 0), "", nl));
        }
        if let Some(pkg) = parsed.package() {
            let line = span_range(sources, pkg.span)?.end.line;
            return Ok((Position::new(line + 1, 0), nl, nl));
        }
    }
    Ok((Position::new(0, 0), "", nl))
}

fn build_single_file_edit(
    uri: &Uri,
    doc_version: Option<i32>,
    edit: TextEdit,
    supports_document_changes: bool,
) -> WorkspaceEdit {
    if supports_document_changes {
        WorkspaceEdit {
            document_changes: Some(DocumentChanges::Operations(vec![
                DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier {
                        uri: uri.clone(),
                        version: doc_version,
                    },
                    edits: vec![OneOf::Left(edit)],
                }),
            ])),
            ..WorkspaceEdit::default()
        }
    } else {
        #[allow(clippy::mutable_key_type)]
        let changes = HashMap::from([(uri.clone(), vec![edit])]);
        WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        }
    }
}
