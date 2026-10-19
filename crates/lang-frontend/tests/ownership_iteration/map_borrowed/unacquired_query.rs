//! 原 D1/D2 四观察的独立只读 query；只证明未发布已 Acquire 的计划/清理。
use super::super::type_facts::{symbol_at, unit_symbol};
use super::{Case, Entry, Observation, Observed, Products, Report, Sites, analyze, sites};
use lang_frontend::{
    ast::StatementId,
    name_resolution::SymbolKind,
    ownership_checking::{
        IterationCleanupAction as Action, UnitDropPoint, validate_iteration_facts,
    },
    parser::Statement,
    type_checking::{
        BuiltinType, IntrinsicTypeConstructor, MapIterationKind, TypeKind, UnitExpressionId,
        UnitStatementId, UnitTypeKind,
    },
};

type QueryResult<T = ()> = Result<T, &'static str>;

fn require(passed: bool, reason: &'static str) -> QueryResult {
    if passed { Ok(()) } else { Err(reason) }
}

fn map_kind(case: Case) -> (MapIterationKind, IntrinsicTypeConstructor) {
    match case {
        Case::SourceReturn => (MapIterationKind::Map, IntrinsicTypeConstructor::Map),
        Case::DeadTail => (
            MapIterationKind::MutableMap,
            IntrinsicTypeConstructor::MutableMap,
        ),
    }
}

fn recorded_success(observed: &Observed, name: &'static str) -> bool {
    let matches = observed
        .report
        .checks
        .iter()
        .filter(|(actual, _)| *actual == name)
        .collect::<Vec<_>>();
    matches.len() == 1 && matches[0].1
}

fn for_body(observed: &Observed, sites: &Sites) -> QueryResult<StatementId> {
    let node = observed
        .fixture
        .file
        .ast()
        .statements()
        .get(sites.statement)
        .map_err(|_| "missing original For")?;
    let Statement::For { source, body, .. } = node.payload() else {
        return Err("original statement is not For");
    };
    require(
        *source == sites.source,
        "For has a different source identity",
    )?;
    // 这是完整 For.body Block，绝不拿末尾子句替代 body 的退出点。
    Ok(*body)
}

fn is_current_map_cleanup(action: &Action, statement: StatementId) -> bool {
    match action {
        Action::EndBinding {
            statement: actual, ..
        }
        | Action::EndElement(actual)
        | Action::FinishProvider(actual)
        | Action::EndSource(actual) => *actual == statement,
        _ => false,
    }
}

