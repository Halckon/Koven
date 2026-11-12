//! Success assertions for direct-call prefixes cancelled by source control flow.
//! Only inspect the real returned Program; no synthetic facts, IDs, or SSA are constructed.

use lang_frontend::{
    parser::{Expression, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile, Statement},
    source::SourceMap,
};

use super::{dropped, is_direct_call, operation_count};
use crate::ssa::{
    model::{
        EntityId, Function, FunctionId, Instruction, Operation, Origin, Program, ScalarConstant,
        TerminatorKind,
    },
    verify::verify_program,
};

pub(super) fn assert_cancelled_scalar_call(
    program: &Program,
    entry: FunctionId,
    returning: bool,
    sources: &SourceMap,
    parsed: &ParsedFile,
) {
    verify_program(program).expect("original direct-argument source now verifies");
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    assert_eq!(
        operation_count(function, is_direct_call),
        0,
        "the cancelled callee is never invoked"
    );
    assert!(
        dropped(function).is_empty(),
        "the Copyable prefix creates no owner obligation"
    );
    let endings = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
        .collect::<Vec<_>>();
    assert_eq!(
        endings.len(),
        1,
        "the sole new prefix loan ends exactly once"
    );
    let ending = endings[0];
    let begins = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. }))
        .collect::<Vec<_>>();
    assert_eq!(begins.len(), 1, "one actual prefix loan was created");
    let Operation::BorrowEnd { loan } = ending.operation else {
        unreachable!("selected the actual BorrowEnd operation")
    };
    assert_eq!(begins[0].results, [EntityId::Loan(loan)]);
    let block = function.block(ending.block).unwrap();
    assert_eq!(block.instructions.last(), Some(&ending.id));
    let term = block.terminator.as_ref().unwrap();
    if returning {
        assert_eq!(
            term.origin, ending.origin,
            "cleanup immediately precedes the exact source transfer"
        );
    } else {
        assert_single_break_tail(sources, parsed, function, ending);
    }
    match &term.kind {
        TerminatorKind::Return { values } if returning => {
            let [returned] = values.as_slice() else {
                panic!("the original return source returns exactly one scalar")
            };
            let constants = function
                .instructions
                .iter()
                .filter(|instruction| {
                    instruction.results == [EntityId::Value(*returned)]
                        && instruction.operation == Operation::Constant(ScalarConstant::Integer(7))
                })
                .count();
            assert_eq!(
                constants, 1,
                "the original source returns 7 without calling select"
            );
        }
        TerminatorKind::Return { values } if !returning => {
            assert!(
                values.is_empty(),
                "the original loop naturally returns Unit"
            );
        }
        _ => panic!("the original return/break source retains its exact transfer kind"),
    }
    crate::llvm::render_verified_program(program)
        .expect("original migrated source emits verified LLVM");
}

pub(super) fn assert_cancelled_nested_break(
    program: &Program,
    entry: FunctionId,
    sources: &SourceMap,
) {
    verify_program(program).expect("nested break source verifies");
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    let endings: Vec<_> = function
        .instructions
        .iter()
        .filter_map(
            |instruction| match (&instruction.operation, &instruction.origin) {
                (Operation::BorrowEnd { loan }, Origin::Source(span))
                    if sources.slice(*span) == Ok("break") =>
                {
                    Some((instruction, *loan))
                }
                _ => None,
            },
        )
        .collect();
    let [(ending, loan)] = endings.as_slice() else {
        panic!(
            "expected one borrow end on the nested break edge: {}",
            super::render_program(program)
        );
    };
    let begins: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. }))
        .collect();
    let [begin] = begins.as_slice() else {
        panic!("the nested break creates only the scalar prefix loan");
    };
    // The verifier checks the provenance of the loan transported through
    // block parameters before the actual break cleanup.
    let ended = EntityId::Loan(*loan);
    assert!(
        begin.results == [ended]
            || function
                .blocks
                .iter()
                .any(|block| { block.parameters.contains(&ended) })
    );
    let block = function.block(ending.block).unwrap();
    assert_eq!(block.instructions.last(), Some(&ending.id));
    assert!(!function.instructions.iter().any(|instruction| {
        instruction.block == ending.block && is_direct_call(&instruction.operation)
    }));
    crate::llvm::render_verified_program(program).expect("nested break emits verified LLVM");
}

