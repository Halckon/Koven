//! `textDocument/documentSymbol` 与 `workspace/symbol` 实现（支持 AST 回退与 Flat/Nested 能力协商）。

use std::collections::BTreeMap;

use lang_frontend::{
    name_resolution::{Namespace, SourceUnitId, SymbolKind as FrontSymbolKind, UnitDeclaration},
    parser::{Item, NameMarker, ParsedFile},
    source::{SourceId, SourceMap, Span},
    type_checking::{CompilationUnitTypes, UnitCallableSignature, UnitFieldSignature},
};
use lsp_types::{DocumentSymbol, Location, SymbolInformation, SymbolKind, Uri};

use crate::{
    ide_features::format_unit_type,
    position_adapter::{PositionMappingError, span_range},
    unit_analysis_snapshot::UnitAnalysisSnapshot,
};

/// 去重同一 `object` 在 Type/Value 双命名空间产生的重复声明，优先保留 `Namespace::Type` 以保留成员树。
fn deduplicated_declarations(declarations: &[UnitDeclaration]) -> Vec<&UnitDeclaration> {
    let mut by_span: BTreeMap<(SourceUnitId, usize, usize), &UnitDeclaration> = BTreeMap::new();
    for decl in declarations {
        let key = (
            decl.source_unit(),
            decl.name_span().start(),
            decl.name_span().end(),
        );
        match by_span.get(&key) {
            Some(existing) if existing.namespace() == Namespace::Type => {}
            _ => {
                by_span.insert(key, decl);
            }
        }
    }
    by_span.into_values().collect()
}

/// 将层级 `DocumentSymbol` 树展平为 `SymbolInformation`（供不支持 `hierarchicalDocumentSymbolSupport` 的客户端使用）。
pub(crate) fn flatten_document_symbols(
    uri: &Uri,
    symbols: &[DocumentSymbol],
) -> Vec<SymbolInformation> {
    let mut out = Vec::new();
    for sym in symbols {
        push_flattened(uri, sym, None, &mut out);
    }
    out
}

fn push_flattened(
    uri: &Uri,
    sym: &DocumentSymbol,
    container_name: Option<String>,
    out: &mut Vec<SymbolInformation>,
) {
    #[allow(deprecated)]
    out.push(SymbolInformation {
        name: sym.name.clone(),
        kind: sym.kind,
        tags: sym.tags.clone(),
        deprecated: sym.deprecated,
        location: Location::new(uri.clone(), sym.selection_range),
        container_name,
    });
    if let Some(children) = &sym.children {
        for child in children {
            push_flattened(uri, child, Some(sym.name.clone()), out);
        }
    }
}

/// 为单个源码文件构造确定性的层级 `DocumentSymbol` 树（当全单元类型检查失败时自动回退到 AST 提取成员与枚举变体）。
pub(crate) fn document_symbols(
    snapshot: &UnitAnalysisSnapshot,
    source_unit: SourceUnitId,
    source_id: SourceId,
) -> Result<Vec<DocumentSymbol>, PositionMappingError> {
    let sources = snapshot.name_snapshot().sources();
    let names = snapshot.name_snapshot().names();
    let typed = snapshot.typed();
    let parsed_file = snapshot
        .name_snapshot()
        .parsed_files()
        .iter()
        .find(|p| p.source_id() == source_id);

    let mut out = Vec::new();
    for decl in deduplicated_declarations(names.index().declarations()) {
        if decl.source_unit() != source_unit {
            continue;
        }
        let selection_range = span_range(sources, decl.name_span())?;
        let full_span = parsed_file
            .and_then(|p| p.ast().items().get(decl.root()).ok())
            .map_or(decl.name_span(), |item| item.span());
        let range = span_range(sources, full_span)?;

        let mut children = Vec::new();
        let mut detail = None;
        if let Some(typed) = typed
            && let Some(sig) = typed.signatures().declaration(decl.id())
        {
            if let Some(callable) = sig.callable() {
                detail = Some(format_callable_detail(callable, typed, names));
            } else if let Some(nominal) = sig.nominal() {
                for field in nominal.fields() {
                    if field.span().source_id() == source_id {
                        children.push(field_symbol(sources, field, typed, names)?);
                    }
                }
                for case in nominal.enum_cases() {
                    if case.name_span().source_id() == source_id {
                        let r = span_range(sources, case.name_span())?;
                        #[allow(deprecated)]
                        children.push(DocumentSymbol {
                            name: case.name().to_owned(),
                            detail: None,
                            kind: SymbolKind::ENUM_MEMBER,
                            tags: None,
                            deprecated: None,
                            range: r,
                            selection_range: r,
                            children: None,
                        });
                    }
                }
                for member in nominal
                    .members()
                    .iter()
                    .chain(nominal.companion_members().iter())
                {
                    if member.name_span().source_id() == source_id {
                        let r = span_range(sources, member.name_span())?;
                        #[allow(deprecated)]
                        children.push(DocumentSymbol {
                            name: member.name().to_owned(),
                            detail: Some(format_callable_detail(member, typed, names)),
                            kind: SymbolKind::METHOD,
                            tags: None,
                            deprecated: None,
                            range: r,
                            selection_range: r,
                            children: None,
                        });
                    }
                }
            } else if let Some(ty_text) = format_unit_type(sig.ty(), typed, names) {
                detail = Some(ty_text);
            }
        } else if let Some(parsed) = parsed_file {
            // AST fallback: keep fields, enum variants, and methods visible even when another file fails type-checking.
            for (name, kind, full, sel) in ast_classifier_members(sources, parsed, decl.root()) {
                let r = span_range(sources, full)?;
                let sr = span_range(sources, sel)?;
                #[allow(deprecated)]
                children.push(DocumentSymbol {
                    name,
                    detail: None,
                    kind,
                    tags: None,
                    deprecated: None,
                    range: r,
                    selection_range: sr,
                    children: None,
                });
            }
        }

        children.sort_by(|a, b| {
            (
                a.selection_range.start.line,
                a.selection_range.start.character,
                &a.name,
            )
                .cmp(&(
                    b.selection_range.start.line,
                    b.selection_range.start.character,
                    &b.name,
                ))
        });

        #[allow(deprecated)]
        out.push(DocumentSymbol {
            name: decl.name().to_owned(),
            detail,
            kind: map_symbol_kind(decl.kind()),
            tags: None,
            deprecated: None,
            range,
            selection_range,
            children: (!children.is_empty()).then_some(children),
        });
    }

    out.sort_by(|a, b| {
        (
            a.selection_range.start.line,
            a.selection_range.start.character,
            &a.name,
        )
            .cmp(&(
                b.selection_range.start.line,
                b.selection_range.start.character,
                &b.name,
            ))
    });
    Ok(out)
}

