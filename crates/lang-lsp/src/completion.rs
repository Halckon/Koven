//! `textDocument/completion` 实现：支持真实输入 `.` 错误恢复、词法作用域与导入可见性。

use std::collections::BTreeMap;

use lang_frontend::{
    analysis::{UnitSourceDescriptor, analyze_unit_names},
    lexer::{Lexeme, LexemeKind, Symbol, TokenKind, lex},
    name_resolution::{
        CompilationUnitNames, DeclarationVisibility, Namespace, ScopeId, SourceUnitId,
        SymbolKind as FrontSymbolKind, UnitDeclaration, UnitSymbolId,
    },
    parser::ParsedFile,
    source::{SourceId, SourceMap},
    type_checking::{
        CompilationUnitTypes, UnitTypeId, UnitTypeKind, check_compilation_unit_types,
        standard_environments,
    },
};
use lsp_types::{CompletionItem, CompletionItemKind};

use crate::{
    ide_features::format_unit_type, symbols::format_callable_detail,
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

const BUILTIN_TYPES: &[&str] = &[
    "Any",
    "Array",
    "Boolean",
    "Box",
    "Byte",
    "Char",
    "Copyable",
    "Double",
    "Err",
    "Float",
    "Hashable",
    "Int",
    "Int8",
    "Int16",
    "Int32",
    "Int64",
    "IntRange",
    "List",
    "Long",
    "LongRange",
    "Map",
    "MutableList",
    "MutableMap",
    "Nothing",
    "Ok",
    "Pair",
    "Rc",
    "Result",
    "Short",
    "String",
    "Transferable",
    "UInt",
    "UInt8",
    "UInt16",
    "UInt32",
    "UInt64",
    "Unit",
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
];

/// 计算给定位置的补全候选（支持 `.` / `?.` 成员与多段包补全，以及作用域/导入感知标识符补全）。
pub(crate) fn completions_at(
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
                prefix,
            );
        }
        // P1: When typing `origin.` or `Color.` or `MyGreeter.` caused a parse error so `typed()`
        // is `None`, replace the incomplete `<receiver>.<prefix>` with a same-length dummy literal
        // (`0   `) in memory so surrounding declarations/locals type-check cleanly.
        let replace_start = receiver_tokens
            .last()
            .map_or(dot.span().start(), |tok| tok.span().start());
        if let Some((repaired, repaired_su, repaired_sid)) =
            recover_snapshot_for_dot(snapshot, source_unit, source_id, replace_start, erase_end)
        {
            return dot_completions(
                &repaired,
                repaired_su,
                repaired_sid,
                text,
                receiver_tokens,
                prefix,
            );
        }
        return dot_completions(
            snapshot,
            source_unit,
            source_id,
            text,
            receiver_tokens,
            prefix,
        );
    }

    scope_completions(snapshot, source_unit, source_id, clamped, prefix)
}

