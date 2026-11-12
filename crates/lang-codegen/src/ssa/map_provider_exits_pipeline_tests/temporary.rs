//! 两份原字节 temporary source × MemoryDocument/rooted Unit；四次完整观察后统一硬成功断言。
//! 每份 entry 有两个真实 For；golden 仅留给后续独立准入的 native，不是执行结果。
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
#[allow(unused_imports)] pub(in crate::ssa) use query_ssa::ast;
#[allow(unused_imports)] pub(in crate::ssa) use crate::ssa::map_provider_exits_pipeline_tests::scope_value_births::borrow_alias_observer::facts::unit_9 as unit;

use super::{
    Report,
    fixture::{Failure, failure, verify_failure},
    identity, pipeline,
};

pub(crate) fn sources() -> [(&'static str, bool, &'static str, &'static str, &'static str); 2] {
    [
        (
            "map-temporary",
            true,
            "17546f80b9cadebf9d1ca24f679e1a9a9ad6dc9076101acfd96fd3007ab40016",
            include_str!("../../../tests/fixtures/map-provider-temporary-0331/map-temporary.ko"),
            include_str!(
                "../../../tests/fixtures/map-provider-temporary-0331/map-temporary.stdout"
            ),
        ),
        (
            "mutable-map-temporary",
            false,
            "64d53cc0799ec7b68303f8aa1bfc5b5e4db4d04a73ddf3b24c54655b4289c5dd",
            include_str!(
                "../../../tests/fixtures/map-provider-temporary-0331/mutable-map-temporary.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-temporary-0331/mutable-map-temporary.stdout"
            ),
        ),
    ]
}

#[derive(Debug)]
struct Observation {
    label: &'static str,
    rooted: bool,
    report: Report,
    outcome: Result<(), Failure>,
}

fn observe(
    label: &'static str,
    map: bool,
    source_hash: &str,
    text: &str,
    rooted: bool,
) -> Observation {
    let mut report = Report::default();
    // SHA 是仓外核验的冻结输入标识；不声称这里进行了运行时哈希或 native 求值。
    diagnostic!(
        "SPEC0331 TEMPORARY INPUT case={label} rooted={rooted} path=provider.ko frozen_sha256={source_hash} text={text:?}"
    );
    let outcome = ast::Input::new(text, map).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query("temporary actual two Call-source For AST anchors", &anchors);
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
            "SPEC0331 TEMPORARY SSA case={label} rooted={rooted}\n{}",
            crate::ssa::render::render_program(&program)
        );
        // 只复用无单 For/Name/Guard 假设的入口与源码函数可达查询。
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("temporary AST identity", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "temporary actual entry and four reachable functions",
            &reachable,
        );
        // 只读查询失败留在 report，不隐藏真实 verifier/LLVM 的原错误。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("temporary SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check("temporary LLVM render/verify", true, llvm.len());
        diagnostic!("SPEC0331 TEMPORARY LLVM case={label} rooted={rooted}\n{llvm}");
        Ok(())
    });
    Observation {
        label,
        rooted,
        report,
        outcome,
    }
}

#[test]
fn map_and_mutable_map_temporary_sources_reach_verified_llvm() {
    let mut observations = Vec::new();
    for (label, map, hash, source, _golden) in sources() {
        for rooted in [false, true] {
            observations.push(observe(label, map, hash, source, rooted));
        }
    }
    for observation in &observations {
        diagnostic!(
            "SPEC0331 TEMPORARY OUTCOME case={} rooted={} report={:#?}",
            observation.label,
            observation.rooted,
            observation.report
        );
        match &observation.outcome {
            Ok(()) => diagnostic!("stage=LLVM render/verify result=ok"),
            Err(error) => diagnostic!("stage={} original={}", error.stage, error.original),
        }
    }
    assert_diagnostic!(
        observations.len() == 4
            && observations
                .iter()
                .all(|o| o.report.passed() && o.outcome.is_ok()),
        "四格、八个真实For点必须完成同源frontend、完整validator/view、实际entry、SSA和LLVM；\
         query失败与原阶段失败分账，UnsupportedNode不是成功oracle。\n{observations:#?}"
    );
}

mod query_body {
    //! 每 site 的 K/V、可选派生 Borrow 与 read/End/Next 实际 operand 闭合。
    use super::{
        super::{
            fixture::{Failure, failure, need},
            query_graph::{Graph, Uses, loan, result, value},
            query_inputs::one,
        },
        query_inputs::SiteInput,
        query_ssa::Roles,
    };
    use crate::ssa::model::*;
    use std::collections::{BTreeMap, BTreeSet};

