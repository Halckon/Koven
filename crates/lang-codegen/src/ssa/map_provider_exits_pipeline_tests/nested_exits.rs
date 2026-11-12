//! 同根双层 inner continue/break/return 的独立首证 collector。
//! 十二次 fresh 同源 pipeline 全部观察后统一硬成功；不执行 native，不预言复杂 cleanup 拓扑。
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast;
mod guards {
    //! 两个普通 Resource local 的真实形成/构造/deinit/Drop 来源。
    //! formed 是历史，不能代替当前可用性；不对未观察的 owner/phi/condition/Drop 数量做预言。
    use super::{super::Report, ast::Anchors, unit as unit_facts};
    use crate::test_support::diagnostics::diagnostic;
    use lang_frontend::{
        name_resolution::{SymbolKind, ValidatedCompilationUnitNames},
        ownership_checking::{CompilationUnitOwnership, UnitDropTarget},
        type_checking::{
            CompilationUnitTypes, NominalKind, ParameterMode, UnitConstructionTarget, UnitExpressionId,
            UnitItemId, UnitStatementId, UnitTypeKind,
        },
    };

    pub(super) fn unit(
        a: &Anchors,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        report: &mut Report,
    ) {
        let [source] = names.names().source_units() else {
            report.check(
                "unit nested exits Guard unique source",
                false,
                names.names().source_units(),
            );
            return;
        };
        let source_unit = source.source_unit();
        diagnostic!(
            "SPEC0331 NESTED EXITS UNIT TRANSFER expression={:?}",
            UnitExpressionId::new(source_unit, a.transfer)
        );
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
            report.check("unit nested exits qualified Guard Resource constructor/deinit identity",
                local.is_some() && class.is_some() && matches!(nominals.as_slice(), [nominal]
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
                        && local.is_some_and(|id| typed.symbol_type(id) == Some(c.result_type()))
                        && typed.is_resource_type(c.result_type()) == Some(true)),
                (guard, &classes, &locals, construction, &nominals));
            let drops = owned
                .drops()
                .iter()
                .filter(|drop| matches!(drop.target(), UnitDropTarget::Named(id) if Some(id) == local))
                .collect::<Vec<_>>();
            report.check(
                "unit nested exits Guard qualified formation history and actual Drop origins",
                local.is_some()
                    && !drops.is_empty()
                    && drops.iter().all(|drop| drop.value_origin() == guard.marker),
                (guard.marker, local, &drops, owned.formed_owned_locals()),
            );
            // Unit 不发布 single owner/condition 表；原 drop 与全 plan/cleanup 原样保全。
            diagnostic!(
                "SPEC0331 NESTED EXITS UNIT GUARD marker={:?} initializer={expression:?} construction_span={:?} deinit_span={:?} drops={drops:#?}",
                guard.marker,
                guard.span,
                guard.deinit_span
            );
        }
    }
}
pub(in crate::ssa) mod query_body;
#[allow(unused_imports)] pub(in crate::ssa) use crate::ssa::map_provider_exits_pipeline_tests::scope_lambda_body::graph::query_callers as query_callers;
mod query_graph {
    //! Nested-only role witness, beside the unchanged generic read-only graph helper.
    use super::super::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, inherits},
    };
    use crate::ssa::model::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(super) enum Role {
        Caller,
        OuterSource,
        OuterCursor,
        OuterKey,
        OuterValue,
        InnerSource,
        InnerCursor,
        InnerKey,
        InnerValue,
        OuterGuard,
        InnerGuard,
        CallerOwnerA,
        CallerOwnerB,
        CallerResultA,
        CallerResultB,
    }
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct Roles(pub BTreeMap<Role, EntityId>);
    impl Roles {
        pub fn get(&self, role: Role) -> Result<EntityId, Failure> {
            self.0
                .get(&role)
                .copied()
                .ok_or_else(|| failure("nested exits SSA missing current role", (role, self)))
        }
        pub fn loan(&self, role: Role) -> Result<LoanId, Failure> {
            as_loan(self.get(role)?)
        }
        pub fn value(&self, role: Role) -> Result<ValueId, Failure> {
            as_value(self.get(role)?)
        }
        pub fn insert(&mut self, role: Role, entity: EntityId) {
            self.0.insert(role, entity);
        }
        pub fn retain(&self, roles: &[Role]) -> Result<Self, Failure> {
            Ok(Self(
                roles
                    .iter()
                    .map(|&r| Ok((r, self.get(r)?)))
                    .collect::<Result<_, Failure>>()?,
            ))
        }
    }
    pub(super) fn as_loan(entity: EntityId) -> Result<LoanId, Failure> {
        match entity {
            EntityId::Loan(v) => Ok(v),
            _ => Err(failure("nested exits SSA role Loan", entity)),
        }
    }
    pub(super) fn as_value(entity: EntityId) -> Result<ValueId, Failure> {
        match entity {
            EntityId::Value(v) => Ok(v),
            _ => Err(failure("nested exits SSA role Value", entity)),
        }
    }
    pub(super) fn ty(graph: &Graph<'_>, entity: EntityId) -> Result<EntityType, Failure> {
        graph
            .function
            .entity(entity)
            .map(|e| e.ty)
            .ok_or_else(|| failure("nested exits SSA entity type", entity))
    }
    pub(super) fn full_graph(graph: &Graph<'_>) -> Result<(), Failure> {
        let blocks = graph.blocks.iter().copied().collect::<BTreeSet<_>>();
        let instructions = graph
            .instructions
            .iter()
            .map(|i| i.id)
            .collect::<BTreeSet<_>>();
        let entities = graph
            .function
            .blocks
            .iter()
            .flat_map(|b| b.parameters.iter().copied())
            .chain(
                graph
                    .instructions
                    .iter()
                    .flat_map(|i| i.results.iter().copied()),
            )
            .collect::<Vec<_>>();
        let unique_entities = entities.iter().copied().collect::<BTreeSet<_>>();
        need(
            entities.len() == unique_entities.len()
                && entities.len()
                    == graph.function.values.len()
                        + graph.function.places.len()
                        + graph.function.loans.len()
                && graph.instructions.iter().all(|i| {
                    i.results.iter().all(|&e| {
                        graph
                            .function
                            .entity(e)
                            .is_some_and(|d| d.origin == i.origin)
                    })
                }),
            "nested exits SSA complete entity arena and exact result origins",
            graph.function,
        )?;
        need(
            blocks.len() == graph.function.blocks.len()
                && graph.function.blocks.iter().all(|b| blocks.contains(&b.id))
                && instructions.len() == graph.function.instructions.len()
                && graph
                    .function
                    .instructions
                    .iter()
                    .all(|i| instructions.contains(&i.id)),
            "nested exits SSA complete arenas equal referenced reachable graph",
            graph.function,
        )
    }

    #[derive(Default)]
    pub(super) struct Witness {
        pub uses: Uses,
        pub aliases: BTreeMap<Role, BTreeSet<EntityId>>,
        edges: BTreeMap<(BlockId, BlockId), Vec<EntityId>>,
        pub conditions: BTreeMap<BlockId, ValueId>,
        pub providers: BTreeSet<InstructionId>,
        pub returns: BTreeMap<BlockId, Vec<ValueId>>,
        pub returned: Option<(BlockId, ValueId)>,
    }
    impl Witness {
        pub fn protect(&mut self, roles: &Roles) -> Result<(), Failure> {
            for (&role, &entity) in &roles.0 {
                need(
                    self.aliases
                        .iter()
                        .all(|(&other, ids)| other == role || !ids.contains(&entity)),
                    "nested exits SSA distinct caller/lease/cursor/component role lineage",
                    (role, entity, &self.aliases),
                )?;
                self.aliases.entry(role).or_default().insert(entity);
                self.uses.protect(entity);
            }
            Ok(())
        }
        pub fn edge(
            &mut self,
            graph: &Graph<'_>,
            from: BlockId,
            edge: &Edge,
            roles: &Roles,
        ) -> Result<Roles, Failure> {
            self.protect(roles)?;
            let expected = roles.0.values().copied().collect::<BTreeSet<_>>();
            need(
                expected.len() == roles.0.len()
                    && edge
                        .arguments
                        .iter()
                        .all(|argument| expected.contains(argument))
                    && edge
                        .arguments
                        .iter()
                        .copied()
                        .collect::<BTreeSet<_>>()
                        .len()
                        == edge.arguments.len(),
                "nested exits SSA full exact current-role edge operands",
                (from, edge, roles),
            )?;
            need(
                self.edges
                    .insert((from, edge.target), edge.arguments.clone())
                    .is_none(),
                "nested exits SSA each actual edge proved once",
                (from, edge),
            )?;
            let target = graph.block(edge.target)?;
            need(
                target.parameters.iter().all(|&parameter| {
                    graph
                        .function
                        .entity(parameter)
                        .is_some_and(|data| data.origin == target.origin)
                }),
                "nested exits SSA every role parameter retains actual target block origin",
                target,
            )?;
            let next = Roles(
                roles
                    .0
                    .iter()
                    .map(|(&r, &e)| {
                        if edge.arguments.contains(&e) {
                            Ok((r, graph.transported(edge, e)?))
                        } else {
                            need(
                                inherits(graph, e, edge.target)?,
                                "nested exits SSA inherited role still dominates target",
                                (from, edge, r, e),
                            )?;
                            Ok((r, e))
                        }
                    })
                    .collect::<Result<_, Failure>>()?,
            );
            self.protect(&next)?;
            Ok(next)
        }
        pub fn finish(&self, graph: &Graph<'_>) -> Result<(), Failure> {
            full_graph(graph)?;
            need(
                self.edges.len() == graph.edges.len(),
                "nested exits SSA all actual edges covered",
                (&self.edges, &graph.edges),
            )?;
            for &(from, edge) in &graph.edges {
                need(
                    self.edges.get(&(from, edge.target)) == Some(&edge.arguments),
                    "nested exits SSA every edge exact proved operands",
                    (from, edge),
                )?;
            }
            // The shared Uses helper accepts any occurrence on some edge. This layer checks every
            // terminator and every edge separately, and excludes protected roles as conditions.
            for &block in &graph.blocks {
                let term = graph.term(block)?;
                match &term.kind {
                    TerminatorKind::Conditional {
                        condition,
                        when_true,
                        when_false,
                    } => {
                        need(
                            self.conditions.get(&block) == Some(condition)
                                && !self
                                    .aliases
                                    .values()
                                    .any(|s| s.contains(&EntityId::Value(*condition)))
                                && !when_true.arguments.contains(&EntityId::Value(*condition))
                                && !when_false.arguments.contains(&EntityId::Value(*condition)),
                            "nested exits SSA actual condition distinct from transport roles",
                            term,
                        )?;
                    }
                    TerminatorKind::Branch(_) => {}
                    TerminatorKind::Return { values } => need(
                        self.returns
                            .get(&block)
                            .map_or(values.is_empty(), |expected| expected == values),
                        "nested exits SSA Unit return with no escaped role",
                        term,
                    )?,
                    _ => return Err(failure("nested exits SSA unexpected terminal", term)),
                }
            }
            self.uses.check(graph, self.returned)
        }
    }
}
pub(in crate::ssa) mod query_inputs;
#[allow(unused_imports)] pub(in crate::ssa) use query_inputs::query_resources;
#[allow(unused_imports)] pub(in crate::ssa) use query_body::query_ssa;
mod query_tail {
    //! The actual post-loop/caller size Ifs, including both incoming join edges.
    use super::super::{
        fixture::{Failure, contains, failure, need},
        query_graph::{Graph, result, value},
        query_inputs::one,
    };
    use super::{
        ast::Input,
        query_graph::{Roles, Witness},
    };
    use crate::ssa::model::*;
    use lang_frontend::{parser::Expression, source::Span};

