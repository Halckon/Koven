//! `textDocument/signatureHelp` 实现：支持未闭合调用恢复、显式泛型调用、零字段构造器、可见性约束与具名实参高亮。

use lang_frontend::{
    lexer::{Lexeme, LexemeKind, Symbol, TokenKind},
    name_resolution::{
        CompilationUnitNames, DeclarationVisibility, Namespace, SourceUnitId, UnitSymbolId,
    },
    source::SourceId,
    type_checking::{
        CompilationUnitTypes, NominalKind, UnitCallableSignature, UnitDeclarationSignature,
        UnitTypeId, UnitTypeKind,
    },
};
use lsp_types::{ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation};

use crate::{
    ide_features::format_unit_type,
    scope_query::{
        collect_qualifier_segments, expression_type_ending_at, find_local_symbol_in_scope,
        find_receiver_expression_start, non_trivia_tokens_through, recover_unit_snapshot,
        replace_span_with_spaces, replace_span_with_zero, skip_angle_brackets_backwards,
        visible_declarations,
    },
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

struct SignatureEntry {
    info: SignatureInformation,
    param_names: Vec<String>,
}

/// 在给定位置计算 `textDocument/signatureHelp`。
pub(crate) fn signature_help_at(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
) -> Option<SignatureHelp> {
    let sources = snapshot.name_snapshot().sources();
    let text = sources.source_text(source_id).ok()?;
    let clamped = offset.min(text.len());
    let tokens = non_trivia_tokens_through(sources, source_id, clamped);

    // Walk backwards to locate the innermost unclosed `(`, count top-level commas (ignoring
    // nested `(...)`, `[...]`, `{...}`, and generic `<...>`), and find where the active arg begins.
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut comma_count = 0u32;
    let mut open_paren_idx = None;
    let mut last_top_comma_idx = None;
    let mut idx = tokens.len();
    while idx > 0 {
        idx -= 1;
        match tokens[idx].kind() {
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
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater))
                if paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && matches!(
                        tokens.get(idx + 1).map(|t| t.kind()),
                        Some(LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)))
                    ) =>
            {
                if let Some(lt_idx) = skip_angle_brackets_backwards(&tokens, idx) {
                    idx = lt_idx;
                }
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::Comma))
                if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 =>
            {
                if last_top_comma_idx.is_none() {
                    last_top_comma_idx = Some(idx);
                }
                comma_count += 1;
            }
            _ => {}
        }
    }
    let open_idx = open_paren_idx?;
    let callee_idx = find_callee_token_before_paren(&tokens, open_idx)?;
    let callee_tok = tokens[callee_idx];
    let callee_name = &text[callee_tok.span().start()..callee_tok.span().end()];

    // Check if the active argument has a named argument prefix (`name = ...`), inspecting the
    // full token stream from the argument start so both `|left = 1` and `left = |1` highlight `left`.
    let arg_start_byte =
        last_top_comma_idx.map_or(tokens[open_idx].span().end(), |i| tokens[i].span().end());
    let all_tokens = non_trivia_tokens_through(sources, source_id, text.len());
    let full_arg_start = all_tokens.partition_point(|t| t.span().start() < arg_start_byte);
    let active_named_arg = active_named_argument(&all_tokens[full_arg_start..], text);

    // P1: When typing `combine(` or `combine(1,` causes a parse error so `snapshot.typed()` is
    // `None`, recover a typed snapshot in memory so signatures remain available during live edits.
    let recovered_holder;
    let (active_snap, active_su, active_sid) = if snapshot.typed().is_some() {
        (snapshot, source_unit, source_id)
    } else {
        recovered_holder = recover_for_signature_help(
            snapshot,
            source_unit,
            source_id,
            text,
            &tokens,
            callee_idx,
            clamped,
        )?;
        (&recovered_holder.0, recovered_holder.1, recovered_holder.2)
    };

    let typed = active_snap.typed()?;
    let names = active_snap.name_snapshot().names();
    let active_sources = active_snap.name_snapshot().sources();
    let parsed_file = active_snap
        .name_snapshot()
        .parsed_files()
        .iter()
        .find(|p| p.source_id() == active_sid);

    let mut entries: Vec<SignatureEntry> = Vec::new();
    // 1. Resolve via definition index at callee token (functions, overloads, methods, constructors).
    if let Some((_, targets)) = active_snap
        .definitions()
        .entry_at(active_su, callee_tok.span().start())
    {
        for target in targets {
            if let Some(decl) = names
                .index()
                .declarations()
                .iter()
                .find(|d| d.source_unit() == target.source_unit && d.name_span() == target.span)
                && let Some(sig) = typed.signatures().declaration(decl.id())
            {
                push_decl_signature(decl.name(), sig, typed, names, &mut entries);
            } else {
                for decl_sig in typed.signatures().declarations() {
                    if let Some(nominal) = decl_sig.nominal() {
                        for member in nominal
                            .members()
                            .iter()
                            .chain(nominal.companion_members().iter())
                        {
                            if member.name_span() == target.span {
                                entries.push(callable_signature_entry(member, typed, names));
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Receiver-aware fallback when call was recovered in memory (e.g. `obj.method(`, `lib.combine(`, `MyGreeter.create(`).
    if entries.is_empty()
        && callee_idx >= 2
        && matches!(
            tokens[callee_idx - 1].kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Dot | Symbol::QuestionDot))
        )
    {
        let receiver_tokens = &tokens[..callee_idx - 1];
        resolve_dot_callee_signatures(
            typed,
            names,
            parsed_file,
            active_sources,
            active_su,
            active_sid,
            text,
            receiver_tokens,
            callee_name,
            &mut entries,
        );
    }

    // 3. Bare-call fallback: only search declarations actually visible in the current file.
    if entries.is_empty() {
        for (visible_name, decl) in
            visible_declarations(names, parsed_file, active_sources, active_su)
        {
            if visible_name == callee_name
                && let Some(sig) = typed.signatures().declaration(decl.id())
            {
                push_decl_signature(&visible_name, sig, typed, names, &mut entries);
            }
        }
    }

    if entries.is_empty() {
        return None;
    }

    let active_parameter = active_named_arg
        .and_then(|label| {
            entries[0]
                .param_names
                .iter()
                .position(|p| p == label)
                .map(|i| i as u32)
        })
        .unwrap_or(comma_count);

    Some(SignatureHelp {
        signatures: entries.into_iter().map(|e| e.info).collect(),
        active_signature: Some(0),
        active_parameter: Some(active_parameter),
    })
}

fn recover_for_signature_help(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    text: &str,
    tokens: &[Lexeme],
    callee_idx: usize,
    clamped: usize,
) -> Option<(UnitAnalysisSnapshot, SourceUnitId, SourceId)> {
    // Pass 1: for method calls (`obj.method(`, `factory().method(`), erase `.method(...` with
    // spaces so the receiver expression keeps its exact inferred type.
    if callee_idx >= 2
        && matches!(
            tokens[callee_idx - 1].kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Dot | Symbol::QuestionDot))
        )
        && let Some(repaired_text) =
            replace_span_with_spaces(text, tokens[callee_idx - 1].span().start(), clamped)
        && let Some(recovered) =
            recover_unit_snapshot(snapshot, source_unit, source_id, repaired_text)
    {
        return Some(recovered);
    }

    // Pass 2: replace the incomplete call expression `[call_start..clamped]` with `0   `.
    let call_start = find_receiver_expression_start(&tokens[..=callee_idx])
        .unwrap_or_else(|| tokens[callee_idx].span().start());
    if let Some(repaired_text) = replace_span_with_zero(text, call_start, clamped)
        && let Some(recovered) =
            recover_unit_snapshot(snapshot, source_unit, source_id, repaired_text)
    {
        return Some(recovered);
    }

    // Pass 3: if nested inside another unclosed `(`, close all remaining unclosed parens at `clamped`.
    let mut unclosed = 0usize;
    for tok in tokens {
        match tok.kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)) => unclosed += 1,
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                unclosed = unclosed.saturating_sub(1);
            }
            _ => {}
        }
    }
    if unclosed > 0 {
        let mut repaired = String::with_capacity(text.len() + unclosed + 2);
        let trim_end = tokens
            .last()
            .filter(|t| {
                matches!(
                    t.kind(),
                    LexemeKind::Token(TokenKind::Symbol(Symbol::Comma))
                )
            })
            .map_or(clamped, |t| t.span().start());
        repaired.push_str(&text[..trim_end]);
        for _ in 0..unclosed {
            repaired.push(')');
        }
        repaired.push_str(&text[clamped..]);
        if let Some(recovered) = recover_unit_snapshot(snapshot, source_unit, source_id, repaired) {
            return Some(recovered);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn resolve_dot_callee_signatures(
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    parsed_file: Option<&lang_frontend::parser::ParsedFile>,
    sources: &lang_frontend::source::SourceMap,
    source_unit: SourceUnitId,
    source_id: SourceId,
    text: &str,
    receiver_tokens: &[Lexeme],
    callee_name: &str,
    entries: &mut Vec<SignatureEntry>,
) {
    let Some(recv_tok) = receiver_tokens.last() else {
        return;
    };
    let qualifier = collect_qualifier_segments(text, receiver_tokens);
    // A. Package-qualified call (`lib.combine(`).
    if !qualifier.is_empty() {
        for decl in names.index().declarations() {
            if decl.name() != callee_name {
                continue;
            }
            let pkg_matches = names
                .index()
                .packages()
                .get(decl.package().index())
                .and_then(|p| p.name())
                .is_some_and(|p| {
                    p.segments().len() == qualifier.len()
                        && p.segments()
                            .iter()
                            .zip(qualifier.iter())
                            .all(|(a, b)| a.as_str() == *b)
                });
            let accessible = decl.source_unit() == source_unit
                || matches!(
                    decl.visibility(),
                    DeclarationVisibility::Public | DeclarationVisibility::Internal
                );
            if pkg_matches
                && accessible
                && let Some(sig) = typed.signatures().declaration(decl.id())
            {
                push_decl_signature(decl.name(), sig, typed, names, entries);
            }
        }
        if !entries.is_empty() {
            return;
        }
    }

    // B. Receiver expression type (from Pass 1) or local symbol type -> instance methods.
    let expr_start = find_receiver_expression_start(receiver_tokens);
    if let Some(ty_id) = expression_type_ending_at(
        typed,
        parsed_file,
        source_unit,
        expr_start,
        recv_tok.span().end(),
    ) {
        push_type_method_signatures(ty_id, callee_name, typed, names, entries);
        if !entries.is_empty() {
            return;
        }
    }
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
        push_type_method_signatures(ty_id, callee_name, typed, names, entries);
        if !entries.is_empty() {
            return;
        }
    }

    // C. Companion / object methods on a visible nominal type (`MyGreeter.create(`).
    for (visible_name, decl) in visible_declarations(names, parsed_file, sources, source_unit) {
        if visible_name == recv_name
            && decl.namespace() == Namespace::Type
            && let Some(nominal) = typed
                .signatures()
                .declaration(decl.id())
                .and_then(|s| s.nominal())
        {
            for member in nominal
                .companion_members()
                .iter()
                .chain(nominal.members().iter())
            {
                if member.name() == callee_name {
                    entries.push(callable_signature_entry(member, typed, names));
                }
            }
        }
    }
}

fn push_type_method_signatures(
    ty_id: UnitTypeId,
    callee_name: &str,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    entries: &mut Vec<SignatureEntry>,
) {
    let inner = match typed.types().get(ty_id) {
        Some(UnitTypeKind::Nullable(inner)) => *inner,
        _ => ty_id,
    };
    let Some(UnitTypeKind::Nominal { declaration, .. }) = typed.types().get(inner) else {
        return;
    };
    let Some(nominal) = typed
        .signatures()
        .declaration(*declaration)
        .and_then(|s| s.nominal())
    else {
        return;
    };
    for member in nominal.members() {
        if member.name() == callee_name {
            entries.push(callable_signature_entry(member, typed, names));
        }
    }
    for &iface_ty in nominal.interfaces() {
        if let Some(UnitTypeKind::Nominal {
            declaration: iface_decl,
            ..
        }) = typed.types().get(iface_ty)
            && let Some(iface_nominal) = typed
                .signatures()
                .declaration(*iface_decl)
                .and_then(|s| s.nominal())
        {
            for member in iface_nominal.members() {
                if member.name() == callee_name {
                    entries.push(callable_signature_entry(member, typed, names));
                }
            }
        }
    }
}

/// 跳过显式泛型实参 `<...>` 找到真正的被调用者 token（如 `identity<Int>(` -> `identity`）。
fn find_callee_token_before_paren(tokens: &[Lexeme], open_paren_idx: usize) -> Option<usize> {
    let mut idx = open_paren_idx.checked_sub(1)?;
    if matches!(
        tokens[idx].kind(),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Greater))
    ) {
        let lt_idx = skip_angle_brackets_backwards(tokens, idx)?;
        idx = lt_idx.checked_sub(1)?;
    }
    matches!(tokens[idx].kind(), LexemeKind::Token(TokenKind::Identifier)).then_some(idx)
}