    pub(super) struct Body<'a, 'ir> {
        pub graph: &'a Graph<'ir>,
        pub read: &'ir Function,
        pub module: &'ir Module,
        pub site: &'a SiteInput,
        pub block: BlockId,
        pub roles: Roles,
    }

    impl Body<'_, '_> {
        pub(super) fn check(
            &self,
            uses: &mut Uses,
            protected: &mut BTreeSet<EntityId>,
        ) -> Result<ValueId, Failure> {
            let (graph, site, roles) = (self.graph, self.site, self.roles);
            let ops = graph.ops(self.block)?;
            let key = one(
                ops.iter()
                    .copied()
                    .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. })),
                "temporary SSA unique actual KeyLoan",
            )?;
            let val = one(
                ops.iter()
                    .copied()
                    .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
                "temporary SSA unique actual ValueLoan",
            )?;
            let next = one(
                ops.iter()
                    .copied()
                    .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
                "temporary SSA unique actual Next",
            )?;
            need(
                key.operation
                    == Operation::MapIterationKeyLoan {
                        source: roles.source,
                        cursor: roles.cursor,
                    }
                    && val.operation
                        == Operation::MapIterationValueLoan {
                            source: roles.source,
                            cursor: roles.cursor,
                        }
                    && next.operation
                        == Operation::MapIterationNext {
                            source: roles.source,
                            cursor: roles.cursor,
                        }
                    && [key, val, next]
                        .iter()
                        .all(|i| i.origin == Origin::Source(site.span)),
                "temporary SSA K/V/Next use this current source and cursor",
                (key, val, next, roles),
            )?;
            let (k, v) = (loan(key)?, loan(val)?);
            need(
                k != v
                    && [k, v].iter().all(|l| {
                        graph.function.entity(EntityId::Loan(*l)).is_some_and(|e| {
                            matches!(e.ty, EntityType::Loan { kind: LoanKind::Shared, target }
                        if self.module.type_kind(target) == Some(&SsaTypeKind::StringOwner))
                        })
                    }),
                "temporary SSA distinct K/V Shared String result identities",
                (key, val),
            )?;
            let read_entry = self
                .read
                .entry_block()
                .and_then(|id| self.read.block(id))
                .ok_or_else(|| failure("temporary SSA actual read entry", self.read.id))?;
            need(
                self.read.receiver.is_none()
                    && self.read.borrow_return.is_none()
                    && self.read.carrier_return.is_none()
                    && self.read.return_types.is_empty()
                    && read_entry.parameters.len() == 2
                    && read_entry.parameters.iter().zip([k, v]).all(|(&p, l)| {
                        self.read.entity(p).map(|d| d.ty)
                            == graph.function.entity(EntityId::Loan(l)).map(|d| d.ty)
                    }),
                "temporary SSA actual read Borrow signature matches K/V",
                self.read,
            )?;
            let mut roots = BTreeMap::<LoanId, (usize, Option<LoanId>)>::new();
            let mut live = BTreeSet::new();
            let mut ends = Vec::new();
            let mut calls = 0;
            for i in &ops {
                match &i.operation {
                    Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. } => {
                        let l = loan(i)?;
                        let index = if l == k { 0 } else { 1 };
                        need(
                            [k, v].contains(&l)
                                && roots.insert(l, (index, None)).is_none()
                                && live.insert(l),
                            "temporary SSA actual component birth",
                            i,
                        )?;
                        for operand in [EntityId::Loan(roles.source), EntityId::Value(roles.cursor)] {
                            uses.permit(operand, i);
                        }
                        uses.protect(EntityId::Loan(l));
                        protected.insert(EntityId::Loan(l));
                    }
                    Operation::SharedReborrow { source } => {
                        let (index, _) = *roots
                            .get(source)
                            .ok_or_else(|| failure("temporary SSA derived Borrow actual parent", i))?;
                        let l = loan(i)?;
                        need(
                            live.contains(source)
                                && graph.function.entity(EntityId::Loan(l)).map(|e| e.ty)
                                    == graph.function.entity(EntityId::Loan(*source)).map(|e| e.ty)
                                && i.origin == Origin::Source(site.arguments[index])
                                && roots.insert(l, (index, Some(*source))).is_none()
                                && live.insert(l),
                            "temporary SSA live typed component-derived Borrow",
                            i,
                        )?;
                        uses.permit(EntityId::Loan(*source), i);
                        uses.protect(EntityId::Loan(l));
                        protected.insert(EntityId::Loan(l));
                    }
                    Operation::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    } => {
                        need(
                            *callee == self.read.id
                                && receiver.is_none()
                                && arguments.len() == 2
                                && i.origin == Origin::Source(site.read_call)
                                && i.results.is_empty(),
                            "temporary SSA actual read Call target/origin and Unit result",
                            i,
                        )?;
                        for (index, &argument) in arguments.iter().enumerate() {
                            let EntityId::Loan(l) = argument else {
                                return Err(failure("temporary SSA read Borrow operand", argument));
                            };
                            need(
                                live.contains(&l)
                                    && roots.get(&l).is_some_and(|(root, _)| *root == index),
                                "temporary SSA actual ordered read K/V lineage",
                                (index, i, &roots),
                            )?;
                            uses.permit(argument, i);
                        }
                        calls += 1;
                    }
                    Operation::BorrowEnd { loan } => {
                        let (index, parent) = roots.get(loan).copied().ok_or_else(|| {
                            failure("temporary SSA ended actual component lineage", i)
                        })?;
                        need(
                            calls == 1
                                && live.contains(loan)
                                && !roots.iter().any(|(child, (_, parent))| {
                                    *parent == Some(*loan) && live.contains(child)
                                })
                                && i.results.is_empty()
                                && i.origin
                                    == Origin::Source(if parent.is_none() {
                                        site.span
                                    } else {
                                        site.read_call
                                    }),
                            "temporary SSA derived End follows read and precedes parent End",
                            (i, index, parent),
                        )?;
                        live.remove(loan);
                        if parent.is_none() {
                            ends.push(*loan);
                        }
                        uses.permit(EntityId::Loan(*loan), i);
                    }
                    Operation::MapIterationNext { .. } => {
                        need(
                            i.id == next.id
                                && calls == 1
                                && live.is_empty()
                                && ends == [v, k]
                                && graph.function.entity(result(i)?).map(|e| e.ty)
                                    == graph
                                        .function
                                        .entity(EntityId::Value(roles.cursor))
                                        .map(|e| e.ty),
                            "temporary SSA all derived/KV end before actual Next",
                            (i, &live, &ends, calls),
                        )?;
                        uses.permit(EntityId::Loan(roles.source), i);
                        uses.permit(EntityId::Value(roles.cursor), i);
                        uses.protect(result(i)?);
                        protected.insert(result(i)?);
                    }
                    _ => return Err(failure("temporary SSA unexpected body operation", i)),
                }
            }
            need(
                calls == 1
                    && live.is_empty()
                    && ends == [v, k]
                    && ops.first().map(|i| i.id) == Some(key.id)
                    && graph.position(key)? < graph.position(val)?
                    && ops.last().map(|i| i.id) == Some(next.id),
                "temporary SSA entire body lifetime closure",
                (&ops, &roots, ends),
            )?;
            need(
                graph.edges.iter().all(|(_, e)| {
                    e.arguments
                        .iter()
                        .all(|a| !matches!(a, EntityId::Loan(l) if roots.contains_key(l)))
                }),
                "temporary SSA no K/V or derived loan escapes body edges",
                &graph.edges,
            )?;
            value(next)
        }
    }
}
mod query_cleanup {
    //! 两 site 的真实 temporary owner 与 normal 清理；unit 不借用 single 的 owner API。
    use super::{
        super::{
            fixture::{Failure, failure, need},
            query_inputs::one,
        },
        ast::Site,
    };
    use lang_frontend::{
        ownership_checking::{
            CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
            UnitIterationExitKind as UE,
        },
        source::Span,
        type_checking::{UnitExpressionId, UnitStatementId},
    };