// The original single-exit loop reuses its break block, then returns Unit naturally.
fn assert_single_break_tail(
    sources: &SourceMap,
    parsed: &ParsedFile,
    function: &Function,
    ending: &Instruction,
) {
    let ast = parsed.ast();
    let entries = ast
        .items()
        .iter()
        .filter(|(_, node)| {
            matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                if sources.slice(*name) == Ok("entry"))
        })
        .collect::<Vec<_>>();
    let [(_, entry)] = entries.as_slice() else {
        panic!("the original source has one actual entry declaration")
    };
    let Item::Function {
        form:
            FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            },
        ..
    } = entry.payload()
    else {
        panic!("the original entry has its explicit Unit block body")
    };
    let Statement::Block { elements } = ast.statements().get(*body).unwrap().payload() else {
        panic!("the actual entry body is a block")
    };
    let [_, loop_id] = elements.as_slice() else {
        panic!("the original entry has only its scalar declaration and tail loop")
    };
    let loop_node = ast.statements().get(*loop_id).unwrap();
    let Statement::Loop { body, .. } = loop_node.payload() else {
        panic!("the actual tail statement is an unconditional loop")
    };
    let Statement::Block { elements } = ast.statements().get(*body).unwrap().payload() else {
        panic!("the actual loop body is a block")
    };
    let [local] = elements.as_slice() else {
        panic!("the original loop contains only the cancelling call declaration")
    };
    let Statement::LocalVariable { declaration } = ast.statements().get(*local).unwrap().payload()
    else {
        panic!("the original call is the local initializer")
    };
    let Item::Variable { initializer, .. } = ast.items().get(*declaration).unwrap().payload()
    else {
        panic!("the actual local declaration has its initializer")
    };
    let Expression::Call { arguments, .. } = ast.expressions().get(*initializer).unwrap().payload()
    else {
        panic!("the actual initializer is the direct call")
    };
    let [_, exit] = arguments.as_slice() else {
        panic!("the original direct call has exactly two arguments")
    };
    let exit = ast.expressions().get(exit.value).unwrap();
    assert!(matches!(exit.payload(), Expression::Break { .. }));
    assert_eq!(ending.origin, Origin::Source(exit.span()));
    assert_eq!(entry.span().source_id(), parsed.source_id());
    assert_eq!(loop_node.span().source_id(), parsed.source_id());
    assert_eq!(exit.span().source_id(), parsed.source_id());
    assert_eq!(function.origin, Origin::Source(entry.span()));
    assert!(function.return_types.is_empty());
    let [entry_block, loop_block] = function.blocks.as_slice() else {
        panic!("the single-exit source has only entry and the reused loop block")
    };
    assert_eq!(function.entry_block(), Some(entry_block.id));
    assert_eq!(entry_block.origin, Origin::Source(entry.span()));
    assert_eq!(loop_block.origin, Origin::Source(loop_node.span()));
    assert_eq!(ending.block, loop_block.id);
    let transfer = entry_block.terminator.as_ref().unwrap();
    assert_eq!(transfer.origin, Origin::Source(loop_node.span()));
    let TerminatorKind::Branch(edge) = &transfer.kind else {
        panic!("entry transfers directly to the actual loop header")
    };
    assert_eq!(edge.target, loop_block.id);
    assert_ne!(edge.target, entry_block.id);
    let tail = loop_block.terminator.as_ref().unwrap();
    assert_eq!(tail.origin, Origin::Source(entry.span()));
    let TerminatorKind::Return { values } = &tail.kind else {
        panic!("the reused single break block becomes the natural Unit return")
    };
    assert!(values.is_empty());
}

