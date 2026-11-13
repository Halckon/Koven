//! 同根双层正常遍历两源 × MemoryDocument/rooted Unit-basic；四次 fresh 观察后统一硬成功。
//! Map Borrow 独立记账；golden 是预测，复杂 owner/exit/cleanup 先完整观察，不预填拓扑。
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast;
#[allow(unused_imports)]
pub(in crate::ssa) use query_ssa::query_body;
#[allow(unused_imports)]
pub(in crate::ssa) use query_ssa::query_callers;
mod query_cleanup {
    //! 独立核完整 normal actions 与 caller owner；unit 只消费自己的公开事实。
    use super::{
        super::{
            fixture::{Failure, failure, need},
            query_inputs::one,
        },
        ast::Site,
    };
    use lang_frontend::{
        name_resolution::UnitSymbolId,
        ownership_checking::{
            CompilationUnitOwnership, UnitDropPoint, UnitDropTarget,
            UnitIterationCleanupAction as UA, UnitIterationExitKind as UE,
        },
        source::Span,
        type_checking::{UnitExpressionId, UnitStatementId},
    };

    pub(super) fn unit_site(
        site: &Site,
        statement: UnitStatementId,
        source: UnitExpressionId,
        owned: &CompilationUnitOwnership,
    ) -> Result<(), Failure> {
        let plan = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("nested FE qualified cleanup plan", statement))?;
        let descriptor = plan.descriptor();
        let expected = [
            (
                UE::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(
                    statement.source_unit(),
                    site.body,
                )),
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
                vec![UA::FinishProvider(statement), UA::EndSource(statement)],
            ),
        ];
        need(
            plan.exits().len() == expected.len(),
            "nested FE exact two qualified normal exits",
            plan.exits(),
        )?;
        for (kind, point, actions) in expected {
            let exit = one(
                plan.exits().iter().filter(|e| e.kind() == kind),
                "nested FE unique qualified normal exit",
            )?;
            need(
                exit.point() == point && exit.actions() == actions,
                "nested FE qualified full ordered actions",
                (exit, &actions),
            )?;
            // 这是同表一致性；不假造 unit 未公开的 condition、owner 或 phi 模型。
            need(
                owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "nested FE qualified complete same-point cleanup consistency",
                (point, &actions),
            )?;
        }
        need(
            !owned
                .drops()
                .iter()
                .any(|d| d.target() == UnitDropTarget::Temporary(source)),
            "nested FE qualified borrowed source has no temporary Drop",
            source,
        )
    }

    pub(super) fn unit_caller(
        local: UnitSymbolId,
        marker: Span,
        size: UnitExpressionId,
        owned: &CompilationUnitOwnership,
    ) -> Result<Span, Failure> {
        let drop = one(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(local)),
            "nested FE unique qualified caller local Drop",
        )?;
        need(
            drop.point() == UnitDropPoint::CallReturn(size) && drop.value_origin() == marker,
            "nested FE qualified caller Drop follows actual size with local origin",
            (local, marker, size, drop),
        )?;
        Ok(drop.value_origin())
    }
}
mod query_graph {
    //! Nested-only role witness, beside the unchanged generic read-only graph helper.
    use super::super::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, inherits},
    };
    use crate::ssa::model::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(in crate::ssa) enum Role {
        Caller,
        OuterSource,
        OuterCursor,
        OuterKey,
        OuterValue,
        InnerSource,
        InnerCursor,
        InnerKey,
        InnerValue,
    }
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct Roles(pub BTreeMap<Role, EntityId>);
    impl Roles {
        pub fn get(&self, role: Role) -> Result<EntityId, Failure> {
            self.0
                .get(&role)
                .copied()
                .ok_or_else(|| failure("nested SSA missing current role", (role, self)))
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
            _ => Err(failure("nested SSA role Loan", entity)),
        }
    }
    pub(super) fn as_value(entity: EntityId) -> Result<ValueId, Failure> {
        match entity {
            EntityId::Value(v) => Ok(v),
            _ => Err(failure("nested SSA role Value", entity)),
        }
    }
    pub(super) fn ty(graph: &Graph<'_>, entity: EntityId) -> Result<EntityType, Failure> {
        graph
            .function
            .entity(entity)
            .map(|e| e.ty)
            .ok_or_else(|| failure("nested SSA entity type", entity))
    }
    pub(super) fn full_graph(graph: &Graph<'_>) -> Result<(), Failure> {
        let blocks = graph.blocks.iter().copied().collect::<BTreeSet<_>>();
        let instructions = graph
            .instructions
            .iter()
            .map(|i| i.id)
            .collect::<BTreeSet<_>>();
        need(
            blocks.len() == graph.function.blocks.len()
                && graph.function.blocks.iter().all(|b| blocks.contains(&b.id))
                && instructions.len() == graph.function.instructions.len()
                && graph
                    .function
                    .instructions
                    .iter()
                    .all(|i| instructions.contains(&i.id)),
            "nested SSA complete arenas equal referenced reachable graph",
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
    }
    impl Witness {
        pub fn protect(&mut self, roles: &Roles) -> Result<(), Failure> {
            for (&role, &entity) in &roles.0 {
                need(
                    self.aliases
                        .iter()
                        .all(|(&other, ids)| other == role || !ids.contains(&entity)),
                    "nested SSA distinct caller/lease/cursor/component role lineage",
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
                "nested SSA full exact current-role edge operands",
                (from, edge, roles),
            )?;
            need(
                self.edges
                    .insert((from, edge.target), edge.arguments.clone())
                    .is_none(),
                "nested SSA each actual edge proved once",
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
                "nested SSA every role parameter retains actual target block origin",
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
                                "nested SSA inherited role still dominates target",
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
                "nested SSA all actual edges covered",
                (&self.edges, &graph.edges),
            )?;
            for &(from, edge) in &graph.edges {
                need(
                    self.edges.get(&(from, edge.target)) == Some(&edge.arguments),
                    "nested SSA every edge exact proved operands",
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
                            "nested SSA actual condition distinct from transport roles",
                            term,
                        )?;
                    }
                    TerminatorKind::Branch(_) => {}
                    TerminatorKind::Return { values } => need(
                        values.is_empty(),
                        "nested SSA Unit return with no escaped role",
                        term,
                    )?,
                    _ => return Err(failure("nested SSA unexpected terminal", term)),
                }
            }
            self.uses.check(graph, None)
        }
    }
}
mod query_inputs;
pub(in crate::ssa) mod query_ssa;
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
            "nested SSA actual size instruction",
        )?;
        need(
            i.operation == Operation::MapSize { owner },
            "nested SSA size exact current source",
            i,
        )?;
        witness.uses.permit(owner, i);
        Ok(i)
    }

    pub(super) fn conditional(
        graph: &Graph<'_>,
        f: &Input,
        size: &Instruction,
        roles: &Roles,
        witness: &mut Witness,
    ) -> Result<(BlockId, Roles), Failure> {
        let span = size.origin.span();
        let control = one(
            f.parsed.ast().expressions().iter().filter_map(|(_, node)| {
                let Expression::If {
                    condition,
                    then_branch,
                    else_span: None,
                    else_branch: None,
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
                ))
            }),
            "nested SSA actual AST size If",
        )?;
        let term = graph.term(size.block)?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &term.kind
        else {
            return Err(failure("nested SSA size actual conditional", term));
        };
        let cmp = graph.definition(EntityId::Value(*condition))?;
        let Operation::Compare {
            operator: ComparisonOperator::Equal,
            left,
            right,
        } = cmp.operation
        else {
            return Err(failure("nested SSA size equality condition", cmp));
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
                        if matches!((f.sources.slice(constant.origin.span()), actual), (Ok("0"), 0) | (Ok("2"), 2))
                )
                && graph.position(size)? < graph.position(cmp)?
                && graph.position(constant)? < graph.position(cmp)?
                && f.sources
                    .slice(constant.origin.span())
                    .is_ok_and(|s| s == "0" || s == "2"),
            "nested SSA size actual comparison result drives its source If",
            (size, cmp, constant, term),
        )?;
        witness.conditions.insert(size.block, *condition);
        let then = when_true.target;
        let otherwise = when_false.target;
        need(
            then != otherwise && then != size.block && otherwise != size.block,
            "nested SSA distinct size If arms",
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
                && graph.block(otherwise)?.origin == Origin::Source(control.0),
            "nested SSA size arms actual AST origins",
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
            "nested SSA both actual size arms enter same join",
            (ea, eb),
        )?;
        graph.incoming_from(ea.target, &[then, otherwise])?;
        let joined = witness.edge(graph, then, ea, &a)?;
        let other_joined = witness.edge(graph, otherwise, eb, &b)?;
        need(
            joined == other_joined && graph.block(ea.target)?.origin == Origin::Source(control.0),
            "nested SSA all size join incoming preserve identical role columns",
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
                "nested SSA marker-only size branch",
                graph.ops(block)?,
            )?;
        }
        let _ = result(size)?;
        Ok((ea.target, joined))
    }
}
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

    fn observe(
        label: &'static str,
        map: bool,
        hash: &str,
        text: &str,
        rooted: bool,
    ) -> Observation {
        let mut report = Report::default();
        diagnostic!(
            "SPEC0331 NESTED IDENTITY INPUT case={label} rooted={rooted} path=provider.ko frozen_sha256={hash} text={text:?}"
        );
        let outcome = ast::Input::new(text, map).and_then(|input| {
            let anchors = ast::collect(&input);
            report.query(
                "nested identity actual parent/child same-root AST anchors",
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
                        .map_err(|e| failure("nested identity AST for FE", e))
                        .and_then(|a| query_inputs::unit(&input, a, names, typed, owned));
                    report.query(
                        "nested identity same-analysis FE source/caller/component/cleanup bridge",
                        &queried,
                    );
                    inputs = Some(queried);
                },
            )?;
            diagnostic!(
                "SPEC0331 NESTED IDENTITY SSA case={label} rooted={rooted}\n{}",
                crate::ssa::render::render_program(&program)
            );
            let reachable = anchors
                .as_ref()
                .map_err(|e| failure("nested identity actual AST roots", e))
                .and_then(|a| {
                    identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
                });
            report.query(
                "nested identity actual returned entry and reachable functions",
                &reachable,
            );
            let inputs = inputs.unwrap_or_else(|| {
                Err(failure(
                    "nested identity same-analysis FE observer",
                    "observer did not run",
                ))
            });
            let queried = inputs
                .as_ref()
                .map_err(|e| failure("nested identity FE bridge for SSA", e))
                .and_then(|inputs| query_ssa::check(&program, entry, &input, inputs));
            report.query(
                "nested identity two distinct leases all incoming current outer resume and complete uses",
                &queried,
            );
            // 不使用 query 的 ?：保持原 verifier/LLVM 的真实到达与首错误。
            crate::ssa::verify::verify_program(&program)
                .map_err(|e| verify_failure(&input.sources, e))?;
            report.check("nested identity original SSA verifier", true, entry);
            let llvm = crate::llvm::render_verified_program(&program)
                .map_err(|e| failure("LLVM render/verify", e))?;
            report.check(
                "nested identity original LLVM render/verify",
                true,
                llvm.len(),
            );
            diagnostic!("SPEC0331 NESTED IDENTITY LLVM case={label} rooted={rooted}\n{llvm}");
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
    fn nested_map_sources_preserve_caller_and_all_edge_identity() {
        let mut observations = Vec::new();
        for (label, map, hash, text, _golden) in super::sources() {
            for rooted in [false, true] {
                observations.push(observe(label, map, hash, text, rooted));
            }
        }
        for observation in &observations {
            diagnostic!(
                "SPEC0331 NESTED IDENTITY OUTCOME case={} rooted={} report={:#?}",
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
            "两份原源×MemoryDocument/rooted Unit四fresh、八site必须完成同次FE、完整validator、真实caller/两lease/四component全部incoming/uses、SSA和LLVM；\
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

pub(crate) fn sources() -> [(&'static str, bool, &'static str, &'static str, &'static str); 2] {
    [
        (
            "mutable-map-nested",
            false,
            "dd0c91d0e149c0b0e527fed025f5cc38c77914d3efef4f6da39f25b041568afd",
            include_str!("../../../tests/fixtures/map-provider-nested-0331/mutable-map-nested.ko"),
            include_str!(
                "../../../tests/fixtures/map-provider-nested-0331/mutable-map-nested.stdout"
            ),
        ),
        (
            "map-borrow-nested",
            true,
            "76b3a5ce538956558e4d89aeb6b290721cd7b60a45e31ba3ae960a4753f76082",
            include_str!("../../../tests/fixtures/map-provider-nested-0331/map-borrow-nested.ko"),
            include_str!(
                "../../../tests/fixtures/map-provider-nested-0331/map-borrow-nested.stdout"
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
        "SPEC0331 NESTED INPUT case={label} rooted={rooted} path=provider.ko frozen_sha256={source_hash} text={text:?}"
    );
    let outcome = ast::Input::new(text, map).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query(
            "nested actual parent/child For and same-source AST anchors",
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
            "SPEC0331 NESTED SSA case={label} rooted={rooted}\n{}",
            crate::ssa::render::render_program(&program)
        );
        // 只复用无单 For/Name/Guard 假设的入口与源码函数可达查询。
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("nested AST identity", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "nested actual entry and four reachable functions",
            &reachable,
        );
        // 只读查询失败留在 report，不隐藏真实 verifier/LLVM 的原错误。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("nested SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check("nested LLVM render/verify", true, llvm.len());
        diagnostic!("SPEC0331 NESTED LLVM case={label} rooted={rooted}\n{llvm}");
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
fn map_and_mutable_map_nested_borrow_sources_reach_verified_llvm() {
    let mut observations = Vec::new();
    for (label, map, hash, source, _golden) in sources() {
        for rooted in [false, true] {
            observations.push(observe(label, map, hash, source, rooted));
        }
    }
    for observation in &observations {
        diagnostic!(
            "SPEC0331 NESTED OUTCOME case={} rooted={} report={:#?}",
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
        "四格、八个真实For点必须完成同次frontend、完整validator/view、实际entry、SSA和LLVM；\
         Map Borrow与MutableMap分账；query失败不遮实际lower，UnsupportedNode不是成功oracle。\n{observations:#?}"
    );
}
