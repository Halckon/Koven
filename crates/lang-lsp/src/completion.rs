//! `textDocument/completion` 与 `textDocument/signatureHelp` 实现。

use std::collections::BTreeMap;

use lang_frontend::{
    lexer::{Lexeme, LexemeKind, Symbol, TokenKind, lex},
    name_resolution::{
        CompilationUnitNames, DeclarationVisibility, SourceUnitId, SymbolKind as FrontSymbolKind,
        UnitSymbolId,
    },
    source::{SourceId, SourceMap},
    type_checking::{
        CompilationUnitTypes, UnitCallableSignature, UnitDeclarationSignature, UnitTypeKind,
    },
};
use lsp_types::{
    CompletionItem, CompletionItemKind, ParameterInformation, ParameterLabel, SignatureHelp,
    SignatureInformation,
};

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
    "Double",
    "Err",
    "Float",
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
    "UInt",
    "UInt8",
    "UInt16",
    "UInt32",
    "UInt64",
    "Unit",
    "View",
];

const BUILTIN_FUNCTIONS: &[(&str, &str)] = &[
    ("println", "(value: Any) -> Unit"),
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
];

/// 计算给定位置的补全候选（支持 `.` / `?.` 成员与包补全，以及作用域内标识符补全）。
pub(crate) fn completions_at(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
) -> Vec<CompletionItem> {
    let sources = snapshot.name_snapshot().sources();
    let names = snapshot.name_snapshot().names();
    let typed = snapshot.typed();
    let Ok(text) = sources.source_text(source_id) else {
        return Vec::new();
    };
    let clamped = offset.min(text.len());
    let tokens = non_trivia_tokens_before(sources, source_id, clamped);

    // Detect `prefix` being typed right at cursor.
    let (prefix, before_prefix) = match tokens.last() {
        Some(last)
            if last.span().end() == clamped
                && matches!(
                    last.kind(),
                    LexemeKind::Token(TokenKind::Identifier | TokenKind::Keyword(_))
                ) =>
        {
            (
                &text[last.span().start()..clamped],
                &tokens[..tokens.len() - 1],
            )
        }
        _ => ("", tokens.as_slice()),
    };

    // Check if preceded by `.` or `?.` -> member / companion / package completion.
    if let Some(dot) = before_prefix.last()
        && matches!(
            dot.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Dot | Symbol::QuestionDot))
        )
    {
        let receiver_tokens = &before_prefix[..before_prefix.len() - 1];
        return dot_completions(snapshot, source_unit, text, receiver_tokens, prefix);
    }

    scope_completions(
        snapshot,
        source_unit,
        source_id,
        clamped,
        names,
        typed,
        prefix,
    )
}