pub(in crate::ssa) mod borrow_storage {
//! 通用 Borrow storage 参数取代旧 String-only closure 布局限制。
use super::super::*;
use crate::ssa::verify::verify_program;

#[test]
fn move_only_borrow_parameter_uses_shared_storage_without_payload_owner() {
    // Preserve the exact former unsupported fixture as a positive ABI contract.
    let source = "package test\n\
        class Host {}\n\
        fun entry(): Unit {\n\
            val action: move (borrow Host) -> Unit = move { item -> }\n\
        }";
    let mut sources = SourceMap::new();
    let (source_id, file) = parsed(&mut sources, "test/move-only-borrow-parameter.ko", source);
    let inputs = [SourceUnitInput::new(
        "root",
        "test/move-only-borrow-parameter.ko",
        source_id,
        &file,
    )];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .unwrap();
    verify_program(&program).unwrap();
    let module = &program.modules[0];
    let thunk = module
        .functions
        .iter()
        .find(|f| f.name.contains("lambda"))
        .unwrap();
    let [EntityId::Loan(parameter)] = thunk
        .block(thunk.entry_block().unwrap())
        .unwrap()
        .parameters
        .as_slice()
    else {
        panic!("one shared storage parameter")
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = thunk.entity(EntityId::Loan(*parameter)).unwrap().ty
    else {
        panic!("borrow ABI")
    };
    assert!(matches!(
        module.type_kind(target),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    assert!(!thunk.instructions.iter().any(|i| matches!(
        i.operation,
        Operation::Drop { .. } | Operation::Copy { .. } | Operation::SharedRetain { .. }
    )));
    // Entry Borrow is a call-scoped parameter; Return ends this callee view.
    // The caller remains responsible for its actual source loan continuation.
    assert_eq!(
        operation_count(thunk, |i| matches!(i, Operation::BorrowEnd { .. })),
        0
    );
    let llvm = crate::llvm::render_verified_program(&program).unwrap();
    let header = llvm
        .lines()
        .find(|line| line.starts_with("define internal void") && line.contains("lambda"))
        .unwrap();
    assert!(header.contains("ptr %l"));
}
}

pub(in crate::ssa) mod shared_capture {
//! A nonescaping borrowed closure now consumes the published shared-capture contract.
use super::super::*;
use lang_frontend::{
    ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode as FrontendCaptureMode},
    parser::Expression,
    type_checking::UnitExpressionId,
};

#[test]
fn borrowed_closure_keeps_its_source_owner_until_the_closure_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "test/borrowed.ko",
        "package test\n\
         fun inspect(message: String): Unit {}\n\
         fun entry(): Unit {\n\
             val message = \"borrowed\"\n\
             val action: () -> Unit = { -> val read = inspect(message) }\n\
             val invoked = action()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/borrowed.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let lambda = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| matches!(node.payload(), Expression::Lambda { .. }))
        .expect("real source lambda")
        .0;
    let lambda = UnitExpressionId::new(names.names().source_units()[0].source_unit(), lambda);
    let captures = owned.ownership().captures_of(lambda).collect::<Vec<_>>();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].mode(), FrontendCaptureMode::Shared);
    assert_eq!(captures[0].effect(), ClosureCaptureEffect::Borrow);
    let arena = typed.types().types().len();
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("guide permits a borrowed closure's synchronous call in its defining callable");
    let module = &program.modules[0];
    let entry = function(module, "test.entry");
    let owner = entry
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
        .expect("captured source owner")
        .results[0];
    let closure = entry
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::ClosureConstruct { .. }))
        .expect("shared capture environment")
        .results[0];
    let drop_index = |entity| {
        entry
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction.operation, Operation::Drop { owner }
                if EntityId::Value(owner) == entity)
            })
            .expect("one real owner drop")
    };
    assert_eq!(operation_count(entry, is_callable_invoke), 1);
    assert_eq!(operation_count(entry, is_drop), 2);
    assert!(drop_index(closure) < drop_index(owner));
    let thunk = module
        .functions
        .iter()
        .find(|function| function.name.contains(".thunk"))
        .expect("borrowed closure thunk");
    assert!(thunk.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::SharedReferenceFollow { .. }
    )));
    crate::llvm::render_verified_program(&program)
        .expect("shared capture lifetime reaches verified LLVM without consuming the source");
    assert_eq!(typed.types().types().len(), arena);
}
}

