//! temporary Call source continue/break/return 的独立首证 collector。
//! 十二次 fresh 同源 pipeline 全部观察后统一硬成功；不执行 native，不预言复杂 cleanup 拓扑。
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast;
pub(in crate::ssa) mod calls_unit;
mod guards {
    //! 两个普通 Resource local 的真实形成/构造/deinit/Drop 来源。
    //! formed 是历史，不能代替当前可用性；不对未观察的 owner/phi/condition/Drop 数量做预言。
    use super::{
        super::Report,
        ast::{Anchors, Input},
        unit as unit_facts,
    };
    use crate::test_support::diagnostics::diagnostic;
    use lang_frontend::{
        name_resolution::{Namespace, SymbolKind, ValidatedCompilationUnitNames},
        ownership_checking::{CompilationUnitOwnership, UnitDropTarget},
        type_checking::{
            CompilationUnitTypes, ExpressionCategory, NominalKind, ParameterMode,
            UnitConstructionTarget, UnitExpressionId, UnitItemId, UnitStatementId, UnitTypeKind,
        },
    };

    pub(super) fn unit(
        f: &Input,
        a: &Anchors,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        report: &mut Report,
    ) {
        let [source] = names.names().source_units() else {
            report.check(
                "unit temporary exits Guard unique source",
                false,
                names.names().source_units(),
            );
            return;
        };
        let source_unit = source.source_unit();
        let mut local_ids = Vec::new();
        for guard in &a.guards {
            // Guard 无普通 Name 读取；从公开形成历史取真实 qualified ID，再回核 AST marker。
            // 不从没有该 local 的全局 binding 表查询能力，也不构造私有 UnitSymbolId。
            let locals = owned
                .formed_owned_locals()
                .iter()
                .copied()
                .filter(|id| {
                    id.source_unit() == source_unit
                        && unit_facts::binding_at(names, *id, guard.marker, SymbolKind::Variable)
                })
                .collect::<Vec<_>>();
            let local = match locals.as_slice() {
                [id] => Some(*id),
                _ => None,
            };
            let classes = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| {
                    d.source_unit() == source_unit
                        && d.root() == guard.class_item
                        && d.name_span() == guard.class_marker
                        && d.kind() == SymbolKind::Classifier
                        && d.namespace() == Namespace::Type
                })
                .collect::<Vec<_>>();
            let class = match classes.as_slice() {
                [d] => Some(d.id()),
                _ => None,
            };
            let nominals = typed
                .signatures()
                .declarations()
                .iter()
                .filter(|d| Some(d.declaration()) == class)
                .filter_map(|d| d.nominal())
                .collect::<Vec<_>>();
            let expression = UnitExpressionId::new(source_unit, guard.initializer);
            let construction = typed.construction(expression);
            report.check("unit temporary exits qualified Guard Resource constructor/deinit identity",
                local.is_some() && class.is_some_and(|id| super::calls_unit::callee(f, names, guard.initializer, id, Namespace::Type))
                    && matches!(nominals.as_slice(), [nominal]
                    if nominal.kind() == NominalKind::Class && nominal.fields().is_empty()
                        && Some(nominal.declaration()) == class
                        && nominal.deinit().is_some_and(|deinit| Some(deinit.owner()) == class
                            && deinit.item() == UnitItemId::new(source_unit, guard.deinit_item)
                            && deinit.body() == UnitStatementId::new(source_unit, guard.deinit_body)
                            && deinit.receiver_mode() == ParameterMode::Borrow))
                    && construction.is_some_and(|c| c.expression() == expression
                        && c.arguments().is_empty() && c.instance().type_arguments().is_empty()
                        && matches!(c.target(), UnitConstructionTarget::Nominal(id) if Some(id) == class)
                        && matches!(typed.types().get(c.result_type()), Some(UnitTypeKind::Nominal { declaration, arguments })
                            if Some(*declaration) == class && arguments.is_empty())
                        && Some(c.result_type()) == typed.expression_type(expression)
                        && typed.expression_category(expression) == Some(ExpressionCategory::Temporary)
                        && local.is_some_and(|id| typed.symbol_type(id) == Some(c.result_type()))
                        && typed.is_resource_type(c.result_type()) == Some(true)),
                (guard, &classes, &locals, construction, &nominals));
            let drops = owned
                .drops()
                .iter()
                .filter(|drop| matches!(drop.target(), UnitDropTarget::Named(id) if Some(id) == local))
                .collect::<Vec<_>>();
            report.check(
                "unit temporary exits Guard qualified formation history and actual Drop origins",
                local.is_some()
                    && !drops.is_empty()
                    && drops.iter().all(|drop| drop.value_origin() == guard.marker),
                (guard.marker, local, &drops, owned.formed_owned_locals()),
            );
            if let Some(local) = local {
                local_ids.push(local);
            }
            // Unit 不发布 single owner/condition 表；原 drop 与全 plan/cleanup 原样保全。
            diagnostic!(
                "SPEC0331 TEMPORARY EXITS UNIT GUARD marker={:?} initializer={expression:?} construction_span={:?} deinit_span={:?} drops={drops:#?}",
                guard.marker,
                guard.span,
                guard.deinit_span
            );
        }
        report.check("unit temporary exits two distinct qualified Guard locals share the real class/deinit",
            matches!(local_ids.as_slice(), [left, right] if left != right && left.source_unit() == source_unit && right.source_unit() == source_unit)
                && a.guards[0].class_item == a.guards[1].class_item
                && a.guards[0].deinit_item == a.guards[1].deinit_item,
            (&local_ids, &a.guards));
    }
}
mod unit {
    //! 同次 unit 分析逐完整 qualified For/source/binding 身份查询；不读取私有 acquired 表。
    use super::{
        super::Report,
        ast::{Anchors, Function, Input},
    };
    use crate::test_support::diagnostics::diagnostic;
    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::{
            DeclarationId, SourceUnitId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
            ValidatedCompilationUnitNames,
        },
        ownership_checking::{
            CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
            OwnershipBindingKind, UnitDropTarget, UnitIterationSourceAccess, UnitLoanTarget,
        },
        source::Span,
        type_checking::{
            BuiltinType, CompilationUnitTypes, ExpressionCategory, IntrinsicTypeConstructor,
            MapIterationKind, UnitExpressionId, UnitStatementId, UnitTypeKind,
        },
    };

    pub(super) fn declaration(
        names: &ValidatedCompilationUnitNames,
        function: &Function,
        unit: SourceUnitId,
    ) -> Option<DeclarationId> {
        let found = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| {
                d.source_unit() == unit
                    && d.root() == function.item
                    && d.name_span() == function.name_span
                    && d.kind() == SymbolKind::Function
                    && d.name() == function.name
            })
            .collect::<Vec<_>>();
        match found.as_slice() {
            [d] => Some(d.id()),
            _ => None,
        }
    }

    pub(super) fn reference(
        f: &Input,
        names: &ValidatedCompilationUnitNames,
        expression: ExpressionId,
    ) -> Option<UnitSymbolId> {
        let (_, span) = f.expression(expression).ok()?;
        let found = names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span)
            .collect::<Vec<_>>();
        match found.as_slice() {
            [r] => match r.target() {
                UnitReferenceTarget::Symbol(id) => Some(*id),
                _ => None,
            },
            _ => None,
        }
    }

    pub(super) fn symbol(
        names: &ValidatedCompilationUnitNames,
        unit: SourceUnitId,
        span: Span,
    ) -> Option<UnitSymbolId> {
        // UnitSymbolId 构造器是 frontend 私有；只取本次真实引用中的 qualified ID，再核声明 marker。
        let mut found = Vec::new();
        for reference in names.names().references() {
            let UnitReferenceTarget::Symbol(id) = reference.target() else {
                continue;
            };
            let actual = id.source_unit() == unit
                && names
                    .names()
                    .source_units()
                    .iter()
                    .filter(|source| source.source_unit() == unit)
                    .any(|source| {
                        source.resolution().symbols().iter().any(|symbol| {
                            symbol.id() == id.symbol()
                                && symbol.span() == span
                                && !symbol.is_synthetic()
                        })
                    });
            if actual && !found.contains(id) {
                found.push(*id);
            }
        }
        match found.as_slice() {
            [id] => Some(*id),
            _ => None,
        }
    }

    pub(super) fn binding_at(
        names: &ValidatedCompilationUnitNames,
        id: UnitSymbolId,
        span: Span,
        kind: SymbolKind,
    ) -> bool {
        names
            .names()
            .source_units()
            .iter()
            .filter(|s| s.source_unit() == id.source_unit())
            .any(|source| {
                source.resolution().symbols().iter().any(|s| {
                    s.id() == id.symbol() && s.span() == span && s.kind() == kind && !s.is_synthetic()
                })
            })
    }

    pub(super) fn observe(
        f: &Input,
        anchors: Option<&Anchors>,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        report: &mut Report,
    ) {
        // 先保全实际公开产物，再做任何可失败窄查询；不预言 owner/phi/cleanup 形状。
        diagnostic!(
            "SPEC0331 TEMPORARY EXITS UNIT REAL typed_maps={:#?} maps={:#?} drops={:#?} conditional_drops={:#?} call_argument_contracts={:#?} loans={:#?} value_deliveries={:#?} formed_owned_locals={:#?}",
            typed.map_iterations(),
            owned.map_iterations(),
            owned.drops(),
            owned.conditional_receiver_drops(),
            owned.call_argument_contracts(),
            owned.loans(),
            owned.value_deliveries(),
            owned.formed_owned_locals()
        );
        let mut points = Vec::new();
        for plan in owned.map_iterations() {
            diagnostic!("SPEC0331 TEMPORARY EXITS UNIT PLAN {plan:#?}");
            for exit in plan.exits() {
                if !points.contains(&exit.point()) {
                    points.push(exit.point());
                }
            }
        }
        for point in points {
            let plans = owned
                .map_iterations()
                .iter()
                .filter_map(|plan| {
                    plan.exits()
                        .iter()
                        .any(|exit| exit.point() == point)
                        .then_some(plan.descriptor().statement())
                })
                .collect::<Vec<_>>();
            let whole = owned.iteration_cleanup_at(point);
            diagnostic!(
                "SPEC0331 TEMPORARY EXITS UNIT WHOLE POINT point={point:?} plans={plans:?} sequence={whole:#?}"
            );
        }
        let Some(a) = anchors else { return };
        let [source_names] = names.names().source_units() else {
            report.check(
                "unit temporary exits exactly one actual source",
                false,
                names.names().source_units(),
            );
            return;
        };
        let unit = source_names.source_unit();
        let expression = |id| UnitExpressionId::new(unit, id);
        report.check(
            "unit temporary exits original source and exact two typed/owned plans",
            source_names.resolution().source_id() == f.parsed.source_id()
                && typed.map_iterations().len() == 2
                && owned.map_iterations().len() == 2,
            (unit, typed.map_iterations(), owned.map_iterations()),
        );

        let mut owners = Vec::new();
        let mut bindings = Vec::new();
        for site in &a.sites {
            let statement = UnitStatementId::new(unit, site.statement);
            let source = expression(site.source);
            report.check(
                "unit temporary exits actual qualified AST site",
                site.span.source_id() == f.parsed.source_id()
                    && site.source_span.source_id() == f.parsed.source_id()
                    && f.sources.source_name(site.span.source_id()) == Ok("provider.ko"),
                (
                    statement,
                    source,
                    site.body,
                    super::super::fixture::location(&f.sources, site.source_span),
                ),
            );
            let drops = owned
                .drops()
                .iter()
                .filter(|drop| matches!(drop.target(), UnitDropTarget::Temporary(id) if id == source))
                .collect::<Vec<_>>();
            diagnostic!(
                "SPEC0331 TEMPORARY EXITS UNIT SOURCE statement={statement:?} source={source:?} transfer={:?} drops={drops:#?}",
                expression(site.transfer)
            );
            let (Some(d), Some(p)) = (
                typed.map_iteration(statement),
                owned.map_iteration(statement),
            ) else {
                report.check(
                    "unit temporary exits actual qualified site descriptor/plan",
                    false,
                    (
                        statement,
                        source,
                        typed.map_iterations(),
                        owned.map_iterations(),
                    ),
                );
                continue;
            };
            let constructor = if f.map {
                IntrinsicTypeConstructor::Map
            } else {
                IntrinsicTypeConstructor::MutableMap
            };
            let kind = if f.map {
                MapIterationKind::Map
            } else {
                MapIterationKind::MutableMap
            };
            report.check(
                "unit temporary exits exact qualified For/source and intrinsic String K/V",
                d.statement() == statement && d.source() == source && p.descriptor() == d
                    && typed.expression_type(source) == Some(d.source_type())
                    && typed.sequential_iteration(statement).is_none() && owned.iteration(statement).is_none()
                    && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                        if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
                    && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&UnitTypeKind::Builtin(BuiltinType::String))),
                (statement, source, d, p),
            );
            let owner = match p.source() {
                Some(UnitLoanTarget::Temporary(id)) => Some(*id),
                _ => None,
            };
            report.check(
                "unit temporary exits public category and qualified expression origin",
                typed.expression_category(source) == Some(ExpressionCategory::Temporary)
                    && p.source_access() == UnitIterationSourceAccess::Temporary
                    && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Temporary(id)) if *id == source)
                    && owner == Some(source),
                (statement, source, p.source_access(), p.source_kind()),
            );
            if let Some(owner) = owner {
                owners.push(owner);
            }
            bindings.push(d.bindings());
            report.check(
                "unit temporary exits distinct qualified Key/Value bindings and loans",
                d.key_binding() != d.value_binding() && p.loans()[0] != p.loans()[1],
                (statement, d.bindings(), p.loans()),
            );
            for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
                .into_iter()
                .enumerate()
            {
                let binding = d.bindings()[i];
                let loan = p.loan(component);
                report.check(
                    "unit temporary exits actual marker/type/Shared component lineage",
                    binding.source_unit() == unit
                        && binding_at(names, binding, site.bindings[i], SymbolKind::ForBinding)
                        && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][i])
                        && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                        && p.bindings()[i].symbol() == binding
                        && p.bindings()[i].kind() == OwnershipBindingKind::Shared
                        && loan == &p.loans()[i]
                        && loan.statement() == statement
                        && loan.source() == source
                        && loan.binding() == binding
                        && loan.component() == component
                        && loan.kind() == LoanKind::Shared
                        && Some(loan.origin()) == p.source(),
                    (statement, binding, loan),
                );
            }
        }
        report.check(
            "unit temporary exits two qualified sites retain independent origins and bindings",
            matches!(owners.as_slice(), [first, second] if first != second && first.source_unit() == unit && second.source_unit() == unit)
                && matches!(bindings.as_slice(), [first, second] if first.iter().all(|binding| !second.contains(binding))),
            (owners, bindings),
        );
        super::calls_unit::observe(f, a, names, typed, owned, report);
        super::guards::unit(f, a, names, typed, owned, report);
    }
}