    pub(super) fn size<'a>(
        graph: &Graph<'a>,
        span: Span,
        owner: EntityId,
        witness: &mut Witness,
    ) -> Result<&'a Instruction, Failure> {
        let i = one(
            graph.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(span) && matches!(i.operation, Operation::MapSize { .. })
            }),
            "nested exits SSA actual size instruction",
        )?;
        need(
            i.operation == Operation::MapSize { owner },
            "nested exits SSA size exact current source",
            i,
        )?;
        witness.uses.permit(owner, i);
        Ok(i)
    }

    pub(super) fn conditional(
        graph: &Graph<'_>,
        f: &Input,
        size: &Instruction,
        checked_reference: Option<(Span, i128)>,
        roles: &Roles,
        witness: &mut Witness,
    ) -> Result<(BlockId, Roles), Failure> {
        let span = checked_reference.map_or(size.origin.span(), |(span, _)| span);
        let control = one(
            f.parsed.ast().expressions().iter().filter_map(|(_, node)| {
                let Expression::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                let condition_node = f.parsed.ast().expressions().get(*condition).ok()?;
                contains(condition_node.span(), span).then_some((
                    node.span(),
                    condition_node.span(),
                    *then_branch,
                    *else_branch,
                ))
            }),
            "nested exits SSA actual AST size If",
        )?;
        let term = graph.term(size.block)?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &term.kind
        else {
            return Err(failure("nested exits SSA size actual conditional", term));
        };
        let cmp = graph.definition(EntityId::Value(*condition))?;
        let Operation::Compare {
            operator: ComparisonOperator::Equal,
            left,
            right,
        } = cmp.operation
        else {
            return Err(failure("nested exits SSA size equality condition", cmp));
        };
        let constant = graph.definition(EntityId::Value(right))?;
        need(
            term.origin == Origin::Source(control.0)
                && cmp.origin == Origin::Source(control.1)
                && cmp.block == size.block
                && left == value(size)?
                && constant.block == size.block
                && contains(control.1, constant.origin.span())
                && matches!(
                    constant.operation,
                    Operation::Constant(ScalarConstant::Integer(actual))
                        if checked_reference.map_or(matches!((f.sources.slice(constant.origin.span()), actual), (Ok("0"), 0) | (Ok("2"), 2)), |(_, expected)| actual == expected && f.sources.slice(constant.origin.span()).is_ok_and(|s| s == expected.to_string()))
                )
                && graph.position(size)? < graph.position(cmp)?
                && graph.position(constant)? < graph.position(cmp)?
                && f.sources.slice(constant.origin.span()).is_ok_and(|s| {
                    s == "0" || s == "2" || checked_reference.is_some_and(|(_, n)| s == n.to_string())
                }),
            "nested exits SSA size actual comparison result drives its source If",
            (size, cmp, constant, term),
        )?;
        witness.conditions.insert(size.block, *condition);
        if checked_reference.is_some() {
            witness.uses.permit(result(size)?, cmp);
        }
        let then = when_true.target;
        let otherwise = when_false.target;
        need(
            then != otherwise && then != size.block && otherwise != size.block,
            "nested exits SSA distinct size If arms",
            term,
        )?;
        graph.incoming_from(then, &[size.block])?;
        graph.incoming_from(otherwise, &[size.block])?;
        let a = witness.edge(graph, size.block, when_true, roles)?;
        let b = witness.edge(graph, size.block, when_false, roles)?;
        let then_span = f
            .parsed
            .ast()
            .statements()
            .get(control.2)
            .map_err(|e| failure("nested AST then body", e))?
            .span();
        need(
            graph.block(then)?.origin == Origin::Source(then_span)
                && graph.block(otherwise)?.origin
                    == Origin::Source(if let Some(id) = control.3 {
                        f.parsed
                            .ast()
                            .statements()
                            .get(id)
                            .map_err(|e| failure("nested exits AST else body", e))?
                            .span()
                    } else {
                        control.0
                    }),
            "nested exits SSA size arms actual AST origins",
            (graph.block(then)?, graph.block(otherwise)?),
        )?;
        let ea = graph.branch(then)?;
        let eb = graph.branch(otherwise)?;
        need(
            ea.target == eb.target
                && ea.target != then
                && ea.target != otherwise
                && graph.term(then)?.origin == Origin::Source(control.0)
                && graph.term(otherwise)?.origin == Origin::Source(control.0),
            "nested exits SSA both actual size arms enter same join",
            (ea, eb),
        )?;
        graph.incoming_from(ea.target, &[then, otherwise])?;
        let joined = witness.edge(graph, then, ea, &a)?;
        let other_joined = witness.edge(graph, otherwise, eb, &b)?;
        need(
            joined == other_joined && graph.block(ea.target)?.origin == Origin::Source(control.0),
            "nested exits SSA all size join incoming preserve identical role columns",
            (&joined, &other_joined),
        )?;
        // All MapSize operations are checked by the caller. Neither If arm acquires provider roles.
        for block in [then, otherwise] {
            need(
                !graph.ops(block)?.iter().any(|i| {
                    matches!(
                        i.operation,
                        Operation::DirectCall { .. }
                            | Operation::MapSize { .. }
                            | Operation::SharedReborrow { .. }
                    )
                }),
                "nested exits SSA marker-only size branch",
                graph.ops(block)?,
            )?;
        }
        let _ = result(size)?;
        Ok((ea.target, joined))
    }
}
mod query_test {
    //! 新 exact 独立十二次 fresh；查询 Err 只入 report，原 validator/SSA/LLVM 错误继续保全。
    use super::{
        super::{
            Report,
            fixture::{Failure, failure, verify_failure},
            identity, pipeline,
        },
        Case, Observation, ast, query_inputs, query_ssa, unit,
    };
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};

    fn observe(case: &Case, rooted: bool) -> Observation {
        let (label, map, hash, text) = (case.label, case.map, case.source_hash, case.text);
        let mut report = Report::default();
        diagnostic!(
            "SPEC0331 NESTED EXITS IDENTITY INPUT case={label} rooted={rooted} path=provider.ko frozen_sha256={hash} text={text:?}"
        );
        let outcome = ast::Input::new(text, map, case.exit).and_then(|input| {
            let anchors = ast::collect(&input);
            report.query(
                "nested exits identity actual parent/child same-root AST anchors",
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
                        .map_err(|e| failure("nested exits identity AST for FE", e))
                        .and_then(|a| query_inputs::unit(&input, a, names, typed, owned));
                    report.query(
                        "nested exits identity same-analysis FE source/caller/component/cleanup bridge",
                        &queried,
                    );
                    inputs = Some(queried);
                },
            )?;
            diagnostic!(
                "SPEC0331 NESTED EXITS IDENTITY SSA case={label} rooted={rooted}\n{}",
                crate::ssa::render::render_program(&program)
            );
            let reachable = anchors
                .as_ref()
                .map_err(|e| failure("nested exits identity actual AST roots", e))
                .and_then(|a| {
                    identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
                });
            report.query(
                "nested exits identity actual returned entry and reachable functions",
                &reachable,
            );
            let inputs = inputs.unwrap_or_else(|| {
                Err(failure(
                    "nested exits identity same-analysis FE observer",
                    "observer did not run",
                ))
            });
            let queried = inputs
                .as_ref()
                .map_err(|e| failure("nested exits identity FE bridge for SSA", e))
                .and_then(|inputs| query_ssa::check(&program, entry, &input, inputs));
            report.query(
                "nested exits identity caller two Guard identities both leases all incoming full cleanup and complete uses",
                &queried,
            );
            // 不使用 query 的 ?：保持原 verifier/LLVM 的真实到达与首错误。
            crate::ssa::verify::verify_program(&program)
                .map_err(|e| verify_failure(&input.sources, e))?;
            report.check("nested exits identity original SSA verifier", true, entry);
            let llvm = crate::llvm::render_verified_program(&program)
                .map_err(|e| failure("LLVM render/verify", e))?;
            report.check(
                "nested exits identity original LLVM render/verify",
                true,
                llvm.len(),
            );
            diagnostic!("SPEC0331 NESTED EXITS IDENTITY LLVM case={label} rooted={rooted}\n{llvm}");
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
    fn nested_inner_exits_preserve_full_caller_guard_and_edge_identity() {
        let mut observations = Vec::new();
        for case in super::sources() {
            for rooted in [false, true] {
                observations.push(observe(&case, rooted));
            }
        }
        for observation in &observations {
            diagnostic!(
                "SPEC0331 NESTED EXITS IDENTITY OUTCOME case={} rooted={} report={:#?}",
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
            observations.len() == 12
                && observations
                    .iter()
                    .all(|o| o.report.passed() && o.outcome.is_ok()),
            "六份原源×MemoryDocument/rooted Unit十二fresh、二十四site必须完成同次FE、完整validator、真实caller/两Guard/两lease/四component全部incoming/uses、SSA和LLVM；\
             query失败与原阶段错误分账，未运行不算通过，不推断动态次数。\n{observations:#?}"
        );
    }
}
mod unit;

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
            label: "map-borrow-nested-continue",
            map: true,
            exit: Exit::Continue,
            source_hash: "58fd50d10b4f68127c146702aa2113be739a0504681b2d88877b2fa43843a964",
            golden_hash: "705c097f36e3a9f41f858997ab6a96f5f4e17e3db724083b2763fed28fa5594d",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-continue.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-continue.stdout"
            ),
        },
        Case {
            label: "map-borrow-nested-break",
            map: true,
            exit: Exit::Break,
            source_hash: "a46755450fa6e24db7a589366263d0153c28c805e06f42ca97cbe0764989be76",
            golden_hash: "583a0d8bfa98cddb8f1eb5bb19f583bfd6d20363c6eb5721ddb568d813f57722",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-break.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-break.stdout"
            ),
        },
        Case {
            label: "map-borrow-nested-return",
            map: true,
            exit: Exit::Return,
            source_hash: "e8f03c0e07445c9842772d07198fd2d42898b3ee9987538902b30751813762a6",
            golden_hash: "0f96d2872f26120fc4b29614689a76c8418b3ac4fc6c2bb63cfeebace937b6aa",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-return.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-return.stdout"
            ),
        },
        Case {
            label: "mutable-map-nested-continue",
            map: false,
            exit: Exit::Continue,
            source_hash: "c1a738c7d4a7d73e75af8fd7339d98c55a5fcf4d30c657b9256828d827a7a4db",
            golden_hash: "705c097f36e3a9f41f858997ab6a96f5f4e17e3db724083b2763fed28fa5594d",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-continue.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-continue.stdout"
            ),
        },
        Case {
            label: "mutable-map-nested-break",
            map: false,
            exit: Exit::Break,
            source_hash: "62ade3dad99e15685359bc47e4251417f0424726b45edba9cfb04747b5f1ed40",
            golden_hash: "583a0d8bfa98cddb8f1eb5bb19f583bfd6d20363c6eb5721ddb568d813f57722",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-break.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-break.stdout"
            ),
        },
        Case {
            label: "mutable-map-nested-return",
            map: false,
            exit: Exit::Return,
            source_hash: "a244449378282b40055e5468b46507765f0b50f860b84b61dd9fecd999159835",
            golden_hash: "0f96d2872f26120fc4b29614689a76c8418b3ac4fc6c2bb63cfeebace937b6aa",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-return.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-return.stdout"
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
        "SPEC0331 NESTED EXITS INPUT case={} rooted={rooted} path=provider.ko frozen_sha256={} golden_sha256={} golden_bytes={} golden_status=predicted_only text={:?}",
        case.label,
        case.source_hash,
        case.golden_hash,
        case.golden.len(),
        case.text
    );
    let outcome = ast::Input::new(case.text, case.map, case.exit).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query(
            "nested exits actual AST parent/child For, Guards, calls and caller identities",
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
            "SPEC0331 NESTED EXITS SSA case={} rooted={rooted}\n{}",
            case.label,
            crate::ssa::render::render_program(&program)
        );
        // 无单层/固定 read 数假设的源码 origin 与真实可达函数身份查询。
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("nested exits AST identity", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "nested exits actual entry and reachable source functions/deinits",
            &reachable,
        );
        // 查询失败只记 report；不能遮住真实 verifier/LLVM 的首断点。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("nested exits SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check("nested exits LLVM render/verify", true, llvm.len());
        diagnostic!(
            "SPEC0331 NESTED EXITS LLVM case={} rooted={rooted}\n{llvm}",
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
fn map_and_mutable_map_nested_inner_exits_reach_verified_llvm() {
    let mut observations = Vec::new();
    for case in sources() {
        for rooted in [false, true] {
            observations.push(observe(&case, rooted));
        }
    }
    for observation in &observations {
        diagnostic!(
            "SPEC0331 NESTED EXITS OUTCOME case={} rooted={} report={:#?}",
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
