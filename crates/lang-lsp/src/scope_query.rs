//! 补全与签名帮助共享的作用域查询、表达式定位与内存错误恢复工具。

use lang_frontend::{
    analysis::{DISPLAY_PATH, UnitSourceDescriptor, analyze_unit_names, append_builtin_display},
    lexer::{Lexeme, LexemeKind, Symbol, TokenKind, lex},
    name_resolution::{
        CompilationUnitNames, DeclarationVisibility, SourceUnitId, SourceUnitKey,
        SymbolKind as FrontSymbolKind, UnitDeclaration,
    },
    parser::{Item, NameMarker, ParsedFile, Statement},
    source::{SourceId, SourceMap},
    type_checking::{
        CompilationUnitTypes, UnitExpressionId, UnitTypeId, check_compilation_unit_types,
        standard_environments,
    },
};

use crate::unit_analysis_snapshot::UnitAnalysisSnapshot;

/// 在内存中替换目标源码文本并重建带嵌入式 `Display` 授权的编译单元快照。
pub(crate) fn recover_unit_snapshot(
    snapshot: &UnitAnalysisSnapshot,
    target_unit: SourceUnitId,
    target_source_id: SourceId,
    repaired_text: String,
) -> Option<(UnitAnalysisSnapshot, SourceUnitId, SourceId)> {
    let orig_sources = snapshot.name_snapshot().sources();
    let orig_units = snapshot.name_snapshot().names().index().source_units();
    let mut new_sources = SourceMap::new();
    let mut descriptors = Vec::with_capacity(orig_units.len());
    let mut new_target_sid = None;

    for su in orig_units {
        let sid = su.source_id();
        let name = orig_sources.source_name(sid).ok()?;
        // Skip the previously appended embedded Display source; we re-append it via
        // `append_builtin_display` so the new `NameEnvironment` authorizes its exact `SourceId`.
        if su
            .key()
            .logical_path()
            .is_some_and(|p| p.as_str() == DISPLAY_PATH)
            && name.starts_with("koven-std/")
        {
            continue;
        }
        let orig_text = orig_sources.source_text(sid).ok()?;
        let text = if su.id() == target_unit && sid == target_source_id {
            repaired_text.clone()
        } else {
            orig_text.to_owned()
        };
        let new_sid = new_sources.add_source(name, text).ok()?;
        if su.id() == target_unit {
            new_target_sid = Some(new_sid);
        }
        let desc = match su.key() {
            SourceUnitKey::Rooted { root, logical_path } => {
                UnitSourceDescriptor::new(root.as_str(), logical_path.as_str(), new_sid)
            }
            SourceUnitKey::MemoryDocument => UnitSourceDescriptor::memory_document(new_sid),
        };
        descriptors.push(desc);
    }

    let (mut name_env, type_env) = standard_environments();
    append_builtin_display(
        &mut new_sources,
        &mut descriptors,
        &mut name_env,
        "koven-std",
    )
    .ok()?;
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

/// 将 `orig_text[start..end]` 替换为等长空格（保留换行，维持其余源码偏移不变）。
pub(crate) fn replace_span_with_spaces(
    orig_text: &str,
    start: usize,
    end: usize,
) -> Option<String> {
    if start > end || end > orig_text.len() {
        return None;
    }
    let mut out = String::with_capacity(orig_text.len());
    out.push_str(&orig_text[..start]);
    for ch in orig_text[start..end].chars() {
        out.push(if ch == '\n' { '\n' } else { ' ' });
    }
    out.push_str(&orig_text[end..]);
    Some(out)
}

/// 将 `orig_text[start..end]` 替换为以 `0` 开头的等长空白占位（保留换行）。
pub(crate) fn replace_span_with_zero(orig_text: &str, start: usize, end: usize) -> Option<String> {
    if start > end || end > orig_text.len() {
        return None;
    }
    let mut out = String::with_capacity(orig_text.len());
    out.push_str(&orig_text[..start]);
    let mut wrote_zero = false;
    for ch in orig_text[start..end].chars() {
        if ch == '\n' {
            out.push('\n');
        } else if !wrote_zero {
            out.push('0');
            wrote_zero = true;
        } else {
            out.push(' ');
        }
    }
    out.push_str(&orig_text[end..]);
    Some(out)
}

/// 从后向前定位完整后缀接收者表达式的起点字节偏移（支持 `a.b.c`、`factory()`、`arr[0]`、`f<T>()`）。
pub(crate) fn find_receiver_expression_start(receiver_tokens: &[Lexeme]) -> Option<usize> {
    if receiver_tokens.is_empty() {
        return None;
    }
    let mut idx = receiver_tokens.len();
    let mut start = receiver_tokens.last()?.span().start();
    loop {
        if idx == 0 {
            break;
        }
        let tok = receiver_tokens[idx - 1];
        match tok.kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)) => {
                let open = match_open_delim_backwards(
                    receiver_tokens,
                    idx - 1,
                    Symbol::LeftParen,
                    Symbol::RightParen,
                )?;
                start = receiver_tokens[open].span().start();
                idx = open;
                // Check for optional `<...>` type arguments immediately before `(`.
                if idx > 0
                    && matches!(
                        receiver_tokens[idx - 1].kind(),
                        LexemeKind::Token(TokenKind::Symbol(Symbol::Greater))
                    )
                    && let Some(lt) = skip_angle_brackets_backwards(receiver_tokens, idx - 1)
                {
                    start = receiver_tokens[lt].span().start();
                    idx = lt;
                }
            }
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBracket)) => {
                let open = match_open_delim_backwards(
                    receiver_tokens,
                    idx - 1,
                    Symbol::LeftBracket,
                    Symbol::RightBracket,
                )?;
                start = receiver_tokens[open].span().start();
                idx = open;
            }
            LexemeKind::Token(TokenKind::Identifier | TokenKind::Keyword(_)) => {
                start = tok.span().start();
                idx -= 1;
                if idx > 0
                    && matches!(
                        receiver_tokens[idx - 1].kind(),
                        LexemeKind::Token(TokenKind::Symbol(Symbol::Dot | Symbol::QuestionDot))
                    )
                {
                    idx -= 1;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    Some(start)
}

fn match_open_delim_backwards(
    tokens: &[Lexeme],
    close_idx: usize,
    open_sym: Symbol,
    close_sym: Symbol,
) -> Option<usize> {
    let mut depth = 1usize;
    let mut i = close_idx;
    while i > 0 && depth > 0 {
        i -= 1;
        match tokens[i].kind() {
            LexemeKind::Token(TokenKind::Symbol(sym)) if sym == close_sym => depth += 1,
            LexemeKind::Token(TokenKind::Symbol(sym)) if sym == open_sym => depth -= 1,
            _ => {}
        }
    }
    (depth == 0).then_some(i)
}

/// 若 `gt_idx` 处的 `>` 闭合了一个合法泛型实参列表 `<...>`，返回对应 `<` 的索引；否则返回 `None`。
pub(crate) fn skip_angle_brackets_backwards(tokens: &[Lexeme], gt_idx: usize) -> Option<usize> {
    if !matches!(
        tokens.get(gt_idx).map(|t| t.kind()),
        Some(LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)))
    ) {
        return None;
    }
    let mut depth = 1usize;
    let mut i = gt_idx;
    while i > 0 && depth > 0 {
        i -= 1;
        match tokens[i].kind() {
            LexemeKind::Token(TokenKind::Symbol(Symbol::Greater)) => depth += 1,
            LexemeKind::Token(TokenKind::Symbol(Symbol::Less)) => depth -= 1,
            LexemeKind::Token(TokenKind::Symbol(
                Symbol::Semicolon | Symbol::LeftBrace | Symbol::RightBrace | Symbol::Equal,
            )) => return None,
            _ => {}
        }
    }
    (depth == 0).then_some(i)
}