pub(in crate::ssa) mod provider_7 {
//! One actual epoch provider with complete current-owner/source/cursor columns.
use crate::ssa::map_provider_exits_pipeline_tests::local_reassignment::graph::*;
pub(in crate::ssa) struct Evidence<'a> {
    pub first: &'a Instruction,
    pub end: &'a Instruction,
    pub exhausted_owner: EntityId,
}
pub(in crate::ssa) fn check<'a>(
    g: &Graph<'a>,
    module: &Module,
    (sources, parsed): (&SourceMap, &ParsedFile),
    l: &ast::Loop,
    anchor: EntityId,
    owners: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<Evidence<'a>, Failure> {
    let op = |p: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.origin == Origin::Source(l.span) && p(&i.operation)),
            "actual epoch For operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let has = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    let next = op(|o| matches!(o, Operation::MapIterationNext { .. }))?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
    let birth = g.definition(EntityId::Loan(source))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = birth.operation
    else {
        return Err(failure("epoch actual source lease", birth));
    };
    let root = g.definition(EntityId::Place(place))?;
    need(
        root.operation
            == Operation::RootPlace {
                owner: match anchor {
                    EntityId::Value(v) => v,
                    _ => return Err(failure("epoch Value anchor", anchor)),
                },
            }
            && root.block == first.block
            && birth.block == first.block
            && g.position(root)? < g.position(birth)?
            && g.position(birth)? < g.position(first)?,
        "First borrows current complete factory RHS, never old declaration epoch",
        root,
    )?;
    uses.permit(anchor, root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), birth);
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = has.operation
    else {
        unreachable!()
    };
    let Operation::MapIterationKeyLoan {
        source: bs,
        cursor: bc,
    } = key.operation
    else {
        unreachable!()
    };
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("epoch guard CFG", has));
    };
    need(
        *condition == value(has)?
            && when_true.target == key.block
            && val.block == key.block
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: bs,
                    cursor: bc,
                },
        "actual epoch guard and K/V loan operands",
        (has, key, val),
    )?;
    let entry = g.branch(first.block)?;
    need(
        entry.target == has.block
            && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "actual epoch full source/cursor incoming",
        entry,
    )?;
    let ho = g.transported(entry, anchor)?;
    let bo = g.transported(when_true, ho)?;
    let xo = g.transported(when_false, ho)?;
    need(
        [ho, bo, xo].iter().all(|e| owners.contains(e)),
        "actual epoch owner incoming columns",
        (ho, bo, xo),
    )?;
    g.incoming_from(has.block, &[first.block, key.block])?;
    g.incoming_from(key.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let back = g.branch(key.block)?;
    need(
        next.block == key.block
            && next.operation
                == Operation::MapIterationNext {
                    source: bs,
                    cursor: bc,
                }
            && back.target == has.block
            && g.argument(back, ho)? == bo
            && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.argument(back, EntityId::Value(hc))? == result(next)?,
        "actual epoch complete owner/source/cursor backedge",
        (next, back),
    )?;
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let xc = g.transported(when_false, EntityId::Value(hc))?;
    let leases = family(g, EntityId::Loan(source))?;
    let ends=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if leases.contains(&EntityId::Loan(loan)))).collect::<Vec<_>>();
    let end = one(ends.into_iter(), "only epoch exhaustion ends its source")?;
    need(
        matches!(end.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==xs)
            && end.block == when_false.target,
        "epoch ends exact exhaustion source column",
        end,
    )?;
    for &e in &leases {
        uses.protect(e);
    }
    uses.permit(EntityId::Loan(source), first);
    uses.permit(EntityId::Loan(hs), has);
    for i in [key, val, next] {
        uses.permit(EntityId::Loan(bs), i);
    }
    uses.permit(xs, end);
    let cursors = [
        result(first)?,
        EntityId::Value(hc),
        EntityId::Value(bc),
        result(next)?,
        xc,
    ];
    for &e in &cursors {
        uses.protect(e);
        need(g.function.entity(e).is_some_and(|d|matches!(d.ty,EntityType::Value(t) if module.type_kind(t)==Some(&SsaTypeKind::Integer{bits:64,signed:false}))),"epoch exact cursor storage",e)?;
    }
    uses.permit(EntityId::Value(hc), has);
    for i in [key, val, next] {
        uses.permit(EntityId::Value(bc), i);
    }
    for (from, edge) in &g.edges {
        for e in &edge.arguments {
            if leases.contains(e) || cursors.contains(e) {
                need(
                    (*from == first.block && edge.target == has.block)
                        || (*from == has.block
                            && [key.block, when_false.target].contains(&edge.target))
                        || (*from == key.block && edge.target == has.block),
                    "epoch provider state never survives cleanup or crosses other For",
                    (from, edge, e),
                )?;
            }
        }
    }
    let reader = function(module, ast::declaration(sources, parsed, "read")?)?;
    let read=one(g.instructions.iter().copied().filter(|i|i.block==key.block && matches!(&i.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==reader.id && arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])),"actual epoch selected reader and current K/V operands")?;
    let (_, kt, vt) = module
        .map_container(g.function.entity(anchor).unwrap().ty.semantic_type())
        .ok_or_else(|| failure("epoch map storage", anchor))?;
    let mut component_ends = Vec::new();
    for (index, (component, target)) in [(key, kt), (val, vt)].into_iter().enumerate() {
        let e = result(component)?;
        let ty = EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        };
        need(
            g.function.entity(e).unwrap().ty == ty
                && reader
                    .block(reader.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .get(index)
                    .and_then(|p| reader.entity(*p))
                    .is_some_and(|p| p.ty == ty),
            "epoch exact represented Shared component ABI",
            component,
        )?;
        let ending = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowEnd {
                        loan: loan(component).unwrap(),
                    }
            }),
            "epoch unique component End",
        )?;
        need(
            ending.block == key.block,
            "epoch component End actual body",
            ending,
        )?;
        uses.protect(e);
        uses.permit(e, read);
        uses.permit(e, ending);
        component_ends.push(ending);
    }
    need(
        g.position(read)? < g.position(component_ends[1])?
            && g.position(component_ends[1])? < g.position(component_ends[0])?
            && g.position(component_ends[0])? < g.position(next)?,
        "epoch read/VEnd/KEnd/Next order",
        &component_ends,
    )?;
    guard(
        g,
        module,
        (sources, parsed),
        l,
        read,
        component_ends[1],
        uses,
    )?;
    let inner = match module.type_kind(vt) {
        Some(SsaTypeKind::NullableHandle { inner }) => *inner,
        _ => vt,
    };
    // Pure String temporaries printed as markers are unrelated to the borrowed Map
    // component; protecting exact component loans forbids owner extraction already.
    if module.type_kind(inner) != Some(&SsaTypeKind::StringOwner) {
        need(!g.instructions.iter().any(|i|matches!(i.operation,Operation::Drop{owner} if [vt,inner].contains(&g.function.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()))),"no borrowed Resource component owner Drop",g.function.id)?;
    }
    Ok(Evidence {
        first,
        end,
        exhausted_owner: xo,
    })
}
fn guard(
    g: &Graph<'_>,
    module: &Module,
    (sources, parsed): (&SourceMap, &ParsedFile),
    l: &ast::Loop,
    read: &Instruction,
    vend: &Instruction,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let body = parsed.ast().statements().get(l.body).unwrap().span();
    let (name, init) = one(
        parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if ast::inside(n.span(), body) && sources.slice(*name) == Ok("local") => {
                    Some((*name, *initializer))
                }
                _ => None,
            }),
        "epoch original Guard local",
    )?;
    let origin = ast::expression(parsed, init);
    need(
        sources.slice(origin) == Ok("Guard()"),
        "epoch original Guard constructor",
        origin,
    )?;
    let allocation = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(origin)
                && matches!(i.operation, Operation::HeapAllocate { .. })
        }),
        "epoch actual Guard allocation",
    )?;
    let Operation::HeapAllocate { owner: ty, payload } = allocation.operation else {
        unreachable!()
    };
    let payload = g.definition(EntityId::Value(payload))?;
    need(
        payload.origin == Origin::Source(origin)
            && matches!(&payload.operation,Operation::AggregateConstruct{fields,..} if fields.is_empty()),
        "epoch actual empty Guard payload",
        payload,
    )?;
    let class = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Classifier(c) = n.payload() else {
                return None;
            };
            matches!(c.name,NameMarker::Present(name) if sources.slice(name)==Ok("Guard"))
                .then_some(c)
        }),
        "original Guard class",
    )?;
    let deinit = one(
        class
            .body
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                matches!(n.payload(), Item::Deinit { .. }).then_some(n.span())
            }),
        "original Guard deinit",
    )?;
    need(
        module
            .deinits
            .get(&ty)
            .and_then(|id| module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(deinit)),
        "epoch Guard selects actual original deinit",
        ty,
    )?;
    let owner = value(allocation)?;
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner }),
        "epoch unique Guard Drop",
    )?;
    need(
        allocation.block == read.block
            && drop.block == read.block
            && drop.origin == Origin::Source(name)
            && g.position(allocation)? < g.position(read)?
            && g.position(read)? < g.position(drop)?
            && g.position(drop)? < g.position(vend)?,
        "epoch Guard allocation/read/Drop before component cleanup",
        drop,
    )?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), drop);
    Ok(())
}
}
