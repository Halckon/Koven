//! Actual address selects the true thunk; its internal Map has an independent owner lifetime.
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod factory {
    //! The internal make() result returns the same fully populated factory owner.
    use super::super::super::temporary_arguments;
    use super::*;
    pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Input<'a> {
        pub make: &'a temporary_arguments::ast::Function,
        pub produce: &'a temporary_arguments::ast::Function,
    }
    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn check(
        sources: &SourceMap,
        parsed: &ParsedFile,
        i: &Input<'_>,
        module: &Module,
        map: SsaTypeId,
    ) -> Result<(), Failure> {
        let make = function(module, i.make.span)?;
        let factory = function(module, i.produce.span)?;
        let argument_ast = temporary_arguments::ast::function(sources, parsed, "argument", None)?;
        let evaluator = function(module, argument_ast.span)?;
        let g = Graph::new(make)?;
        let call_expr = |wanted| {
            one(
                parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter_map(|(id, n)| match n.payload() {
                        Expression::Call { callee, .. }
                            if ast::inside(i.make.span, n.span())
                                && sources.slice(ast::span(parsed, *callee)) == Ok(wanted) =>
                        {
                            Some(id)
                        }
                        _ => None,
                    }),
                "A2 actual make body source Call",
            )
        };
        let produce_id = call_expr("produce")?;
        let argument_id = call_expr("argument")?;
        let argument = at(&g, ast::span(parsed, argument_id), |op| {
            matches!(op, Operation::DirectCall { .. })
        })?;
        let call = at(&g, ast::span(parsed, produce_id), |op| {
            matches!(op, Operation::DirectCall { .. })
        })?;
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments,
        } = &call.operation
        else {
            return Err(failure("A2 actual selected factory ABI", call));
        };
        let [EntityId::Loan(actual)] = arguments.as_slice() else {
            return Err(failure("A2 actual original Borrow Int slot", call));
        };
        let begin = g.definition(EntityId::Loan(*actual))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return Err(failure("A2 actual argument Shared birth", begin));
        };
        let root = g.definition(EntityId::Place(place))?;
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::BorrowEnd { loan: *actual }),
            "A2 actual synchronous factory argument End",
        )?;
        need(
            argument.operation
                == Operation::DirectCall {
                    callee: evaluator.id,
                    receiver: None,
                    arguments: vec![],
                }
                && *callee == factory.id
                && root.operation
                    == Operation::RootPlace {
                        owner: value(argument)?,
                    }
                && root.origin == argument.origin
                && begin.origin == argument.origin
                && end.origin == call.origin
                && g.blocks.len() == 1
                && g.edges.is_empty()
                && g.position(argument)? < g.position(root)?
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(call)?
                && g.position(call)? < g.position(end)?
                && make.return_types.as_slice() == [map]
                && factory.return_types.as_slice() == [map]
                && matches!(&g.term(call.block)?.kind,TerminatorKind::Return{values}if values.as_slice()==[value(call)?]),
            "A2 exact evaluator/Root/Borrow/factory/End/owned result Return",
            (argument, root, begin, call, end),
        )?;
        let mut uses = Uses::default();
        for e in [
            result(argument)?,
            EntityId::Place(place),
            EntityId::Loan(*actual),
            result(call)?,
        ] {
            uses.protect(e);
        }
        uses.permit(result(argument)?, root);
        uses.permit(EntityId::Place(place), begin);
        uses.permit(EntityId::Loan(*actual), call);
        uses.permit(EntityId::Loan(*actual), end);
        uses.check(&g, Some((call.block, value(call)?)))?;
        let case = temporary_arguments::sources().remove(0);
        temporary_arguments::graph::factory::check(&case, sources, parsed, module, factory, map)
    }
}
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

