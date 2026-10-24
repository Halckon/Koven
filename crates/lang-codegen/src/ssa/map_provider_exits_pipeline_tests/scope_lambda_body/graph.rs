//! Actual address selects the true thunk; its internal Map has an independent owner lifetime.
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod factory;
use super::super::{
    births,
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, result, value},
    query_inputs::one,
};
use super::{Case, ast, facts::Proof};
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
            .filter(|op| op.origin == Origin::Source(span) && predicate(&op.operation)),
        "A2 exact original SSA operation",
    )
}
fn family(g: &Graph<'_>, seed: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut all = BTreeSet::from([seed]);
    loop {
        let n = all.len();
        for (_, edge) in &g.edges {
            for (&a, &p) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if all.contains(&a) {
                    all.insert(p);
                }
            }
        }
        if n == all.len() {
            break;
        }
    }
    for &e in &all {
        g.anchored(e, &[seed])?;
    }
    Ok(all)
}
fn done<'ir>(
    g: &Graph<'ir>,
    parsed: &ParsedFile,
    i: &ast::Input,
) -> Result<&'ir Instruction, Failure> {
    at(g, ast::span(parsed, i.done), |op| match op {
        Operation::PrintLiteral { bytes } => bytes == b"done\n",
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
                |op| matches!(&op.operation,Operation::StringLiteral{bytes,..}if bytes==b"done"),
            )
        }
        _ => false,
    })
}
pub(super) fn check(
    c: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    i: &ast::Input,
    proof: &Proof,
    program: &Program,
) -> Result<(), Failure> {
    need(
        c.provider == i.site.is_some(),
        "A2 exact original For/control body distinction",
        c.label,
    )?;
    let module = one(program.modules.iter(), "A2 actual module")?;
    let entry = function(module, i.entry.span)?;
    let scan = function(module, i.scan.span)?;
    let eg = Graph::new(entry)?;
    let scanner = at(&eg, ast::span(parsed, i.scan_call), |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    need(
        scanner.operation
            == Operation::DirectCall {
                callee: scan.id,
                receiver: None,
                arguments: vec![],
            }
            && scanner.results.is_empty()
            && eg.blocks.len() == 1
            && matches!(&eg.term(scanner.block)?.kind,TerminatorKind::Return{values}if values.is_empty()),
        "A2 original entry selects actual zeroarg Unit scan",
        scanner,
    )?;
    let outer = Graph::new(scan)?;
    let address = at(&outer, ast::span(parsed, i.lambda), |op| {
        matches!(op, Operation::FunctionAddress { .. })
    })?;
    let Operation::FunctionAddress { target } = address.operation else {
        unreachable!()
    };
    let thunk = one(
        module.functions.iter().filter(|f| f.id == target),
        "A2 actual FunctionAddress target thunk",
    )?;
    let lambda = parsed.ast().expressions().get(i.lambda).unwrap();
    let Expression::Lambda { body, .. } = lambda.payload() else {
        unreachable!()
    };
    need(
        *body == i.body
            && matches!(
                parsed.ast().statements().get(*body).unwrap().payload(),
                lang_frontend::parser::Statement::LambdaBody { .. }
            )
            && thunk.origin == Origin::Source(lambda.span())
            && thunk.origin != scan.origin
            && thunk.receiver.is_none()
            && thunk.return_types.is_empty()
            && thunk.borrow_return.is_none()
            && thunk.carrier_return.is_none()
            && thunk
                .block(thunk.entry_block().unwrap())
                .unwrap()
                .parameters
                .is_empty(),
        "A2 true original LambdaBody thunk has empty environment and Unit ABI",
        thunk,
    )?;
    let invoke = at(&outer, ast::span(parsed, i.invoke), |op| {
        matches!(op, Operation::CallableInvoke { .. })
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
            && invoke.results.is_empty()
            && outer.blocks.len() == 1
            && outer.edges.is_empty()
            && outer.function.return_types.is_empty()
            && outer
                .block(scan.entry_block().unwrap())?
                .parameters
                .is_empty()
            && outer.position(address)? < outer.position(invoke)?
            && matches!(module.type_kind(scan.entity(result(address)?).unwrap().ty.semantic_type()),Some(SsaTypeKind::FunctionPointer{signature})if signature.parameters.is_empty()&&signature.returns.is_empty()),
        "A2 actual noncapturing pointer address and void Invoke ABI",
        (address, invoke),
    )?;
    let mut uses = Uses::default();
    let mut lineage = BTreeSet::from([result(address)?]);
    uses.protect(result(address)?);
    for op in &outer.instructions {
        if let Operation::Copy { source } = op.operation
            && lineage.contains(&EntityId::Value(source))
        {
            need(
                op.origin == Origin::Source(ast::span(parsed, i.callee))
                    && op.block == invoke.block
                    && outer.position(op)? < outer.position(invoke)?
                    && scan.entity(result(op)?).unwrap().ty
                        == scan.entity(result(address)?).unwrap().ty,
                "A2 actual callable read copies this original callback",
                op,
            )?;
            uses.permit(EntityId::Value(source), op);
            uses.protect(result(op)?);
            lineage.insert(result(op)?);
        }
    }
    need(
        lineage.contains(&EntityId::Value(*callable)),
        "A2 Invoke consumes actual current callback lineage",
        invoke,
    )?;
    uses.permit(EntityId::Value(*callable), invoke);
    let cleanup=one(outer.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::Drop{owner}if lineage.contains(&EntityId::Value(owner)))),"A2 unique actual original callback Drop")?;
    need(
        cleanup.operation
            == Operation::Drop {
                owner: value(address)?,
            }
            && cleanup.origin == Origin::Source(proof.callback_drop)
            && proof.callback_drop == i.callback
            && cleanup.block == invoke.block
            && outer.position(cleanup)? == outer.position(invoke)? + 1,
        "A2 exact FE callable Drop closes original address after actual Invoke",
        cleanup,
    )?;
    uses.permit(result(address)?, cleanup);
    need(
        outer
            .instructions
            .iter()
            .filter(|op| matches!(op.operation, Operation::FunctionAddress { .. }))
            .count()
            == 1
            && outer
                .instructions
                .iter()
                .filter(|op| matches!(op.operation, Operation::CallableInvoke { .. }))
                .count()
                == 1
            && !outer
                .instructions
                .iter()
                .any(|op| matches!(op.operation, Operation::ClosureConstruct { .. }))
            && matches!(&outer.term(invoke.block)?.kind,TerminatorKind::Return{values}if values.is_empty()),
        "A2 no capture allocation or substitute invocation/result owner",
        scan.id,
    )?;
    uses.check(&outer, None)?;
    let g = Graph::new(thunk)?;
    let birth = at(&g, i.birth_span, |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    let make = function(module, i.make.span)?;
    need(
        birth.operation
            == Operation::DirectCall {
                callee: make.id,
                receiver: None,
                arguments: vec![],
            },
        "A2 internal actual named Map birth is exact make Call",
        birth,
    )?;
    let seed = result(birth)?;
    let map = thunk.entity(seed).unwrap().ty.semantic_type();
    need(
        module.map_container(map).is_some_and(|(kind, key, val)| {
            kind == MapContainerKind::Map
                && [key, val]
                    .iter()
                    .all(|t| module.type_kind(*t) == Some(&SsaTypeKind::StringOwner))
        }),
        "A2 actual concrete internal Map String/String owner",
        map,
    )?;
    factory::check(
        sources,
        parsed,
        &factory::Input {
            make: &i.make,
            produce: &i.produce,
        },
        module,
        map,
    )?;
    let owners = family(&g, seed)?;
    let mut uses = Uses::default();
    for &e in &owners {
        uses.protect(e);
    }
    let drop=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::Drop{owner}if owners.contains(&EntityId::Value(owner)))),"A2 sole internal current Map owner cleanup")?;
    need(
        drop.origin == Origin::Source(proof.source_drop) && proof.source_drop == i.source,
        "A2 source-qualified exact lambda-local Drop fact identity",
        drop,
    )?;
    let done = done(&g, parsed, i)?;
    if let Some(site) = &i.site {
        births::graph::provider::check(
            &g,
            module,
            function(module, i.reader)?,
            parsed,
            (site, seed, drop, done),
            &mut uses,
        )?;
    } else {
        control(parsed, i, module, &g, (birth, drop, done), &mut uses)?;
    }
    let Operation::Drop { owner } = drop.operation else {
        unreachable!()
    };
    uses.permit(EntityId::Value(owner), drop);
    for (index, data) in thunk.values.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            need(
                owners.contains(&EntityId::Value(ValueId {
                    function: thunk.id,
                    index,
                })),
                "A2 every Map Value is actual current internal owner family",
                (index, data),
            )?;
        }
    }
    need(g.instructions.iter().filter(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(thunk.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count()==1&&!g.instructions.iter().any(|op|matches!(op.operation,Operation::MapConstruct{..}|Operation::MapPut{..}|Operation::MapConsume{..}|Operation::ClosureConstruct{..}))&&g.blocks.iter().all(|&block|match &g.term(block).unwrap().kind{TerminatorKind::Return{values}=>values.is_empty()&&g.term(block).unwrap().origin==Origin::Source(lambda.span()),_=>true}),"A2 sole current cleanup and true lambda Unit Return, no alternate Map birth",thunk.id)?;
    crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
        &g,
        module,
        &BTreeSet::from([seed]),
    )?;
    uses.check(&g, None)
}
fn control(
    parsed: &ParsedFile,
    i: &ast::Input,
    module: &Module,
    g: &Graph<'_>,
    selected: (&Instruction, &Instruction, &Instruction),
    uses: &mut Uses,
) -> Result<(), Failure> {
    let (birth, drop, done) = selected;
    let source = value(birth)?;
    let call_id = i.observe_call.unwrap();
    let call = at(g, ast::span(parsed, call_id), |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    let Operation::DirectCall {
        callee,
        receiver: None,
        arguments,
    } = &call.operation
    else {
        return Err(failure("A2 control actual observe ABI", call));
    };
    let [argument] = arguments.as_slice() else {
        return Err(failure("A2 control actual sole Borrow argument", call));
    };
    let begin = g.definition(*argument)?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("A2 control actual current Map borrow", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Expression::Call {
        arguments: ast_arguments,
        ..
    } = parsed.ast().expressions().get(call_id).unwrap().payload()
    else {
        unreachable!()
    };
    let arg_span = ast::span(parsed, ast_arguments[0].value);
    let end=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==*argument)),"A2 control unique actual argument End")?;
    need(
        *callee == function(module, i.observe)?.id
            && call.results.is_empty()
            && root.operation == Operation::RootPlace { owner: source }
            && root.origin == Origin::Source(arg_span)
            && begin.origin == root.origin
            && end.origin == call.origin
            && drop.operation == Operation::Drop { owner: source }
            && [root, begin, call, end, drop, done]
                .iter()
                .all(|op| op.block == birth.block)
            && g.blocks.len() == 1
            && g.edges.is_empty()
            && g.position(birth)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(call)?
            && g.position(call)? < g.position(end)?
            && g.position(drop)? == g.position(end)? + 1
            && g.position(drop)? < g.position(done)?
            && !g.instructions.iter().any(|op| {
                matches!(
                    op.operation,
                    Operation::MapIterationFirst { .. }
                        | Operation::MapIterationHasNext { .. }
                        | Operation::MapIterationNext { .. }
                        | Operation::MapIterationKeyLoan { .. }
                        | Operation::MapIterationValueLoan { .. }
                )
            }),
        "A2 actual control birth/Root/Borrow/observe/End/current Drop/done exact order",
        (birth, root, begin, call, end, drop, done),
    )?;
    uses.permit(EntityId::Value(source), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.protect(*argument);
    uses.permit(*argument, call);
    uses.permit(*argument, end);
    Ok(())
}