use super::{
    Report,
    fixture::{Failure, failure, verify_failure},
    identity, pipeline,
};
use ast::Exit;

struct Case {
    label: &'static str,
    map: bool,
    exit: Exit,
    source_hash: &'static str,
    golden_hash: &'static str,
    text: &'static str,
    golden: &'static str,
}

fn sources() -> [Case; 6] {
    [
        Case {
            label: "map-temporary-continue",
            map: true,
            exit: Exit::Continue,
            source_hash: "a5071340c9ff411a2b3bbd718845972a792abc0330160f099b229b31b5e17dc2",
            golden_hash: "45604a95bf7d814e739b2f0df7ffddd7b08c19d672bb55b552918e2dd63a767b",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-continue.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-continue.stdout"
            ),
        },
        Case {
            label: "map-temporary-break",
            map: true,
            exit: Exit::Break,
            source_hash: "322e3067889445a2d726c062e46c4c91ab2b964f8093ffd72c038a2ae234c420",
            golden_hash: "bd12c9d7fbcf2d28a05a9979a6e73fd2df5bf7a3f0883cbd9ab4d5a340cbd957",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-break.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-break.stdout"
            ),
        },
        Case {
            label: "map-temporary-return",
            map: true,
            exit: Exit::Return,
            source_hash: "4405c5c5249842eba5a53ed010ebe0232d3409bc374aac60418add5613238212",
            golden_hash: "487425b0aaec53ffb4462fdc3bfac85613eb107f666f57ba6abd5c364cb8fc98",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-return.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-return.stdout"
            ),
        },
        Case {
            label: "mutable-map-temporary-continue",
            map: false,
            exit: Exit::Continue,
            source_hash: "db2355c3c578ab306115f6ac26fdfd9f0bfb5fbf0a5f3680c997143262d147d2",
            golden_hash: "f0e831442c4c142840327c92903e47b86dfdb16d4c303ea9945cbb2276027fc1",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-continue.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-continue.stdout"
            ),
        },
        Case {
            label: "mutable-map-temporary-break",
            map: false,
            exit: Exit::Break,
            source_hash: "aae92d25d99f39637b01de43f418bc3099f94c04e32add62f2caa08dfec94811",
            golden_hash: "543c9ca03eb1954d929ceb4c5617eaebc162be68c2fd53bc14f0697eec99475f",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-break.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-break.stdout"
            ),
        },
        Case {
            label: "mutable-map-temporary-return",
            map: false,
            exit: Exit::Return,
            source_hash: "e90cee83581136e06a39f97035b6f89004c86ea7b56fd47d0d7b73e63083a79b",
            golden_hash: "721e19210c52871e2e609a9fa0e590e76f7a720e6f4fa9798f951722ba5827e2",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-return.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-return.stdout"
            ),
        },
    ]
}