pub(in crate::ssa) mod query_callers {
//! Real entry owner -> call loan -> scan parameter, and read's borrowed consumer slots.
use crate::ssa::map_provider_exits_pipeline_tests::{
    fixture::{Failure, contains, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use crate::ssa::map_provider_exits_pipeline_tests::nested_exits::{
    ast::{Exit, Input},
    query_body::{Context, INNER, OUTER},
    query_graph::{Role, Roles, Witness, full_graph, ty},
    query_inputs::Inputs,
    query_tail,
};
use crate::ssa::model::*;
use lang_frontend::parser::Expression;
use std::collections::BTreeSet;

fn signature(
    function: &Function,
    expected: &[EntityType],
    returns: &[SsaTypeId],
) -> Result<(), Failure> {
    let entry = function
        .entry_block()
        .and_then(|b| function.block(b))
        .ok_or_else(|| failure("nested exits SSA callable entry", function.id))?;
    need(
        function.receiver.is_none()
            && function.borrow_return.is_none()
            && function.carrier_return.is_none()
            && function.return_types == returns
            && entry.origin == function.origin
            && entry.parameters.len() == expected.len()
            && entry
                .parameters
                .iter()
                .zip(expected)
                .enumerate()
                .all(|(index, (&p, ty))| {
                    function.entity(p).is_some_and(|e| {
                        e.ty == *ty
                            && e.origin == function.origin
                            && e.definition
                                == Definition::BlockParameter {
                                    block: entry.id,
                                    index,
                                }
                    })
                }),
        "nested exits SSA source callable actual parameter definitions/signature",
        function,
    )
}

fn consumer(read: &Function, f: &Input, inputs: &Inputs, string: SsaTypeId) -> Result<(), Failure> {
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: string,
    };
    signature(read, &[shared, shared], &[])?;
    let g = Graph::new(read)?;
    full_graph(&g)?;
    let block = one(
        g.blocks.iter().copied(),
        "nested exits SSA actual read single body",
    )?;
    let params = &g.block(block)?.parameters;
    let calls = f
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            matches!(n.payload(), Expression::Call { .. })
                .then_some(n.span())
                .filter(|&s| contains(inputs.read, s))
        })
        .collect::<Vec<_>>();
    need(
        calls.len() == 2 && g.instructions.len() == 2 && params[0] != params[1],
        "nested exits SSA exact source read consumers",
        (&calls, read),
    )?;
    let mut calls = calls;
    calls.sort_by_key(|s| s.start());
    let mut uses = Uses::default();
    for (index, &param) in params.iter().enumerate() {
        uses.protect(param);
        let instruction = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.origin == Origin::Source(calls[index])),
            "nested exits SSA actual println source consumer",
        )?;
        let EntityId::Loan(actual) = param else {
            return Err(failure("nested exits SSA actual read Borrow slot", param));
        };
        need(
            instruction.operation == Operation::PrintString { value: actual }
                && instruction.results.is_empty()
                && g.position(instruction)? == index,
            "nested exits SSA consumer reads its own ordered Borrow parameter",
            instruction,
        )?;
        uses.permit(param, instruction);
    }
    need(
        matches!(&g.term(block)?.kind, TerminatorKind::Return { values } if values.is_empty())
            && g.term(block)?.origin == Origin::Source(inputs.read),
        "nested exits SSA read Unit return no borrowed escape",
        g.term(block)?,
    )?;
    uses.check(&g, None)
}

fn operand_consumer(
    module: &Module,
    f: &Input,
    inputs: &Inputs,
    string: SsaTypeId,
) -> Result<(), Failure> {
    let Some(anchor) = &inputs.operand else {
        return Ok(());
    };
    let function = function(module, anchor.function)?;
    let int = *one(
        function.return_types.iter(),
        "nested exits SSA operand sole Int result",
    )?;
    need(
        crate::ssa::map_provider_exits_pipeline_tests::nested_exits::query_resources::module_int(module, int),
        "nested exits SSA operand actual Int type",
        int,
    )?;
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: string,
    };
    signature(function, &[shared; 4], &[int])?;
    let g = Graph::new(function)?;
    full_graph(&g)?;
    let block = one(
        g.blocks.iter().copied(),
        "nested exits SSA operand actual single block",
    )?;
    let mut roles = Roles::default();
    for (role, &entity) in [
        Role::OuterKey,
        Role::OuterValue,
        Role::InnerKey,
        Role::InnerValue,
    ]
    .into_iter()
    .zip(&g.block(block)?.parameters)
    {
        roles.insert(role, entity);
    }
    let mut w = Witness::default();
    w.protect(&roles)?;
    let context = Context {
        graph: &g,
        module,
        read: function_by_read(module, inputs)?,
    };
    let first = context.read(block, &inputs.reads[1], OUTER, &roles, &mut w)?;
    let second = context.read(block, &inputs.reads[2], INNER, &roles, &mut w)?;
    need(
        g.position(first)? < g.position(second)?
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                .map(|i| i.id)
                .collect::<BTreeSet<_>>()
                == [first.id, second.id].into_iter().collect(),
        "nested exits SSA operand reads its own four ordered Borrow parameters",
        function,
    )?;
    let TerminatorKind::Return { values } = &g.term(block)?.kind else {
        return Err(failure(
            "nested exits SSA operand real Return",
            g.term(block)?,
        ));
    };
    let returned = one(
        values.iter().copied(),
        "nested exits SSA operand one return value",
    )?;
    let producer = g.definition(EntityId::Value(returned))?;
    need(
        producer.operation == Operation::Constant(ScalarConstant::Integer(7))
            && producer.block == block
            && g.position(second)? < g.position(producer)?
            && ty(&g, EntityId::Value(returned))? == EntityType::Value(int)
            && f.sources.slice(producer.origin.span()) == Ok("7")
            && contains(anchor.function, g.term(block)?.origin.span())
            && contains(g.term(block)?.origin.span(), producer.origin.span()),
        "nested exits SSA operand source literal/result return after both actual reads",
        producer,
    )?;
    w.returns.insert(block, vec![returned]);
    w.finish(&g)
}

