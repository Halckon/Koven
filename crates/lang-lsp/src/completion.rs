//! `textDocument/completion` 实现：支持真实输入 `.` 错误恢复、词法作用域与导入可见性。

use std::collections::BTreeMap;

use lang_frontend::{
    lexer::{Lexeme, LexemeKind, Symbol, TokenKind},
    name_resolution::{
        CompilationUnitNames, DeclarationVisibility, Namespace, SourceUnitId,
        SymbolKind as FrontSymbolKind, UnitDeclaration, UnitSymbolId,
    },
    parser::{Item, NameMarker, ParsedFile, VisibilityModifier},
    source::SourceId,
    type_checking::{BuiltinType, CompilationUnitTypes, UnitTypeId, UnitTypeKind},
};
use lsp_types::{CompletionItem, CompletionItemKind, InsertTextFormat, Range, TextEdit};

use crate::{
    code_actions::import_insertion_point,
    ide_features::format_unit_type,
    scope_query::{
        collect_qualifier_segments, expression_type_ending_at, find_local_symbol_in_scope,
        find_receiver_expression_start, is_inside_nominal_decl, local_symbols_in_scope,
        non_trivia_tokens_through, recover_unit_snapshot, replace_span_with_spaces,
        replace_span_with_zero, visible_declarations,
    },
    symbols::format_callable_detail,
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

/// 除 `BuiltinType::ALL` 外，由 `standard_environments()` 预声明的能力与内建泛型类型构造器。
const BUILTIN_CAPABILITIES_AND_INTRINSICS: &[&str] = &[
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

const BUILTIN_FUNCTIONS: &[(&str, &str)] = &[
    ("println", "(value: String) -> Unit"),
    ("error", "(message: String) -> Nothing"),
    ("listOf", "(vararg elements: T) -> List<T>"),
    ("mutableListOf", "(vararg elements: T) -> MutableList<T>"),
    ("mapOf", "(vararg entries: Pair<K, V>) -> Map<K, V>"),
    (
        "mutableMapOf",
        "(vararg entries: Pair<K, V>) -> MutableMap<K, V>",
    ),
    ("arrayOf", "(vararg elements: T) -> Array<T>"),
    ("replace", "(inout place: T, own value: T) -> T"),
    ("swap", "(inout a: T, inout b: T) -> Unit"),
    ("rangeView", "(range: IntRange) -> View<Int>"),
    ("textByteLength", "(text: String) -> Int"),
    ("textByteAt", "(text: String, index: Int) -> UByte"),
    ("textUtf8ErrorOffset", "(bytes: Array<UByte>) -> Int"),
    ("textCopyBytes", "(bytes: Array<UByte>) -> String"),
    (
        "textCopySlice",
        "(text: String, start: Int, end: Int) -> String",
    ),
];

/// 计算给定位置的补全候选（支持 `.` / `?.` 成员与多段包补全、作用域/导入感知标识符补全与可选 Snippet）。
#[allow(dead_code)]
pub(crate) fn completions_at(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
) -> Vec<CompletionItem> {
    completions_at_with_options(snapshot, source_unit, source_id, offset, false)
}

/// 带客户端 `snippet_support` 选项计算补全候选。
pub(crate) fn completions_at_with_options(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
    snippet_support: bool,
) -> Vec<CompletionItem> {
    let mut items = compute_completions(snapshot, source_unit, source_id, offset);
    if snippet_support {
        for item in &mut items {
            if matches!(
                item.kind,
                Some(CompletionItemKind::FUNCTION | CompletionItemKind::METHOD)
            ) && item.insert_text.is_none()
            {
                let empty_params = item.detail.as_deref().is_some_and(|d| d.starts_with("()"));
                item.insert_text = Some(if empty_params {
                    format!("{}()", item.label)
                } else {
                    format!("{}($1)", item.label)
                });
                item.insert_text_format = Some(InsertTextFormat::SNIPPET);
            }
        }
    }
    items
}

fn compute_completions(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
) -> Vec<CompletionItem> {
    let sources = snapshot.name_snapshot().sources();
    let Ok(text) = sources.source_text(source_id) else {
        return Vec::new();
    };
    let clamped = offset.min(text.len());
    let tokens = non_trivia_tokens_through(sources, source_id, clamped);

    // Detect `prefix` even when cursor sits inside an existing identifier (e.g. `origin.no|rm()`).
    let (prefix, before_prefix, erase_end) = match tokens.last() {
        Some(last)
            if last.span().start() <= clamped
                && clamped <= last.span().end()
                && matches!(
                    last.kind(),
                    LexemeKind::Token(TokenKind::Identifier | TokenKind::Keyword(_))
                ) =>
        {
            (
                &text[last.span().start()..clamped],
                &tokens[..tokens.len() - 1],
                last.span().end(),
            )
        }
        _ => ("", tokens.as_slice(), clamped),
    };

    // Check if preceded by `.` or `?.` -> member / companion / enum / package completion.
    if let Some(dot) = before_prefix.last()
        && matches!(
            dot.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Dot | Symbol::QuestionDot))
        )
    {
        let receiver_tokens = &before_prefix[..before_prefix.len() - 1];
        if snapshot.typed().is_some() {
            return dot_completions(
                snapshot,
                source_unit,
                source_id,
                text,
                receiver_tokens,
                clamped,
                prefix,
            );
        }
        // Pass 1: erase only `.<prefix>` so value receivers (`origin`, `obj.field`, `factory()`)
        // remain intact expressions and preserve their exact inferred `UnitTypeId`.
        if let Some(repaired_text) = replace_span_with_spaces(text, dot.span().start(), erase_end)
            && let Some((repaired, repaired_su, repaired_sid)) =
                recover_unit_snapshot(snapshot, source_unit, source_id, repaired_text)
        {
            return dot_completions(
                &repaired,
                repaired_su,
                repaired_sid,
                text,
                receiver_tokens,
                clamped,
                prefix,
            );
        }
        // Pass 2: fallback for type/package receivers (`Color.`, `MyGreeter.`, `a.b.MyGreeter.`)
        // by replacing the full receiver expression with a same-length `0   ` placeholder.
        let expr_start =
            find_receiver_expression_start(receiver_tokens).unwrap_or_else(|| dot.span().start());
        if let Some(repaired_text) = replace_span_with_zero(text, expr_start, erase_end)
            && let Some((repaired, repaired_su, repaired_sid)) =
                recover_unit_snapshot(snapshot, source_unit, source_id, repaired_text)
        {
            return dot_completions(
                &repaired,
                repaired_su,
                repaired_sid,
                text,
                receiver_tokens,
                clamped,
                prefix,
            );
        }
        return dot_completions(
            snapshot,
            source_unit,
            source_id,
            text,
            receiver_tokens,
            clamped,
            prefix,
        );
    }

    scope_completions(snapshot, source_unit, source_id, clamped, prefix)
}

