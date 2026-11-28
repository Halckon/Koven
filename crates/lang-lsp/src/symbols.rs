//! `textDocument/documentSymbol` 与 `workspace/symbol` 实现。

use std::collections::BTreeMap;

use lang_frontend::{
    name_resolution::{Namespace, SourceUnitId, SymbolKind as FrontSymbolKind, UnitDeclaration},
    source::SourceId,
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

/// 为单个源码文件构造确定性的层级 `DocumentSymbol` 树。
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
            } else if let Some(ty_text) = format_unit_type(sig.ty(), typed, names) {
                detail = Some(ty_text);
            }
        }

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

/// 在整个编译单元内按大小写不敏感子串搜索符号（支持 `workspace/symbol`）。
pub(crate) fn workspace_symbols(
    snapshot: &UnitAnalysisSnapshot,
    uris_by_source_unit: &BTreeMap<SourceUnitId, Uri>,
    query: &str,
) -> Result<Vec<SymbolInformation>, PositionMappingError> {
    let sources = snapshot.name_snapshot().sources();
    let names = snapshot.name_snapshot().names();
    let typed = snapshot.typed();
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

        if let Some(typed) = typed
            && let Some(sig) = typed.signatures().declaration(decl.id())
            && let Some(nominal) = sig.nominal()
        {
            let container = Some(match &pkg_name {
                Some(pkg) => format!("{pkg}.{}", decl.name()),
                None => decl.name().to_owned(),
            });
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

fn matches_query(name: &str, needle_lower: &str) -> bool {
    needle_lower.is_empty() || name.to_ascii_lowercase().contains(needle_lower)
}

fn field_symbol(
    sources: &lang_frontend::source::SourceMap,
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
