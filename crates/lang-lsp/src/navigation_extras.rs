//! P3 导航与编辑辅助：`textDocument/documentHighlight`、`textDocument/foldingRange` 与 `textDocument/typeDefinition`。

use std::collections::BTreeSet;

use lang_frontend::{
    lexer::{LexemeKind, TriviaKind, lex},
    name_resolution::{Namespace, SourceUnitId, UnitSymbolId},
    parser::{Expression, Item, ParsedFile, Statement},
    source::{SourceId, Span},
    type_checking::{UnitExpressionId, UnitTypeId, UnitTypeKind},
};
use lsp_types::{
    DocumentHighlight, DocumentHighlightKind, FoldingRange, FoldingRangeKind,
    GotoDefinitionResponse, Location, Uri,
};

use crate::{
    position_adapter::{PositionMappingError, span_range},
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

/// 计算当前文件内同一符号的全部高亮出现位置（区分 `WRITE` 声明/赋值目标与 `READ` 读取）。
pub(crate) fn document_highlights(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
) -> Result<Vec<DocumentHighlight>, PositionMappingError> {
    let sources = snapshot.name_snapshot().sources();
    let parsed_file = snapshot
        .name_snapshot()
        .parsed_files()
        .iter()
        .find(|p| p.source_id() == source_id);

    let write_spans = collect_write_spans(snapshot, parsed_file, source_unit, source_id);
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();

    for target in snapshot
        .definitions()
        .references_at(source_unit, offset, true)
    {
        if target.source_unit != source_unit || target.span.source_id() != source_id {
            continue;
        }
        let key = (target.span.start(), target.span.end());
        if !seen.insert(key) {
            continue;
        }
        let range = span_range(sources, target.span)?;
        let kind = if write_spans.contains(&key) {
            DocumentHighlightKind::WRITE
        } else {
            DocumentHighlightKind::READ
        };
        out.push(DocumentHighlight {
            range,
            kind: Some(kind),
        });
    }

    out.sort_by(|a, b| {
        (
            a.range.start.line,
            a.range.start.character,
            a.range.end.line,
            a.range.end.character,
        )
            .cmp(&(
                b.range.start.line,
                b.range.start.character,
                b.range.end.line,
                b.range.end.character,
            ))
    });
    Ok(out)
}

/// 收集当前文件内所有赋值左侧写入位置的 `(start, end)` 字节范围。
pub(crate) fn collect_assignment_write_spans(
    parsed_file: Option<&ParsedFile>,
) -> BTreeSet<(usize, usize)> {
    let mut writes = BTreeSet::new();
    if let Some(parsed) = parsed_file {
        for (_, node) in parsed.ast().expressions().iter() {
            if let Expression::Assignment { target, .. } = node.payload()
                && let Ok(target_node) = parsed.ast().expressions().get(*target)
            {
                match target_node.payload() {
                    Expression::Name => {
                        let s = target_node.span();
                        writes.insert((s.start(), s.end()));
                    }
                    Expression::Member { name_span, .. } => {
                        writes.insert((name_span.start(), name_span.end()));
                    }
                    _ => {}
                }
            }
        }
    }
    writes
}

fn collect_write_spans(
    snapshot: &UnitAnalysisSnapshot,
    parsed_file: Option<&ParsedFile>,
    source_unit: SourceUnitId,
    source_id: SourceId,
) -> BTreeSet<(usize, usize)> {
    let mut writes = collect_assignment_write_spans(parsed_file);
    let names = snapshot.name_snapshot().names();
    for decl in names.index().declarations() {
        if decl.source_unit() == source_unit && decl.name_span().source_id() == source_id {
            writes.insert((decl.name_span().start(), decl.name_span().end()));
        }
    }
    if let Some(unit_names) = names
        .source_units()
        .iter()
        .find(|u| u.source_unit() == source_unit)
    {
        for sym in unit_names.resolution().symbols() {
            if !sym.is_synthetic() && sym.span().source_id() == source_id {
                writes.insert((sym.span().start(), sym.span().end()));
            }
        }
    }
    writes
}

/// 为单个源码文件构造确定性的代码折叠区域（导入块、多行/连续注释、类型/伴生/函数/控制流块）。
pub(crate) fn folding_ranges(
    snapshot: &UnitAnalysisSnapshot,
    source_id: SourceId,
) -> Result<Vec<FoldingRange>, PositionMappingError> {
    let sources = snapshot.name_snapshot().sources();
    let parsed_file = snapshot
        .name_snapshot()
        .parsed_files()
        .iter()
        .find(|p| p.source_id() == source_id);
    let mut seen: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut out = Vec::new();

    // 1. Imports block (`FoldingRangeKind::Imports`).
    if let Some(parsed) = parsed_file
        && parsed.imports().len() >= 2
    {
        let first = parsed.imports()[0].span;
        let last = parsed.imports()[parsed.imports().len() - 1].span;
        let start_line = span_range(sources, first)?.start.line;
        let end_line = span_range(sources, last)?.end.line;
        if end_line > start_line && seen.insert((start_line, end_line)) {
            out.push(FoldingRange {
                start_line,
                start_character: None,
                end_line,
                end_character: None,
                kind: Some(FoldingRangeKind::Imports),
                collapsed_text: None,
            });
        }
    }

    // 2. Multi-line block comments and consecutive `//` line comments (`FoldingRangeKind::Comment`).
    if let Ok(lexed) = lex(sources, source_id) {
        let mut run_start: Option<u32> = None;
        let mut run_end: Option<u32> = None;
        for lexeme in lexed.lexemes() {
            match lexeme.kind() {
                LexemeKind::Trivia(TriviaKind::BlockComment) => {
                    flush_comment_run(&mut run_start, &mut run_end, &mut seen, &mut out);
                    let range = span_range(sources, lexeme.span())?;
                    if range.end.line > range.start.line
                        && seen.insert((range.start.line, range.end.line))
                    {
                        out.push(FoldingRange {
                            start_line: range.start.line,
                            start_character: None,
                            end_line: range.end.line,
                            end_character: None,
                            kind: Some(FoldingRangeKind::Comment),
                            collapsed_text: None,
                        });
                    }
                }
                LexemeKind::Trivia(TriviaKind::LineComment) => {
                    let line = span_range(sources, lexeme.span())?.start.line;
                    match (run_start, run_end) {
                        (Some(_), Some(prev_end)) if line == prev_end + 1 => {
                            run_end = Some(line);
                        }
                        _ => {
                            flush_comment_run(&mut run_start, &mut run_end, &mut seen, &mut out);
                            run_start = Some(line);
                            run_end = Some(line);
                        }
                    }
                }
                LexemeKind::Trivia(TriviaKind::Whitespace | TriviaKind::Newline) => {}
                _ => {
                    flush_comment_run(&mut run_start, &mut run_end, &mut seen, &mut out);
                }
            }
        }
        flush_comment_run(&mut run_start, &mut run_end, &mut seen, &mut out);
    }

    // 3. Multi-line AST items, blocks, and `when` expressions.
    if let Some(parsed) = parsed_file {
        for (_, node) in parsed.ast().items().iter() {
            if matches!(
                node.payload(),
                Item::Classifier(_)
                    | Item::Companion(_)
                    | Item::Function { .. }
                    | Item::Deinit { .. }
            ) {
                push_region_fold(sources, node.span(), &mut seen, &mut out)?;
            }
        }
        for (_, node) in parsed.ast().statements().iter() {
            if matches!(
                node.payload(),
                Statement::Block { .. }
                    | Statement::ControlBody { .. }
                    | Statement::LambdaBody { .. }
                    | Statement::While { .. }
                    | Statement::For { .. }
                    | Statement::Loop { .. }
            ) {
                push_region_fold(sources, node.span(), &mut seen, &mut out)?;
            }
        }
        for (_, node) in parsed.ast().expressions().iter() {
            if matches!(node.payload(), Expression::When { .. }) {
                push_region_fold(sources, node.span(), &mut seen, &mut out)?;
            }
        }
    }

    out.sort_by_key(|r| (r.start_line, r.end_line));
    Ok(out)
}

fn flush_comment_run(
    run_start: &mut Option<u32>,
    run_end: &mut Option<u32>,
    seen: &mut BTreeSet<(u32, u32)>,
    out: &mut Vec<FoldingRange>,
) {
    if let (Some(start), Some(end)) = (*run_start, *run_end)
        && end > start
        && seen.insert((start, end))
    {
        out.push(FoldingRange {
            start_line: start,
            start_character: None,
            end_line: end,
            end_character: None,
            kind: Some(FoldingRangeKind::Comment),
            collapsed_text: None,
        });
    }
    *run_start = None;
    *run_end = None;
}

fn push_region_fold(
    sources: &lang_frontend::source::SourceMap,
    span: Span,
    seen: &mut BTreeSet<(u32, u32)>,
    out: &mut Vec<FoldingRange>,
) -> Result<(), PositionMappingError> {
    let range = span_range(sources, span)?;
    if range.end.line <= range.start.line {
        return Ok(());
    }
    // Keep closing `}` visible on its own line when the region spans 3+ lines.
    let end_line = if range.end.line > range.start.line + 1 {
        range.end.line - 1
    } else {
        range.end.line
    };
    if end_line > range.start.line && seen.insert((range.start.line, end_line)) {
        out.push(FoldingRange {
            start_line: range.start.line,
            start_character: None,
            end_line,
            end_character: None,
            kind: Some(FoldingRangeKind::Region),
            collapsed_text: None,
        });
    }
    Ok(())
}

/// 跳转到光标处符号或表达式的底层名义类型声明（`textDocument/typeDefinition`）。
pub(crate) fn type_definition<F>(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
    offset: usize,
    uri_for_unit: F,
) -> Result<Option<GotoDefinitionResponse>, PositionMappingError>
where
    F: Fn(SourceUnitId) -> Option<Uri>,
{
    let Some(typed) = snapshot.typed() else {
        return Ok(None);
    };
    let names = snapshot.name_snapshot().names();
    let sources = snapshot.name_snapshot().sources();
    let parsed_file = snapshot
        .name_snapshot()
        .parsed_files()
        .iter()
        .find(|p| p.source_id() == source_id);

    // 1. If cursor is already on a nominal type reference/declaration, jump directly to it.
    let entry_target = snapshot
        .definitions()
        .entry_at(source_unit, offset)
        .and_then(|(_, targets)| targets.first().copied());
    if let Some(target) = entry_target
        && let Some(decl) = names.index().declarations().iter().find(|d| {
            d.source_unit() == target.source_unit
                && d.name_span() == target.span
                && d.namespace() == Namespace::Type
        })
    {
        return decl_location_response(sources, decl, &uri_for_unit);
    }

    // 2. Prefer substituted expression type at cursor (preserves concrete generic projections like `h.value` -> `Point`).
    let mut resolved_type: Option<UnitTypeId> = None;
    if let Some(parsed) = parsed_file {
        let mut best: Option<(usize, UnitTypeId)> = None;
        for (expr_id, node) in parsed.ast().expressions().iter() {
            let s = node.span();
            if s.start() <= offset
                && offset <= s.end()
                && let Some(ty) = typed.expression_type(UnitExpressionId::new(source_unit, expr_id))
            {
                let len = s.len();
                if best.is_none_or(|(best_len, _)| len < best_len) {
                    best = Some((len, ty));
                }
            }
        }
        if let Some((_, ty)) = best
            && unwrap_nominal_declaration(ty, typed).is_some()
        {
            resolved_type = Some(ty);
        }
    }

    // 3. Fallback to declaration/symbol/field type (for variable/parameter declarations without an expression node).
    if resolved_type.is_none()
        && let Some(target) = entry_target
    {
        if let Some(unit_names) = names
            .source_units()
            .iter()
            .find(|u| u.source_unit() == target.source_unit)
            && let Some(sym) = unit_names
                .resolution()
                .symbols()
                .iter()
                .find(|s| s.span() == target.span)
        {
            resolved_type = typed.symbol_type(UnitSymbolId::new(target.source_unit, sym.id()));
        }
        if resolved_type.is_none() {
            'outer: for sig in typed.signatures().declarations() {
                if let Some(nominal) = sig.nominal() {
                    for field in nominal.fields() {
                        if field.span() == target.span {
                            resolved_type = Some(field.ty());
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    let Some(ty_id) = resolved_type else {
        return Ok(None);
    };
    let Some(declaration) = unwrap_nominal_declaration(ty_id, typed) else {
        return Ok(None);
    };
    let Some(decl) = names
        .index()
        .declarations()
        .iter()
        .find(|d| d.id() == declaration)
    else {
        return Ok(None);
    };
    decl_location_response(sources, decl, &uri_for_unit)
}

fn unwrap_nominal_declaration(
    ty_id: UnitTypeId,
    typed: &lang_frontend::type_checking::CompilationUnitTypes,
) -> Option<lang_frontend::name_resolution::DeclarationId> {
    let inner = match typed.types().get(ty_id) {
        Some(UnitTypeKind::Nullable(inner)) => *inner,
        _ => ty_id,
    };
    match typed.types().get(inner) {
        Some(UnitTypeKind::Nominal { declaration, .. }) => Some(*declaration),
        _ => None,
    }
}

fn decl_location_response<F>(
    sources: &lang_frontend::source::SourceMap,
    decl: &lang_frontend::name_resolution::UnitDeclaration,
    uri_for_unit: &F,
) -> Result<Option<GotoDefinitionResponse>, PositionMappingError>
where
    F: Fn(SourceUnitId) -> Option<Uri>,
{
    let Some(uri) = uri_for_unit(decl.source_unit()) else {
        return Ok(None);
    };
    let range = span_range(sources, decl.name_span())?;
    Ok(Some(GotoDefinitionResponse::Scalar(Location::new(
        uri, range,
    ))))
}

/// 查找光标处接口或接口方法的全部具体实现（`textDocument/implementation`）。
pub(crate) fn implementations<F>(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    offset: usize,
    uri_for_unit: F,
) -> Result<Option<GotoDefinitionResponse>, PositionMappingError>
where
    F: Fn(SourceUnitId) -> Option<Uri>,
{
    let Some(typed) = snapshot.typed() else {
        return Ok(None);
    };
    let names = snapshot.name_snapshot().names();
    let sources = snapshot.name_snapshot().sources();
    let Some((_, targets)) = snapshot.definitions().entry_at(source_unit, offset) else {
        return Ok(None);
    };
    let Some(target) = targets.first().copied() else {
        return Ok(None);
    };

    // Case 1: Cursor is on an `interface` declaration/reference -> find all concrete implementing types.
    if let Some(iface_decl) = names.index().declarations().iter().find(|d| {
        d.source_unit() == target.source_unit
            && d.name_span() == target.span
            && d.namespace() == Namespace::Type
    }) && typed
        .signatures()
        .declaration(iface_decl.id())
        .and_then(|s| s.nominal())
        .is_some_and(|n| n.kind() == lang_frontend::type_checking::NominalKind::Interface)
    {
        let mut locations = Vec::new();
        for decl in names.index().declarations() {
            if decl.namespace() != Namespace::Type || decl.id() == iface_decl.id() {
                continue;
            }
            let Some(nominal) = typed
                .signatures()
                .declaration(decl.id())
                .and_then(|s| s.nominal())
            else {
                continue;
            };
            if nominal.kind() == lang_frontend::type_checking::NominalKind::Interface {
                continue;
            }
            let implements = nominal.interfaces().iter().any(|&ty| {
                matches!(
                    typed.types().get(ty),
                    Some(UnitTypeKind::Nominal { declaration, .. }) if *declaration == iface_decl.id()
                )
            });
            if implements && let Some(uri) = uri_for_unit(decl.source_unit()) {
                let range = span_range(sources, decl.name_span())?;
                locations.push(Location::new(uri, range));
            }
        }
        return finish_implementation_locations(locations);
    }

    // Case 2: Cursor is on an `interface` method -> find overriding methods in concrete implementors.
    let mut iface_owner_and_method: Option<(
        lang_frontend::name_resolution::DeclarationId,
        String,
    )> = None;
    for decl in names.index().declarations() {
        if decl.namespace() != Namespace::Type {
            continue;
        }
        let Some(nominal) = typed
            .signatures()
            .declaration(decl.id())
            .and_then(|s| s.nominal())
        else {
            continue;
        };
        if nominal.kind() != lang_frontend::type_checking::NominalKind::Interface {
            continue;
        }
        if let Some(member) = nominal
            .members()
            .iter()
            .find(|m| m.name_span() == target.span)
        {
            iface_owner_and_method = Some((decl.id(), member.name().to_owned()));
            break;
        }
    }
    if let Some((iface_id, method_name)) = iface_owner_and_method {
        let mut locations = Vec::new();
        for decl in names.index().declarations() {
            if decl.namespace() != Namespace::Type || decl.id() == iface_id {
                continue;
            }
            let Some(nominal) = typed
                .signatures()
                .declaration(decl.id())
                .and_then(|s| s.nominal())
            else {
                continue;
            };
            if nominal.kind() == lang_frontend::type_checking::NominalKind::Interface {
                continue;
            }
            let implements = nominal.interfaces().iter().any(|&ty| {
                matches!(
                    typed.types().get(ty),
                    Some(UnitTypeKind::Nominal { declaration, .. }) if *declaration == iface_id
                )
            });
            if !implements {
                continue;
            }
            for member in nominal.members() {
                if member.name() == method_name
                    && let Some(uri) = uri_for_unit(decl.source_unit())
                {
                    let range = span_range(sources, member.name_span())?;
                    locations.push(Location::new(uri, range));
                }
            }
        }
        return finish_implementation_locations(locations);
    }

    Ok(None)
}

fn finish_implementation_locations(
    mut locations: Vec<Location>,
) -> Result<Option<GotoDefinitionResponse>, PositionMappingError> {
    if locations.is_empty() {
        return Ok(None);
    }
    locations.sort_by(|a, b| {
        (
            a.uri.as_str(),
            a.range.start.line,
            a.range.start.character,
            a.range.end.line,
            a.range.end.character,
        )
            .cmp(&(
                b.uri.as_str(),
                b.range.start.line,
                b.range.start.character,
                b.range.end.line,
                b.range.end.character,
            ))
    });
    Ok(Some(GotoDefinitionResponse::Array(locations)))
}