#[allow(clippy::too_many_arguments)]
fn dot_completions(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    text: &str,
    receiver_tokens: &[Lexeme],
    cursor_offset: usize,
    prefix: &str,
) -> Vec<CompletionItem> {
    let Some(recv_tok) = receiver_tokens.last() else {
        return Vec::new();
    };
    let names = snapshot.name_snapshot().names();
    let typed = snapshot.typed();
    let prefix_lower = prefix.to_ascii_lowercase();
    let mut items: BTreeMap<String, CompletionItem> = BTreeMap::new();

    // 1. Multi-segment package qualifier completion (e.g. `a.shared.`).
    let qualifier = collect_qualifier_segments(text, receiver_tokens);
    if !qualifier.is_empty() {
        for pkg in names.index().packages() {
            let Some(pkg_name) = pkg.name() else {
                continue;
            };
            let segs = pkg_name.segments();
            if segs.len() > qualifier.len()
                && segs
                    .iter()
                    .zip(qualifier.iter())
                    .all(|(a, b)| a.as_str() == *b)
            {
                let next_seg = &segs[qualifier.len()];
                if matches_prefix(next_seg, &prefix_lower) {
                    insert_item(
                        &mut items,
                        next_seg,
                        CompletionItemKind::MODULE,
                        Some("package".to_owned()),
                        "0",
                    );
                }
            }
        }
        for decl in names.index().declarations() {
            let matches_pkg = names
                .index()
                .packages()
                .get(decl.package().index())
                .and_then(|p| p.name())
                .is_some_and(|name| {
                    name.segments().len() == qualifier.len()
                        && name
                            .segments()
                            .iter()
                            .zip(qualifier.iter())
                            .all(|(a, b)| a.as_str() == *b)
                });
            let accessible = decl.source_unit() == source_unit
                || matches!(
                    decl.visibility(),
                    DeclarationVisibility::Public | DeclarationVisibility::Internal
                );
            if matches_pkg && accessible && matches_prefix(decl.name(), &prefix_lower) {
                let detail = typed.and_then(|t| decl_detail(decl.id(), t, names));
                insert_item(
                    &mut items,
                    decl.name(),
                    map_completion_kind(decl.kind()),
                    detail,
                    "1",
                );
            }
        }
    }

    let Some(typed) = typed else {
        return items.into_values().collect();
    };
    let parsed_files = snapshot.name_snapshot().parsed_files();
    let parsed_file = parsed_files.iter().find(|p| p.source_id() == source_id);

    // 2. If `recv_tok` resolves via definition index to a nominal type, emit static/companion/enum/object members.
    if let Some((_, targets)) = snapshot
        .definitions()
        .entry_at(source_unit, recv_tok.span().start())
        && let Some(target) = targets.first().copied()
        && names.index().declarations().iter().any(|d| {
            d.source_unit() == target.source_unit
                && d.name_span() == target.span
                && d.namespace() == Namespace::Type
        })
    {
        add_target_dot_completions(
            names,
            typed,
            parsed_files,
            source_unit,
            cursor_offset,
            target.source_unit,
            target.span,
            &prefix_lower,
            &mut items,
        );
        return items.into_values().collect();
    }

    // 3. Expression-type lookup handles chained access (`obj.field.`), call results (`factory().`), and locals (`origin.`).
    let expr_start = find_receiver_expression_start(receiver_tokens);
    if let Some(ty_id) = expression_type_ending_at(
        typed,
        parsed_file,
        source_unit,
        expr_start,
        recv_tok.span().end(),
    ) {
        add_type_member_completions(
            ty_id,
            typed,
            names,
            parsed_files,
            source_unit,
            cursor_offset,
            &prefix_lower,
            &mut items,
        );
        return items.into_values().collect();
    }

    // 4. Definition-index lookup for value symbols when receiver AST was replaced or partial.
    if let Some((_, targets)) = snapshot
        .definitions()
        .entry_at(source_unit, recv_tok.span().start())
        && let Some(target) = targets.first().copied()
    {
        add_target_dot_completions(
            names,
            typed,
            parsed_files,
            source_unit,
            cursor_offset,
            target.source_unit,
            target.span,
            &prefix_lower,
            &mut items,
        );
        return items.into_values().collect();
    }

    // 5. Package-qualified nominal type receiver (e.g. `a.b.MyGreeter.` recovered via Pass 2).
    if qualifier.len() >= 2 {
        let (pkg_segs, type_name) = (
            &qualifier[..qualifier.len() - 1],
            qualifier[qualifier.len() - 1],
        );
        for decl in names.index().declarations() {
            if decl.namespace() != Namespace::Type || decl.name() != type_name {
                continue;
            }
            let pkg_matches = names
                .index()
                .packages()
                .get(decl.package().index())
                .and_then(|p| p.name())
                .is_some_and(|name| {
                    name.segments().len() == pkg_segs.len()
                        && name
                            .segments()
                            .iter()
                            .zip(pkg_segs.iter())
                            .all(|(a, b)| a.as_str() == *b)
                });
            let accessible = decl.source_unit() == source_unit
                || matches!(
                    decl.visibility(),
                    DeclarationVisibility::Public | DeclarationVisibility::Internal
                );
            if pkg_matches && accessible {
                add_target_dot_completions(
                    names,
                    typed,
                    parsed_files,
                    source_unit,
                    cursor_offset,
                    decl.source_unit(),
                    decl.name_span(),
                    &prefix_lower,
                    &mut items,
                );
            }
        }
        if !items.is_empty() {
            return items.into_values().collect();
        }
    }

    // 6. Lexical scope or visible declaration fallback (e.g. `Color.` or `MyGreeter.` recovered via Pass 2).
    let recv_name = &text[recv_tok.span().start()..recv_tok.span().end()];
    if let Some((sym_id, _)) = find_local_symbol_in_scope(
        names,
        parsed_file,
        source_unit,
        source_id,
        recv_tok.span().start(),
        recv_name,
    ) && let Some(ty_id) = typed.symbol_type(UnitSymbolId::new(source_unit, sym_id))
    {
        add_type_member_completions(
            ty_id,
            typed,
            names,
            parsed_files,
            source_unit,
            cursor_offset,
            &prefix_lower,
            &mut items,
        );
        return items.into_values().collect();
    }

    for (visible_name, decl) in visible_declarations(
        names,
        parsed_file,
        snapshot.name_snapshot().sources(),
        source_unit,
    ) {
        if visible_name == recv_name {
            add_target_dot_completions(
                names,
                typed,
                parsed_files,
                source_unit,
                cursor_offset,
                decl.source_unit(),
                decl.name_span(),
                &prefix_lower,
                &mut items,
            );
        }
    }

    items.into_values().collect()
}

