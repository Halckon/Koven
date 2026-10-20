//! 检查原分析的 provider、Error binding 和真实 body facts；不创建任何 unit symbol。

use lang_frontend::{
    name_resolution::{
        NameResolution, Namespace, ReferenceTarget, SourceUnitId, Symbol, SymbolId, SymbolKind,
        UnitSymbolId,
    },
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypes, DeferredReason, IntrinsicTypeConstructor, NominalKind,
        TypeKind, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

use super::{
    Case,
    analysis::{Analysis, Blocked, Observed, blocked},
    sites::{Body, Sites},
};

pub(super) struct Facts {
    pub(super) valid: bool,
    pub(super) details: Vec<String>,
}

impl Facts {
    fn check(&mut self, label: &str, matches: bool, actual: impl std::fmt::Debug) {
        self.valid &= matches;
        self.details
            .push(format!("{label}: matches={matches} actual={actual:#?}"));
    }
}

fn symbol(names: &NameResolution, span: Span, kind: SymbolKind) -> Result<&Symbol, Blocked> {
    let found = names
        .symbols()
        .iter()
        .filter(|symbol| symbol.span() == span)
        .collect::<Vec<_>>();
    let [symbol] = found.as_slice() else {
        return Err(blocked("names identity", (span, found)));
    };
    let namespace = if kind == SymbolKind::Classifier {
        Namespace::Type
    } else {
        Namespace::Value
    };
    if symbol.kind() != kind || symbol.namespace() != namespace || symbol.is_synthetic() {
        return Err(blocked("names identity", symbol));
    }
    Ok(symbol)
}

fn refers_to(names: &NameResolution, span: Span, symbol: SymbolId) -> bool {
    let references = names
        .references()
        .iter()
        .filter(|item| item.span() == span && item.namespace() == Namespace::Value)
        .collect::<Vec<_>>();
    matches!(references.as_slice(), [reference] if reference.target() == &ReferenceTarget::Symbol(symbol))
}

fn unit_symbol(
    raw: &CompilationUnitTypes,
    unit: SourceUnitId,
    local: SymbolId,
) -> Result<UnitSymbolId, Blocked> {
    let found = raw
        .body_symbol_types()
        .keys()
        .copied()
        .filter(|symbol| symbol.source_unit() == unit && symbol.symbol() == local)
        .collect::<Vec<_>>();
    match found.as_slice() {
        [symbol] => Ok(*symbol),
        _ => Err(blocked("unit body symbol identity", found)),
    }
}

pub(super) fn facts(observed: &Observed, sites: &Sites, case: Case) -> Result<Facts, Blocked> {
    let mut result = Facts {
        valid: true,
        details: vec![format!("source_span={:?}", sites.source_span)],
    };
    let ast = observed.fixture.file.ast();
    match &observed.analysis {
        Analysis::Single(analysis) => {
            let names = &analysis.names;
            let raw = &analysis.raw;
            let source_kind = raw
                .expression_type(sites.source)
                .and_then(|ty| raw.types().get(ty));
            let source_matches = match (case, source_kind) {
                (
                    Case::SequentialArity,
                    Some(TypeKind::Intrinsic {
                        constructor: IntrinsicTypeConstructor::List,
                        arguments,
                    }),
                ) if arguments.len() == 1 => {
                    if let (Some((_, span)), Some(TypeKind::Nominal { nominal, arguments })) =
                        (sites.nominal, raw.types().get(arguments[0]))
                    {
                        let declared = symbol(names, span, SymbolKind::Classifier)?;
                        arguments.is_empty()
                            && nominal.symbol() == declared.id()
                            && raw.nominals().iter().any(|item| {
                                item.id() == *nominal
                                    && item.kind() == NominalKind::ValueClass
                                    && item.fields().len() == 2
                            })
                    } else {
                        false
                    }
                }
                (
                    _,
                    Some(TypeKind::Intrinsic {
                        constructor: IntrinsicTypeConstructor::Map,
                        arguments,
                    }),
                ) if case != Case::SequentialArity && arguments.len() == 2 => {
                    raw.types().get(arguments[0]) == Some(&TypeKind::Builtin(BuiltinType::Int))
                        && raw.types().get(arguments[1])
                            == Some(&TypeKind::Builtin(BuiltinType::String))
                }
                _ => false,
            };
            result.check("real provider type", source_matches, source_kind);
            for (span, expected) in sites.markers.iter().zip(case.markers()) {
                let binding = symbol(names, *span, SymbolKind::ForBinding)?;
                let kind = raw
                    .symbol_type(binding.id())
                    .and_then(|ty| raw.types().get(ty));
                result.check(
                    "real Error binding",
                    binding.name() == *expected && kind == Some(&TypeKind::Error),
                    (binding, kind),
                );
            }
            let no_plans = raw.map_iterations().is_empty()
                && raw.sequential_iterations().is_empty()
                && raw.map_iteration(sites.statement).is_none()
                && raw.sequential_iteration(sites.statement).is_none();
            result.check(
                "no partial plans",
                no_plans,
                (
                    raw.map_iterations(),
                    raw.sequential_iterations(),
                    raw.map_iteration(sites.statement),
                    raw.sequential_iteration(sites.statement),
                ),
            );
            match sites.body {
                Body::Empty => {}
                Body::Independent {
                    receiver,
                    member,
                    call,
                    number,
                    ..
                } => {
                    let declaration = symbol(names, number, SymbolKind::Variable)?;
                    let span = ast
                        .expressions()
                        .get(receiver)
                        .map_err(|error| blocked("body identity", error))?
                        .span();
                    let receiver_kind = raw
                        .expression_type(receiver)
                        .and_then(|ty| raw.types().get(ty));
                    let member_kind = raw
                        .expression_type(member)
                        .and_then(|ty| raw.types().get(ty));
                    result.check(
                        "body receiver is independent real Int local",
                        refers_to(names, span, declaration.id())
                            && receiver_kind == Some(&TypeKind::Builtin(BuiltinType::Int)),
                        (declaration, receiver_kind),
                    );
                    result.check(
                        "body member is raw deferred with no selected call",
                        member_kind == Some(&TypeKind::Deferred(DeferredReason::MemberAccess))
                            && raw.call(call).is_none(),
                        (member_kind, raw.call(call)),
                    );
                }
                Body::Derived {
                    declaration,
                    initializer,
                } => {
                    let derived = symbol(names, declaration, SymbolKind::Variable)?;
                    let key = symbol(names, sites.markers[0], SymbolKind::ForBinding)?;
                    let span = ast
                        .expressions()
                        .get(initializer)
                        .map_err(|error| blocked("body identity", error))?
                        .span();
                    let expression = raw
                        .expression_type(initializer)
                        .and_then(|ty| raw.types().get(ty));
                    let binding = raw
                        .symbol_type(derived.id())
                        .and_then(|ty| raw.types().get(ty));
                    result.check(
                        "real key use and derived remain Error",
                        refers_to(names, span, key.id())
                            && expression == Some(&TypeKind::Error)
                            && binding == Some(&TypeKind::Error),
                        (key, derived, expression, binding),
                    );
                }
            }
        }
        Analysis::Unit(analysis) => {
            let all_names = analysis.names.names();
            let [source_names] = all_names.source_units() else {
                return Err(blocked(
                    "unit identity",
                    "expected one source names product",
                ));
            };
            let [source_unit] = all_names.index().source_units() else {
                return Err(blocked("unit identity", "expected one indexed source"));
            };
            let unit = source_names.source_unit();
            if source_unit.id() != unit
                || source_unit.source_id() != observed.fixture.file.source_id()
                || source_unit
                    .key()
                    .logical_path()
                    .expect("rooted source")
                    .as_str()
                    != "iteration.ko"
            {
                return Err(blocked("unit identity", source_unit));
            }
            let names = source_names.resolution();
            let raw = &analysis.raw;
            let source_kind = raw
                .expression_type(UnitExpressionId::new(unit, sites.source))
                .and_then(|ty| raw.types().get(ty));
            let source_matches = match (case, source_kind) {
                (
                    Case::SequentialArity,
                    Some(UnitTypeKind::Intrinsic {
                        constructor: IntrinsicTypeConstructor::List,
                        arguments,
                    }),
                ) if arguments.len() == 1 => {
                    if let (
                        Some((root, span)),
                        Some(UnitTypeKind::Nominal {
                            declaration,
                            arguments,
                        }),
                    ) = (sites.nominal, raw.types().get(arguments[0]))
                    {
                        let declared = symbol(names, span, SymbolKind::Classifier)?;
                        let indexed = all_names
                            .index()
                            .declarations()
                            .iter()
                            .find(|item| item.id() == *declaration);
                        let nominal = raw
                            .signatures()
                            .declaration(*declaration)
                            .and_then(|item| item.nominal());
                        arguments.is_empty()
                            && indexed.is_some_and(|item| {
                                item.root() == root
                                    && item.name_span() == span
                                    && item.source_unit() == unit
                                    && item.kind() == SymbolKind::Classifier
                            })
                            && nominal.is_some_and(|item| {
                                item.kind() == NominalKind::ValueClass
                                    && item.fields().len() == 2
                                    && item.symbol().source_unit() == unit
                                    && item.symbol().symbol() == declared.id()
                                    && all_names.declaration_symbol(*declaration)
                                        == Some(item.symbol())
                            })
                    } else {
                        false
                    }
                }
                (
                    _,
                    Some(UnitTypeKind::Intrinsic {
                        constructor: IntrinsicTypeConstructor::Map,
                        arguments,
                    }),
                ) if case != Case::SequentialArity && arguments.len() == 2 => {
                    raw.types().get(arguments[0]) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                        && raw.types().get(arguments[1])
                            == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                }
                _ => false,
            };
            result.check("real unit provider type", source_matches, source_kind);
            for (span, expected) in sites.markers.iter().zip(case.markers()) {
                let local = symbol(names, *span, SymbolKind::ForBinding)?;
                let binding = unit_symbol(raw, unit, local.id())?;
                let kind = raw.symbol_type(binding).and_then(|ty| raw.types().get(ty));
                result.check(
                    "real unit Error binding",
                    local.name() == *expected && kind == Some(&UnitTypeKind::Error),
                    (local, binding, kind),
                );
            }
            let statement = UnitStatementId::new(unit, sites.statement);
            let no_plans = raw.map_iterations().is_empty()
                && raw.sequential_iterations().is_empty()
                && raw.map_iteration(statement).is_none()
                && raw.sequential_iteration(statement).is_none();
            result.check(
                "no unit partial plans",
                no_plans,
                (
                    raw.map_iterations(),
                    raw.sequential_iterations(),
                    raw.map_iteration(statement),
                    raw.sequential_iteration(statement),
                ),
            );
            match sites.body {
                Body::Empty => {}
                Body::Independent {
                    receiver,
                    member,
                    call,
                    number,
                    ..
                } => {
                    let declaration = symbol(names, number, SymbolKind::Variable)?;
                    let span = ast
                        .expressions()
                        .get(receiver)
                        .map_err(|error| blocked("body identity", error))?
                        .span();
                    let receiver_kind = raw
                        .expression_type(UnitExpressionId::new(unit, receiver))
                        .and_then(|ty| raw.types().get(ty));
                    let member_kind = raw
                        .expression_type(UnitExpressionId::new(unit, member))
                        .and_then(|ty| raw.types().get(ty));
                    let call = raw.call(UnitExpressionId::new(unit, call));
                    result.check(
                        "unit body receiver is independent real Int local",
                        refers_to(names, span, declaration.id())
                            && receiver_kind == Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
                        (declaration, receiver_kind),
                    );
                    result.check(
                        "unit body member is raw deferred with no selected call",
                        member_kind == Some(&UnitTypeKind::Deferred(DeferredReason::MemberAccess))
                            && call.is_none(),
                        (member_kind, call),
                    );
                }
                Body::Derived {
                    declaration,
                    initializer,
                } => {
                    let derived = symbol(names, declaration, SymbolKind::Variable)?;
                    let derived_unit = unit_symbol(raw, unit, derived.id())?;
                    let key = symbol(names, sites.markers[0], SymbolKind::ForBinding)?;
                    let span = ast
                        .expressions()
                        .get(initializer)
                        .map_err(|error| blocked("body identity", error))?
                        .span();
                    let expression = raw
                        .expression_type(UnitExpressionId::new(unit, initializer))
                        .and_then(|ty| raw.types().get(ty));
                    let binding = raw
                        .symbol_type(derived_unit)
                        .and_then(|ty| raw.types().get(ty));
                    result.check(
                        "real unit key use and derived remain Error",
                        refers_to(names, span, key.id())
                            && expression == Some(&UnitTypeKind::Error)
                            && binding == Some(&UnitTypeKind::Error),
                        (key, derived, derived_unit, expression, binding),
                    );
                }
            }
        }
    }
    Ok(result)
}