fn recover_snapshot_for_dot(
    snapshot: &UnitAnalysisSnapshot,
    target_unit: SourceUnitId,
    target_source_id: SourceId,
    dot_start: usize,
    erase_end: usize,
) -> Option<(UnitAnalysisSnapshot, SourceUnitId, SourceId)> {
    let orig_sources = snapshot.name_snapshot().sources();
    let orig_units = snapshot.name_snapshot().names().index().source_units();
    let mut new_sources = SourceMap::new();
    let mut descriptors = Vec::with_capacity(orig_units.len());
    let mut new_target_sid = None;

    for su in orig_units {
        let sid = su.source_id();
        let name = orig_sources.source_name(sid).ok()?;
        let orig_text = orig_sources.source_text(sid).ok()?;
        let text = if su.id() == target_unit && sid == target_source_id {
            if dot_start > erase_end || erase_end > orig_text.len() {
                return None;
            }
            let mut repaired = String::with_capacity(orig_text.len());
            repaired.push_str(&orig_text[..dot_start]);
            let mut wrote_placeholder = false;
            for ch in orig_text[dot_start..erase_end].chars() {
                if ch == '\n' {
                    repaired.push('\n');
                } else if !wrote_placeholder {
                    repaired.push('0');
                    wrote_placeholder = true;
                } else {
                    repaired.push(' ');
                }
            }
            repaired.push_str(&orig_text[erase_end..]);
            repaired
        } else {
            orig_text.to_owned()
        };
        let new_sid = new_sources.add_source(name, text).ok()?;
        if su.id() == target_unit {
            new_target_sid = Some(new_sid);
        }
        let desc = match su.key() {
            lang_frontend::name_resolution::SourceUnitKey::Rooted { root, logical_path } => {
                UnitSourceDescriptor::new(root.as_str(), logical_path.as_str(), new_sid)
            }
            lang_frontend::name_resolution::SourceUnitKey::MemoryDocument => {
                UnitSourceDescriptor::memory_document(new_sid)
            }
        };
        descriptors.push(desc);
    }

    let (name_env, type_env) = standard_environments();
    let name_snapshot = analyze_unit_names(new_sources, descriptors, name_env).ok()?;
    let validated = name_snapshot.validated_names()?;
    let inputs = name_snapshot.inputs();
    let typed =
        check_compilation_unit_types(name_snapshot.sources(), &inputs, validated, &type_env)
            .ok()?;
    let new_sid = new_target_sid?;
    let new_su = name_snapshot
        .names()
        .index()
        .source_units()
        .iter()
        .find(|u| u.source_id() == new_sid)?
        .id();
    let recovered =
        UnitAnalysisSnapshot::finish(name_snapshot, Some(typed), None, Vec::new()).ok()?;
    Some((recovered, new_su, new_sid))
}

fn dot_completions(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    text: &str,
    receiver_tokens: &[Lexeme],
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

    // 2. Resolve receiver via definition index, or fall back to scope-aware lookup when parser
    // recovered an incomplete `origin.` / `Color.` into `Expression::Error`.
    let Some(typed) = typed else {
        return items.into_values().collect();
    };
    let parsed_files = snapshot.name_snapshot().parsed_files();
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
            target.source_unit,
            target.span,
            &prefix_lower,
            &mut items,
        );
        return items.into_values().collect();
    }

    // Fallback for Expression::Error recovery: resolve receiver name in lexical scope or imports.
    let recv_name = &text[recv_tok.span().start()..recv_tok.span().end()];
    if let Some((sym_id, _)) = find_local_symbol_in_scope(
        names,
        source_unit,
        source_id,
        recv_tok.span().start(),
        recv_name,
    ) && let Some(ty_id) = typed.symbol_type(UnitSymbolId::new(source_unit, sym_id))
    {
        add_type_member_completions(ty_id, typed, names, source_unit, &prefix_lower, &mut items);
        return items.into_values().collect();
    }

    let parsed_file = parsed_files.iter().find(|p| p.source_id() == source_id);
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
        let same_file = caller_unit == decl.source_unit();
        for member in nominal.companion_members() {
            if (same_file || member.visibility() != DeclarationVisibility::Private)
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
        // Companion constants declared inside this nominal type's AST span.
        if let Some(decl_ast_span) = parsed_files
            .iter()
            .find(|p| p.source_id() == decl.name_span().source_id())
            .and_then(|p| p.ast().items().get(decl.root()).ok())
            .map(|item| item.span())
            && let Some(unit_names) = names
                .source_units()
                .iter()
                .find(|u| u.source_unit() == decl.source_unit())
        {
            for sym in unit_names.resolution().symbols() {
                if sym.kind() == FrontSymbolKind::Constant
                    && sym.span().start() >= decl_ast_span.start()
                    && sym.span().end() <= decl_ast_span.end()
                    && matches_prefix(sym.name(), prefix_lower)
                {
                    let detail = typed
                        .symbol_type(UnitSymbolId::new(decl.source_unit(), sym.id()))
                        .and_then(|ty| format_unit_type(ty, typed, names));
                    insert_item(items, sym.name(), CompletionItemKind::CONSTANT, detail, "0");
                }
            }
        }
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
                decl.source_unit(),
                nominal,
                typed,
                names,
                caller_unit,
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
        add_type_member_completions(ty_id, typed, names, caller_unit, prefix_lower, items);
    }
}