#[allow(clippy::too_many_arguments)]
fn add_target_dot_completions(
    names: &CompilationUnitNames,
    typed: &CompilationUnitTypes,
    parsed_files: &[ParsedFile],
    caller_unit: SourceUnitId,
    cursor_offset: usize,
    target_unit: SourceUnitId,
    target_span: lang_frontend::source::Span,
    prefix_lower: &str,
    items: &mut BTreeMap<String, CompletionItem>,
) {
    // Case A: target is a nominal type declaration (static companion / constants / enum cases).
    if let Some(decl) = names.index().declarations().iter().find(|d| {
        d.source_unit() == target_unit
            && d.name_span() == target_span
            && d.namespace() == Namespace::Type
    }) && let Some(sig) = typed.signatures().declaration(decl.id())
        && let Some(nominal) = sig.nominal()
    {
        let inside_decl = is_inside_nominal_decl(parsed_files, decl, caller_unit, cursor_offset);
        for member in nominal.companion_members() {
            if (inside_decl || member.visibility() != DeclarationVisibility::Private)
                && matches_prefix(member.name(), prefix_lower)
            {
                insert_item(
                    items,
                    member.name(),
                    CompletionItemKind::METHOD,
                    Some(format_callable_detail(member, typed, names)),
                    "0",
                );
            }
        }
        // Companion constants with AST visibility check.
        add_companion_constant_completions(
            decl,
            parsed_files,
            names,
            typed,
            inside_decl,
            prefix_lower,
            items,
        );
        for case in nominal.enum_cases() {
            if matches_prefix(case.name(), prefix_lower) {
                insert_item(
                    items,
                    case.name(),
                    CompletionItemKind::ENUM_MEMBER,
                    Some(decl.name().to_owned()),
                    "0",
                );
            }
        }
        // If this is a singleton `object`, also expose its instance members directly.
        if nominal.kind() == lang_frontend::type_checking::NominalKind::Object {
            add_nominal_instance_members(
                decl,
                nominal,
                typed,
                names,
                parsed_files,
                caller_unit,
                cursor_offset,
                prefix_lower,
                items,
            );
        }
    }

    // Case B: target is a value symbol (local/param/const/field) whose type is a Nominal type.
    if let Some(unit_names) = names
        .source_units()
        .iter()
        .find(|u| u.source_unit() == target_unit)
        && let Some(sym) = unit_names
            .resolution()
            .symbols()
            .iter()
            .find(|s| s.span() == target_span)
        && let Some(ty_id) = typed.symbol_type(UnitSymbolId::new(target_unit, sym.id()))
    {
        add_type_member_completions(
            ty_id,
            typed,
            names,
            parsed_files,
            caller_unit,
            cursor_offset,
            prefix_lower,
            items,
        );
    }
}