fn function_by_read<'a>(module: &'a Module, input: &Inputs) -> Result<&'a Function, Failure> {
    function(module, input.read)
}

pub(in crate::ssa) fn check(
    module: &Module,
    map_type: SsaTypeId,
    f: &Input,
    inputs: &Inputs,
) -> Result<(), Failure> {
    let entry = function(module, inputs.entry)?;
    let scan = function(module, inputs.scan)?;
    let read = function(module, inputs.read)?;
    let factory = function(module, inputs.factory)?;
    let (_, string, val) = module
        .map_container(map_type)
        .ok_or_else(|| failure("nested exits SSA caller map type", map_type))?;
    need(string == val, "nested exits SSA exact String K/V", map_type)?;
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    };
    signature(entry, &[], &[])?;
    signature(scan, &[shared], &scan.return_types)?;
    if f.exit == Exit::Return {
        let int = one(
            scan.return_types.iter().copied(),
            "nested exits SSA scan Int signature",
        )?;
        need(
            crate::ssa::map_provider_exits_pipeline_tests::nested_exits::query_resources::module_int(module, int),
            "nested exits SSA scan Int result",
            scan,
        )?;
    } else {
        need(
            scan.return_types.is_empty(),
            "nested exits SSA scan Unit result",
            scan,
        )?;
    }
    signature(factory, &[], &[map_type])?;
    consumer(read, f, inputs, string)?;
    operand_consumer(module, f, inputs, string)?;
    let g = Graph::new(entry)?;
    full_graph(&g)?;
    let mut w = Witness::default();
    let mut protected = BTreeSet::new();
    let mut calls = BTreeSet::new();
    let mut tail = entry
        .entry_block()
        .ok_or_else(|| failure("nested exits SSA entry", entry.id))?;
    g.incoming_from(tail, &[])?;
    let mut carried = Roles::default();
    for (index, anchor) in inputs.callers.iter().enumerate() {
        let role = if index == 0 {
            Role::CallerOwnerA
        } else {
            Role::CallerOwnerB
        };
        let birth = one(g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(anchor.initializer) && if index == 0 {
                matches!(&i.operation, Operation::DirectCall { callee, receiver: None, arguments } if *callee == factory.id && arguments.is_empty())
            } else { i.operation == Operation::MapConstruct { map_type } }
        }), "nested exits SSA caller exact local initializer")?;
        let owner = value(birth)?;
        need(
            birth.block == tail && ty(&g, result(birth)?)? == EntityType::Value(map_type),
            "nested exits SSA caller own result in source continuation",
            birth,
        )?;
        if index == 0 {
            calls.insert(birth.id);
        }
        let root = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::RootPlace { owner }),
            "nested exits SSA caller exact owner RootPlace",
        )?;
        let EntityId::Place(place) = result(root)? else {
            return Err(failure("nested exits SSA caller Place", root));
        };
        let begin = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    }
            }),
            "nested exits SSA caller Shared loan",
        )?;
        let source = loan(begin)?;
        let call = one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(anchor.call)
            && matches!(&i.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == scan.id && arguments.as_slice() == [EntityId::Loan(source)])),
            "nested exits SSA caller actual scan target and sole Borrow slot")?;
        calls.insert(call.id);
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: source }),
            "nested exits SSA caller exact call loan end",
        )?;
        let prefix = [birth, root, begin, call, end];
        need(
            prefix.iter().all(|i| i.block == tail)
                && prefix.windows(2).all(|p| {
                    g.position(p[0])
                        .ok()
                        .zip(g.position(p[1]).ok())
                        .is_some_and(|(a, b)| a < b)
                })
                && root.origin == Origin::Source(anchor.argument)
                && begin.origin == Origin::Source(anchor.argument)
                && end.origin == Origin::Source(anchor.call)
                && end.results.is_empty()
                && ty(&g, EntityId::Place(place))? == EntityType::Place(map_type)
                && ty(&g, EntityId::Loan(source))? == shared,
            "nested exits SSA caller owner root call loan and result exact order",
            prefix,
        )?;
        for entity in [
            EntityId::Value(owner),
            EntityId::Place(place),
            EntityId::Loan(source),
        ] {
            need(
                protected.insert(entity),
                "nested exits SSA two caller owners/places/loans distinct",
                entity,
            )?;
            w.uses.protect(entity);
        }
        w.uses.permit(EntityId::Value(owner), root);
        w.uses.permit(EntityId::Place(place), begin);
        w.uses.permit(EntityId::Loan(source), call);
        w.uses.permit(EntityId::Loan(source), end);
        let mut current = EntityId::Value(owner);
        if let Some(reference) = anchor.result_reference {
            need(
                f.exit == Exit::Return
                    && ty(&g, result(call)?)? == EntityType::Value(scan.return_types[0]),
                "nested exits SSA caller actual returned Int",
                call,
            )?;
            w.uses.protect(result(call)?);
            let mut roles = carried.clone();
            roles.insert(role, current);
            roles.insert(
                if index == 0 {
                    Role::CallerResultA
                } else {
                    Role::CallerResultB
                },
                result(call)?,
            );
            let (joined, next) = query_tail::conditional(
                &g,
                f,
                call,
                Some((reference, if index == 0 { 7 } else { 0 })),
                &roles,
                &mut w,
            )?;
            tail = joined;
            current = next.get(role)?;
            carried = next;
            carried.0.remove(&role);
        } else {
            need(
                f.exit != Exit::Return && call.results.is_empty(),
                "nested exits SSA caller actual Unit call",
                call,
            )?;
        }
        let size = query_tail::size(&g, anchor.size, current, &mut w)?;
        let EntityId::Value(current_owner) = current else {
            return Err(failure(
                "nested exits SSA transported caller owner",
                current,
            ));
        };
        let drop = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::Drop {
                        owner: current_owner,
                    }
            }),
            "nested exits SSA current caller owner Drop",
        )?;
        need(
            size.block == tail
                && drop.block == tail
                && g.position(size)? < g.position(drop)?
                && (f.exit == Exit::Return || g.position(end)? < g.position(size)?)
                && drop.origin == Origin::Source(anchor.drop)
                && anchor.drop == anchor.local
                && drop.results.is_empty(),
            "nested exits SSA caller size then FE Drop on exact current owner",
            (size, drop, anchor),
        )?;
        w.uses.permit(current, drop);
        (tail, carried) = query_tail::conditional(&g, f, size, None, &carried, &mut w)?;
    }
    need(
        g.ops(tail)?.is_empty()
            && matches!(&g.term(tail)?.kind, TerminatorKind::Return { values } if values.is_empty())
            && g.term(tail)?.origin == Origin::Source(inputs.entry),
        "nested exits SSA caller final Unit Return",
        g.term(tail)?,
    )?;
    need(
        g.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
            .map(|i| i.id)
            .collect::<BTreeSet<_>>()
            == calls
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapSize { .. }))
                .count()
                == 2
            && !g
                .instructions
                .iter()
                .any(|i| crate::ssa::map_provider_exits_pipeline_tests::nested_exits::query_body::provider(i)),
        "nested exits SSA complete actual caller call/size/provider set",
        &calls,
    )?;
    let mut related = protected;
    for role in [Role::CallerOwnerA, Role::CallerOwnerB] {
        if let Some(entities) = w.aliases.get(&role) {
            related.extend(entities.iter().copied());
        }
    }
    crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(&g, &[map_type], &related)?;
    w.finish(&g)
}
}
