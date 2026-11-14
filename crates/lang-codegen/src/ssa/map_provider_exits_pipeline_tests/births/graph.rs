//! Source-born SSA owner anchors, complete edge transport and all protected operand uses.
mod callable {
    //! Stateless lambda address and its real thunk return supply this CallableInvoke owner.
    use super::*;
    use crate::ssa::map_provider_exits_pipeline_tests::temporary_arguments;
    pub(super) fn check(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        selected: (&ast::Input, &super::super::facts::Proof),
        module: &Module,
        g: &Graph<'_>,
        uses: &mut Uses,
    ) -> Result<EntityId, Failure> {
        let (input, proof) = selected;
        let lambda = ast::span(parsed, input.lambda.unwrap());
        let address = at(g, lambda, |o| {
            matches!(o, Operation::FunctionAddress { .. })
        })?;
        let Operation::FunctionAddress { target } = address.operation else {
            unreachable!()
        };
        let thunk = module
            .functions
            .iter()
            .find(|f| f.id == target)
            .ok_or_else(|| failure("birth actual FunctionAddress thunk", address))?;
        need(
            thunk.origin == Origin::Source(lambda)
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::FunctionAddress { .. }))
                    .count()
                    == 1,
            "birth address selects the actual original noncapturing lambda thunk",
            address,
        )?;
        let invoke = at(g, ast::span(parsed, input.birth), |o| {
            matches!(o, Operation::CallableInvoke { .. })
        })?;
        let Operation::CallableInvoke {
            callable,
            arguments,
        } = &invoke.operation
        else {
            unreachable!()
        };
        need(
            arguments.is_empty()
                && address.block == invoke.block
                && g.position(address)? < g.position(invoke)?,
            "birth actual zeroarg FunctionValue invocation after lambda address",
            invoke,
        )?;
        let mut lineage = BTreeSet::from([result(address)?]);
        uses.protect(result(address)?);
        for op in &g.instructions {
            if let Operation::Copy { source } = op.operation
                && lineage.contains(&EntityId::Value(source))
            {
                need(
                    op.origin == Origin::Source(ast::span(parsed, input.callee.unwrap()))
                        && op.block == invoke.block
                        && g.position(op)? < g.position(invoke)?
                        && g.function.entity(result(op)?).unwrap().ty
                            == g.function.entity(result(address)?).unwrap().ty,
                    "birth actual callable copy descends from original lambda",
                    op,
                )?;
                uses.permit(EntityId::Value(source), op);
                uses.protect(result(op)?);
                lineage.insert(result(op)?);
            }
        }
        need(
            lineage.contains(&EntityId::Value(*callable)),
            "birth CallableInvoke uses the exact original callable value",
            invoke,
        )?;
        uses.permit(EntityId::Value(*callable), invoke);
        let fact = proof
            .callable_drop
            .ok_or_else(|| failure("birth same-analysis callable Drop fact", proof))?;
        let original = value(address)?;
        let drop=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::Drop{owner}if lineage.contains(&EntityId::Value(owner)))),"birth unique actual callable-family Drop")?;
        need(
            drop.operation == Operation::Drop { owner: original }
                && drop.origin == Origin::Source(fact.value_origin())
                && fact.value_origin() == input.factory.unwrap()
                && drop.block == invoke.block
                && g.position(drop)? == g.position(invoke)? + 1,
            "birth exact FE callable Drop consumes original address after copied invocation",
            (fact, address, invoke, drop),
        )?;
        uses.permit(result(address)?, drop);

        // Noncapturing function pointers are Copyable; no hidden owned closure may
        // substitute a capture allocation or consume a different callable family.
        need(
            !g.instructions
                .iter()
                .any(|op| matches!(op.operation, Operation::ClosureConstruct { .. })),
            "birth original empty-capture lambda uses no hidden environment",
            lambda,
        )?;
        thunk_check(
            case,
            sources,
            parsed,
            input,
            module,
            thunk,
            g.function
                .entity(result(invoke)?)
                .unwrap()
                .ty
                .semantic_type(),
        )?;
        result(invoke)
    }
    fn thunk_check(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        input: &ast::Input,
        module: &Module,
        thunk: &Function,
        map: SsaTypeId,
    ) -> Result<(), Failure> {
        let actual_factory =
            temporary_arguments::ast::function(sources, parsed, "produce", Some("Int"))?;
        let actual_argument =
            temporary_arguments::ast::function(sources, parsed, "argument", None)?;
        let factory = function(module, actual_factory.span)?;
        let argument = function(module, actual_argument.span)?;
        let g = Graph::new(thunk)?;
        let evaluate = at(
            &g,
            ast::span(parsed, input.argument.unwrap()),
            |o| matches!(o,Operation::DirectCall{callee,receiver:None,arguments}if *callee==argument.id&&arguments.is_empty()),
        )?;
        let call = at(
            &g,
            ast::span(parsed, input.produce.unwrap()),
            |o| matches!(o,Operation::DirectCall{callee,receiver:None,..}if *callee==factory.id),
        )?;
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        let [EntityId::Loan(actual)] = arguments.as_slice() else {
            return Err(failure(
                "birth thunk actual sole Borrow Int ABI argument",
                call,
            ));
        };
        let begin = g.definition(EntityId::Loan(*actual))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return Err(failure("birth thunk actual Shared argument", begin));
        };
        let root = g.definition(EntityId::Place(place))?;
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: *actual }),
            "birth thunk unique synchronous argument End",
        )?;
        let int = thunk.entity(result(evaluate)?).unwrap().ty.semantic_type();
        need(
            root.operation
                == Operation::RootPlace {
                    owner: value(evaluate)?,
                }
                && [evaluate, root, begin, call, end]
                    .iter()
                    .all(|i| i.block == call.block)
                && evaluate.origin == root.origin
                && root.origin == begin.origin
                && end.origin == call.origin
                && g.position(evaluate)? < g.position(root)?
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(call)?
                && g.position(call)? < g.position(end)?
                && module.type_kind(int)
                    == Some(&SsaTypeKind::Integer {
                        bits: 32,
                        signed: true,
                    })
                && argument.return_types.as_slice() == [int]
                && factory.return_types.as_slice() == [map]
                && thunk.return_types.as_slice() == [map]
                && factory
                    .block(factory.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .len()
                    == 1
                && factory
                    .entity(
                        factory
                            .block(factory.entry_block().unwrap())
                            .unwrap()
                            .parameters[0],
                    )
                    .unwrap()
                    .ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: int,
                    },
            "birth thunk actual evaluator/Root/Shared/selected factory/CallReturn End",
            (evaluate, root, begin, call, end),
        )?;
        need(
            g.blocks.len() == 1
                && g.edges.is_empty()
                && g.block(thunk.entry_block().unwrap())?.parameters.is_empty()
                && matches!(&g.term(call.block)?.kind,TerminatorKind::Return{values}if values.as_slice()==[value(call)?])
                && g.term(call.block)?.origin
                    == Origin::Source(ast::span(parsed, input.lambda.unwrap())),
            "birth actual lambda normal Return delivers same selected factory owner",
            thunk.id,
        )?;
        let mut uses = Uses::default();
        for e in [
            result(evaluate)?,
            EntityId::Place(place),
            EntityId::Loan(*actual),
            result(call)?,
        ] {
            uses.protect(e);
        }
        uses.permit(result(evaluate)?, root);
        uses.permit(EntityId::Place(place), begin);
        uses.permit(EntityId::Loan(*actual), call);
        uses.permit(EntityId::Loan(*actual), end);
        uses.check(&g, Some((call.block, value(call)?)))?;
        let original = temporary_arguments::sources()
            .into_iter()
            .find(|c| c.mutable == case.mutable)
            .unwrap();
        temporary_arguments::graph::factory::check(&original, sources, parsed, module, factory, map)
    }
}
mod control {
    //! Exhaustive actual CFG order proves cleanup dominates every later marker and Return.
    use super::*;
    pub(super) fn check(
        g: &Graph<'_>,
        seed: EntityId,
        size: &Instruction,
        drop: &Instruction,
        after: &Instruction,
    ) -> Result<(), Failure> {
        let birth = g.definition(seed)?;
        need(
            birth.block == size.block
                && size.block == drop.block
                && g.position(birth)? < g.position(size)?
                && g.position(drop)? == g.position(size)? + 1,
            "birth control actual producer/size/immediate cleanup in preheader",
            (birth, size, drop),
        )?;
        let mut pending = vec![(g.function.entry_block().unwrap(), 0u8, 0u8, 0u8, 0u8)];
        let mut seen = BTreeSet::new();
        let mut returned = 0;
        let mut aborted = 0;
        while let Some((block, mut made, mut read, mut freed, mut marked)) = pending.pop() {
            if !seen.insert((block, made, read, freed, marked)) {
                continue;
            }
            for op in g.ops(block)? {
                if op.id == birth.id {
                    made += 1;
                    need(
                        (made, read, freed, marked) == (1, 0, 0, 0),
                        "birth control actual result created once before any use",
                        op,
                    )?;
                }
                if op.id == size.id {
                    read += 1;
                    need(
                        (made, read, freed, marked) == (1, 1, 0, 0),
                        "birth control size reads actual live result once",
                        op,
                    )?;
                }
                if op.id == drop.id {
                    freed += 1;
                    need(
                        (made, read, freed, marked) == (1, 1, 1, 0),
                        "birth control unique Drop follows size on every path",
                        op,
                    )?;
                }
                if op.id == after.id {
                    marked += 1;
                    need(
                        (made, read, freed, marked) == (1, 1, 1, 1),
                        "birth control actual cleanup dominates every after marker path",
                        op,
                    )?;
                }
            }
            match &g.term(block)?.kind {
                TerminatorKind::Return { values } => {
                    returned += 1;
                    need(
                        values.is_empty() && (made, read, freed, marked) == (1, 1, 1, 1),
                        "birth every actual normal Return follows one size/cleanup/marker",
                        g.term(block)?,
                    )?;
                }
                TerminatorKind::Abort => {
                    aborted += 1;
                    need(
                        (made, read, freed, marked) == (1, 1, 1, 0),
                        "birth no-For failure edge starts after cleanup and before after",
                        g.term(block)?,
                    )?;
                }
                _ => {}
            }
            for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
                pending.push((edge.target, made, read, freed, marked));
            }
        }
        need(
            returned == 1 && aborted == 1,
            "birth control actual successful and bad-size paths both accounted",
            (returned, aborted),
        )
    }
}
mod intrinsic {
    //! MapNew→optional Put→optional Consume supplies the actual current provider owner.
    use super::*;
    pub(super) fn check<'ir>(
        case: &Case,
        parsed: &ParsedFile,
        input: &ast::Input,
        module: &Module,
        g: &Graph<'ir>,
        uses: &mut Uses,
    ) -> Result<(EntityId, BTreeSet<EntityId>), Failure> {
        let construct = at(g, ast::span(parsed, input.construction.unwrap()), |o| {
            matches!(o, Operation::MapConstruct { .. })
        })?;
        let initial = result(construct)?;
        let Operation::MapConstruct { map_type: mutable } = construct.operation else {
            unreachable!()
        };
        let (kind, key, val) = module
            .map_container(mutable)
            .ok_or_else(|| failure("birth actual construction map storage", construct))?;
        need(
            kind == if case.mutable || case.kind == Kind::Consume {
                MapContainerKind::MutableMap
            } else {
                MapContainerKind::Map
            } && [key, val]
                .iter()
                .all(|ty| module.type_kind(*ty) == Some(&SsaTypeKind::StringOwner))
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
                    .count()
                    == 1,
            "birth exact actual MapNew kind/StringKV and once",
            construct,
        )?;
        let mut current = initial;
        let mut previous = construct;
        let mut prior = BTreeSet::new();
        uses.protect(current);
        if let Some(put) = input.put {
            let put = at(g, ast::span(parsed, put), |o| {
                matches!(o, Operation::MapPut { .. })
            })?;
            let Operation::MapPut { owner, .. } = put.operation else {
                unreachable!()
            };
            need(
                EntityId::Value(owner) == current
                    && put.block == previous.block
                    && g.position(previous)? < g.position(put)?
                    && g.function.entity(result(put)?).unwrap().ty
                        == g.function.entity(initial).unwrap().ty
                    && g.instructions
                        .iter()
                        .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
                        .count()
                        == 1,
                "birth actual Put consumes current constructed owner and returns next owner",
                put,
            )?;
            uses.permit(current, put);
            prior.insert(current);
            current = result(put)?;
            uses.protect(current);
            previous = put;
        } else {
            need(
                !g.instructions
                    .iter()
                    .any(|i| matches!(i.operation, Operation::MapPut { .. })),
                "birth empty map has no fabricated put",
                construct,
            )?;
        }
        if case.kind == Kind::Consume {
            let consume = at(g, ast::span(parsed, input.birth), |o| {
                matches!(o, Operation::MapConsume { .. })
            })?;
            let Operation::MapConsume { owner, map_type } = consume.operation else {
                unreachable!()
            };
            need(
                EntityId::Value(owner) == current
                    && module.map_container(map_type) == Some((MapContainerKind::Map, key, val))
                    && consume.block == previous.block
                    && g.position(previous)? < g.position(consume)?
                    && g.instructions
                        .iter()
                        .filter(|i| matches!(i.operation, Operation::MapConsume { .. }))
                        .count()
                        == 1,
                "birth actual consume transfers latest populated mutable owner",
                consume,
            )?;
            uses.permit(current, consume);
            prior.insert(current);
            current = result(consume)?;
            uses.protect(current);
        } else {
            need(
                !g.instructions
                    .iter()
                    .any(|i| matches!(i.operation, Operation::MapConsume { .. })),
                "birth mutable/new output has no substitute consume",
                input,
            )?;
        }
        need(
            g.instructions.iter().all(|i| {
                !matches!(
                    i.operation,
                    Operation::DirectCall { .. }
                        | Operation::CallableInvoke { .. }
                        | Operation::BorrowCall { .. }
                ) || i.origin
                    == Origin::Source(
                        input
                            .site
                            .as_ref()
                            .map_or(input.after, |s| ast::span(parsed, s.read)),
                    )
            }),
            "birth intrinsic owner is never supplied by an alternate factory",
            input.birth,
        )?;
        Ok((current, prior))
    }
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod provider {
    //! The actual output owner and lease keep exact identities through every provider edge.
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
                edge.arguments.len() == 3
                    && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                        == self.entities().into_iter().collect(),
                "birth complete current three-column provider edge",
                (self, edge),
            )?;
            let EntityId::Value(owner) = g.transported(edge, self.entities()[0])? else {
                return Err(failure("birth actual owner column", edge));
            };
            let EntityId::Loan(source) = g.transported(edge, self.entities()[1])? else {
                return Err(failure("birth actual source column", edge));
            };
            let EntityId::Value(cursor) = g.transported(edge, self.entities()[2])? else {
                return Err(failure("birth actual cursor column", edge));
            };
            Ok(Self {
                owner,
                source,
                cursor,
            })
        }
        /// The exhaustion edge: the refinement layer drops the dead cursor there, so only
        /// owner and source are carried; the header cursor stands for the absent column
        /// (SPEC-0394). The per-family path carries all three.
        fn via_exit(self, g: &Graph<'_>, edge: &Edge) -> Result<Self, Failure> {
            if edge.arguments.len() == 3 {
                return self.via(g, edge);
            }
            let [owner, source, _] = self.entities();
            need(
                edge.arguments.len() == 2
                    && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                        == [owner, source].into_iter().collect(),
                "birth exhaustion edge carries exactly current owner/source",
                (self, edge),
            )?;
            let EntityId::Value(owner) = g.transported(edge, owner)? else {
                return Err(failure("birth actual owner column", edge));
            };
            let EntityId::Loan(source) = g.transported(edge, source)? else {
                return Err(failure("birth actual source column", edge));
            };
            Ok(Self {
                owner,
                source,
                cursor: self.cursor,
            })
        }
    }
    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn check(
        g: &Graph<'_>,
        module: &Module,
        reader: &Function,
        parsed: &ParsedFile,
        selected: (&ast::Site, EntityId, &Instruction, &Instruction),
        uses: &mut Uses,
    ) -> Result<(), Failure> {
        let (site, seed, drop, after) = selected;
        let EntityId::Value(owner) = seed else {
            return Err(failure(
                "birth provider source must be an owner Value",
                seed,
            ));
        };
        let map = g.function.entity(seed).unwrap().ty.semantic_type();
        let (_, key, val) = module
            .map_container(map)
            .ok_or_else(|| failure("birth provider actual map storage", seed))?;
        let root = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::RootPlace { owner }),
            "birth provider roots the actual current seed",
        )?;
        let EntityId::Place(place) = result(root)? else {
            return Err(failure("birth provider actual root Place", root));
        };
        let begin = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    }
            }),
            "birth actual provider Shared lease",
        )?;
        let source = loan(begin)?;
        let first = at(g, site.span, |o| {
            matches!(o, Operation::MapIterationFirst { .. })
        })?;
        let seed_definition = g.definition(seed)?;
        need(
            root.origin == Origin::Source(site.span)
                && begin.origin == root.origin
                && first.origin == root.origin
                && first.operation == Operation::MapIterationFirst { source }
                && [seed_definition, root, begin]
                    .iter()
                    .all(|i| i.block == first.block)
                && g.position(seed_definition)? < g.position(root)?
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(first)?,
            "birth actual producer/current Root/Shared/First sequence",
            (seed_definition, root, begin, first),
        )?;
        uses.permit(seed, root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        uses.permit(EntityId::Loan(source), first);
        let initial = Roles {
            owner,
            source,
            cursor: value(first)?,
        };
        let entry = g.branch(first.block)?;
        let header = initial.via(g, entry)?;
        let has = one(
            g.ops(entry.target)?.into_iter(),
            "birth header sole HasNext",
        )?;
        need(
            has.operation
                == Operation::MapIterationHasNext {
                    source: header.source,
                    cursor: header.cursor,
                }
                && has.origin == Origin::Source(site.span),
            "birth actual current header operands",
            has,
        )?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &g.term(has.block)?.kind
        else {
            return Err(failure("birth actual HasNext Conditional", has));
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
                    block == has.block
                        || !g
                            .term(block)
                            .unwrap()
                            .kind
                            .entities()
                            .contains(&EntityId::Value(*condition))
                })
                && !when_true.arguments.contains(&EntityId::Value(*condition))
                && !when_false.arguments.contains(&EntityId::Value(*condition)),
            "birth HasNext Boolean has only its actual guard use",
            has,
        )?;
        let current = header.via(g, when_true)?;
        let exit = header.via_exit(g, when_false)?;
        let next = body::check(
            g,
            reader,
            body::Body {
                block: when_true.target,
                span: site.span,
                read: ast::span(parsed, site.read),
                arguments: site.arguments.map(|id| ast::span(parsed, id)),
                types: [key, val],
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
            "birth complete actual current backedge",
            back,
        )?;
        g.incoming_from(has.block, &[first.block, when_true.target])?;
        g.incoming_from(when_true.target, &[has.block])?;
        g.incoming_from(when_false.target, &[has.block])?;
        for roles in [initial, header, current, exit] {
            for e in roles.entities() {
                uses.protect(e);
            }
            g.anchored(EntityId::Value(roles.owner), &[seed])?;
            g.anchored(EntityId::Loan(roles.source), &[EntityId::Loan(source)])?;
            g.anchored(
                EntityId::Value(roles.cursor),
                &[result(first)?, result(next)?],
            )?;
            need(
                g.function.entity(EntityId::Value(roles.owner)).unwrap().ty
                    == EntityType::Value(map)
                    && g.function.entity(EntityId::Loan(roles.source)).unwrap().ty
                        == EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: map,
                        },
                "birth every owner/source role retains exact type",
                roles,
            )?;
        }
        uses.permit(EntityId::Loan(header.source), has);
        uses.permit(EntityId::Value(header.cursor), has);
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: exit.source }),
            "birth unique current source End",
        )?;
        let ops = g.ops(when_false.target)?;
        need(
            end.block == when_false.target
                && end.origin == Origin::Source(site.span)
                && drop.block == end.block
                && after.block == end.block
                && drop.operation == Operation::Drop { owner: exit.owner }
                && ops.first().map(|i| i.id) == Some(end.id)
                && ops.get(1).map(|i| i.id) == Some(drop.id)
                && g.position(drop)? < g.position(after)?
                && matches!(&g.term(end.block)?.kind,TerminatorKind::Return{values}if values.is_empty()),
            "birth source End/current output Drop/after/normal Return exact order",
            (end, drop, after),
        )?;
        uses.permit(EntityId::Loan(exit.source), end);
        uses.permit(EntityId::Value(exit.owner), drop);
        Ok(())
    }
}
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::{Case, Kind, ast};
use crate::ssa::model::*;
use lang_frontend::{
    parser::ParsedFile,
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
        "birth exact original SSA operation",
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
        if before == all.len() {
            break;
        }
    }
    for &e in &all {
        g.anchored(e, &[anchor])?;
    }
    Ok(all)
}
fn after<'ir>(g: &Graph<'ir>, span: Span) -> Result<&'ir Instruction, Failure> {
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
    proof: &super::facts::Proof,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "birth actual original module")?;
    let entry = function(module, input.entry)?;
    let g = Graph::with_abort(entry)?;
    let mut uses = Uses::default();
    need(
        g.block(entry.entry_block().unwrap())?.parameters.is_empty(),
        "birth source entry has no substitute owner parameter",
        entry.id,
    )?;
    let (seed, prior) = if case.kind == Kind::FunctionValue {
        (
            callable::check(case, sources, parsed, (input, proof), module, &g, &mut uses)?,
            BTreeSet::new(),
        )
    } else {
        intrinsic::check(case, parsed, input, module, &g, &mut uses)?
    };
    let map = entry.entity(seed).unwrap().ty.semantic_type();
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("birth actual represented output owner", seed))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && [key, val]
            .iter()
            .all(|ty| module.type_kind(*ty) == Some(&SsaTypeKind::StringOwner)),
        "birth exact output kind/KV storage",
        map,
    )?;
    let owners = family(&g, seed)?;
    for &e in &owners {
        uses.protect(e);
    }
    let marker = after(&g, input.after)?;
    let drop=one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop{owner}if owners.contains(&EntityId::Value(owner)))),"birth unique output family cleanup")?;
    let Operation::Drop { owner } = drop.operation else {
        unreachable!()
    };
    need(
        drop.origin == Origin::Source(input.root.unwrap_or(ast::span(parsed, input.birth))),
        "birth actual named/temporary Drop source identity",
        drop,
    )?;
    if let Some(site) = &input.site {
        provider::check(
            &g,
            module,
            function(module, input.reader)?,
            parsed,
            (site, seed, drop, marker),
            &mut uses,
        )?;
    } else {
        let size = at(&g, ast::span(parsed, input.size.unwrap()), |o| {
            matches!(o, Operation::MapSize { .. })
        })?;
        need(
            size.operation == Operation::MapSize { owner: seed }
                && drop.operation
                    == Operation::Drop {
                        owner: match seed {
                            EntityId::Value(v) => v,
                            _ => return Err(failure("birth actual owner Value", seed)),
                        },
                    }
                && size.block == drop.block
                && g.position(drop)? == g.position(size)? + 1
                && !g.instructions.iter().any(|i| {
                    matches!(
                        i.operation,
                        Operation::MapIterationFirst { .. }
                            | Operation::MapIterationHasNext { .. }
                            | Operation::MapIterationNext { .. }
                            | Operation::MapIterationKeyLoan { .. }
                            | Operation::MapIterationValueLoan { .. }
                    )
                }),
            "birth no-For actual owner size/ASAP Drop before after",
            (size, drop, marker),
        )?;
        uses.permit(seed, size);
        control::check(&g, seed, size, drop, marker)?;
    }
    uses.permit(EntityId::Value(owner), drop);
    for (index, data) in entry.values.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            let e = EntityId::Value(ValueId {
                function: entry.id,
                index,
            });
            need(
                owners.contains(&e) || prior.contains(&e),
                "birth every Map Value belongs to actual current or consumed predecessor family",
                (e, data),
            )?;
        }
    }
    let all_drops=g.instructions.iter().filter(|i|matches!(i.operation,Operation::Drop{owner}if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count();
    need(
        all_drops == 1,
        "birth sole Map cleanup, no old construction/put/consume duplicate",
        all_drops,
    )?;
    let births = prior.iter().copied().chain([seed]).collect::<BTreeSet<_>>();
    crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
        &g, module, &births,
    )?;
    uses.check(&g, None)
}