/// 在整个编译单元内按大小写不敏感子串搜索符号（支持 `workspace/symbol` 及无 typed 快照时的 AST 回退）。
pub(crate) fn workspace_symbols(
    snapshot: &UnitAnalysisSnapshot,
    uris_by_source_unit: &BTreeMap<SourceUnitId, Uri>,
    query: &str,
) -> Result<Vec<SymbolInformation>, PositionMappingError> {
    let sources = snapshot.name_snapshot().sources();
    let names = snapshot.name_snapshot().names();
    let typed = snapshot.typed();
    let parsed_files = snapshot.name_snapshot().parsed_files();
    let needle = query.to_ascii_lowercase();

    let mut out = Vec::new();
    for decl in deduplicated_declarations(names.index().declarations()) {
        let Some(uri) = uris_by_source_unit.get(&decl.source_unit()) else {
            continue;
        };
        let pkg_name = names
            .index()
            .packages()
            .get(decl.package().index())
            .and_then(|pkg| pkg.name())
            .filter(|name| !name.is_default())
            .map(|name| name.segments().join("."));

        if matches_query(decl.name(), &needle) {
            let range = span_range(sources, decl.name_span())?;
            #[allow(deprecated)]
            out.push(SymbolInformation {
                name: decl.name().to_owned(),
                kind: map_symbol_kind(decl.kind()),
                tags: None,
                deprecated: None,
                location: Location::new(uri.clone(), range),
                container_name: pkg_name.clone(),
            });
        }

        let container = Some(match &pkg_name {
            Some(pkg) => format!("{pkg}.{}", decl.name()),
            None => decl.name().to_owned(),
        });

        if let Some(typed) = typed
            && let Some(sig) = typed.signatures().declaration(decl.id())
            && let Some(nominal) = sig.nominal()
        {
            for member in nominal
                .members()
                .iter()
                .chain(nominal.companion_members().iter())
            {
                if matches_query(member.name(), &needle) {
                    let range = span_range(sources, member.name_span())?;
                    #[allow(deprecated)]
                    out.push(SymbolInformation {
                        name: member.name().to_owned(),
                        kind: SymbolKind::METHOD,
                        tags: None,
                        deprecated: None,
                        location: Location::new(uri.clone(), range),
                        container_name: container.clone(),
                    });
                }
            }
            for case in nominal.enum_cases() {
                if matches_query(case.name(), &needle) {
                    let range = span_range(sources, case.name_span())?;
                    #[allow(deprecated)]
                    out.push(SymbolInformation {
                        name: case.name().to_owned(),
                        kind: SymbolKind::ENUM_MEMBER,
                        tags: None,
                        deprecated: None,
                        location: Location::new(uri.clone(), range),
                        container_name: container.clone(),
                    });
                }
            }
        } else if let Some(parsed) = parsed_files
            .iter()
            .find(|p| p.source_id() == decl.name_span().source_id())
        {
            for (name, kind, _, sel) in ast_classifier_members(sources, parsed, decl.root()) {
                if matches!(kind, SymbolKind::METHOD | SymbolKind::ENUM_MEMBER)
                    && matches_query(&name, &needle)
                {
                    let range = span_range(sources, sel)?;
                    #[allow(deprecated)]
                    out.push(SymbolInformation {
                        name,
                        kind,
                        tags: None,
                        deprecated: None,
                        location: Location::new(uri.clone(), range),
                        container_name: container.clone(),
                    });
                }
            }
        }
    }

    out.sort_by(|a, b| {
        (
            a.name.as_str(),
            a.location.uri.as_str(),
            a.location.range.start.line,
            a.location.range.start.character,
        )
            .cmp(&(
                b.name.as_str(),
                b.location.uri.as_str(),
                b.location.range.start.line,
                b.location.range.start.character,
            ))
    });
    Ok(out)
}