#[derive(Debug)]
struct Observation {
    label: &'static str,
    rooted: bool,
    report: Report,
    outcome: Result<(), Failure>,
}

fn observe(case: &Case, rooted: bool) -> Observation {
    let mut report = Report::default();
    // SHA 是经仓外逐字复核的冻结输入标识；这里未做运行时哈希或 native 求值。
    diagnostic!(
        "SPEC0331 TEMPORARY EXITS INPUT case={} rooted={rooted} path=provider.ko frozen_sha256={} golden_sha256={} golden_bytes={} golden_status=predicted_only text={:?}",
        case.label,
        case.source_hash,
        case.golden_hash,
        case.golden.len(),
        case.text
    );
    let outcome = ast::Input::new(case.text, case.map, case.exit).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query(
            "temporary exits actual two Call-source For sites, Guards, calls and caller identities",
            &anchors,
        );
        let (program, entry) = pipeline::unit_input_observed(
            &input.sources,
            &input.parsed,
            "provider.ko",
            &mut report,
            rooted,
            |names, typed, owned, report| {
                unit::observe(&input, anchors.as_ref().ok(), names, typed, owned, report);
            },
        )?;
        diagnostic!(
            "SPEC0331 TEMPORARY EXITS SSA case={} rooted={rooted}\n{}",
            case.label,
            crate::ssa::render::render_program(&program)
        );
        // 无单层/固定 read 数假设的源码 origin 与真实可达函数身份查询。
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("temporary exits AST identity", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "temporary exits actual entry and reachable source functions/deinits",
            &reachable,
        );
        // 查询失败只记 report；不能遮住真实 verifier/LLVM 的首断点。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("temporary exits SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check("temporary exits LLVM render/verify", true, llvm.len());
        diagnostic!(
            "SPEC0331 TEMPORARY EXITS LLVM case={} rooted={rooted}\n{llvm}",
            case.label
        );
        Ok(())
    });
    Observation {
        label: case.label,
        rooted,
        report,
        outcome,
    }
}

