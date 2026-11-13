//! Fixed scope groups are registered only after their corresponding bounded repair.
mod int_storage;
pub(crate) use int_storage::sources as int_storage_sources;

mod borrow_alias_observer;

pub(crate) use borrow_alias_observer::cases as borrow_alias_sources;

mod conditional_owner_observer;
pub(crate) use conditional_owner_observer::cases as conditional_owner_sources;

mod owner_update_observer;
pub(crate) use owner_update_observer::cases as owner_update_sources;

pub(in crate::ssa) mod query_temporary_cleanup {
    //! The original temporary Call owns one LoopExit obligation in each public FE product.
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        ownership_checking::{
            CleanupCondition as C, CleanupOwnerValue as O, CleanupOwnerValueId,
            CompilationUnitOwnership, DropPoint, DropTarget, IterationCleanupAction as A,
            IterationExitKind as E, IterationPhiBoundary as B, IterationPhiIncomingKind as K,
            OwnershipCheckedFile, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
            UnitIterationExitKind as UE,
        },
        source::Span,
        type_checking::{UnitExpressionId, UnitStatementId},
    };

    pub(in crate::ssa) fn single(
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        call: Span,
        owned: &OwnershipCheckedFile,
    ) -> Result<CleanupOwnerValueId, Failure> {
        let p = owned.map_iteration(statement).ok_or_else(|| {
            failure(
                "temporary form actual cleanup plan",
                (statement, source, body, call),
            )
        })?;
        let drop = one(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == DropTarget::Temporary(source)),
            "temporary form unique original Call Drop",
        )?;
        let owner = drop
            .owner()
            .ok_or_else(|| failure("temporary form actual cleanup owner", drop))?;
        let table = owned.cleanup_conditions();
        need(
            drop.point() == DropPoint::LoopExit(statement)
                && drop.value_origin() == call
                && drop.condition().is_none()
                && drop.capture_slot().is_none()
                && drop.instance_address().is_none()
                && matches!(table.owner_value(owner), Some(O::Expression {expression, origin}) if *expression == source && *origin == call),
            "temporary form unique Call-origin owner and precise LoopExit",
            (drop, table.owner_value(owner)),
        )?;
        let d = p.descriptor();
        let expected = [
            (
                E::Fallthrough,
                DropPoint::AfterStatement(body),
                vec![
                    A::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    A::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                E::Exhaustion,
                DropPoint::LoopExit(statement),
                vec![
                    A::FinishProvider(statement),
                    A::EndSource(statement),
                    A::Drop(*drop),
                ],
            ),
        ];
        need(
            p.exits().len() == 2
                && p.closure_phis().is_empty()
                && p.closure_phi_incomings().len() == 3,
            "temporary form complete normal exits and hidden-owner incoming facts",
            p,
        )?;
        for (kind, point, actions) in expected {
            let exit = one(
                p.exits().iter().filter(|e| e.kind() == kind),
                "temporary form original normal exit",
            )?;
            let complete = owned
                .cleanup_steps()
                .iter()
                .filter_map(|(at, action)| (*at == point).then_some(*action))
                .collect::<Vec<_>>();
            need(
                exit.point() == point
                    && exit.actions() == actions
                    && complete == actions
                    && table.get(exit.condition()) == Some(&C::Always),
                "temporary form complete same-point ordered cleanup and Always",
                (exit, complete),
            )?;
        }
        for (kind, point, boundary) in [
            (K::Entry, DropPoint::AfterExpression(source), B::Header),
            (K::Fallthrough, DropPoint::AfterStatement(body), B::Header),
            (K::Exhaustion, DropPoint::LoopExit(statement), B::Exit),
        ] {
            let incoming = one(
                p.closure_phi_incomings()
                    .iter()
                    .filter(|e| e.kind() == kind),
                "temporary form complete actual incoming kind",
            )?;
            need(
                incoming.point() == point
                    && incoming.boundary() == boundary
                    && incoming.bindings().is_empty()
                    && table.get(incoming.condition()) == Some(&C::Always),
                "temporary form original incoming point/boundary/Always/empty bindings",
                incoming,
            )?;
        }
        Ok(owner)
    }

    pub(in crate::ssa) fn unit(
        body: UnitStatementId,
        call: Span,
        statement: UnitStatementId,
        source: UnitExpressionId,
        owned: &CompilationUnitOwnership,
    ) -> Result<(), Failure> {
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("temporary form qualified cleanup plan", statement))?;
        let d = p.descriptor();
        let drop = one(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Temporary(source)),
            "temporary form qualified unique original Call Drop",
        )?;
        need(
            drop.point() == UnitDropPoint::LoopExit(statement) && drop.value_origin() == call,
            "temporary form qualified Call-origin LoopExit",
            drop,
        )?;
        let expected = [
            (
                UE::Fallthrough,
                UnitDropPoint::AfterStatement(body),
                vec![
                    UA::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UA::EndBinding {
                        statement,
                        symbol: d.key_binding(),
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
            p.exits().len() == 2,
            "temporary form qualified complete normal exits",
            p,
        )?;
        for (kind, point, actions) in expected {
            let exit = one(
                p.exits().iter().filter(|e| e.kind() == kind),
                "temporary form actual qualified exit",
            )?;
            need(
                exit.point() == point
                    && exit.actions() == actions
                    && owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "temporary form qualified independent complete cleanup table",
                (exit, actions),
            )?;
        }
        Ok(())
    }
}

pub(in crate::ssa) mod query_map_normal_body {
    //! Shared K/V lineage, optional ordinary reborrows and complete body cleanup.
    use super::super::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, loan, result},
        query_inputs::one,
    };
    use crate::ssa::model::*;
    use lang_frontend::source::Span;
    use std::collections::{BTreeMap, BTreeSet};

    /// Original AST ranges and represented types for one normal Map body.
    #[derive(Clone, Copy, Debug)]
    pub(in crate::ssa) struct Body {
        pub block: BlockId,
        pub span: Span,
        pub read: Span,
        pub arguments: [Span; 2],
        pub types: [SsaTypeId; 2],
    }

    pub(in crate::ssa) fn check<'ir>(
        g: &Graph<'ir>,
        reader: &Function,
        site: Body,
        source: LoanId,
        cursor: ValueId,
        uses: &mut Uses,
    ) -> Result<&'ir Instruction, Failure> {
        let [key_type, value_type] = site.types;
        let ops = g.ops(site.block)?;
        let key = one(
            ops.iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. })),
            "temporary form actual body key loan",
        )?;
        let val = one(
            ops.iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
            "temporary form actual body value loan",
        )?;
        let next = one(
            ops.iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
            "temporary form actual body Next",
        )?;
        need(
            key.operation == Operation::MapIterationKeyLoan { source, cursor }
                && val.operation == Operation::MapIterationValueLoan { source, cursor }
                && next.operation == Operation::MapIterationNext { source, cursor }
                && [key, val, next]
                    .iter()
                    .all(|i| i.origin == Origin::Source(site.span)),
            "temporary form original body current source/cursor operations",
            (key, val, next),
        )?;
        let roots = [loan(key)?, loan(val)?];
        let reader_entry = reader.block(reader.entry_block().unwrap()).unwrap();
        need(
            reader.receiver.is_none()
                && reader.borrow_return.is_none()
                && reader.carrier_return.is_none()
                && reader.return_types.is_empty()
                && reader_entry.parameters.len() == 2
                && roots.iter().zip([key_type, value_type]).enumerate().all(
                    |(index, (&root, target))| {
                        let ty = EntityType::Loan {
                            kind: LoanKind::Shared,
                            target,
                        };
                        g.function.entity(EntityId::Loan(root)).unwrap().ty == ty
                            && reader.entity(reader_entry.parameters[index]).unwrap().ty == ty
                    },
                ),
            "temporary form reader Shared ABI preserves actual represented key/value",
            reader.id,
        )?;
        let mut lineage = BTreeMap::<LoanId, (usize, Option<LoanId>)>::new();
        let mut live = BTreeSet::new();
        let mut ended_roots = Vec::new();
        let mut calls = 0;
        for instruction in &ops {
            match &instruction.operation {
                Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. } => {
                    let current = loan(instruction)?;
                    let index = if current == roots[0] { 0 } else { 1 };
                    need(
                        roots.contains(&current)
                            && lineage.insert(current, (index, None)).is_none()
                            && live.insert(current),
                        "temporary form exact independent component birth",
                        instruction,
                    )?;
                    uses.protect(EntityId::Loan(current));
                    uses.permit(EntityId::Loan(source), instruction);
                    uses.permit(EntityId::Value(cursor), instruction);
                }
                Operation::SharedReborrow { source } => {
                    let (index, _) = *lineage.get(source).ok_or_else(|| {
                        failure("temporary form actual reborrow parent", instruction)
                    })?;
                    let current = loan(instruction)?;
                    need(
                        live.contains(source)
                            && instruction.origin == Origin::Source(site.arguments[index])
                            && g.function.entity(EntityId::Loan(current)).unwrap().ty
                                == g.function.entity(EntityId::Loan(*source)).unwrap().ty
                            && lineage.insert(current, (index, Some(*source))).is_none()
                            && live.insert(current),
                        "temporary form typed live Borrow descends from this original argument",
                        instruction,
                    )?;
                    uses.permit(EntityId::Loan(*source), instruction);
                    uses.protect(EntityId::Loan(current));
                }
                Operation::DirectCall {
                    callee,
                    receiver,
                    arguments,
                } => {
                    need(
                        *callee == reader.id
                            && receiver.is_none()
                            && arguments.len() == 2
                            && instruction.origin == Origin::Source(site.read)
                            && instruction.results.is_empty()
                            && calls == 0,
                        "temporary form actual original reader Call",
                        instruction,
                    )?;
                    for (index, &argument) in arguments.iter().enumerate() {
                        let EntityId::Loan(current) = argument else {
                            return Err(failure("temporary form actual Borrow operand", argument));
                        };
                        need(
                            live.contains(&current)
                                && lineage
                                    .get(&current)
                                    .is_some_and(|(root, _)| *root == index),
                            "temporary form actual reader ordered component lineage",
                            (index, argument),
                        )?;
                        uses.permit(argument, instruction);
                    }
                    calls += 1;
                }
                Operation::BorrowEnd { loan } => {
                    let (_, parent) = lineage.get(loan).copied().ok_or_else(|| {
                        failure("temporary form actual ended component", instruction)
                    })?;
                    need(
                        calls == 1
                            && live.contains(loan)
                            && !lineage.iter().any(|(child, (_, parent))| {
                                *parent == Some(*loan) && live.contains(child)
                            })
                            && instruction.origin
                                == Origin::Source(if parent.is_none() {
                                    site.span
                                } else {
                                    site.read
                                }),
                        "temporary form derived Borrow ends before parent after real read",
                        instruction,
                    )?;
                    live.remove(loan);
                    if parent.is_none() {
                        ended_roots.push(*loan);
                    }
                    uses.permit(EntityId::Loan(*loan), instruction);
                }
                Operation::MapIterationNext { .. } => {
                    need(
                        instruction.id == next.id
                            && calls == 1
                            && live.is_empty()
                            && ended_roots.as_slice() == [roots[1], roots[0]]
                            && g.function.entity(result(instruction)?).unwrap().ty
                                == g.function.entity(EntityId::Value(cursor)).unwrap().ty,
                        "temporary form read then derived Ends/VEnd/KEnd then real Next",
                        instruction,
                    )?;
                    uses.permit(EntityId::Loan(source), instruction);
                    uses.permit(EntityId::Value(cursor), instruction);
                    uses.protect(result(instruction)?);
                }
                _ => {
                    return Err(failure(
                        "temporary form unexpected body operation",
                        instruction,
                    ));
                }
            }
        }
        need(
            calls == 1
                && live.is_empty()
                && ended_roots.as_slice() == [roots[1], roots[0]]
                && ops.first().map(|i| i.id) == Some(key.id)
                && g.position(key)? < g.position(val)?
                && ops.last().map(|i| i.id) == Some(next.id)
                && g.edges.iter().all(|(_, edge)| {
                    edge.arguments.iter().all(
                    |entity| !matches!(entity, EntityId::Loan(loan) if lineage.contains_key(loan)),
                )
                }),
            "temporary form complete body and no component escape",
            (&ops, ended_roots),
        )?;
        Ok(next)
    }
}

