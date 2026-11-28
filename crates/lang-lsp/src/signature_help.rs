//! `textDocument/signatureHelp` 实现：支持显式泛型调用、零字段构造器、可见性约束与具名实参高亮。

use lang_frontend::{
    lexer::{Lexeme, LexemeKind, Symbol, TokenKind},
    name_resolution::{CompilationUnitNames, SourceUnitId},
    source::SourceId,
    type_checking::{
        CompilationUnitTypes, NominalKind, UnitCallableSignature, UnitDeclarationSignature,
    },
};
use lsp_types::{ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation};

use crate::{
    completion::{non_trivia_tokens_through, visible_declarations},
    ide_features::format_unit_type,
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
    let typed = snapshot.typed()?;
    let sources = snapshot.name_snapshot().sources();
    let names = snapshot.name_snapshot().names();
    let text = sources.source_text(source_id).ok()?;
    let clamped = offset.min(text.len());
    let tokens = non_trivia_tokens_through(sources, source_id, clamped);

    // Walk backwards to locate the innermost unclosed `(`, count top-level commas,
    // and track the token index where the current argument begins.
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut comma_count = 0u32;
    let mut open_paren_idx = None;
    let mut last_top_comma_idx = None;
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
    let arg_start_byte = last_top_comma_idx.map_or(tokens[open_idx].span().end(), |idx| {
        tokens[idx].span().end()
    });
    let all_tokens = non_trivia_tokens_through(sources, source_id, text.len());
    let full_arg_start = all_tokens.partition_point(|t| t.span().start() < arg_start_byte);
    let active_named_arg = active_named_argument(&all_tokens[full_arg_start..], text);

    let mut entries: Vec<SignatureEntry> = Vec::new();
    // 1. Resolve via definition index at callee token (functions, overloads, methods, constructors).
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

    // 2. Fallback: only search declarations actually visible in the current file (respecting imports/visibility).
    if entries.is_empty() {
        let parsed_file = snapshot
            .name_snapshot()
            .parsed_files()
            .iter()
            .find(|p| p.source_id() == source_id);
        for (visible_name, decl) in visible_declarations(names, parsed_file, sources, source_unit) {
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
                .map(|idx| idx as u32)
        })
        .unwrap_or(comma_count);

    Some(SignatureHelp {
        signatures: entries.into_iter().map(|e| e.info).collect(),
        active_signature: Some(0),
        active_parameter: Some(active_parameter),
    })
}

/// 跳过显式泛型实参 `<...>` 找到真正的被调用者 token（如 `identity<Int>(` -> `identity`）。
fn find_callee_token_before_paren(tokens: &[Lexeme], open_paren_idx: usize) -> Option<usize> {
    let mut idx = open_paren_idx.checked_sub(1)?;
    if matches!(
        tokens[idx].kind(),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Greater))
    ) {
        let mut depth = 1usize;
        while idx > 0 && depth > 0 {
            idx -= 1;
            match tokens[idx].kind() {
                LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)) => depth += 1,
                LexemeKind::Token(TokenKind::Symbol(Symbol::Less)) => depth -= 1,
                _ => {}
            }
        }
        if depth != 0 {
            return None;
        }
        idx = idx.checked_sub(1)?;
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