#[test]
fn map_and_mutable_map_temporary_exits_reach_verified_llvm() {
    let mut observations = Vec::new();
    for case in sources() {
        for rooted in [false, true] {
            observations.push(observe(&case, rooted));
        }
    }
    for observation in &observations {
        diagnostic!(
            "SPEC0331 TEMPORARY EXITS OUTCOME case={} rooted={} report={:#?}",
            observation.label,
            observation.rooted,
            observation.report
        );
        match &observation.outcome {
            Ok(()) => diagnostic!("stage=LLVM render/verify result=ok"),
            Err(error) => diagnostic!("stage={} original={}", error.stage, error.original),
        }
    }
    // 不以缺事实或 UnsupportedNode 为成功；不因首红丢掉其余十一格。
    assert_diagnostic!(
        observations.len() == 12
            && observations
                .iter()
                .all(|o| o.report.passed() && o.outcome.is_ok()),
        "十二格、二十四真实 For 必须完成同次 frontend、完整 validator/view、真实 lower、SSA 与 LLVM；\
         query 失败不遮真实阶段，缺事实/UnsupportedNode 不算成功；本首证不宣称全实体/全 edge 闭包或 native。\n{observations:#?}"
    );
}

/// 同字节六源与完整 golden，供后续独立准入的普通 native 测试复用。
/// 保持 Case/Exit 私有，不改变首证 collector 的任何阶段或成功条件。
pub(crate) fn native_sources() -> [(&'static str, &'static str, &'static str); 6] {
    sources().map(|case| (case.label, case.text, case.golden))
}

// Independent deep identity query; the original collector never enters this path.
#[allow(unused_imports)] pub(in crate::ssa) use calls_unit::query_body;
mod query_callers;
mod query_inputs;
mod query_ssa;
mod query_test {
    //! New exact: collect all twelve fresh pipelines before asserting; query Err still verifies LLVM.
    use super::{
        super::{
            Report,
            fixture::{Failure, failure, verify_failure},
            identity, pipeline,
        },
        Case, Observation, ast, query_inputs, query_ssa, sources, unit,
    };
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};

    fn observe(case: &Case, rooted: bool) -> Observation {
        let mut report = Report::default();
        // SHA 是经仓外逐字复核的冻结输入标识；这里未做运行时哈希或 native 求值。
        diagnostic!(
            "SPEC0331 TEMPORARY EXITS IDENTITY INPUT case={} rooted={rooted} path=provider.ko frozen_sha256={} golden_sha256={} golden_bytes={} golden_status=predicted_only text={:?}",
            case.label,
            case.source_hash,
            case.golden_hash,
            case.golden.len(),
            case.text
        );
        let outcome = ast::Input::new(case.text, case.map, case.exit).and_then(|input| {
            let anchors = ast::collect(&input);
            report.query(
                "temporary exits actual two Call-source For sites, Guards, calls and caller identities",
                &anchors,
            );
            let mut inputs: Option<Result<query_inputs::Inputs, Failure>> = None;
            let (program, entry) = pipeline::unit_input_observed(
                &input.sources,
                &input.parsed,
                "provider.ko",
                &mut report,
                rooted,
                |names, typed, owned, report| {
                    unit::observe(&input, anchors.as_ref().ok(), names, typed, owned, report);
                    let queried = anchors
                        .as_ref()
                        .map_err(|e| failure("temporary exits identity AST for FE", e))
                        .and_then(|a| query_inputs::unit(&input, a, names, typed, owned));
                    report.query("temporary exits identity same-analysis FE bridge", &queried);
                    inputs = Some(queried);
                },
            )?;
            diagnostic!(
                "SPEC0331 TEMPORARY EXITS IDENTITY SSA case={} rooted={rooted}\n{}",
                case.label,
                crate::ssa::render::render_program(&program)
            );
            // 无单层/固定 read 数假设的源码 origin 与真实可达函数身份查询。
            let reachable = anchors
                .as_ref()
                .map_err(|e| failure("temporary exits AST identity", e))
                .and_then(|a| {
                    identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
                });
            report.query(
                "temporary exits actual entry and reachable source functions/deinits",
                &reachable,
            );
            let inputs = inputs.unwrap_or_else(|| {
                Err(failure(
                    "temporary exits identity same-analysis observer",
                    "observer did not run",
                ))
            });
            let queried = inputs
                .as_ref()
                .map_err(|e| failure("temporary exits identity FE bridge for SSA", e))
                .and_then(|inputs| query_ssa::check(&program, entry, &input, inputs));
            report.query(
                "temporary exits identity actual factory/owner/lease/Guard/all-edge/all-use closure",
                &queried,
            );
            // 查询失败只记 report；不能遮住真实 verifier/LLVM 的首断点。
            crate::ssa::verify::verify_program(&program)
                .map_err(|e| verify_failure(&input.sources, e))?;
            report.check("temporary exits SSA verifier", true, entry);
            let llvm = crate::llvm::render_verified_program(&program)
                .map_err(|e| failure("LLVM render/verify", e))?;
            report.check("temporary exits LLVM render/verify", true, llvm.len());
            diagnostic!(
                "SPEC0331 TEMPORARY EXITS IDENTITY LLVM case={} rooted={rooted}\n{llvm}",
                case.label
            );
            Ok(())
        });
        Observation {
            label: case.label,
            rooted,
            report,
            outcome,
        }
    }

    #[test]
    fn temporary_map_exits_preserve_factory_owner_and_all_edge_identity() {
        let mut observations = Vec::new();
        for case in sources() {
            for rooted in [false, true] {
                observations.push(observe(&case, rooted));
            }
        }
        for observation in &observations {
            diagnostic!(
                "SPEC0331 TEMPORARY EXITS IDENTITY OUTCOME case={} rooted={} report={:#?}",
                observation.label,
                observation.rooted,
                observation.report
            );
            match &observation.outcome {
                Ok(()) => diagnostic!("stage=LLVM render/verify result=ok"),
                Err(error) => diagnostic!("stage={} original={}", error.stage, error.original),
            }
        }
        // 不以缺事实或 UnsupportedNode 为成功；不因首红丢掉其余十一格。
        assert_diagnostic!(
            observations.len() == 12
                && observations
                    .iter()
                    .all(|o| o.report.passed() && o.outcome.is_ok()),
            "十二格、二十四真实 For 必须完成同次 frontend、完整 validator/view、真实 lower、SSA 与 LLVM；\
             query 失败不遮真实阶段，缺事实/UnsupportedNode 不算成功；本查询只证静态身份/全 edge/use，不声称动态次数、leak 或 native。\n{observations:#?}"
        );
    }
}