pub(in crate::ssa) mod formation_runtime {
    //! Ordinary resource witnesses make source evaluation and cleanup visible without fault instrumentation.

    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
    #[derive(Clone, Copy, Debug)]
    pub(crate) enum History {
        ParameterMove,
        PrefixPut,
        AliasRebound,
        OwnerTransfer,
    }

    pub(crate) struct Case {
        pub label: String,
        pub source: String,
        pub golden: String,
        pub history: History,
        pub mutable: bool,
        pub populated: bool,
    }

    pub(crate) fn sources() -> Vec<Case> {
        let mut cases = Vec::new();
        for mutable in [false, true] {
            let kind = if mutable { "MutableMap" } else { "Map" };
            let finish = if mutable {
                "return result"
            } else {
                "val output=result.consume();return output"
            };
            for history in [
                History::ParameterMove,
                History::PrefixPut,
                History::AliasRebound,
                History::OwnerTransfer,
            ] {
                if matches!(history, History::PrefixPut) && !mutable {
                    continue;
                }
                for populated in [false, true] {
                    let prefix_put = matches!(history, History::PrefixPut);
                    let put = "result.put(\"key\".clone(),R(\"old\".clone()));";
                    let factory_put = if populated && !prefix_put { put } else { "" };
                    let common = format!(
                        "class R(val name:String){{deinit(){{println(\"drop\");println(name)}}}}\n\
                     class Guard{{deinit(){{println(\"guard\")}}}}\n\
                     fun make():{kind}<String,R>{{println(\"factory\");var result=mutableMapOf<String,R>();{factory_put}{finish}}}\n\
                     fun other():{kind}<String,R>{{println(\"other-factory\");var result=mutableMapOf<String,R>();result.put(\"new-key\".clone(),R(\"new\".clone()));{finish}}}\n\
                     fun read(key:String,value:R):Unit{{println(key);println(value.name)}}\n\
                     fun readMap(source:{kind}<String,R>):Unit{{if(source.size==1){{println(\"other-one\")}}else{{error(\"wrong-other\")}}}}\n\
                     fun sink(own value:{kind}<String,R>):Unit{{println(\"sink\");if(value.size==0){{println(\"empty\")}}else{{if(value.size==1){{println(\"one\")}}else{{error(\"wrong-size\")}}}}}}\n"
                    );
                    let (parameters, formation, after, entry) = match history {
                        History::ParameterMove => (
                            format!("own incoming:{kind}<String,R>"),
                            "val source=incoming;".to_owned(),
                            "println(\"after\");",
                            "val input=make();scan(input)",
                        ),
                        History::PrefixPut => (
                            String::new(),
                            format!(
                                "var source=make();{}",
                                if populated {
                                    "source.put(\"key\".clone(),R(\"old\".clone()));"
                                } else {
                                    ""
                                }
                            ),
                            "println(\"after\");",
                            "scan()",
                        ),
                        History::AliasRebound => (
                            String::new(),
                            "var old=make();val source=old;old=other();".to_owned(),
                            "readMap(old);println(\"after\");",
                            "scan()",
                        ),
                        History::OwnerTransfer => (
                            format!("own source:{kind}<String,R>"),
                            String::new(),
                            "sink(source);println(\"after\");",
                            "val input=make();scan(input)",
                        ),
                    };
                    let source = format!(
                        "{common}fun scan({parameters}):Unit{{{formation}println(\"gap\");for((key,value) in source){{val guard=Guard();read(key,value)}};{after}}}\n\
                     fun entry():Unit{{{entry};println(\"done\")}}\n"
                    );
                    let mut golden = "factory\n".to_owned();
                    if matches!(history, History::AliasRebound) {
                        golden.push_str("other-factory\n");
                    }
                    golden.push_str("gap\n");
                    if populated {
                        golden.push_str("key\nold\nguard\n");
                    }
                    if matches!(history, History::OwnerTransfer) {
                        golden.push_str(if populated {
                            "sink\none\n"
                        } else {
                            "sink\nempty\n"
                        });
                        if populated {
                            golden.push_str("drop\nold\n");
                        }
                        golden.push_str("after\n");
                    } else {
                        if matches!(history, History::AliasRebound) {
                            golden.push_str("other-one\n");
                        }
                        golden.push_str("after\n");
                        if populated {
                            golden.push_str("drop\nold\n");
                        }
                        if matches!(history, History::AliasRebound) {
                            golden.push_str("drop\nnew\n");
                        }
                    }
                    golden.push_str("done\n");
                    cases.push(Case {
                        label: format!(
                            "formation-resource-{kind}-{history:?}-{}",
                            if populated { "populated" } else { "empty" }
                        ),
                        source,
                        golden,
                        history,
                        mutable,
                        populated,
                    });
                }
            }
        }
        assert_eq!(cases.len(), 14);
        cases
    }