/// 查找在 `recv_end` 结束（且优先匹配 `expr_start`）的 AST 表达式并读取其推导类型。
pub(crate) fn expression_type_ending_at(
    typed: &CompilationUnitTypes,
    parsed_file: Option<&ParsedFile>,
    source_unit: SourceUnitId,
    expr_start: Option<usize>,
    recv_end: usize,
) -> Option<UnitTypeId> {
    let parsed = parsed_file?;
    let mut best: Option<(bool, usize, UnitTypeId)> = None;
    for (expr_id, node) in parsed.ast().expressions().iter() {
        let span = node.span();
        if span.end() != recv_end {
            continue;
        }
        let Some(ty) = typed.expression_type(UnitExpressionId::new(source_unit, expr_id)) else {
            continue;
        };
        let exact_start = expr_start == Some(span.start());
        let len = span.len();
        let replace = match best {
            None => true,
            Some((best_exact, best_len, _)) => {
                (exact_start && !best_exact) || (exact_start == best_exact && len > best_len)
            }
        };
        if replace {
            best = Some((exact_start, len, ty));
        }
    }
    best.map(|(_, _, ty)| ty)
}

/// 返回在 `offset` 处可见的局部符号列表（由内向外排列，排除已退出块、兄弟函数以及当前正在初始化的自身声明）。
pub(crate) fn local_symbols_in_scope<'a>(
    names: &'a CompilationUnitNames,
    parsed_file: Option<&ParsedFile>,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
) -> Vec<(
    lang_frontend::name_resolution::SymbolId,
    &'a lang_frontend::name_resolution::Symbol,
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

    let mut scope_chain = Vec::new();
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
            if matches!(
                sym.kind(),
                FrontSymbolKind::Variable | FrontSymbolKind::Constant
            ) && is_offset_inside_own_initializer(parsed_file, sym.span(), offset)
            {
                continue;
            }
            out.push((sym.id(), sym));
        }
    }
    out
}