fn dot_completions(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
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

    // 1. Check if receiver is a package qualifier (e.g. `demo.`).
    let recv_name = &text[recv_tok.span().start()..recv_tok.span().end()];
    let current_pkg = names
        .index()
        .source_units()
        .iter()
        .find(|u| u.id() == source_unit)
        .map(|u| u.package());
    for decl in names.index().declarations() {
        let matches_pkg = names
            .index()
            .packages()
            .get(decl.package().index())
            .and_then(|p| p.name())
            .is_some_and(|name| name.segments().last().map(String::as_str) == Some(recv_name));
        if matches_pkg
            && (decl.visibility() == DeclarationVisibility::Public
                || Some(decl.package()) == current_pkg)
            && matches_prefix(decl.name(), &prefix_lower)
        {
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

    // 2. Resolve receiver symbol or expression type for nominal members / companion / enum cases.
    if let Some(typed) = typed
        && let Some((_, targets)) = snapshot
            .definitions()
            .entry_at(source_unit, recv_tok.span().start())
        && let Some(target) = targets.first().copied()
    {
        // Case A: receiver is a type/nominal declaration itself (static companion or enum case).
        if let Some(decl) = names
            .index()
            .declarations()
            .iter()
            .find(|d| d.source_unit() == target.source_unit && d.name_span() == target.span)
            && let Some(sig) = typed.signatures().declaration(decl.id())
            && let Some(nominal) = sig.nominal()
        {
            for member in nominal.companion_members() {
                if matches_prefix(member.name(), &prefix_lower) {
                    insert_item(
                        &mut items,
                        member.name(),
                        CompletionItemKind::METHOD,
                        Some(format_callable_detail(member, typed, names)),
                        "0",
                    );
                }
            }
            for case in nominal.enum_cases() {
                if matches_prefix(case.name(), &prefix_lower) {
                    insert_item(
                        &mut items,
                        case.name(),
                        CompletionItemKind::ENUM_MEMBER,
                        Some(decl.name().to_owned()),
                        "0",
                    );
                }
            }
        }

        // Case B: receiver is a value symbol whose type is a Nominal class/interface/enum.
        if let Some(unit_names) = names
            .source_units()
            .iter()
            .find(|u| u.source_unit() == target.source_unit)
            && let Some(sym) = unit_names
                .resolution()
                .symbols()
                .iter()
                .find(|s| s.span() == target.span)
            && let Some(ty_id) = typed.symbol_type(UnitSymbolId::new(target.source_unit, sym.id()))
        {
            add_type_member_completions(ty_id, typed, names, &prefix_lower, &mut items);
        }
    }

    items.into_values().collect()
}

fn add_type_member_completions(
    ty_id: lang_frontend::type_checking::UnitTypeId,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
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
    let Some(sig) = typed.signatures().declaration(*declaration) else {
        return;
    };
    let Some(nominal) = sig.nominal() else {
        return;
    };
    for field in nominal.fields() {
        if matches_prefix(field.name(), prefix_lower) {
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
        if matches_prefix(member.name(), prefix_lower) {
            insert_item(
                items,
                member.name(),
                CompletionItemKind::METHOD,
                Some(format_callable_detail(member, typed, names)),
                "1",
            );
        }
    }
}

fn scope_completions(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
    names: &CompilationUnitNames,
    typed: Option<&CompilationUnitTypes>,
    prefix: &str,
) -> Vec<CompletionItem> {
    let prefix_lower = prefix.to_ascii_lowercase();
    let mut items: BTreeMap<String, CompletionItem> = BTreeMap::new();

    // 1. Local symbols declared before cursor in current file.
    if let Some(unit_names) = names
        .source_units()
        .iter()
        .find(|u| u.source_unit() == source_unit)
    {
        for sym in unit_names.resolution().symbols() {
            if sym.is_synthetic() || sym.span().source_id() != source_id {
                continue;
            }
            if sym.span().start() >= offset {
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
            if !matches_prefix(sym.name(), &prefix_lower) {
                continue;
            }
            let detail = typed.and_then(|t| {
                t.symbol_type(UnitSymbolId::new(source_unit, sym.id()))
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
    }

    // 2. Visible top-level declarations (same package + public in other packages).
    let current_pkg = names
        .index()
        .source_units()
        .iter()
        .find(|u| u.id() == source_unit)
        .map(|u| u.package());
    for decl in names.index().declarations() {
        let same_pkg = Some(decl.package()) == current_pkg;
        let same_file = decl.source_unit() == source_unit;
        if !same_file && !same_pkg && decl.visibility() != DeclarationVisibility::Public {
            continue;
        }
        if !matches_prefix(decl.name(), &prefix_lower) {
            continue;
        }
        let detail = typed.and_then(|t| decl_detail(decl.id(), t, names));
        let rank = if same_pkg { "1" } else { "2" };
        insert_item(
            &mut items,
            decl.name(),
            map_completion_kind(decl.kind()),
            detail,
            rank,
        );
    }

    // 3. Builtin functions and types.
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

    let _ = snapshot;
    items.into_values().collect()
}

/// 在给定位置计算 `textDocument/signatureHelp`。
pub(crate) fn signature_help_at(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
) -> Option<SignatureHelp> {
    let typed = snapshot.typed()?;
    let sources = snapshot.name_snapshot().sources();
    let names = snapshot.name_snapshot().names();
    let text = sources.source_text(source_id).ok()?;
    let clamped = offset.min(text.len());
    let tokens = non_trivia_tokens_before(sources, source_id, clamped);

    // Walk backwards to locate the innermost unclosed `(` and count top-level commas.
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut comma_count = 0u32;
    let mut open_paren_idx = None;
    for (idx, tok) in tokens.iter().enumerate().rev() {
        match tok.kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => paren_depth += 1,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => {
                if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
                    open_paren_idx = Some(idx);
                    break;
                }
                paren_depth = paren_depth.saturating_sub(1);
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => bracket_depth += 1,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBracket)) => {
                bracket_depth = bracket_depth.saturating_sub(1);
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)) => brace_depth += 1,
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) => {
                brace_depth = brace_depth.saturating_sub(1);
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma))
                if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 =>
            {
                comma_count += 1;
            }
            _ => {}
        }
    }
    let open_idx = open_paren_idx?;
    let callee_tok = tokens.get(open_idx.checked_sub(1)?)?;
    let callee_name = &text[callee_tok.span().start()..callee_tok.span().end()];

    let mut signatures = Vec::new();
    // 1. Resolve via definition index at callee token (covers functions, overloads, methods, constructors).
    if let Some((_, targets)) = snapshot
        .definitions()
        .entry_at(source_unit, callee_tok.span().start())
    {
        for target in targets {
            if let Some(decl) = names
                .index()
                .declarations()
                .iter()
                .find(|d| d.source_unit() == target.source_unit && d.name_span() == target.span)
                && let Some(sig) = typed.signatures().declaration(decl.id())
            {
                push_decl_signature(decl.name(), sig, typed, names, &mut signatures);
            } else {
                // Member method target inside a nominal type.
                for decl_sig in typed.signatures().declarations() {
                    if let Some(nominal) = decl_sig.nominal() {
                        for member in nominal
                            .members()
                            .iter()
                            .chain(nominal.companion_members().iter())
                        {
                            if member.name_span() == target.span {
                                signatures.push(callable_signature_info(member, typed, names));
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Fallback: match by name in visible declarations (helpful while typing incomplete call arguments).
    if signatures.is_empty() {
        for decl in names.index().declarations() {
            if decl.name() == callee_name
                && let Some(sig) = typed.signatures().declaration(decl.id())
            {
                push_decl_signature(decl.name(), sig, typed, names, &mut signatures);
            }
        }
    }

    if signatures.is_empty() {
        return None;
    }
    Some(SignatureHelp {
        signatures,
        active_signature: Some(0),
        active_parameter: Some(comma_count),
    })
}

fn push_decl_signature(
    name: &str,
    sig: &UnitDeclarationSignature,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    out: &mut Vec<SignatureInformation>,
) {
    if let Some(callable) = sig.callable() {
        out.push(callable_signature_info(callable, typed, names));
    } else if let Some(nominal) = sig.nominal()
        && !nominal.fields().is_empty()
    {
        let mut params = Vec::with_capacity(nominal.fields().len());
        let mut param_labels = Vec::with_capacity(nominal.fields().len());
        for field in nominal.fields() {
            let ty = format_unit_type(field.ty(), typed, names).unwrap_or_else(|| "?".to_owned());
            let label = format!("{}: {ty}", field.name());
            param_labels.push(label.clone());
            params.push(ParameterInformation {
                label: ParameterLabel::Simple(label),
                documentation: None,
            });
        }
        out.push(SignatureInformation {
            label: format!("{name}({})", param_labels.join(", ")),
            documentation: None,
            parameters: Some(params),
            active_parameter: None,
        });
    }
}

fn callable_signature_info(
    callable: &UnitCallableSignature,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
) -> SignatureInformation {
    let mut params = Vec::with_capacity(callable.parameters().len());
    let mut param_labels = Vec::with_capacity(callable.parameters().len());
    for param in callable.parameters() {
        let pname = param.name().unwrap_or("_");
        let ty = format_unit_type(param.ty(), typed, names).unwrap_or_else(|| "?".to_owned());
        let label = format!("{pname}: {ty}");
        param_labels.push(label.clone());
        params.push(ParameterInformation {
            label: ParameterLabel::Simple(label),
            documentation: None,
        });
    }
    let ret =
        format_unit_type(callable.return_type(), typed, names).unwrap_or_else(|| "Unit".to_owned());
    SignatureInformation {
        label: format!(
            "fun {}({}): {ret}",
            callable.name(),
            param_labels.join(", ")
        ),
        documentation: None,
        parameters: Some(params),
        active_parameter: None,
    }
}

fn non_trivia_tokens_before(
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