fn active_named_argument<'a>(arg_tokens: &[Lexeme], text: &'a str) -> Option<&'a str> {
    let [first, second, ..] = arg_tokens else {
        return None;
    };
    if matches!(first.kind(), LexemeKind::Token(TokenKind::Identifier))
        && matches!(
            second.kind(),
            LexemeKind::Token(TokenKind::Symbol(Symbol::Equal))
        )
    {
        Some(&text[first.span().start()..first.span().end()])
    } else {
        None
    }
}

fn push_decl_signature(
    name: &str,
    sig: &UnitDeclarationSignature,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    out: &mut Vec<SignatureEntry>,
) {
    if let Some(callable) = sig.callable() {
        out.push(callable_signature_entry(callable, typed, names));
    } else if let Some(nominal) = sig.nominal()
        && matches!(nominal.kind(), NominalKind::Class | NominalKind::ValueClass)
    {
        let mut params = Vec::with_capacity(nominal.fields().len());
        let mut param_labels = Vec::with_capacity(nominal.fields().len());
        let mut param_names = Vec::with_capacity(nominal.fields().len());
        for field in nominal.fields() {
            let ty = format_unit_type(field.ty(), typed, names).unwrap_or_else(|| "?".to_owned());
            let label = format!("{}: {ty}", field.name());
            param_names.push(field.name().to_owned());
            param_labels.push(label.clone());
            params.push(ParameterInformation {
                label: ParameterLabel::Simple(label),
                documentation: None,
            });
        }
        out.push(SignatureEntry {
            info: SignatureInformation {
                label: format!("{name}({})", param_labels.join(", ")),
                documentation: None,
                parameters: Some(params),
                active_parameter: None,
            },
            param_names,
        });
    }
}

fn callable_signature_entry(
    callable: &UnitCallableSignature,
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
) -> SignatureEntry {
    let mut params = Vec::with_capacity(callable.parameters().len());
    let mut param_labels = Vec::with_capacity(callable.parameters().len());
    let mut param_names = Vec::with_capacity(callable.parameters().len());
    for param in callable.parameters() {
        let pname = param.name().unwrap_or("_");
        let ty = format_unit_type(param.ty(), typed, names).unwrap_or_else(|| "?".to_owned());
        let label = format!("{pname}: {ty}");
        param_names.push(pname.to_owned());
        param_labels.push(label.clone());
        params.push(ParameterInformation {
            label: ParameterLabel::Simple(label),
            documentation: None,
        });
    }
    let ret =
        format_unit_type(callable.return_type(), typed, names).unwrap_or_else(|| "Unit".to_owned());
    SignatureEntry {
        info: SignatureInformation {
            label: format!(
                "fun {}({}): {ret}",
                callable.name(),
                param_labels.join(", ")
            ),
            documentation: None,
            parameters: Some(params),
            active_parameter: None,
        },
        param_names,
    }
}
