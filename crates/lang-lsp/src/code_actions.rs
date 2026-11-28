//! `textDocument/codeAction` 实现：未解析名称自动导入（QuickFix）与显式类型标注（Refactor）。

use std::collections::{BTreeSet, HashMap};

use lang_frontend::{
    lexer::{LexemeKind, Symbol, TokenKind},
    name_resolution::{
        CompilationUnitNames, DeclarationId, DeclarationVisibility, Namespace, SourceUnitId,
        UnitReferenceTarget, UnitSymbolId,
    },
    parser::{ClassifierKind, Item, NameMarker, ParsedFile},
    source::SourceId,
    type_checking::{CompilationUnitTypes, ParameterMode, UnitTypeId, UnitTypeKind},
};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, DocumentChangeOperation, DocumentChanges,
    OneOf, OptionalVersionedTextDocumentIdentifier, Position, Range, TextDocumentEdit, TextEdit,
    Uri, WorkspaceEdit,
};

use crate::{
    position_adapter::{PositionMappingError, span_range, utf16_position},
    rename::is_valid_rename_identifier,
    scope_query::{non_trivia_tokens_through, visible_declarations},
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

const UNRESOLVED_NAME_CODE: &str = "L0080";
const MISSING_INTERFACE_MEMBER_CODE: &str = "L0101";

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
    let parsed_files = snapshot.name_snapshot().parsed_files();
    let parsed_file = parsed_files.iter().find(|p| p.source_id() == source_id);
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
        let existing_imports = collect_existing_import_bindings(parsed_file, text);
        let (insert_pos, insert_prefix, insert_suffix) =
            import_insertion_point(sources, source_id, parsed_file, text)?;

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

            // Only offer auto-import for genuine bare unresolved name/type references (never for
            // missing/inaccessible members like `x.foo()`, qualified paths `a.foo`, or `import` paths).
            let span = diag.primary_span();
            let inside_import = parsed_file.is_some_and(|p| {
                p.imports()
                    .iter()
                    .any(|imp| imp.span.start() <= span.start() && span.end() <= imp.span.end())
            });
            if inside_import {
                continue;
            }
            let preceded_by_dot = non_trivia_tokens_through(sources, source_id, span.start())
                .last()
                .is_some_and(|tok| {
                    matches!(
                        tok.kind(),
                        LexemeKind::Token(TokenKind::Symbol(Symbol::Dot | Symbol::QuestionDot))
                    )
                });
            if preceded_by_dot {
                continue;
            }
            let Some(unresolved_ref) = names.references().iter().find(|r| {
                r.source_unit() == source_unit
                    && r.span() == span
                    && matches!(r.target(), UnitReferenceTarget::Unresolved)
            }) else {
                continue;
            };
            let expected_ns = unresolved_ref.namespace().or_else(|| {
                parsed_file.and_then(|p| {
                    let in_type = p.ast().type_refs().iter().any(|(_, n)| {
                        n.span().start() <= span.start() && span.end() <= n.span().end()
                    });
                    in_type.then_some(Namespace::Type)
                })
            });

            // Collect all unique importable FQNs matching `ident` and namespace in other packages.
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
                let ns_ok = match expected_ns {
                    Some(Namespace::Type) => decl.namespace() == Namespace::Type,
                    Some(Namespace::Value) => {
                        decl.namespace() == Namespace::Value
                            || is_constructible_classifier(decl, parsed_files)
                    }
                    None => true,
                };
                if !ns_ok {
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
                // Only skip if this exact `(fqn, local_name)` binding already exists.
                if !existing_imports.contains(&(fqn.clone(), ident.to_owned())) {
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
        // 1b. QuickFix: Implement missing abstract interface members for L0101.
        if let Some(typed) = snapshot.typed()
            && let Some(parsed) = parsed_file
        {
            let visible_types: Vec<(String, DeclarationId)> =
                visible_declarations(names, Some(parsed), sources, source_unit)
                    .into_iter()
                    .filter(|(_, d)| d.namespace() == Namespace::Type)
                    .map(|(n, d)| (n, d.id()))
                    .collect();
            add_missing_interface_member_actions(
                snapshot,
                typed,
                names,
                parsed,
                parsed_files,
                sources,
                source_unit,
                source_id,
                text,
                uri,
                doc_version,
                requested_range,
                &visible_types,
                supports_document_changes,
                &mut actions,
            )?;
        }

        // 1c. QuickFix: Remove unused exact imports intersecting `requested_range`.
        if let Some(parsed) = parsed_file {
            add_unused_import_actions(
                names,
                parsed,
                sources,
                source_unit,
                source_id,
                text,
                uri,
                doc_version,
                requested_range,
                supports_document_changes,
                &mut actions,
            )?;
        }
    }

    // 2. Refactor: Add explicit type annotation `: T` on unannotated `val`/`var` intersecting `requested_range`,
    //    preserving import aliases or using qualified names when the bare name does not resolve to the target type.
    if allow_refactor
        && let Some(typed) = snapshot.typed()
        && let Some(parsed) = parsed_file
        && let Some(unit_names) = names
            .source_units()
            .iter()
            .find(|u| u.source_unit() == source_unit)
    {
        let visible_types: Vec<(String, DeclarationId)> =
            visible_declarations(names, Some(parsed), sources, source_unit)
                .into_iter()
                .filter(|(_, d)| d.namespace() == Namespace::Type)
                .map(|(n, d)| (n, d.id()))
                .collect();
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
            let Some(ty_text) = format_type_in_scope(ty_id, typed, names, &visible_types, 0) else {
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

fn is_constructible_classifier(
    decl: &lang_frontend::name_resolution::UnitDeclaration,
    parsed_files: &[ParsedFile],
) -> bool {
    if decl.namespace() != Namespace::Type {
        return false;
    }
    let Some(parsed) = parsed_files
        .iter()
        .find(|p| p.source_id() == decl.name_span().source_id())
    else {
        return false;
    };
    let Ok(node) = parsed.ast().items().get(decl.root()) else {
        return false;
    };
    let payload = match node.payload() {
        Item::Modified { declaration, .. } => parsed
            .ast()
            .items()
            .get(*declaration)
            .ok()
            .map(|n| n.payload()),
        other => Some(other),
    };
    matches!(
        payload,
        Some(Item::Classifier(c))
            if matches!(
                c.kind,
                ClassifierKind::Class { .. }
                    | ClassifierKind::ValueClass { .. }
                    | ClassifierKind::Object { .. }
            )
    )
}

/// 按当前文件的作用域/导入别名格式化类型文本，确保插入后可准确解析回原声明。
fn format_type_in_scope(
    ty: UnitTypeId,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    visible_types: &[(String, DeclarationId)],
    depth: usize,
) -> Option<String> {
    if depth > 16 {
        return None;
    }
    match typed.types().get(ty)? {
        UnitTypeKind::Builtin(b) => Some(b.name().to_owned()),
        UnitTypeKind::Nullable(inner) => Some(format!(
            "{}?",
            format_type_in_scope(*inner, typed, names, visible_types, depth + 1)?
        )),
        UnitTypeKind::Function {
            move_only,
            parameters,
            return_type,
        } => {
            let mut params = Vec::with_capacity(parameters.len());
            for p in parameters {
                let pt = format_type_in_scope(p.ty(), typed, names, visible_types, depth + 1)?;
                let mode = match p.mode() {
                    ParameterMode::Value => "own ",
                    ParameterMode::Borrow => "",
                    ParameterMode::Inout => "inout ",
                };
                params.push(format!("{mode}{pt}"));
            }
            let ret = format_type_in_scope(*return_type, typed, names, visible_types, depth + 1)?;
            let prefix = if *move_only { "move " } else { "" };
            Some(format!("{prefix}({}) -> {ret}", params.join(", ")))
        }
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            let base = resolve_nominal_type_spelling(*declaration, names, visible_types)?;
            if arguments.is_empty() {
                Some(base)
            } else {
                let mut args = Vec::with_capacity(arguments.len());
                for arg in arguments {
                    args.push(format_type_in_scope(
                        *arg,
                        typed,
                        names,
                        visible_types,
                        depth + 1,
                    )?);
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
                    args.push(format_type_in_scope(
                        *arg,
                        typed,
                        names,
                        visible_types,
                        depth + 1,
                    )?);
                }
                Some(format!("{base}<{}>", args.join(", ")))
            }
        }
        UnitTypeKind::TypeParameter(sym) => names
            .source_units()
            .iter()
            .find(|u| u.source_unit() == sym.source_unit())
            .and_then(|u| {
                u.resolution()
                    .symbols()
                    .iter()
                    .find(|s| s.id() == sym.symbol())
            })
            .map(|s| s.name().to_owned()),
        UnitTypeKind::StaticSelf(_) => Some("Self".to_owned()),
        UnitTypeKind::Capability(cap) => Some(format!("{cap:?}")),
        UnitTypeKind::EnumCase { .. }
        | UnitTypeKind::IntegerLiteral(_)
        | UnitTypeKind::Deferred(_)
        | UnitTypeKind::Error => None,
    }
}

fn resolve_nominal_type_spelling(
    target_id: DeclarationId,
    names: &CompilationUnitNames,
    visible_types: &[(String, DeclarationId)],
) -> Option<String> {
    // Prefer an unambiguous visible name (including explicit alias like `import model.Point as P`).
    for (visible_name, id) in visible_types {
        if *id == target_id {
            let unique = visible_types
                .iter()
                .filter(|(n, _)| n == visible_name)
                .all(|(_, other)| *other == target_id);
            if unique {
                return Some(visible_name.clone());
            }
        }
    }
    // Otherwise fall back to fully qualified `pkg.Name` when non-default package.
    let decl = names
        .index()
        .declarations()
        .iter()
        .find(|d| d.id() == target_id)?;
    let pkg = names
        .index()
        .packages()
        .get(decl.package().index())
        .and_then(|p| p.name())?;
    if pkg.is_default() {
        None
    } else {
        Some(format!("{}.{}", pkg.segments().join("."), decl.name()))
    }
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

/// 返回已存在的 `(fqn, bound_local_name)` 精确导入集合，避免别名导入屏蔽原名自动导入。
fn collect_existing_import_bindings(
    parsed_file: Option<&ParsedFile>,
    text: &str,
) -> BTreeSet<(String, String)> {
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
        if let Some(&last) = segs.last() {
            let alias = import.alias.and_then(|a| {
                (!a.name_span.is_empty() && a.name_span.end() <= text.len())
                    .then_some(&text[a.name_span.start()..a.name_span.end()])
            });
            out.insert((segs.join("."), alias.unwrap_or(last).to_owned()));
        }
    }
    out
}

/// 按语法边界定位导入插入点（兼容同行 `package app; fun main()` 与紧跟多行块注释）。
pub(crate) fn import_insertion_point(
    sources: &lang_frontend::source::SourceMap,
    source_id: SourceId,
    parsed_file: Option<&ParsedFile>,
    text: &str,
) -> Result<(Position, &'static str, &'static str), PositionMappingError> {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    if let Some(parsed) = parsed_file {
        if let Some(last_import) = parsed.imports().last() {
            return insertion_after_header_span(
                sources,
                source_id,
                text,
                last_import.span.end(),
                nl,
                false,
            );
        }
        if let Some(pkg) = parsed.package() {
            return insertion_after_header_span(sources, source_id, text, pkg.span.end(), nl, true);
        }
    }
    Ok((Position::new(0, 0), "", nl))
}

fn insertion_after_header_span(
    sources: &lang_frontend::source::SourceMap,
    source_id: SourceId,
    text: &str,
    header_end: usize,
    nl: &'static str,
    blank_line_before: bool,
) -> Result<(Position, &'static str, &'static str), PositionMappingError> {
    let bytes = text.as_bytes();
    let mut cursor = header_end.min(bytes.len());
    // Skip horizontal whitespace and an optional `;` immediately terminating the header.
    while cursor < bytes.len() && matches!(bytes[cursor], b' ' | b'\t') {
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b';') {
        cursor += 1;
        while cursor < bytes.len() && matches!(bytes[cursor], b' ' | b'\t') {
            cursor += 1;
        }
    }
    // If the header ends cleanly at a line break or EOF, insert on the next line.
    if cursor >= bytes.len() {
        let pos = utf16_position(sources, source_id, bytes.len())?;
        return Ok((pos, nl, nl));
    }
    if bytes[cursor] == b'\r' {
        let next = if bytes.get(cursor + 1) == Some(&b'\n') {
            cursor + 2
        } else {
            cursor + 1
        };
        let pos = utf16_position(sources, source_id, next)?;
        let prefix = if blank_line_before { nl } else { "" };
        return Ok((pos, prefix, nl));
    }
    if bytes[cursor] == b'\n' {
        let pos = utf16_position(sources, source_id, cursor + 1)?;
        let prefix = if blank_line_before { nl } else { "" };
        return Ok((pos, prefix, nl));
    }
    // Same-line trailing item or block comment (e.g. `package app; fun main() {` or `package app /* ... */`):
    // insert right at `cursor` wrapped in newlines so we never enter a function body or comment.
    let pos = utf16_position(sources, source_id, cursor)?;
    Ok((pos, nl, nl))
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

#[allow(clippy::too_many_arguments)]
fn add_missing_interface_member_actions(
    snapshot: &UnitAnalysisSnapshot,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    parsed: &ParsedFile,
    parsed_files: &[ParsedFile],
    sources: &lang_frontend::source::SourceMap,
    source_unit: SourceUnitId,
    source_id: SourceId,
    text: &str,
    uri: &Uri,
    doc_version: Option<i32>,
    requested_range: Range,
    visible_types: &[(String, DeclarationId)],
    supports_document_changes: bool,
    actions: &mut Vec<CodeActionOrCommand>,
) -> Result<(), PositionMappingError> {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut handled_decls: BTreeSet<DeclarationId> = BTreeSet::new();
    for diag in snapshot.diagnostics() {
        if diag.code().to_string() != MISSING_INTERFACE_MEMBER_CODE
            || diag.primary_span().source_id() != source_id
        {
            continue;
        }
        let diag_range = span_range(sources, diag.primary_span())?;
        if !ranges_overlap(diag_range, requested_range) {
            continue;
        }
        let Some(decl) = names.index().declarations().iter().find(|d| {
            d.source_unit() == source_unit
                && d.namespace() == Namespace::Type
                && d.name_span() == diag.primary_span()
        }) else {
            continue;
        };
        if !handled_decls.insert(decl.id()) {
            continue;
        }
        let Some(nominal) = typed
            .signatures()
            .declaration(decl.id())
            .and_then(|s| s.nominal())
        else {
            continue;
        };
        let existing_methods: BTreeSet<&str> = nominal.members().iter().map(|m| m.name()).collect();

        let mut stubs = Vec::new();
        let mut seen_names: BTreeSet<String> = BTreeSet::new();
        for &iface_ty in nominal.interfaces() {
            let Some(UnitTypeKind::Nominal {
                declaration: iface_id,
                ..
            }) = typed.types().get(iface_ty)
            else {
                continue;
            };
            let Some(iface_nominal) = typed
                .signatures()
                .declaration(*iface_id)
                .and_then(|s| s.nominal())
            else {
                continue;
            };
            for member in iface_nominal.members() {
                if existing_methods.contains(member.name())
                    || !seen_names.insert(member.name().to_owned())
                {
                    continue;
                }
                if !is_abstract_interface_method(parsed_files, member.name_span()) {
                    continue;
                }
                let mut params = Vec::with_capacity(member.parameters().len());
                for p in member.parameters() {
                    let pname = p.name().unwrap_or("_");
                    let pty = format_type_in_scope(p.ty(), typed, names, visible_types, 0)
                        .unwrap_or_else(|| "Any".to_owned());
                    params.push(format!("{pname}: {pty}"));
                }
                let ret =
                    format_type_in_scope(member.return_type(), typed, names, visible_types, 0)
                        .unwrap_or_else(|| "Unit".to_owned());
                stubs.push(format!(
                    "    public override fun {}({}): {ret} {{ error(\"TODO\") }}",
                    member.name(),
                    params.join(", ")
                ));
            }
        }
        if stubs.is_empty() {
            continue;
        }

        let Ok(root_node) = parsed.ast().items().get(decl.root()) else {
            continue;
        };
        let classifier_payload = match root_node.payload() {
            Item::Modified { declaration, .. } => parsed
                .ast()
                .items()
                .get(*declaration)
                .ok()
                .map(|n| n.payload()),
            other => Some(other),
        };
        let Some(Item::Classifier(classifier)) = classifier_payload else {
            continue;
        };
        let (insert_pos, new_text) = if let Some(body) = &classifier.body
            && let Some(rbrace) = body.right_brace_span
        {
            let pos = utf16_position(sources, source_id, rbrace.start())?;
            (pos, format!("{}{nl}", stubs.join(nl)))
        } else {
            let pos = utf16_position(sources, source_id, root_node.span().end())?;
            (pos, format!(" {{{nl}{}{nl}}}", stubs.join(nl)))
        };
        let edit = build_single_file_edit(
            uri,
            doc_version,
            TextEdit::new(Range::new(insert_pos, insert_pos), new_text),
            supports_document_changes,
        );
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Implement missing interface members".to_owned(),
            kind: Some(CodeActionKind::QUICKFIX),
            is_preferred: Some(true),
            edit: Some(edit),
            ..CodeAction::default()
        }));
    }
    Ok(())
}

fn is_abstract_interface_method(
    parsed_files: &[ParsedFile],
    method_name_span: lang_frontend::source::Span,
) -> bool {
    let Some(parsed) = parsed_files
        .iter()
        .find(|p| p.source_id() == method_name_span.source_id())
    else {
        return true;
    };
    for (_, node) in parsed.ast().items().iter() {
        let payload = match node.payload() {
            Item::Modified { declaration, .. } => parsed
                .ast()
                .items()
                .get(*declaration)
                .ok()
                .map(|n| n.payload()),
            other => Some(other),
        };
        if let Some(Item::Function {
            name: NameMarker::Present(s),
            form,
            ..
        }) = payload
            && *s == method_name_span
        {
            return matches!(
                form,
                lang_frontend::parser::FunctionForm::ImplicitUnitAbsent
                    | lang_frontend::parser::FunctionForm::Explicit {
                        body: lang_frontend::parser::FunctionBody::Absent,
                        ..
                    }
            );
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn add_unused_import_actions(
    names: &CompilationUnitNames,
    parsed: &ParsedFile,
    sources: &lang_frontend::source::SourceMap,
    source_unit: SourceUnitId,
    source_id: SourceId,
    text: &str,
    uri: &Uri,
    doc_version: Option<i32>,
    requested_range: Range,
    supports_document_changes: bool,
    actions: &mut Vec<CodeActionOrCommand>,
) -> Result<(), PositionMappingError> {
    if parsed.imports().is_empty() {
        return Ok(());
    }
    let import_spans: Vec<(usize, usize)> = parsed
        .imports()
        .iter()
        .map(|i| (i.span.start(), i.span.end()))
        .collect();
    let mut used_decls: BTreeSet<DeclarationId> = BTreeSet::new();
    for r in names.references() {
        if r.source_unit() != source_unit || r.span().source_id() != source_id {
            continue;
        }
        let in_import = import_spans
            .iter()
            .any(|&(s, e)| s <= r.span().start() && r.span().end() <= e);
        if in_import {
            continue;
        }
        match r.target() {
            UnitReferenceTarget::Declaration(id) => {
                used_decls.insert(*id);
            }
            UnitReferenceTarget::OverloadSet(ids) => {
                used_decls.extend(ids.iter().copied());
            }
            _ => {}
        }
    }

    for import in parsed.imports() {
        if import.wildcard_span.is_some() {
            continue;
        }
        let imp_range = span_range(sources, import.span)?;
        if !ranges_overlap(imp_range, requested_range) {
            continue;
        }
        let mut target_decls: Vec<DeclarationId> = Vec::new();
        for r in names.references() {
            if r.source_unit() == source_unit
                && r.span().source_id() == source_id
                && import.span.start() <= r.span().start()
                && r.span().end() <= import.span.end()
            {
                match r.target() {
                    UnitReferenceTarget::Declaration(id) => target_decls.push(*id),
                    UnitReferenceTarget::OverloadSet(ids) => {
                        target_decls.extend(ids.iter().copied())
                    }
                    _ => {}
                }
            }
        }
        if target_decls.is_empty() || target_decls.iter().any(|id| used_decls.contains(id)) {
            continue;
        }
        // Delete the full import line (including trailing newline when on its own line).
        let bytes = text.as_bytes();
        let mut end_byte = import.span.end();
        while end_byte < bytes.len() && matches!(bytes[end_byte], b' ' | b'\t') {
            end_byte += 1;
        }
        if bytes.get(end_byte) == Some(&b'\r') {
            end_byte += 1;
            if bytes.get(end_byte) == Some(&b'\n') {
                end_byte += 1;
            }
        } else if bytes.get(end_byte) == Some(&b'\n') {
            end_byte += 1;
        }
        let start_pos = utf16_position(sources, source_id, import.span.start())?;
        let end_pos = utf16_position(sources, source_id, end_byte)?;
        let edit = build_single_file_edit(
            uri,
            doc_version,
            TextEdit::new(Range::new(start_pos, end_pos), String::new()),
            supports_document_changes,
        );
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Remove unused import".to_owned(),
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(edit),
            ..CodeAction::default()
        }));
    }
    Ok(())
}