    pub(super) fn unit(
        site: &Site,
        statement: UnitStatementId,
        source: UnitExpressionId,
        owned: &CompilationUnitOwnership,
    ) -> Result<Span, Failure> {
        let plan = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("temporary FE actual qualified cleanup plan", statement))?;
        let descriptor = plan.descriptor();
        let drop = one(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Temporary(source)),
            "temporary FE unique qualified source Drop",
        )?;
        need(
            drop.point() == UnitDropPoint::LoopExit(statement)
                && drop.value_origin() == site.source_span,
            "temporary FE actual qualified Call Drop point and origin",
            (site, drop),
        )?;
        let expected = [
            (
                UE::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(statement.source_unit(), site.body)),
                vec![
                    UA::EndBinding {
                        statement,
                        symbol: descriptor.value_binding(),
                    },
                    UA::EndBinding {
                        statement,
                        symbol: descriptor.key_binding(),
                    },
                ],
            ),
            (
                UE::Exhaustion,
                UnitDropPoint::LoopExit(statement),
                vec![
                    UA::FinishProvider(statement),
                    UA::EndSource(statement),
                    UA::Drop(*drop),
                ],
            ),
        ];
        need(
            plan.exits().len() == expected.len(),
            "temporary FE exact qualified two normal exits",
            plan.exits(),
        )?;
        for (kind, point, actions) in expected {
            let exit = one(
                plan.exits().iter().filter(|e| e.kind() == kind),
                "temporary FE unique qualified exit",
            )?;
            need(
                exit.point() == point && exit.actions() == actions,
                "temporary FE independently expected qualified cleanup",
                (exit, &actions),
            )?;
            need(
                owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "temporary FE qualified same-table accessor consistency",
                (point, &actions),
            )?;
        }
        Ok(drop.value_origin())
    }
}
mod query_inputs {
    //! 同次公开 typed/ownership 产品到真实 source spans 的桥；不制造 facts 或资格结果。
    use super::{
        super::{
            fixture::{Failure, failure, need},
            query_inputs::one,
        },
        ast::{Anchors, Function, Input, Site},
        query_cleanup,
    };
    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::{
            DeclarationId, NameResolution, SourceUnitId, SymbolId, SymbolKind, UnitReferenceTarget,
            ValidatedCompilationUnitNames,
        },
        ownership_checking::{
            CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
            OwnershipBindingKind, UnitIterationSourceAccess, UnitLoanTarget,
        },
        parser::Expression,
        source::Span,
        type_checking::{
            CallableResultSource, CompilationUnitTypes, ExpressionCategory, ParameterMode,
            UnitCallTarget, UnitExpressionId, UnitStatementId,
        },
    };

    #[derive(Debug)]
    pub(super) struct SiteInput {
        pub span: Span,
        pub source: Span,
        pub factory: Span,
        pub read_call: Span,
        pub arguments: [Span; 2],
        pub drop: Span,
    }

    #[derive(Debug)]
    pub(super) struct Inputs {
        pub entry: Span,
        pub read: Span,
        pub sites: Vec<SiteInput>,
    }

    fn anchor<'a>(a: &'a Anchors, name: &str) -> Result<&'a Function, Failure> {
        one(
            a.functions.iter().filter(|v| v.name == name),
            "temporary FE unique actual function AST",
        )
    }

    fn spans(f: &Input, a: &Anchors, site: &Site, drop: Span) -> Result<SiteInput, Failure> {
        need(
            drop == site.source_span,
            "temporary FE source and Drop original Call span",
            (site, drop),
        )?;
        Ok(SiteInput {
            span: site.span,
            source: site.source_span,
            factory: anchor(a, site.factory)?.span,
            read_call: f.expression(site.read)?.1,
            arguments: [
                f.expression(site.arguments[0])?.1,
                f.expression(site.arguments[1])?.1,
            ],
            drop,
        })
    }

    fn symbol(names: &NameResolution, span: Span) -> Result<SymbolId, Failure> {
        Ok(one(
            names
                .symbols()
                .iter()
                .filter(|s| s.span() == span && !s.is_synthetic()),
            "temporary FE unique actual source symbol",
        )?
        .id())
    }

    fn callee_span(f: &Input, expression: ExpressionId) -> Result<Span, Failure> {
        let (value, _) = f.expression(expression)?;
        let Expression::Call { callee, .. } = value else {
            return Err(failure("temporary FE actual Call expression", value));
        };
        Ok(f.expression(*callee)?.1)
    }

    fn declaration(
        names: &ValidatedCompilationUnitNames,
        unit: SourceUnitId,
        function: &Function,
    ) -> Result<DeclarationId, Failure> {
        Ok(one(
            names.names().index().declarations().iter().filter(|d| {
                d.source_unit() == unit
                    && d.root() == function.item
                    && d.name_span() == function.name_span
                    && d.kind() == SymbolKind::Function
                    && d.name() == function.name
            }),
            "temporary FE actual qualified function declaration",
        )?
        .id())
    }

    fn unit_callee(
        f: &Input,
        names: &ValidatedCompilationUnitNames,
        expression: ExpressionId,
        selected: DeclarationId,
    ) -> Result<(), Failure> {
        let span = callee_span(f, expression)?;
        let reference = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == span),
            "temporary FE qualified actual callee reference",
        )?;
        let selected_present = match reference.target() {
            UnitReferenceTarget::Declaration(id) => *id == selected,
            UnitReferenceTarget::OverloadSet(ids) => ids.contains(&selected),
            _ => false,
        };
        need(
            selected_present,
            "temporary FE qualified typed-selected target belongs to actual reference",
            reference,
        )
    }

    pub(super) fn unit(
        f: &Input,
        a: &Anchors,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
    ) -> Result<Inputs, Failure> {
        let source_names = one(
            names.names().source_units().iter(),
            "temporary FE actual sole source unit",
        )?;
        let unit = source_names.source_unit();
        let expression = |id| UnitExpressionId::new(unit, id);
        need(
            source_names.resolution().source_id() == f.parsed.source_id()
                && typed.map_iterations().len() == 2
                && owned.map_iterations().len() == 2,
            "temporary FE actual qualified source and two plans",
            (unit, typed.map_iterations(), owned.map_iterations()),
        )?;
        let read = anchor(a, "read")?;
        let read_id = declaration(names, unit, read)?;
        let mut sites = Vec::new();
        let mut bindings = Vec::new();
        for site in &a.sites {
            let statement = UnitStatementId::new(unit, site.statement);
            let source = expression(site.source);
            let d = typed
                .map_iteration(statement)
                .ok_or_else(|| failure("temporary FE qualified typed For", statement))?;
            let p = owned
                .map_iteration(statement)
                .ok_or_else(|| failure("temporary FE qualified owned For", statement))?;
            need(
                d.statement() == statement
                    && d.source() == source
                    && p.descriptor() == d
                    && typed.expression_type(source) == Some(d.source_type())
                    && typed.expression_category(source) == Some(ExpressionCategory::Temporary)
                    && p.source_access() == UnitIterationSourceAccess::Temporary
                    && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Temporary(id)) if *id == source),
                "temporary FE actual qualified source expression identity",
                (site, d, p),
            )?;
            let factory_id = declaration(names, unit, anchor(a, site.factory)?)?;
            let call = typed
                .call(source)
                .ok_or_else(|| failure("temporary FE qualified actual factory Call", source))?;
            need(
                call.expression() == source
                    && call.target() == UnitCallTarget::Declaration(factory_id)
                    && call.receiver().is_none()
                    && call.arguments().is_empty()
                    && call.result_source() == CallableResultSource::Owned
                    && call.return_type() == d.source_type(),
                "temporary FE actual qualified factory and Owned result",
                call,
            )?;
            unit_callee(f, names, site.source, factory_id)?;
            for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
                .into_iter()
                .enumerate()
            {
                let binding = d.bindings()[index];
                let loan = p.loan(component);
                need(
                    binding.source_unit() == unit
                        && binding.symbol() == symbol(source_names.resolution(), site.bindings[index])?
                        && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                        && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                        && p.bindings()[index].symbol() == binding
                        && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                        && loan.statement() == statement
                        && loan.source() == source
                        && loan.binding() == binding
                        && loan.component() == component
                        && loan.kind() == LoanKind::Shared
                        && loan.origin() == &UnitLoanTarget::Temporary(source),
                    "temporary FE qualified component source and marker lineage",
                    (statement, binding, loan),
                )?;
            }
            let read_call = expression(site.read);
            let call = typed
                .call(read_call)
                .ok_or_else(|| failure("temporary FE qualified actual read Call", read_call))?;
            need(
                call.expression() == read_call
                    && call.target() == UnitCallTarget::Declaration(read_id)
                    && call.receiver().is_none()
                    && call.arguments().len() == 2,
                "temporary FE qualified read target and arity",
                call,
            )?;
            unit_callee(f, names, site.read, read_id)?;
            for (index, argument) in call.arguments().iter().enumerate() {
                let span = f.expression(site.arguments[index])?.1;
                let reference = one(
                    names
                        .names()
                        .references()
                        .iter()
                        .filter(|r| r.span() == span),
                    "temporary FE qualified actual component reference",
                )?;
                need(
                    argument.argument_index() == index
                        && argument.parameter_index() == index
                        && argument.mode() == ParameterMode::Borrow
                        && argument.parameter_type() == [d.key_type(), d.value_type()][index]
                        && matches!(reference.target(), UnitReferenceTarget::Symbol(id) if *id == d.bindings()[index]),
                    "temporary FE qualified Borrow argument is this component",
                    (argument, reference),
                )?;
            }
            let drop = query_cleanup::unit(site, statement, source, owned)?;
            bindings.extend(d.bindings());
            sites.push(spans(f, a, site, drop)?);
        }
        need(
            bindings.len() == 4
                && bindings
                    .iter()
                    .enumerate()
                    .all(|(i, b)| !bindings[..i].contains(b)),
            "temporary FE four qualified component bindings stay independent",
            bindings,
        )?;
        Ok(Inputs {
            entry: anchor(a, "entry")?.span,
            read: read.span,
            sites,
        })
    }
}
pub(in crate::ssa) mod query_ssa;
mod query_test {
    //! 新 exact 独立四次 fresh；查询 Err 只入 report，原 validator/SSA/LLVM 错误继续保全。
    use super::{
        super::{
            Report,
            fixture::{Failure, failure, verify_failure},
            identity, pipeline,
        },
        Observation, ast, query_inputs, query_ssa, unit,
    };
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};

    fn observe(label: &'static str, map: bool, hash: &str, text: &str, rooted: bool) -> Observation {
        let mut report = Report::default();
        diagnostic!(
            "SPEC0331 TEMPORARY IDENTITY INPUT case={label} rooted={rooted} path=provider.ko frozen_sha256={hash} text={text:?}"
        );
        let outcome = ast::Input::new(text, map).and_then(|input| {
            let anchors = ast::collect(&input);
            report.query(
                "temporary identity actual two Call-source AST anchors",
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
                        .map_err(|e| failure("temporary identity AST for FE", e))
                        .and_then(|a| query_inputs::unit(&input, a, names, typed, owned));
                    report.query(
                        "temporary identity same-analysis FE source/owner/cleanup bridge",
                        &queried,
                    );
                    inputs = Some(queried);
                },
            )?;
            diagnostic!(
                "SPEC0331 TEMPORARY IDENTITY SSA case={label} rooted={rooted}\n{}",
                crate::ssa::render::render_program(&program)
            );
            let reachable = anchors
                .as_ref()
                .map_err(|e| failure("temporary identity actual AST roots", e))
                .and_then(|a| {
                    identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
                });
            report.query(
                "temporary identity actual returned entry and reachable functions",
                &reachable,
            );
            let inputs = inputs.unwrap_or_else(|| {
                Err(failure(
                    "temporary identity same-analysis FE observer",
                    "observer did not run",
                ))
            });
            let queried = inputs
                .as_ref()
                .map_err(|e| failure("temporary identity FE bridge for SSA", e))
                .and_then(|inputs| query_ssa::check(&program, entry, inputs, map));
            report.query(
                "temporary identity both source owners all incoming and complete uses",
                &queried,
            );
            // 不使用 query 的 ?：保持原 verifier/LLVM 的真实到达与首错误。
            crate::ssa::verify::verify_program(&program)
                .map_err(|e| verify_failure(&input.sources, e))?;
            report.check("temporary identity original SSA verifier", true, entry);
            let llvm = crate::llvm::render_verified_program(&program)
                .map_err(|e| failure("LLVM render/verify", e))?;
            report.check(
                "temporary identity original LLVM render/verify",
                true,
                llvm.len(),
            );
            diagnostic!("SPEC0331 TEMPORARY IDENTITY LLVM case={label} rooted={rooted}\n{llvm}");
            Ok(())
        });
        Observation {
            label,
            rooted,
            report,
            outcome,
        }
    }

    #[test]
    fn temporary_map_sources_preserve_owner_and_all_edge_identity() {
        let mut observations = Vec::new();
        for (label, map, hash, text, _golden) in super::sources() {
            for rooted in [false, true] {
                observations.push(observe(label, map, hash, text, rooted));
            }
        }
        for observation in &observations {
            diagnostic!(
                "SPEC0331 TEMPORARY IDENTITY OUTCOME case={} rooted={} report={:#?}",
                observation.label,
                observation.rooted,
                observation.report
            );
            match &observation.outcome {
                Ok(()) => diagnostic!("stage=LLVM render/verify result=ok"),
                Err(error) => diagnostic!("stage={} original={}", error.stage, error.original),
            }
        }
        assert_diagnostic!(
            observations.len() == 4
                && observations
                    .iter()
                    .all(|o| o.report.passed() && o.outcome.is_ok()),
            "两份原源×MemoryDocument/rooted Unit四fresh、八site必须完成同次FE、完整validator、真实Call owner全部incoming/uses、SSA和LLVM；\
             query失败与原阶段错误分账，未运行不算通过，不推断动态次数。\n{observations:#?}"
        );
    }
}