fn query(observation: &Observation, report: &mut Report) -> QueryResult {
    let observed = observation
        .result
        .as_ref()
        .map_err(|_| "original observation is BLOCKED; no product fabricated")?;
    let (kind, constructor) = map_kind(observation.case);
    // 先保留完整公开表和诊断，再执行独立 query；错误仍交由外层四观察汇总。
    match &observed.products {
        Products::Single(product) => {
            let typed = &product.raw;
            report.note("actual typed Map descriptors", typed.map_iterations());
            report.note("actual raw diagnostics", typed.diagnostics());
            report.note("actual complete result", &product.complete);
            report.note("original checks", &observed.report.checks);
            let owned = product
                .owned
                .as_ref()
                .map_err(|_| "original ownership infrastructure failed")?
                .as_ref()
                .ok_or("original ownership was not reached")?;
            report.note("actual ownership diagnostics", owned.diagnostics());
            report.note("actual ownership deferred", owned.deferred());
            report.note("actual Map ownership plans", owned.map_iterations());
            report.note("actual sequential ownership plans", owned.iterations());
            report.note("actual complete cleanup steps", owned.cleanup_steps());
            report.note("actual formed owned locals", owned.formed_owned_locals());
            report.note("actual effective drops", owned.drops());
            require(
                typed.diagnostics().is_empty()
                    && matches!(product.complete, Ok(true))
                    && owned.diagnostics().is_empty()
                    && owned.deferred().is_empty()
                    && typed.is_compatible_with_names(&product.names)
                    && owned.is_compatible_with(&product.names, typed),
                "single query requires original clean compatible Phase 2/3 products",
            )?;
            let sites = sites(&observed.fixture, observation.case)
                .map_err(|_| "retained AST source-return/dead-tail shape changed")?;
            report.note(
                "actual For/source/return",
                (sites.statement, sites.source, sites.returned),
            );
            let [descriptor] = typed.map_iterations() else {
                return Err("typed table must contain exactly the real Map For");
            };
            require(
                descriptor.statement() == sites.statement
                    && typed.map_iteration(sites.statement) == Some(descriptor)
                    && typed.sequential_iteration(sites.statement).is_none()
                    && descriptor.source() == sites.source
                    && typed.expression_type(sites.source) == Some(descriptor.source_type())
                    && descriptor.kind() == kind,
                "typed Map descriptor lost actual For/source/type/kind identity",
            )?;
            let Some(TypeKind::Intrinsic {
                constructor: actual,
                arguments,
            }) = typed.types().get(descriptor.source_type())
            else {
                return Err("actual Map source type is not intrinsic");
            };
            let [key_type, value_type] = arguments.as_slice() else {
                return Err("actual intrinsic Map must have exactly K and V");
            };
            require(
                *actual == constructor
                    && descriptor.key_type() == *key_type
                    && descriptor.value_type() == *value_type,
                "descriptor K/V differ from the original intrinsic type arguments",
            )?;
            let bindings = descriptor.bindings();
            require(
                bindings[0] != bindings[1]
                    && descriptor.key_binding() == bindings[0]
                    && descriptor.value_binding() == bindings[1],
                "typed key/value getters must name two distinct symbols in source order",
            )?;
            for (index, expected_type) in [*key_type, *value_type].into_iter().enumerate() {
                let symbol = symbol_at(&product.names, sites.bindings[index])
                    .ok_or("missing unique original declaration symbol")?;
                require(
                    symbol.kind() == SymbolKind::ForBinding
                        && bindings[index] == symbol.id()
                        && typed.symbol_type(symbol.id()) == Some(expected_type)
                        && typed.types().get(expected_type)
                            == Some(&TypeKind::Builtin(BuiltinType::Int)),
                    "key/value declaration symbol or precise Int component type changed",
                )?;
            }
            let validation =
                validate_iteration_facts(&observed.fixture.file, &product.names, typed, owned);
            report.record(
                "single complete public iteration validator",
                validation.is_ok(),
                &validation,
            );
            validation.map_err(|_| "public iteration validator rejected original ownership")?;
            require(
                owned.map_iteration(sites.statement).is_none() && owned.map_iterations().is_empty(),
                "unacquired For must not publish a Map ownership plan",
            )?;
            require(
                owned.iteration(sites.statement).is_none() && owned.iterations().is_empty(),
                "unacquired Map must not publish a substitute sequential plan",
            )?;
            require(
                owned
                    .cleanup_steps()
                    .iter()
                    .all(|(_, action)| !is_current_map_cleanup(action, sites.statement)),
                "unacquired For published EndBinding/EndElement/FinishProvider/EndSource",
            )?;
            require(
                owned.formed_owned_locals().is_empty() && owned.drops().is_empty(),
                "original empty formed-local/effective-drop contract changed",
            )?;
        }
        Products::Unit(product) => {
            let typed = &product.raw;
            report.note("actual typed Map descriptors", typed.map_iterations());
            report.note("actual raw diagnostics", typed.diagnostics());
            report.note("actual complete result", &product.complete);
            report.note(
                "original checks including public unit validation",
                &observed.report.checks,
            );
            let owned = product
                .owned
                .as_ref()
                .map_err(|_| "original ownership infrastructure failed")?
                .as_ref()
                .ok_or("original ownership was not reached")?;
            report.note("actual ownership diagnostics", owned.diagnostics());
            report.note("actual ownership deferred", owned.deferred());
            report.note("actual Map ownership plans", owned.map_iterations());
            report.note("actual sequential ownership plans", owned.iterations());
            report.note(
                "global typed binding registrations (not acquisition)",
                owned.bindings(),
            );
            report.note("actual formed owned locals", owned.formed_owned_locals());
            report.note("actual effective drops", owned.drops());
            let cleanup = owned
                .iterations()
                .iter()
                .flat_map(|plan| plan.exits())
                .chain(owned.map_iterations().iter().flat_map(|plan| plan.exits()))
                .map(|exit| (exit.point(), owned.iteration_cleanup_at(exit.point())))
                .collect::<Vec<_>>();
            report.note("all publicly queryable unit iteration cleanup", cleanup);
            // 原 analyze 已对同一 owned 执行 consume/validate/into_ownership。
            // 只读核真实成功记录，不克隆或制造 validation wrapper。
            require(
                typed.diagnostics().is_empty()
                    && matches!(product.complete, Ok(true))
                    && owned.diagnostics().is_empty()
                    && owned.deferred().is_empty()
                    && recorded_success(observed, "ownership reached/identity")
                    && recorded_success(observed, "ownership basic qualification"),
                "unit query requires original clean Phase 2/3 products and successful public validation",
            )?;
            let sites = sites(&observed.fixture, observation.case)
                .map_err(|_| "retained AST source-return/dead-tail shape changed")?;
            let [source_names] = product.names.names().source_units() else {
                return Err("fixture must retain exactly one actual source unit");
            };
            require(
                source_names.resolution().source_id() == observed.fixture.file.source_id(),
                "unit names are not for the original source",
            )?;
            let unit = source_names.source_unit();
            let statement = UnitStatementId::new(unit, sites.statement);
            let source = UnitExpressionId::new(unit, sites.source);
            report.note(
                "actual qualified For/source/return",
                (
                    statement,
                    source,
                    UnitExpressionId::new(unit, sites.returned),
                ),
            );
            let [descriptor] = typed.map_iterations() else {
                return Err("unit typed table must contain exactly the real Map For");
            };
            require(
                descriptor.statement() == statement
                    && typed.map_iteration(statement) == Some(descriptor)
                    && typed.sequential_iteration(statement).is_none()
                    && descriptor.source() == source
                    && descriptor.statement().source_unit() == unit
                    && descriptor.source().source_unit() == unit
                    && typed.expression_type(source) == Some(descriptor.source_type())
                    && descriptor.kind() == kind,
                "unit descriptor lost actual qualified For/source/type/kind identity",
            )?;
            let Some(UnitTypeKind::Intrinsic {
                constructor: actual,
                arguments,
            }) = typed.types().get(descriptor.source_type())
            else {
                return Err("actual unit Map source type is not intrinsic");
            };
            let [key_type, value_type] = arguments.as_slice() else {
                return Err("unit intrinsic Map must have exactly K and V");
            };
            require(
                *actual == constructor
                    && descriptor.key_type() == *key_type
                    && descriptor.value_type() == *value_type,
                "unit descriptor K/V differ from the original intrinsic type arguments",
            )?;
            let bindings = descriptor.bindings();
            require(
                bindings[0] != bindings[1]
                    && descriptor.key_binding() == bindings[0]
                    && descriptor.value_binding() == bindings[1],
                "unit key/value getters must name two distinct symbols in source order",
            )?;
            for (index, expected_type) in [*key_type, *value_type].into_iter().enumerate() {
                let local = symbol_at(source_names.resolution(), sites.bindings[index])
                    .ok_or("missing unique original local declaration symbol")?;
                let symbol = unit_symbol(typed, unit, local.id())
                    .ok_or("missing original source-qualified binding")?;
                require(
                    local.kind() == SymbolKind::ForBinding
                        && bindings[index] == symbol
                        && symbol.source_unit() == unit
                        && symbol.symbol() == local.id()
                        && typed.symbol_type(symbol) == Some(expected_type)
                        && typed.types().get(expected_type)
                            == Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
                    "unit key/value symbol or precise Int component type changed",
                )?;
            }
            require(
                owned.map_iteration(statement).is_none() && owned.map_iterations().is_empty(),
                "unacquired For must not publish a unit Map ownership plan",
            )?;
            require(
                owned.iteration(statement).is_none() && owned.iterations().is_empty(),
                "unacquired Map must not publish a substitute unit sequential plan",
            )?;
            // getter 仅从两类计划的 exits 读取。这是空表的一致性旁证，不伪造出口。
            for point in [
                UnitDropPoint::ControlTransfer(UnitExpressionId::new(unit, sites.returned)),
                UnitDropPoint::LoopExit(statement),
                UnitDropPoint::AfterStatement(UnitStatementId::new(
                    unit,
                    for_body(observed, &sites)?,
                )),
            ] {
                let cleanup = owned.iteration_cleanup_at(point);
                report.note(
                    "actual AST point has no published iteration cleanup",
                    (point, cleanup),
                );
                require(
                    cleanup.is_none(),
                    "unacquired For has publicly queryable iteration cleanup",
                )?;
            }
            require(
                owned.formed_owned_locals().is_empty() && owned.drops().is_empty(),
                "original unit empty formed-local/effective-drop contract changed",
            )?;
        }
    }
    Ok(())
}