fn is_offset_inside_own_initializer(
    parsed_file: Option<&ParsedFile>,
    name_span: lang_frontend::source::Span,
    offset: usize,
) -> bool {
    let Some(parsed) = parsed_file else {
        return false;
    };
    for (_, node) in parsed.ast().items().iter() {
        let decl_name = match node.payload() {
            Item::Variable { name, .. } | Item::Constant { name, .. } => name,
            _ => continue,
        };
        if matches!(decl_name, NameMarker::Present(s) if *s == name_span) {
            return offset <= node.span().end();
        }
    }
    for (_, stmt) in parsed.ast().statements().iter() {
        if let Statement::LocalDestructuring { bindings, .. } = stmt.payload()
            && bindings
                .iter()
                .any(|b| matches!(b, NameMarker::Present(s) if *s == name_span))
        {
            return offset <= stmt.span().end();
        }
    }
    false
}

pub(crate) fn find_local_symbol_in_scope<'a>(
    names: &'a CompilationUnitNames,
    parsed_file: Option<&ParsedFile>,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
    name: &str,
) -> Option<(
    lang_frontend::name_resolution::SymbolId,
    &'a lang_frontend::name_resolution::Symbol,
)> {
    local_symbols_in_scope(names, parsed_file, source_unit, source_id, offset)
        .into_iter()
        .find(|(_, sym)| sym.name() == name)
}

/// 计算当前文件内裸名可见的全部顶层声明（同文件全部声明、同包非 private 声明、单段/多段显式导入、别名与通配导入）。
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

    // 2. Explicit imports (including default-package single-segment imports), aliases, and wildcards.
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
            } else {
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
                            if pkg_segs.is_empty() {
                                p.is_default()
                            } else {
                                p.segments().len() == pkg_segs.len()
                                    && p.segments()
                                        .iter()
                                        .zip(pkg_segs.iter())
                                        .all(|(a, b)| a == b)
                            }
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

pub(crate) fn collect_qualifier_segments<'a>(
    text: &'a str,
    receiver_tokens: &[Lexeme],
) -> Vec<&'a str> {
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
