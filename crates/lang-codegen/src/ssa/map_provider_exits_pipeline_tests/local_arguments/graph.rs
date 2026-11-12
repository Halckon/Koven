//! Actual selected SSA Call anchors all current output/argument owner families.
mod arguments {
    //! Ordinary source-order evaluators feed the exact declared argument slots.
    use super::*;
    use lang_frontend::{parser::LiteralKind, type_checking::ParameterMode};
    pub(super) struct Delivery<'ir> {
        pub call: &'ir Instruction,
        pub cleanup: Vec<&'ir Instruction>,
        pub argument_owners: BTreeSet<ValueId>,
    }
    pub(super) fn check<'ir>(
        sources: &SourceMap,
        g: &Graph<'ir>,
        module: &Module,
        factory: &Function,
        parsed: &ParsedFile,
        input: &ast::Input,
        uses: &mut Uses,
    ) -> Result<Delivery<'ir>, Failure> {
        let call = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation,Operation::DirectCall{callee,..}if callee==factory.id),
            ),
            "local argument selected factory called once",
        )?;
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        let formals = &factory
            .block(factory.entry_block().unwrap())
            .unwrap()
            .parameters;
        need(
            call.origin == Origin::Source(ast::span(parsed, input.call))
                && arguments.len() == input.arguments.len()
                && formals.len() == arguments.len() + usize::from(factory.receiver.is_some()),
            "local argument actual Callspan and full ABI",
            call,
        )?;
        let offset = usize::from(factory.receiver.is_some());
        let mut previous = None;
        let mut cleanup = Vec::new();
        let mut argument_owners = BTreeSet::new();
        for a in &input.arguments {
            let node = parsed.ast().expressions().get(a.expression).unwrap();
            let evaluate = match node.payload() {
                Expression::Call {
                    callee, arguments, ..
                } => {
                    need(
                        arguments.is_empty(),
                        "local argument evaluator is zero-argument Call",
                        node,
                    )?;
                    let name = ast::span(parsed, *callee);
                    let declaration=one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok()).filter(|n|matches!(n.payload(),lang_frontend::parser::Item::Function{name:lang_frontend::parser::NameMarker::Present(marker),..}if sources.slice(*marker)==sources.slice(name))),"local argument evaluator actual declaration")?;
                    let f = function(module, declaration.span())?;
                    one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::DirectCall{callee,receiver:None,ref arguments}if callee==f.id&&arguments.is_empty())&&i.origin==Origin::Source(node.span())),"local argument actual selected evaluator once")?
                }
                Expression::Literal(LiteralKind::Integer(_)) => at(g, node.span(), |o| {
                    *o == Operation::Constant(ScalarConstant::Integer(1))
                })?,
                _ => return Err(failure("local argument supported real evaluator", node)),
            };
            let owner = value(evaluate)?;
            let entity = EntityId::Value(owner);
            let ty = g.function.entity(entity).unwrap().ty.semantic_type();
            need(
                evaluate.block == call.block
                    && g.position(evaluate)? < g.position(call)?
                    && previous.is_none_or(|p| p < g.position(evaluate).unwrap())
                    && argument_owners.insert(owner),
                "local argument each evaluator once in source order",
                evaluate,
            )?;
            previous = Some(g.position(evaluate)?);
            uses.protect(entity);
            let actual = arguments[a.parameter];
            let formal = formals[a.parameter + offset];
            if input.factory.parameters[a.parameter].mode == ParameterMode::Value {
                need(
                    actual == entity && factory.entity(formal).unwrap().ty == EntityType::Value(ty),
                    "local argument exact declared Value slot",
                    (actual, formal),
                )?;
                uses.permit(entity, call);
            } else {
                let EntityId::Loan(current) = actual else {
                    return Err(failure("local argument Shared ABI loan", actual));
                };
                let begin = g.definition(actual)?;
                let Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                } = begin.operation
                else {
                    return Err(failure("local argument Shared Borrow birth", begin));
                };
                let root = g.definition(EntityId::Place(place))?;
                let end = one(
                    g.instructions
                        .iter()
                        .copied()
                        .filter(|i| i.operation == Operation::BorrowEnd { loan: current }),
                    "local argument unique synchronous End",
                )?;
                let formal_ty = EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                };
                need(
                    root.operation == Operation::RootPlace { owner }
                        && root.origin == Origin::Source(a.span)
                        && begin.origin == Origin::Source(node.span())
                        && end.origin == call.origin
                        && [root, begin, end].iter().all(|i| i.block == call.block)
                        && g.position(evaluate)? < g.position(root)?
                        && g.position(root)? < g.position(begin)?
                        && g.position(begin)? < g.position(call)?
                        && g.position(call)? < g.position(end)?
                        && g.function.entity(actual).unwrap().ty == formal_ty
                        && factory.entity(formal).unwrap().ty == formal_ty,
                    "local argument owner/Root/Shared/declared ABI/End",
                    (evaluate, root, begin, call, end),
                )?;
                uses.permit(entity, root);
                uses.protect(EntityId::Place(place));
                uses.permit(EntityId::Place(place), begin);
                uses.protect(actual);
                uses.permit(actual, call);
                uses.permit(actual, end);
                cleanup.push(end);
                let drops = g
                    .instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::Drop { owner })
                    .collect::<Vec<_>>();
                let scalar = matches!(
                    module.type_kind(ty),
                    Some(SsaTypeKind::Integer { .. } | SsaTypeKind::Boolean)
                );
                if scalar {
                    need(
                        drops.is_empty(),
                        "local argument Copyable Int has no owner Drop",
                        drops,
                    )?;
                } else {
                    let drop = one(
                        drops.into_iter(),
                        "local argument exact borrowed temporary Drop",
                    )?;
                    need(
                        drop.block == call.block
                            && drop.origin == Origin::Source(node.span())
                            && g.position(end)? < g.position(drop)?,
                        "local argument synchronous End before CallReturn Drop",
                        drop,
                    )?;
                    uses.permit(entity, drop);
                    cleanup.push(drop);
                }
            }
        }
        Ok(Delivery {
            call,
            cleanup,
            argument_owners,
        })
    }
}
mod cfg {
    //! The original shared argument-CFG proof, fed by actual local-source AST anchors.
    use super::*;
    use crate::ssa::map_provider_exits_pipeline_tests::temporary_argument_cfg_query::argument;
    pub(super) fn check<'ir>(
        sources: &SourceMap,
        parsed: &ParsedFile,
        input: &super::ast::Input,
        module: &Module,
        g: &Graph<'ir>,
        factory: &Function,
        uses: &mut Uses,
    ) -> Result<arguments::Delivery<'ir>, Failure> {
        let expression = input.arguments[0].expression;
        let node = parsed.ast().expressions().get(expression).unwrap();
        let Expression::If {
            condition,
            then_branch,
            else_branch: Some(no),
            ..
        } = node.payload()
        else {
            return Err(failure("local argument actual If argument", node));
        };
        let arm = |statement| -> Result<Span, Failure> {
            let lang_frontend::parser::Statement::ControlBody { elements } =
                parsed.ast().statements().get(statement).unwrap().payload()
            else {
                return Err(failure("local argument actual arm body", statement));
            };
            let s = one(
                elements.iter().copied(),
                "local argument sole actual arm expression",
            )?;
            let lang_frontend::parser::Statement::Expression { expression } =
                parsed.ast().statements().get(s).unwrap().payload()
            else {
                return Err(failure("local argument literal arm", s));
            };
            Ok(super::ast::span(parsed, *expression))
        };
        let original = argument::Input {
            call: super::ast::span(parsed, input.call),
            argument: node.span(),
            flag: super::ast::span(parsed, *condition),
            arms: [arm(*then_branch)?, arm(*no)?],
        };
        let (call, end) = argument::check_unit(sources, parsed, &original, module, g, factory, uses)?;
        Ok(arguments::Delivery {
            call,
            cleanup: vec![end],
            argument_owners: BTreeSet::new(),
        })
    }
}
mod provider {
    //! The hidden returned owner keeps exact three-column identity through every loop edge.
    use super::*;
    use crate::ssa::map_provider_exits_pipeline_tests::query_map_normal_body as body;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Roles {
        owner: ValueId,
        source: LoanId,
        cursor: ValueId,
    }
    impl Roles {
        fn entities(self) -> [EntityId; 3] {
            [
                EntityId::Value(self.owner),
                EntityId::Loan(self.source),
                EntityId::Value(self.cursor),
            ]
        }
        fn via(self, g: &Graph<'_>, edge: &Edge) -> Result<Self, Failure> {
            need(
                self.entities()
                    .iter()
                    .all(|e| edge.arguments.iter().filter(|v| *v == e).count() == 1),
                "delivery complete current three-column edge",
                (self, edge),
            )?;
            let EntityId::Value(owner) = g.transported(edge, self.entities()[0])? else {
                return Err(failure("delivery owner column", edge));
            };
            let EntityId::Loan(source) = g.transported(edge, self.entities()[1])? else {
                return Err(failure("delivery source column", edge));
            };
            let EntityId::Value(cursor) = g.transported(edge, self.entities()[2])? else {
                return Err(failure("delivery cursor column", edge));
            };
            Ok(Self {
                owner,
                source,
                cursor,
            })
        }
    }
    pub(super) fn check<'ir>(
        case: &Case,
        g: &Graph<'ir>,
        module: &'ir Module,
        reader: &'ir Function,
        parsed: &ParsedFile,
        selected: (&ast::Input, &arguments::Delivery<'ir>, Option<&Instruction>),
        uses: &mut Uses,
    ) -> Result<(), Failure> {
        let (site, delivered, after) = selected;
        let call = delivered.call;
        let owner = value(call)?;
        let map = g
            .function
            .entity(EntityId::Value(owner))
            .unwrap()
            .ty
            .semantic_type();
        let (_, key_type, value_type) = module
            .map_container(map)
            .ok_or_else(|| failure("delivery provider output Map", map))?;
        let root = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::RootPlace { owner }),
            "delivery actual returned owner root",
        )?;
        let EntityId::Place(place) = result(root)? else {
            return Err(failure("delivery actual provider root place", root));
        };
        let begin = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    }
            }),
            "delivery actual returned source loan",
        )?;
        let source = loan(begin)?;
        let first = at(g, site.span, |o| {
            matches!(o, Operation::MapIterationFirst { .. })
        })?;
        need(
            first.operation == Operation::MapIterationFirst { source }
                && [root, begin, first]
                    .iter()
                    .all(|i| i.block == call.block && i.origin == Origin::Source(site.span))
                && g.position(call)? < g.position(root)?
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(first)?
                && delivered.cleanup.iter().all(|drop| {
                    drop.block == root.block && g.position(drop).unwrap() < g.position(root).unwrap()
                }),
            "delivery all argument cleanup precedes output provider Root/Begin/First",
            (root, begin, first),
        )?;
        let initial = Roles {
            owner,
            source,
            cursor: value(first)?,
        };
        uses.permit(EntityId::Value(owner), root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        uses.permit(EntityId::Loan(source), first);
        let entry = g.branch(first.block)?;
        let header = initial.via(g, entry)?;
        let has = one(
            g.ops(entry.target)?.into_iter(),
            "delivery sole provider HasNext",
        )?;
        need(
            has.operation
                == Operation::MapIterationHasNext {
                    source: header.source,
                    cursor: header.cursor,
                }
                && has.origin == Origin::Source(site.span),
            "delivery current header source and cursor",
            has,
        )?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &g.term(has.block)?.kind
        else {
            return Err(failure("delivery actual HasNext branch", has));
        };
        need(
            *condition == value(has)?
                && g.term(has.block)?.origin == Origin::Source(site.span)
                && g.instructions.iter().all(|i| {
                    !i.operation
                        .entities()
                        .contains(&EntityId::Value(*condition))
                })
                && g.blocks.iter().all(|&block| {
                    !g.term(block)
                        .unwrap()
                        .kind
                        .entities()
                        .contains(&EntityId::Value(*condition))
                        || block == has.block
                            && !when_true.arguments.contains(&EntityId::Value(*condition))
                            && !when_false.arguments.contains(&EntityId::Value(*condition))
                }),
            "delivery HasNext result has only its real condition use",
            has,
        )?;
        let current = header.via(g, when_true)?;
        let exit = header.via(g, when_false)?;
        let next = body::check(
            g,
            reader,
            body::Body {
                block: when_true.target,
                span: site.span,
                read: ast::span(parsed, site.read),
                arguments: site.read_arguments.map(|id| ast::span(parsed, id)),
                types: [key_type, value_type],
            },
            current.source,
            current.cursor,
            uses,
        )?;
        let back = g.branch(when_true.target)?;
        need(
            back.target == has.block
                && Roles {
                    cursor: value(next)?,
                    ..current
                }
                .via(g, back)?
                    == header,
            "delivery complete actual normal backedge",
            back,
        )?;
        g.incoming_from(has.block, &[first.block, when_true.target])?;
        g.incoming_from(when_true.target, &[has.block])?;
        g.incoming_from(when_false.target, &[has.block])?;
        for frame in [initial, header, current, exit] {
            for entity in frame.entities() {
                uses.protect(entity);
            }
            anchored(g, EntityId::Value(frame.owner), &[EntityId::Value(owner)])?;
            anchored(g, EntityId::Loan(frame.source), &[EntityId::Loan(source)])?;
            anchored(
                g,
                EntityId::Value(frame.cursor),
                &[result(first)?, result(next)?],
            )?;
            need(
                !delivered.argument_owners.contains(&frame.owner)
                    && g.function.entity(EntityId::Value(frame.owner)).unwrap().ty
                        == EntityType::Value(map)
                    && g.function.entity(EntityId::Loan(frame.source)).unwrap().ty
                        == EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: map,
                        }
                    && matches!(g.function.entity(EntityId::Value(frame.cursor)).unwrap().ty,EntityType::Value(ty) if module.type_kind(ty)==Some(&SsaTypeKind::Integer {bits:64,signed:false})),
                "delivery every transported output role has exact type and disjoint identity",
                frame,
            )?;
        }
        uses.permit(EntityId::Loan(header.source), has);
        uses.permit(EntityId::Value(header.cursor), has);
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: exit.source }),
            "delivery output source unique End",
        )?;
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::Drop { owner: exit.owner }),
            "delivery current output unique Drop",
        )?;
        let ops = g.ops(when_false.target)?;
        need(
            end.block == when_false.target
                && drop.block == end.block
                && after.is_none_or(|after| after.block == end.block)
                && end.origin == Origin::Source(site.span)
                && drop.origin == Origin::Source(site.root.unwrap_or(ast::span(parsed, site.call)))
                && ops.first().map(|i| i.id) == Some(end.id)
                && g.position(end)? < g.position(drop)?
                && (if case.resource && !case.direct {
                    after.is_some_and(|after| g.position(after).unwrap() < g.position(drop).unwrap())
                } else {
                    ops.get(1).map(|i| i.id) == Some(drop.id)
                        && after
                            .is_none_or(|after| g.position(drop).unwrap() < g.position(after).unwrap())
                })
                && matches!(&g.term(end.block)?.kind,TerminatorKind::Return {values} if values.is_empty())
                && g.blocks
                    .iter()
                    .filter(|&&b| matches!(g.term(b).unwrap().kind, TerminatorKind::Return { .. }))
                    .count()
                    == 1,
            "delivery output End/current Drop/after/unique normal Return",
            (end, drop, after),
        )?;
        uses.permit(EntityId::Loan(exit.source), end);
        uses.permit(EntityId::Value(exit.owner), drop);
        let output_owners = [initial.owner, header.owner, current.owner, exit.owner];
        for (index, data) in g.function.values.iter().enumerate() {
            if module.map_container(data.ty.semantic_type()).is_some() {
                need(
                    output_owners.iter().any(|owner| owner.index() == index)
                        || delivered
                            .argument_owners
                            .iter()
                            .any(|owner| owner.index() == index),
                    "delivery provider has only the two proven disjoint owner families",
                    (index, data),
                )?;
            }
        }
        Ok(())
    }
}
mod receiver {
    //! Actual receiver allocation, resolved field projection and complete synchronous cleanup.
    use super::*;
    fn ast_call(
        parsed: &ParsedFile,
        id: lang_frontend::ast::ExpressionId,
    ) -> Result<(Span, &Expression), Failure> {
        let node = parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(|e| failure("ordinary original Call node", e))?;
        Ok((node.span(), node.payload()))
    }