fn add_type_member_completions(
    ty_id: UnitTypeId,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    caller_unit: SourceUnitId,
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
        decl.source_unit(),
        nominal,
        typed,
        names,
        caller_unit,
        prefix_lower,
        items,
    );
}

fn add_nominal_instance_members(
    owner_unit: SourceUnitId,
    nominal: &lang_frontend::type_checking::UnitNominalSignature,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    caller_unit: SourceUnitId,
    prefix_lower: &str,
    items: &mut BTreeMap<String, CompletionItem>,
) {
    let same_file = caller_unit == owner_unit;
    for field in nominal.fields() {
        if (same_file || field.visibility() != DeclarationVisibility::Private)
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
        if (same_file || member.visibility() != DeclarationVisibility::Private)
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
    // Inherited interface methods (transitive closure).
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
        let iface_same_file = caller_unit == iface_decl.source_unit();
        for member in iface_nominal.members() {
            if (iface_same_file || member.visibility() != DeclarationVisibility::Private)
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

    // 1. Lexically enclosing local scopes (innermost -> outermost so shadowing shows inner type).
    for (sym_id, sym) in local_symbols_in_scope(names, source_unit, source_id, offset) {
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
    for &ty in BUILTIN_TYPES {
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

/// 返回在 `offset` 处可见的局部符号列表（按词法作用域由内向外排列，排除已退出块和兄弟函数）。
fn local_symbols_in_scope(
    names: &CompilationUnitNames,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
) -> Vec<(
    lang_frontend::name_resolution::SymbolId,
    &lang_frontend::name_resolution::Symbol,
)> {
    let Some(unit_names) = names
        .source_units()
        .iter()
        .find(|u| u.source_unit() == source_unit)
    else {
        return Vec::new();
    };
    let resolution = unit_names.resolution();
    let scopes = resolution.scopes();
    let enclosing = scopes
        .iter()
        .filter_map(|scope| {
            let span = scope.span()?;
            (span.source_id() == source_id && span.start() <= offset && offset <= span.end())
                .then_some((scope.id(), span.len()))
        })
        .min_by_key(|(_, len)| *len)
        .map(|(id, _)| id);

    let mut scope_chain: Vec<ScopeId> = Vec::new();
    let mut current = enclosing;
    while let Some(sid) = current {
        scope_chain.push(sid);
        current = scopes
            .iter()
            .find(|s| s.id() == sid)
            .and_then(|s| s.parent());
    }

    let mut out = Vec::new();
    for sid in scope_chain {
        for sym in resolution.symbols() {
            if sym.is_synthetic() || sym.span().source_id() != source_id {
                continue;
            }
            if sym.scope() != sid || sym.span().start() >= offset {
                continue;
            }
            if !matches!(
                sym.kind(),
                FrontSymbolKind::Variable
                    | FrontSymbolKind::ValueParameter
                    | FrontSymbolKind::ForBinding
                    | FrontSymbolKind::LambdaParameter
                    | FrontSymbolKind::Constant
                    | FrontSymbolKind::TypeParameter
            ) {
                continue;
            }
            out.push((sym.id(), sym));
        }
    }
    out
}

fn find_local_symbol_in_scope<'a>(
    names: &'a CompilationUnitNames,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
    name: &str,
) -> Option<(
    lang_frontend::name_resolution::SymbolId,
    &'a lang_frontend::name_resolution::Symbol,
)> {
    local_symbols_in_scope(names, source_unit, source_id, offset)
        .into_iter()
        .find(|(_, sym)| sym.name() == name)
}

/// 计算当前文件内裸名可见的全部顶层声明（同文件全部声明、同包非 private 声明、显式导入/别名与通配导入）。
pub(crate) fn visible_declarations<'a>(
    names: &'a CompilationUnitNames,
    parsed_file: Option<&ParsedFile>,
    sources: &SourceMap,
    source_unit: SourceUnitId,
) -> Vec<(String, &'a UnitDeclaration)> {
    let current_pkg = names
        .index()
        .source_units()
        .iter()
        .find(|u| u.id() == source_unit)
        .map(|u| u.package());
    let mut out = Vec::new();

    // 1. Same file (including private) & same package (Public/Internal only).
    for decl in names.index().declarations() {
        let same_file = decl.source_unit() == source_unit;
        let same_pkg = Some(decl.package()) == current_pkg;
        if same_file
            || (same_pkg
                && matches!(
                    decl.visibility(),
                    DeclarationVisibility::Public | DeclarationVisibility::Internal
                ))
        {
            out.push((decl.name().to_owned(), decl));
        }
    }

    // 2. Explicit imports, aliases, and wildcard imports from the current file.
    if let Some(parsed) = parsed_file
        && let Ok(text) = sources.source_text(parsed.source_id())
    {
        for import in parsed.imports() {
            let segs: Vec<&str> = import
                .segments
                .iter()
                .filter_map(|s| {
                    (s.span.start() <= s.span.end() && s.span.end() <= text.len())
                        .then_some(&text[s.span.start()..s.span.end()])
                })
                .collect();
            if segs.is_empty() {
                continue;
            }
            if import.wildcard_span.is_some() {
                for decl in names.index().declarations() {
                    let pkg_matches = names
                        .index()
                        .packages()
                        .get(decl.package().index())
                        .and_then(|p| p.name())
                        .is_some_and(|p| {
                            p.segments().len() == segs.len()
                                && p.segments().iter().zip(segs.iter()).all(|(a, b)| a == b)
                        });
                    if pkg_matches
                        && matches!(
                            decl.visibility(),
                            DeclarationVisibility::Public | DeclarationVisibility::Internal
                        )
                    {
                        out.push((decl.name().to_owned(), decl));
                    }
                }
            } else if segs.len() >= 2 {
                let (pkg_segs, target_name) = (&segs[..segs.len() - 1], segs[segs.len() - 1]);
                let alias_name = import.alias.and_then(|a| {
                    (!a.name_span.is_empty() && a.name_span.end() <= text.len())
                        .then_some(&text[a.name_span.start()..a.name_span.end()])
                });
                let exposed_name = alias_name.unwrap_or(target_name);
                for decl in names.index().declarations() {
                    let pkg_matches = names
                        .index()
                        .packages()
                        .get(decl.package().index())
                        .and_then(|p| p.name())
                        .is_some_and(|p| {
                            p.segments().len() == pkg_segs.len()
                                && p.segments()
                                    .iter()
                                    .zip(pkg_segs.iter())
                                    .all(|(a, b)| a == b)
                        });
                    if pkg_matches
                        && decl.name() == target_name
                        && matches!(
                            decl.visibility(),
                            DeclarationVisibility::Public | DeclarationVisibility::Internal
                        )
                    {
                        out.push((exposed_name.to_owned(), decl));
                    }
                }
            }
        }
    }

    out
}

fn collect_qualifier_segments<'a>(text: &'a str, receiver_tokens: &[Lexeme]) -> Vec<&'a str> {
    let mut segs = Vec::new();
    let mut expect_ident = true;
    for tok in receiver_tokens.iter().rev() {
        if expect_ident {
            if matches!(tok.kind(), LexemeKind::Token(TokenKind::Identifier)) {
                segs.push(&text[tok.span().start()..tok.span().end()]);
                expect_ident = false;
            } else {
                break;
            }
        } else if matches!(
            tok.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Dot))
        ) {
            expect_ident = true;
        } else {
            break;
        }
    }
    segs.reverse();
    segs
}

pub(crate) fn non_trivia_tokens_through(
    sources: &SourceMap,
    source_id: SourceId,
    offset: usize,
) -> Vec<Lexeme> {
    let Ok(lexed) = lex(sources, source_id) else {
        return Vec::new();
    };
    lexed
        .lexemes()
        .iter()
        .copied()
        .filter(|l| {
            !matches!(l.kind(), LexemeKind::Trivia(_) | LexemeKind::Eof)
                && l.span().start() < offset
        })
        .collect()
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