fn ast_classifier_members(
    sources: &SourceMap,
    parsed: &ParsedFile,
    root: lang_frontend::ast::ItemId,
) -> Vec<(String, SymbolKind, Span, Span)> {
    let Ok(text) = sources.source_text(parsed.source_id()) else {
        return Vec::new();
    };
    let Ok(root_node) = parsed.ast().items().get(root) else {
        return Vec::new();
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
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(ctor) = &classifier.primary_constructor {
        for field in &ctor.fields {
            if let NameMarker::Present(s) = field.name
                && s.end() <= text.len()
            {
                out.push((
                    text[s.start()..s.end()].to_owned(),
                    SymbolKind::FIELD,
                    field.span,
                    s,
                ));
            }
        }
    }
    if let Some(body) = &classifier.body {
        for variant in &body.variants {
            if let NameMarker::Present(s) = variant.name
                && s.end() <= text.len()
            {
                out.push((
                    text[s.start()..s.end()].to_owned(),
                    SymbolKind::ENUM_MEMBER,
                    variant.span,
                    s,
                ));
            }
        }
        for &member_id in &body.members {
            collect_ast_member_item(parsed, text, member_id, &mut out);
        }
    }
    out
}

fn collect_ast_member_item(
    parsed: &ParsedFile,
    text: &str,
    item_id: lang_frontend::ast::ItemId,
    out: &mut Vec<(String, SymbolKind, Span, Span)>,
) {
    let Ok(node) = parsed.ast().items().get(item_id) else {
        return;
    };
    let full = node.span();
    let payload = match node.payload() {
        Item::Modified { declaration, .. } => parsed
            .ast()
            .items()
            .get(*declaration)
            .ok()
            .map(|n| n.payload()),
        other => Some(other),
    };
    match payload {
        Some(Item::Function {
            name: NameMarker::Present(s),
            ..
        }) if s.end() <= text.len() => {
            out.push((
                text[s.start()..s.end()].to_owned(),
                SymbolKind::METHOD,
                full,
                *s,
            ));
        }
        Some(Item::Constant {
            name: NameMarker::Present(s),
            ..
        }) if s.end() <= text.len() => {
            out.push((
                text[s.start()..s.end()].to_owned(),
                SymbolKind::CONSTANT,
                full,
                *s,
            ));
        }
        Some(Item::Companion(comp)) => {
            for &comp_item in &comp.body.members {
                collect_ast_member_item(parsed, text, comp_item, out);
            }
        }
        _ => {}
    }
}

fn matches_query(name: &str, needle_lower: &str) -> bool {
    needle_lower.is_empty() || name.to_ascii_lowercase().contains(needle_lower)
}

fn field_symbol(
    sources: &SourceMap,
    field: &UnitFieldSignature,
    typed: &CompilationUnitTypes,
    names: &lang_frontend::name_resolution::CompilationUnitNames,
) -> Result<DocumentSymbol, PositionMappingError> {
    let range = span_range(sources, field.span())?;
    let detail = format_unit_type(field.ty(), typed, names);
    #[allow(deprecated)]
    Ok(DocumentSymbol {
        name: field.name().to_owned(),
        detail,
        kind: SymbolKind::FIELD,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: None,
    })
}

pub(crate) fn format_callable_detail(
    callable: &UnitCallableSignature,
    typed: &CompilationUnitTypes,
    names: &lang_frontend::name_resolution::CompilationUnitNames,
) -> String {
    let params = callable
        .parameters()
        .iter()
        .map(|param| {
            let name = param.name().unwrap_or("_");
            let ty = format_unit_type(param.ty(), typed, names).unwrap_or_else(|| "?".to_owned());
            format!("{name}: {ty}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let ret =
        format_unit_type(callable.return_type(), typed, names).unwrap_or_else(|| "Unit".to_owned());
    format!("({params}) -> {ret}")
}

fn map_symbol_kind(kind: FrontSymbolKind) -> SymbolKind {
    match kind {
        FrontSymbolKind::Function => SymbolKind::FUNCTION,
        FrontSymbolKind::Constant => SymbolKind::CONSTANT,
        FrontSymbolKind::Variable => SymbolKind::VARIABLE,
        FrontSymbolKind::Field => SymbolKind::FIELD,
        FrontSymbolKind::EnumVariant => SymbolKind::ENUM_MEMBER,
        FrontSymbolKind::Classifier => SymbolKind::CLASS,
        FrontSymbolKind::TypeParameter => SymbolKind::TYPE_PARAMETER,
        _ => SymbolKind::OBJECT,
    }
}