    fn allocation<'a>(g: &Graph<'a>, span: Span) -> Result<&'a Instruction, Failure> {
        one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(span)
                    && matches!(i.operation, Operation::HeapAllocate { .. })
            }),
            "ordinary actual source receiver allocation",
        )
    }

    pub(super) fn check(
        g: &Graph<'_>,
        parsed: &ParsedFile,
        call: &Instruction,
        input: &ast::Input,
        proof: &facts::Proof,
        uses: &mut Uses,
    ) -> Result<(), Failure> {
        let Some(proof) = &proof.receiver else {
            return need(
                matches!(call.operation, Operation::DirectCall { receiver: None, .. }),
                "local argument free factory has no receiver",
                call,
            );
        };
        let original = parsed
            .ast()
            .expressions()
            .get(input.call)
            .unwrap()
            .payload();
        let Expression::Call { callee, .. } = original else {
            return Err(failure("local argument original member Call", original));
        };
        let Expression::Member { receiver, .. } =
            parsed.ast().expressions().get(*callee).unwrap().payload()
        else {
            return Err(failure("ordinary actual member receiver AST", callee));
        };
        let (receiver_span, constructor) = ast_call(parsed, *receiver)?;
        let alloc = allocation(g, receiver_span)?;
        let owner = value(alloc)?;
        let Operation::DirectCall {
            receiver: Some(EntityId::Loan(actual)),
            ..
        } = call.operation
        else {
            return Err(failure("ordinary actual shared receiver operand", call));
        };
        let mut receiver = actual;
        let mut ends = Vec::new();
        uses.protect(EntityId::Loan(actual));
        uses.permit(EntityId::Loan(actual), call);
        if let Some(expected_field) = proof.field {
            let projection = g.definition(EntityId::Loan(actual))?;
            let Operation::SharedHeapFieldLoan { base, field } = projection.operation else {
                return Err(failure(
                    "ordinary delegation really projects the heap field",
                    projection,
                ));
            };
            need(
                field == expected_field && projection.origin == Origin::Source(proof.begin),
                "ordinary resolved delegation field index",
                projection,
            )?;
            receiver = base;
            uses.protect(EntityId::Loan(base));
            uses.permit(EntityId::Loan(base), projection);
            let Expression::Call { arguments, .. } = constructor else {
                return Err(failure("ordinary Host constructor", constructor));
            };
            let [argument] = arguments.as_slice() else {
                return Err(failure(
                    "ordinary one delegate constructor argument",
                    arguments,
                ));
            };
            let (delegate_span, _) = ast_call(parsed, argument.value)?;
            let delegate = allocation(g, delegate_span)?;
            let Operation::HeapAllocate { payload, .. } = alloc.operation else {
                unreachable!()
            };
            let payload = g.definition(EntityId::Value(payload))?;
            need(
                matches!(&payload.operation, Operation::AggregateConstruct {fields, ..}
                if fields.as_slice() == [value(delegate)?]),
                "ordinary Host stores exact Factory owner in delegate field",
                payload,
            )?;
            need(
                g.function
                    .entity(EntityId::Loan(actual))
                    .unwrap()
                    .ty
                    .semantic_type()
                    == g.function
                        .entity(result(delegate)?)
                        .unwrap()
                        .ty
                        .semantic_type(),
                "ordinary projected receiver is actual Factory allocation type",
                projection,
            )?;
            uses.protect(result(delegate)?);
            uses.permit(result(delegate)?, payload);
            ends.push(actual);
        }
        let begin = g.definition(EntityId::Loan(receiver))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return Err(failure(
                "ordinary outer receiver real shared BorrowBegin",
                begin,
            ));
        };
        let root = g.definition(EntityId::Place(place))?;
        need(
            root.operation == Operation::RootPlace { owner }
                && root.origin == Origin::Source(proof.begin)
                && begin.origin == Origin::Source(proof.begin)
                && root.block == alloc.block
                && begin.block == alloc.block
                && call.block == alloc.block
                && g.position(alloc)? < g.position(root)?
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(call)?,
            "ordinary receiver borrows exact source construction",
            (alloc, root, begin, call),
        )?;
        uses.protect(EntityId::Value(owner));
        uses.permit(EntityId::Value(owner), root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        ends.push(receiver);
        let mut previous = call;
        for current in ends {
            let end = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: current }),
                "ordinary unique actual receiver End",
            )?;
            need(
                end.origin == Origin::Source(proof.end)
                    && end.block == call.block
                    && g.position(previous)? < g.position(end)?,
                "ordinary call then delegate End then outer End",
                (previous, end),
            )?;
            uses.permit(EntityId::Loan(current), end);
            previous = end;
        }
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::Drop { owner }),
            "ordinary unique temporary receiver Drop",
        )?;
        need(
            drop.block == call.block && g.position(previous)? < g.position(drop)?,
            "ordinary receiver Drop follows all receiver Ends",
            drop,
        )?;
        uses.permit(EntityId::Value(owner), drop);
        Ok(())
    }
}
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::{Case, ast, facts};
use crate::ssa::model::*;
use lang_frontend::{
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
};
use std::collections::BTreeSet;
fn at<'ir>(
    g: &Graph<'ir>,
    span: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> Result<&'ir Instruction, Failure> {
    one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.origin == Origin::Source(span) && predicate(&i.operation)),
        "local argument exact original SSA operation",
    )
}
fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut all = BTreeSet::from([anchor]);
    loop {
        let before = all.len();
        for (_, edge) in &g.edges {
            for (&a, &p) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if all.contains(&a) {
                    all.insert(p);
                }
            }
        }
        if all.len() == before {
            break;
        }
    }
    for &e in &all {
        g.anchored(e, &[anchor])?;
    }
    Ok(all)
}
fn anchored(g: &Graph<'_>, current: EntityId, anchors: &[EntityId]) -> Result<(), Failure> {
    g.anchored(current, anchors)
}
fn marker<'ir>(g: &Graph<'ir>, span: Span) -> Result<&'ir Instruction, Failure> {
    at(g, span, |o| match o {
        Operation::PrintLiteral { bytes } => bytes == b"after\n",
        Operation::PrintString { value } => {
            let Ok(begin) = g.definition(EntityId::Loan(*value)) else {
                return false;
            };
            let Operation::BorrowBegin { place, .. } = begin.operation else {
                return false;
            };
            let Ok(root) = g.definition(EntityId::Place(place)) else {
                return false;
            };
            let Operation::RootPlace { owner } = root.operation else {
                return false;
            };
            g.definition(EntityId::Value(owner)).is_ok_and(
                |i| matches!(&i.operation,Operation::StringLiteral{bytes,..}if bytes==b"after"),
            )
        }
        _ => false,
    })
}
pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    input: &ast::Input,
    proof: &facts::Proof,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "local argument original module")?;
    let entry = function(module, input.scope.span)?;
    let factory = function(module, proof.implementation)?;
    let g = Graph::with_abort(entry)?;
    let mut uses = Uses::default();
    let delivered = if input.arguments.iter().any(|a| {
        matches!(
            parsed
                .ast()
                .expressions()
                .get(a.expression)
                .unwrap()
                .payload(),
            Expression::If { .. }
        )
    }) {
        cfg::check(sources, parsed, input, module, &g, factory, &mut uses)?
    } else {
        arguments::check(sources, &g, module, factory, parsed, input, &mut uses)?
    };
    receiver::check(&g, parsed, delivered.call, input, proof, &mut uses)?;
    let owner = value(delivered.call)?;
    let map = entry
        .entity(EntityId::Value(owner))
        .unwrap()
        .ty
        .semantic_type();
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("local argument actual returned Map", map))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
            && if case.resource {
                matches!(module.type_kind(val), Some(SsaTypeKind::HeapOwner { .. }))
            } else {
                module.type_kind(val) == Some(&SsaTypeKind::StringOwner)
            }
            && factory.return_types.as_slice() == [map],
        "local argument exact selected map result ABI",
        factory.id,
    )?;
    if case.label.starts_with("generic-wrapper-") {
        super::generic::wrapper::ssa(case.mutable, sources, parsed, module, factory, map)?;
    }
    let after = input.after.map(|span| marker(&g, span)).transpose()?;
    provider::check(
        case,
        &g,
        module,
        function(module, input.reader.span)?,
        parsed,
        (input, &delivered, after),
        &mut uses,
    )?;
    let owners = family(&g, EntityId::Value(owner))?;
    for &e in &owners {
        uses.protect(e);
    }
    need(
        !delivered.argument_owners.contains(&owner),
        "local argument result owner disjoint from argument owners",
        owner,
    )?;
    let map_drops=g.instructions.iter().filter(|i|matches!(i.operation,Operation::Drop{owner}if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count();
    let input_maps = delivered
        .argument_owners
        .iter()
        .filter(|&&owner| {
            module
                .map_container(
                    entry
                        .entity(EntityId::Value(owner))
                        .unwrap()
                        .ty
                        .semantic_type(),
                )
                .is_some()
        })
        .count();
    need(
        map_drops == 1 + input_maps,
        "local argument exact disjoint input/output Map Drop families",
        map_drops,
    )?;
    let mut map_births = BTreeSet::from([EntityId::Value(owner)]);
    map_births.extend(
        delivered
            .argument_owners
            .iter()
            .copied()
            .filter_map(|owner| {
                let entity = EntityId::Value(owner);
                module
                    .map_container(entry.entity(entity).unwrap().ty.semantic_type())
                    .map(|_| entity)
            }),
    );
    crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
        &g,
        module,
        &map_births,
    )?;
    uses.check(&g, None)
}
