//! 从真实 AST 和 names 取身份；错误 binding 不读取 body，也不造 unit symbol。

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{NameResolution, Namespace, Symbol, SymbolKind},
    parser::{ClassifierKind, Expression, ForBinding, Item, NameMarker, Statement},
    source::Span,
    type_checking::{BuiltinType, NominalKind, UnitExpressionId, UnitStatementId, UnitTypeKind},
};

use super::{
    Case,
    analysis::{Blocked, Fixture, Observed, blocked},
};

#[derive(Debug)]
pub(super) struct Sites {
    pub(super) statement: StatementId,
    pub(super) source: ExpressionId,
    pub(super) source_span: Span,
    pub(super) binding_span: Span,
    pub(super) markers: Vec<Span>,
    pub(super) nominal: Option<(ItemId, Span)>,
}

pub(super) fn sites(fixture: &Fixture, case: &Case) -> Result<Sites, Blocked> {
    let ast = fixture.file.ast();
    let sources = &fixture.sources;
    let loops = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| match node.payload() {
            Statement::For {
                binding,
                source,
                body,
                ..
            } => Some((id, binding, *source, *body)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(statement, binding, source, body)] = loops.as_slice() else {
        return Err(blocked("AST fixture", "expected exactly one for statement"));
    };
    let (binding_span, markers) = match binding {
        ForBinding::Name(NameMarker::Present(span)) if !case.destructuring => (*span, vec![*span]),
        ForBinding::Destructuring {
            left_paren_span,
            names,
            right_paren_span: Some(right),
        } if case.destructuring => {
            if sources.slice(*left_paren_span) != Ok("(") || sources.slice(*right) != Ok(")") {
                return Err(blocked(
                    "AST fixture",
                    "binding parentheses must be real tokens",
                ));
            }
            let span = sources
                .span(
                    fixture.file.source_id(),
                    left_paren_span.start(),
                    right.end(),
                )
                .map_err(|error| blocked("AST fixture", error))?;
            let markers = names
                .iter()
                .map(|marker| match marker {
                    NameMarker::Present(span) => Ok(*span),
                    other => Err(blocked("AST fixture", other)),
                })
                .collect::<Result<Vec<_>, _>>()?;
            (span, markers)
        }
        other => return Err(blocked("AST fixture", other)),
    };
    if sources.slice(binding_span) != Ok(case.pattern)
        || markers.len() != case.markers.len()
        || !markers
            .iter()
            .zip(case.markers)
            .all(|(span, expected)| sources.slice(*span) == Ok(*expected))
    {
        return Err(blocked(
            "AST fixture",
            (binding_span, &markers, case.pattern, case.markers),
        ));
    }
    let body = ast
        .statements()
        .get(*body)
        .map_err(|error| blocked("AST fixture", error))?;
    if !matches!(body.payload(), Statement::Block { elements } if elements.is_empty()) {
        return Err(blocked("AST fixture", "expected empty ordinary body"));
    }
    let source_node = ast
        .expressions()
        .get(*source)
        .map_err(|error| blocked("AST fixture", error))?;
    if !matches!(source_node.payload(), Expression::Name)
        || sources.slice(source_node.span()) != Ok("m")
    {
        return Err(blocked(
            "AST fixture",
            "source must be the real name expression m",
        ));
    }
    let classifiers = ast
        .items()
        .iter()
        .filter_map(|(id, node)| match node.payload() {
            Item::Classifier(classifier) => Some((id, classifier)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let nominal = if case.constructor.is_some() {
        if !classifiers.is_empty() {
            return Err(blocked(
                "AST fixture",
                "intrinsic fixtures have no source classifier",
            ));
        }
        None
    } else {
        let [(item, classifier)] = classifiers.as_slice() else {
            return Err(blocked(
                "AST fixture",
                "expected one nominal Map classifier",
            ));
        };
        let NameMarker::Present(name) = classifier.name else {
            return Err(blocked("AST fixture", classifier.name));
        };
        if !matches!(classifier.kind, ClassifierKind::Class { .. })
            || sources.slice(name) != Ok("Map")
        {
            return Err(blocked("AST fixture", "expected ordinary class Map"));
        }
        Some((*item, name))
    };
    Ok(Sites {
        statement: *statement,
        source: *source,
        source_span: source_node.span(),
        binding_span,
        markers,
        nominal,
    })
}

fn real_symbol(
    names: &NameResolution,
    span: Span,
    namespace: Namespace,
    kind: SymbolKind,
) -> Option<&Symbol> {
    let found = names
        .symbols()
        .iter()
        .filter(|symbol| symbol.span() == span)
        .collect::<Vec<_>>();
    match found.as_slice() {
        [symbol]
            if symbol.namespace() == namespace
                && symbol.kind() == kind
                && !symbol.is_synthetic() =>
        {
            Some(symbol)
        }
        _ => None,
    }
}

fn bindings(names: &NameResolution, sites: &Sites, case: &Case) -> (bool, Vec<String>) {
    let mut valid = true;
    let mut report = Vec::new();
    for (span, expected) in sites.markers.iter().zip(case.markers) {
        let symbols = names
            .symbols()
            .iter()
            .filter(|symbol| symbol.span() == *span)
            .collect::<Vec<_>>();
        let matches = if *expected == "_" {
            symbols.is_empty()
        } else {
            real_symbol(names, *span, Namespace::Value, SymbolKind::ForBinding)
                .is_some_and(|symbol| symbol.name() == *expected)
        };
        valid &= matches;
        report.push(format!(
            "marker={expected:?} span={span:?} names={symbols:?} matches={matches}"
        ));
    }
    (valid, report)
}

#[derive(Debug)]
pub(super) struct Facts {
    pub(super) source_matches: bool,
    pub(super) bindings_match: bool,
    pub(super) plans_absent: bool,
    pub(super) details: Vec<String>,
}

pub(super) fn facts(observed: &Observed, sites: &Sites, case: &Case) -> Result<Facts, Blocked> {
    let analysis = &observed.analysis;

    let names = analysis.names.names();
    let [source_names] = names.source_units() else {
        return Err(blocked(
            "unit identity",
            "expected one source names product",
        ));
    };
    let [source_unit] = names.index().source_units() else {
        return Err(blocked("unit identity", "expected one indexed source unit"));
    };
    let unit = source_names.source_unit();
    if source_unit.id() != unit
        || source_unit.source_id() != observed.fixture.file.source_id()
        || !observed.entry.matches_key(source_unit.key())
    {
        return Err(blocked("unit identity", source_unit));
    }
    let raw = &analysis.raw;
    let statement = UnitStatementId::new(unit, sites.statement);
    let kind = raw
        .expression_type(UnitExpressionId::new(unit, sites.source))
        .and_then(|ty| raw.types().get(ty));
    let (bindings_match, mut details) = bindings(source_names.resolution(), sites, case);
    let source_matches = match (case.constructor, kind) {
        (
            Some(expected),
            Some(UnitTypeKind::Intrinsic {
                constructor,
                arguments,
            }),
        ) => {
            *constructor == expected
                && arguments.len() == 2
                && raw.types().get(arguments[0]) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                && raw.types().get(arguments[1])
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
        }
        (
            None,
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }),
        ) => {
            let indexed = names
                .index()
                .declarations()
                .iter()
                .find(|item| item.id() == *declaration);
            let signature = raw
                .signatures()
                .declaration(*declaration)
                .and_then(|item| item.nominal());
            details.push(format!(
                "nominal_declaration={indexed:?} nominal_signature={signature:?}"
            ));
            arguments.is_empty()
                && sites.nominal.is_some_and(|(root, span)| {
                    let symbol = real_symbol(
                        source_names.resolution(),
                        span,
                        Namespace::Type,
                        SymbolKind::Classifier,
                    );
                    indexed.is_some_and(|item| {
                        item.root() == root
                            && item.name_span() == span
                            && item.source_unit() == unit
                            && item.namespace() == Namespace::Type
                            && item.kind() == SymbolKind::Classifier
                    }) && signature.is_some_and(|item| {
                        item.declaration() == *declaration
                            && item.kind() == NominalKind::Class
                            && item.symbol().source_unit() == unit
                            && symbol.is_some_and(|local| item.symbol().symbol() == local.id())
                            && names.declaration_symbol(*declaration) == Some(item.symbol())
                    })
                })
        }
        _ => false,
    };
    let plans_absent = raw.map_iterations().is_empty()
        && raw.sequential_iterations().is_empty()
        && raw.map_iteration(statement).is_none()
        && raw.sequential_iteration(statement).is_none();
    details.push(format!("source_unit={source_unit:?} source_kind={kind:?}"));
    if let Some(UnitTypeKind::Intrinsic { arguments, .. }) = kind {
        details.push(format!(
            "source_KV={:?}",
            arguments
                .iter()
                .map(|ty| raw.types().get(*ty))
                .collect::<Vec<_>>()
        ));
    }
    details.push(format!(
        "map_plans={:?} sequential_plans={:?} map_lookup={:?} sequential_lookup={:?}",
        raw.map_iterations(),
        raw.sequential_iterations(),
        raw.map_iteration(statement),
        raw.sequential_iteration(statement)
    ));
    Ok(Facts {
        source_matches,
        bindings_match,
        plans_absent,
        details,
    })
}