#[test]
fn public_unacquired_map_facts_preserve_typed_identity_without_owned_plans() {
    let observations = [Case::SourceReturn, Case::DeadTail]
        .into_iter()
        .flat_map(|case| {
            [Entry::Single, Entry::Unit].map(|entry| Observation {
                case,
                entry,
                result: analyze(case, entry),
            })
        })
        .collect::<Vec<_>>();
    // 保活全部原产物，原 assess/旧 exact 不接收本 query 结果。
    let original = observations
        .iter()
        .map(Observation::assess)
        .collect::<Vec<_>>();
    let queries = observations
        .iter()
        .map(|observation| {
            let mut report = Report::default();
            let result = query(observation, &mut report);
            report.record(
                "independent public unacquired Map query",
                result.is_ok(),
                result,
            );
            (
                report.passed(),
                format!(
                    "{:?}/{:?}: PUBLIC_UNACQUIRED_MAP_QUERY\nCHECKS {:#?}\nDETAILS {}",
                    observation.case,
                    observation.entry,
                    report.checks,
                    report.details.join("\n")
                ),
            )
        })
        .collect::<Vec<_>>();
    for ((_, original), (_, query)) in original.iter().zip(&queries) {
        println!("{original}\n{query}");
    }
    assert_eq!(observations.len(), 4);
    assert_eq!(original.len(), 4);
    assert_eq!(queries.len(), 4);
    assert!(
        original.iter().all(|(passed, _)| *passed) && queries.iter().all(|(passed, _)| *passed),
        "SPEC-0331 D1/D2 public query failed after all four original and query observations were retained. Typed Map remains present; no claim about private acquisition templates or backend support.\n{}",
        original
            .iter()
            .zip(&queries)
            .map(|((_, old), (_, new))| format!("{old}\n{new}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