    #[test]
    fn ordinary_resource_formations_reach_verified_llvm() {
        use super::super::{
            Report,
            fixture::{Failure, clean, failure},
            local_exits::formation,
            pipeline,
        };
        use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
        let mut outcomes = vec![];
        for case in sources() {
            for rooted in [false, true] {
                diagnostic!(
                    "FORMATION RESOURCE INPUT label={} rooted={rooted} axes={:?} source={:?} golden={:?}",
                    case.label,
                    (case.history, case.mutable, case.populated),
                    case.source,
                    case.golden
                );
                let observed = (|| -> Result<(), Failure> {
                    let mut sources = SourceMap::new();
                    let id = sources
                        .add_source("provider.ko", &case.source)
                        .map_err(|e| failure("source", e))?;
                    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
                    clean(&sources, "lex", lexed.diagnostics())?;
                    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
                    clean(&sources, "parse", parsed.diagnostics())?;
                    let mut report = Report::default();
                    let mut identity = None;
                    let result = pipeline::unit_input_observed(
                        &sources,
                        &parsed,
                        "provider.ko",
                        &mut report,
                        rooted,
                        |names, typed, owned, report| {
                            let original = formation::unit(&sources, &parsed, names, typed, owned);
                            report.query("formation runtime qualified original facts", &original);
                            identity = Some(original);
                        },
                    );
                    if let Ok((program, _)) = &result {
                        if let Some(Ok(input)) = &identity {
                            report.query(
                                "formation runtime actual exact owner/provider graph",
                                &formation::check(&sources, &parsed, program, input),
                            );
                        }
                        report.query(
                            "formation runtime actual SSA verifier",
                            &crate::ssa::verify::verify_program(program)
                                .map_err(|e| failure("SSA", e)),
                        );
                        report.query(
                            "formation runtime actual LLVM renderer",
                            &crate::llvm::render_verified_program(program)
                                .map(|_| ())
                                .map_err(|e| failure("LLVM", e)),
                        );
                    }
                    diagnostic!(
                        "FORMATION RESOURCE OBSERVED label={} rooted={rooted} report={report:#?} result={:?}",
                        case.label,
                        result.as_ref().map(|(_, entry)| entry)
                    );
                    if !report.passed() {
                        return Err(failure("formation runtime independent queries", report));
                    }
                    result?;
                    Ok(())
                })();
                diagnostic!(
                    "FORMATION RESOURCE CASE label={} rooted={rooted} result={observed:?}",
                    case.label
                );
                outcomes.push(observed);
            }
        }
        assert_eq!(outcomes.len(), 28);
        assert_diagnostic!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
    }
}