fn add_companion_constant_completions(
    decl: &UnitDeclaration,
    parsed_files: &[ParsedFile],
    names: &CompilationUnitNames,
    typed: &CompilationUnitTypes,
    inside_decl: bool,
    prefix_lower: &str,
    items: &mut BTreeMap<String, CompletionItem>,
) {
    let Some(parsed) = parsed_files
        .iter()
        .find(|p| p.source_id() == decl.name_span().source_id())
    else {
        return;
    };
    let Ok(root_node) = parsed.ast().items().get(decl.root()) else {
        return;
    };
    let classifier_item = match root_node.payload() {
        Item::Modified { declaration, .. } => parsed
            .ast()
            .items()
            .get(*declaration)
            .ok()
            .map(|n| n.payload()),
        other => Some(other),
    };
    let Some(Item::Classifier(classifier)) = classifier_item else {
        return;
    };
    let Some(body) = &classifier.body else {
        return;
    };
    let unit_names = names
        .source_units()
        .iter()
        .find(|u| u.source_unit() == decl.source_unit());

    for &member_id in &body.members {
        let Ok(member_node) = parsed.ast().items().get(member_id) else {
            continue;
        };
        let member_payload = match member_node.payload() {
            Item::Modified { declaration, .. } => parsed
                .ast()
                .items()
                .get(*declaration)
                .ok()
                .map(|n| n.payload()),
            other => Some(other),
        };
        let Some(Item::Companion(comp)) = member_payload else {
            continue;
        };
        for &comp_item_id in &comp.body.members {
            let Ok(comp_node) = parsed.ast().items().get(comp_item_id) else {
                continue;
            };
            let (is_private, inner_payload) = match comp_node.payload() {
                Item::Modified {
                    modifiers,
                    declaration,
                } => (
                    matches!(modifiers.visibility, Some(VisibilityModifier::Private(_))),
                    parsed
                        .ast()
                        .items()
                        .get(*declaration)
                        .ok()
                        .map(|n| n.payload()),
                ),
                other => (false, Some(other)),
            };
            if is_private && !inside_decl {
                continue;
            }
            if let Some(Item::Constant {
                name: NameMarker::Present(name_span),
                ..
            }) = inner_payload
                && let Some(sym) = unit_names.and_then(|u| {
                    u.resolution()
                        .symbols()
                        .iter()
                        .find(|s| s.span() == *name_span)
                })
                && matches_prefix(sym.name(), prefix_lower)
            {
                let detail = typed
                    .symbol_type(UnitSymbolId::new(decl.source_unit(), sym.id()))
                    .and_then(|ty| format_unit_type(ty, typed, names));
                insert_item(items, sym.name(), CompletionItemKind::CONSTANT, detail, "0");
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn add_type_member_completions(
    ty_id: UnitTypeId,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    parsed_files: &[ParsedFile],
    caller_unit: SourceUnitId,
    cursor_offset: usize,
    prefix_lower: &str,
    items: &mut BTreeMap<String, CompletionItem>,
) {
    let inner = match typed.types().get(ty_id) {
        Some(UnitTypeKind::Nullable(inner)) => *inner,
        _ => ty_id,
    };
    let Some(UnitTypeKind::Nominal { declaration, .. }) = typed.types().get(inner) else {
        return;
    };
    let Some(decl) = names
        .index()
        .declarations()
        .iter()
        .find(|d| d.id() == *declaration)
    else {
        return;
    };
    let Some(sig) = typed.signatures().declaration(*declaration) else {
        return;
    };
    let Some(nominal) = sig.nominal() else {
        return;
    };
    add_nominal_instance_members(
        decl,
        nominal,
        typed,
        names,
        parsed_files,
        caller_unit,
        cursor_offset,
        prefix_lower,
        items,
    );
}

#[allow(clippy::too_many_arguments)]
fn add_nominal_instance_members(
    owner_decl: &UnitDeclaration,
    nominal: &lang_frontend::type_checking::UnitNominalSignature,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    parsed_files: &[ParsedFile],
    caller_unit: SourceUnitId,
    cursor_offset: usize,
    prefix_lower: &str,
    items: &mut BTreeMap<String, CompletionItem>,
) {
    let inside_owner = is_inside_nominal_decl(parsed_files, owner_decl, caller_unit, cursor_offset);
    for field in nominal.fields() {
        if (inside_owner || field.visibility() != DeclarationVisibility::Private)
            && matches_prefix(field.name(), prefix_lower)
        {
            insert_item(
                items,
                field.name(),
                CompletionItemKind::FIELD,
                format_unit_type(field.ty(), typed, names),
                "0",
            );
        }
    }
    for member in nominal.members() {
        if (inside_owner || member.visibility() != DeclarationVisibility::Private)
            && matches_prefix(member.name(), prefix_lower)
        {
            insert_item(
                items,
                member.name(),
                CompletionItemKind::METHOD,
                Some(format_callable_detail(member, typed, names)),
                "1",
            );
        }
    }
    // Inherited interface methods.
    for &iface_ty in nominal.interfaces() {
        let Some(UnitTypeKind::Nominal { declaration, .. }) = typed.types().get(iface_ty) else {
            continue;
        };
        let Some(iface_decl) = names
            .index()
            .declarations()
            .iter()
            .find(|d| d.id() == *declaration)
        else {
            continue;
        };
        let Some(iface_nominal) = typed
            .signatures()
            .declaration(*declaration)
            .and_then(|s| s.nominal())
        else {
            continue;
        };
        let inside_iface =
            is_inside_nominal_decl(parsed_files, iface_decl, caller_unit, cursor_offset);
        for member in iface_nominal.members() {
            if (inside_iface || member.visibility() != DeclarationVisibility::Private)
                && matches_prefix(member.name(), prefix_lower)
            {
                insert_item(
                    items,
                    member.name(),
                    CompletionItemKind::METHOD,
                    Some(format_callable_detail(member, typed, names)),
                    "2",
                );
            }
        }
    }
}

fn scope_completions(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
    prefix: &str,
) -> Vec<CompletionItem> {
    let names = snapshot.name_snapshot().names();
    let typed = snapshot.typed();
    let sources = snapshot.name_snapshot().sources();
    let parsed_file = snapshot
        .name_snapshot()
        .parsed_files()
        .iter()
        .find(|p| p.source_id() == source_id);
    let prefix_lower = prefix.to_ascii_lowercase();
    let mut items: BTreeMap<String, CompletionItem> = BTreeMap::new();

    // 1. Lexically enclosing local scopes (innermost -> outermost; excludes self-initializer).
    for (sym_id, sym) in local_symbols_in_scope(names, parsed_file, source_unit, source_id, offset)
    {
        if !matches_prefix(sym.name(), &prefix_lower) {
            continue;
        }
        let detail = typed.and_then(|t| {
            t.symbol_type(UnitSymbolId::new(source_unit, sym_id))
                .and_then(|ty| format_unit_type(ty, t, names))
        });
        insert_item(
            &mut items,
            sym.name(),
            map_completion_kind(sym.kind()),
            detail,
            "0",
        );
    }

    // 2. Visible top-level declarations (same file, same package non-private, and imports/aliases).
    let current_pkg = names
        .index()
        .source_units()
        .iter()
        .find(|u| u.id() == source_unit)
        .map(|u| u.package());
    for (visible_name, decl) in visible_declarations(names, parsed_file, sources, source_unit) {
        if !matches_prefix(&visible_name, &prefix_lower) {
            continue;
        }
        let detail = typed.and_then(|t| decl_detail(decl.id(), t, names));
        let rank = if Some(decl.package()) == current_pkg {
            "1"
        } else {
            "2"
        };
        insert_item(
            &mut items,
            &visible_name,
            map_completion_kind(decl.kind()),
            detail,
            rank,
        );
    }

    // 2b. Cross-package unimported accessible declarations with `additionalTextEdits` auto-import.
    if !prefix_lower.is_empty()
        && let Ok(text) = sources.source_text(source_id)
        && let Ok((insert_pos, insert_prefix, insert_suffix)) =
            import_insertion_point(sources, source_id, parsed_file, text)
    {
        for decl in names.index().declarations() {
            if Some(decl.package()) == current_pkg
                || !matches!(
                    decl.visibility(),
                    DeclarationVisibility::Public | DeclarationVisibility::Internal
                )
                || !matches_prefix(decl.name(), &prefix_lower)
                || items.contains_key(decl.name())
            {
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
            let base_detail = typed.and_then(|t| decl_detail(decl.id(), t, names));
            let detail = match base_detail {
                Some(d) => Some(format!("{d} (import {fqn})")),
                None => Some(format!("import {fqn}")),
            };
            let edit = TextEdit::new(
                Range::new(insert_pos, insert_pos),
                format!("{insert_prefix}import {fqn}{insert_suffix}"),
            );
            items.insert(
                decl.name().to_owned(),
                CompletionItem {
                    label: decl.name().to_owned(),
                    kind: Some(map_completion_kind(decl.kind())),
                    detail,
                    sort_text: Some(format!("2z_{}", decl.name())),
                    additional_text_edits: Some(vec![edit]),
                    ..CompletionItem::default()
                },
            );
        }
    }

    // 3. Builtin functions and types from standard_environments().
    for &(name, sig) in BUILTIN_FUNCTIONS {
        if matches_prefix(name, &prefix_lower) {
            insert_item(
                &mut items,
                name,
                CompletionItemKind::FUNCTION,
                Some(sig.to_owned()),
                "3",
            );
        }
    }
    for ty in BuiltinType::ALL
        .iter()
        .map(|b| b.name())
        .chain(BUILTIN_CAPABILITIES_AND_INTRINSICS.iter().copied())
    {
        if matches_prefix(ty, &prefix_lower) {
            insert_item(
                &mut items,
                ty,
                CompletionItemKind::STRUCT,
                Some("builtin type".to_owned()),
                "4",
            );
        }
    }

    items.into_values().collect()
}

fn decl_detail(
    decl_id: lang_frontend::name_resolution::DeclarationId,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
) -> Option<String> {
    let sig = typed.signatures().declaration(decl_id)?;
    if let Some(callable) = sig.callable() {
        Some(format_callable_detail(callable, typed, names))
    } else {
        format_unit_type(sig.ty(), typed, names)
    }
}

fn matches_prefix(name: &str, prefix_lower: &str) -> bool {
    prefix_lower.is_empty() || name.to_ascii_lowercase().starts_with(prefix_lower)
}

fn insert_item(
    items: &mut BTreeMap<String, CompletionItem>,
    name: &str,
    kind: CompletionItemKind,
    detail: Option<String>,
    rank: &str,
) {
    items
        .entry(name.to_owned())
        .or_insert_with(|| CompletionItem {
            label: name.to_owned(),
            kind: Some(kind),
            detail,
            sort_text: Some(format!("{rank}_{name}")),
            ..CompletionItem::default()
        });
}

fn map_completion_kind(kind: FrontSymbolKind) -> CompletionItemKind {
    match kind {
        FrontSymbolKind::Function => CompletionItemKind::FUNCTION,
        FrontSymbolKind::Constant => CompletionItemKind::CONSTANT,
        FrontSymbolKind::Variable
        | FrontSymbolKind::ValueParameter
        | FrontSymbolKind::ForBinding
        | FrontSymbolKind::LambdaParameter => CompletionItemKind::VARIABLE,
        FrontSymbolKind::Field => CompletionItemKind::FIELD,
        FrontSymbolKind::EnumVariant => CompletionItemKind::ENUM_MEMBER,
        FrontSymbolKind::Classifier => CompletionItemKind::CLASS,
        FrontSymbolKind::TypeParameter => CompletionItemKind::TYPE_PARAMETER,
        _ => CompletionItemKind::TEXT,
    }
}