pub(in crate::ssa) mod query_cleanup {
    //! 只读清理次序与实体消费；本夹具无条件直线 body，不接纳额外派生 loan。
    use super::super::{
        fixture::{Exit, Failure, failure, need},
        query_graph::{Graph, Uses, function, loan, value},
        query_inputs::{Inputs, one},
    };
    use crate::ssa::model::*;

    pub(in crate::ssa) fn end_source(
        graph: &Graph<'_>,
        input: &Inputs,
        block: BlockId,
        source: EntityId,
        seed: Option<EntityId>,
        uses: &mut Uses,
    ) -> Result<Vec<InstructionId>, Failure> {
        let end = one(graph.instructions.iter().copied().filter(|i| {
        matches!(i.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == source)
    }), "identity SSA unique path EndSource")?;
        need(
            end.block == block
                && [input.site.span, input.site.transfer_span].contains(&end.origin.span()),
            "identity SSA EndSource current path lease",
            end,
        )?;
        uses.permit(source, end);
        let mut sequence = vec![end.id];
        if input.owned {
            let Some(EntityId::Value(owner)) = seed else {
                return Err(failure("identity SSA path current owner", seed));
            };
            let drop = one(
                graph
                    .instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::Drop { owner }),
                "identity SSA unique path source Drop",
            )?;
            need(
                drop.block == block
                    && Some(drop.origin.span()) == input.source_drop
                    && graph.position(drop)? == graph.position(end)? + 1,
                "identity SSA path EndSource immediately precedes current owner Drop",
                (end, drop),
            )?;
            uses.permit(EntityId::Value(owner), drop);
            sequence.push(drop.id);
        }
        Ok(sequence)
    }

    pub(in crate::ssa) fn body(
        graph: &Graph<'_>,
        module: &Module,
        input: &Inputs,
        components: [&Instruction; 2],
        tail: &[InstructionId],
        uses: &mut Uses,
    ) -> Result<Option<ValueId>, Failure> {
        let [key, val] = components;
        let k = loan(key)?;
        let v = loan(val)?;
        let block = key.block;
        let consumer = function(module, input.consumer)?;
        let deinit = function(module, input.deinit)?;
        let ops = graph.ops(block)?;
        let allocation = one(
            ops.iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::HeapAllocate { .. })),
            "identity SSA real Guard allocation",
        )?;
        let Operation::HeapAllocate {
            owner: guard_type,
            payload,
        } = allocation.operation
        else {
            return Err(failure("identity SSA Guard allocation shape", allocation));
        };
        need(
            allocation.origin == Origin::Source(input.site.guard_span)
                && module.deinits.get(&guard_type) == Some(&deinit.id),
            "identity SSA Guard allocation type/deinit",
            allocation,
        )?;
        let payload = graph.definition(EntityId::Value(payload))?;
        need(
            payload.block == block
                && payload.origin == Origin::Source(input.site.guard_span)
                && matches!(&payload.operation, Operation::AggregateConstruct { fields, .. } if fields.is_empty()),
            "identity SSA actual empty Guard payload construction",
            payload,
        )?;
        let guard_owner = value(allocation)?;
        let guard_drop = one(
            graph
                .instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::Drop { owner: guard_owner }),
            "identity SSA unique Guard owner Drop",
        )?;
        need(
            guard_drop.block == block && guard_drop.origin == Origin::Source(input.site.local),
            "identity SSA lexical Guard Drop origin distinct from constructor",
            guard_drop,
        )?;
        uses.permit(EntityId::Value(guard_owner), guard_drop);
        let call = one(graph.instructions.iter().copied().filter(|i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == consumer.id)),
        "identity SSA unique real consumer call")?;
        need(
            call.block == block
                && call.origin == Origin::Source(input.site.call_span)
                && matches!(&call.operation, Operation::DirectCall { arguments, .. } if arguments.as_slice() == [EntityId::Loan(k), EntityId::Loan(v)]),
            "identity SSA exact ordered K/V call operands",
            call,
        )?;
        let entry = consumer
            .entry_block()
            .and_then(|id| consumer.block(id))
            .ok_or_else(|| failure("identity SSA actual consumer signature", consumer.id))?;
        need(
            entry.parameters.len() == 2
                && consumer.borrow_return.is_none()
                && consumer.receiver.is_none(),
            "identity SSA consumer parameter slots",
            consumer,
        )?;
        for (&parameter, component) in entry.parameters.iter().zip([k, v]) {
            need(
                consumer.entity(parameter).map(|d| d.ty)
                    == graph
                        .function
                        .entity(EntityId::Loan(component))
                        .map(|d| d.ty),
                "identity SSA call component/parameter type identity",
                (parameter, component),
            )?;
        }
        let mut ordered = vec![key.id, val.id, payload.id, allocation.id, call.id];
        let mut returned = None;
        if input.exit == Exit::Return {
            let initial = value(call)?;
            let ty = graph
                .function
                .entity(EntityId::Value(initial))
                .ok_or_else(|| failure("identity SSA operand result", initial))?
                .ty;
            need(
                matches!(ty, EntityType::Value(t) if module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 32, signed: true })
            && consumer.return_types.as_slice() == [t] && graph.function.return_types.as_slice() == [t]),
                "identity SSA actual Int operand/result signature",
                ty,
            )?;
            let mut cursor = initial;
            uses.protect(EntityId::Value(initial));
            // Only an actual forward Copy chain after the call and before cleanup is accepted.
            let call_position = graph.position(call)?;
            for i in ops
                .iter()
                .skip(call_position + 1)
                .take_while(|i| matches!(i.operation, Operation::Copy { .. }))
            {
                need(
                    i.operation == Operation::Copy { source: cursor },
                    "identity SSA Return Copy chain has actual call root",
                    i,
                )?;
                uses.permit(EntityId::Value(cursor), i);
                cursor = value(i)?;
                uses.protect(EntityId::Value(cursor));
                ordered.push(i.id);
            }
            let term = graph.term(block)?;
            need(
                term.origin == Origin::Source(input.site.transfer_span)
                    && matches!(&term.kind, TerminatorKind::Return { values } if values.as_slice() == [cursor]),
                "identity SSA actual operand result returned after cleanup",
                term,
            )?;
            returned = Some(cursor);
        } else {
            need(
                call.results.is_empty()
                    && consumer.return_types.is_empty()
                    && graph.function.return_types.is_empty(),
                "identity SSA actual Unit consumer result",
                call,
            )?;
        }
        ordered.push(guard_drop.id);
        for component in [v, k] {
            let end = one(
                graph
                    .instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: component }),
                "identity SSA unique component EndBinding",
            )?;
            need(
                end.block == block
                    && [input.site.span, input.site.transfer_span].contains(&end.origin.span()),
                "identity SSA component ends at actual transfer",
                end,
            )?;
            uses.permit(EntityId::Loan(component), call);
            uses.permit(EntityId::Loan(component), end);
            ordered.push(end.id);
        }
        ordered.extend_from_slice(tail);
        need(
            ops.iter().map(|i| i.id).collect::<Vec<_>>() == ordered,
            "identity SSA full body order: acquire, operand, Guard, value/key, terminal cleanup or Next",
            (&ops, ordered),
        )?;
        Ok(returned)
    }
}
