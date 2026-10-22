//! Real strict Unit sources only: no changed facts, injected IR, counters, or compatibility path.
use super::{OwnedProducts, UnitSource};
use crate::{
    native_tests::{boxed_enum_tests::assert_success, emit_link_and_run},
    ssa::{model::*, verify::verify_program},
};
use lang_frontend::{
    name_resolution::SymbolKind,
    ownership_checking::{UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as Action},
    parser::Statement,
};
use std::collections::BTreeSet;

// Byte-identical to the original whole failing test source. The old test is not edited.
const ORIGINAL: &str = r#"
class Resource(val text: String) { deinit() { println("drop") } }
fun observe(value: Resource) { println(value.text) }
fun entry() {
 var values = mutableMapOf<Int, Resource>()
 values.put(1, Resource("first".clone())); values.put(2, Resource("second".clone()))
 for (key in arrayOf(1, 2, 3)) {
   val removed = values.remove(key)
   if (removed != null) { observe(removed); observe(removed) } else { println("missing") }
   println("iteration")
 }
 println("end")
}
"#;
const ORIGINAL_STDOUT: &[u8] =
    b"first\nfirst\niteration\ndrop\nsecond\nsecond\niteration\ndrop\nmissing\niteration\nend\n";

fn analyzed(text: &str) -> UnitSource {
    let source = UnitSource::analyze("resource-map-loop.ko", text)
        .expect("real source passes strict final type admission and ownership");
    source.assert_frontend_clean();
    assert!(
        matches!(&source.products, OwnedProducts::Basic(_, _)),
        "no artificial constant capability"
    );
    source
}

fn edges(term: &TerminatorKind) -> Vec<&Edge> {
    match term {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        _ => Vec::new(),
    }
}

// Actual incoming-edge provenance. Each proof suffix names the NullableBranch owner.
fn terminals(function: &Function, start: EntityId) -> BTreeSet<(InstructionId, usize)> {
    let mut pending = vec![start];
    let mut seen = BTreeSet::new();
    let mut roots = BTreeSet::new();
    while let Some(entity) = pending.pop() {
        if !seen.insert(entity) {
            continue;
        }
        match function.entity(entity).expect("real entity").definition {
            Definition::InstructionResult { instruction, index } => {
                roots.insert((instruction, index));
            }
            Definition::BlockParameter { block, index } => {
                let mut found = false;
                for predecessor in &function.blocks {
                    let Some(term) = &predecessor.terminator else {
                        continue;
                    };
                    if let TerminatorKind::NullableBranch { owner, view, .. } = term.kind
                        && entity == EntityId::Loan(view)
                    {
                        pending.push(EntityId::Value(owner));
                        found = true;
                    }
                    for edge in edges(&term.kind) {
                        if edge.target == block
                            && let Some(argument) = edge.arguments.get(index)
                        {
                            pending.push(*argument);
                            found = true;
                        }
                    }
                }
                assert!(
                    found || block == function.blocks[0].id,
                    "non-entry parameter has a real incoming owner/proof edge"
                );
            }
        }
    }
    roots
}

fn proof_origins(function: &Function, start: LoanId) -> BTreeSet<LoanId> {
    let mut pending = vec![start];
    let mut seen = BTreeSet::new();
    let mut roots = BTreeSet::new();
    while let Some(loan) = pending.pop() {
        if !seen.insert(loan) {
            continue;
        }
        if function.blocks.iter().any(|block| {
            matches!(block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::NullableBranch { view, .. }) if *view == loan)
        }) {
            roots.insert(loan);
            continue;
        }
        let Definition::BlockParameter { block, index } =
            function.entity(EntityId::Loan(loan)).unwrap().definition
        else {
            panic!("call view is not replaced by an unrelated loan")
        };
        let mut incoming = 0;
        for edge in function
            .blocks
            .iter()
            .flat_map(|block| block.terminator.iter().flat_map(|term| edges(&term.kind)))
        {
            if edge.target == block {
                let EntityId::Loan(prior) = edge.arguments[index] else {
                    panic!("loan transport column")
                };
                pending.push(prior);
                incoming += 1;
            }
        }
        assert!(incoming > 0);
    }
    assert!(!roots.is_empty());
    roots
}

fn assert_loop_protocol(source: &UnitSource, name: &str) -> (Program, FunctionId) {
    let (program, id) = source
        .lower_entry(source.declaration(name, SymbolKind::Function))
        .expect("the actual Basic products lower, without changing the source");
    verify_program(&program).expect("ordinary ownership and dominance verifier");
    let function = program.module(id.module()).unwrap().function(id).unwrap();
    let removes = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::MapRemove { .. }))
        .collect::<Vec<_>>();
    let [remove] = removes.as_slice() else {
        panic!("one static remove per iteration")
    };
    let removed = BTreeSet::from([(remove.id, 1)]);
    let views = function
        .blocks
        .iter()
        .filter_map(
            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::NullableBranch {
                    owner,
                    view,
                    when_null,
                    ..
                }) => {
                    assert_eq!(
                        terminals(function, EntityId::Value(*owner)),
                        removed,
                        "proofs depend on this removed owner, not the map or source provider"
                    );
                    assert!(matches!(
                        function
                            .block(when_null.target)
                            .unwrap()
                            .terminator
                            .as_ref()
                            .map(|term| &term.kind),
                        Some(TerminatorKind::Abort)
                    ));
                    Some(*view)
                }
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(
        views.len(),
        2,
        "two call views, not a persistent loop proof"
    );
    let calls = function
        .instructions
        .iter()
        .filter_map(|instruction| {
            let Operation::DirectCall { arguments, .. } = &instruction.operation else {
                return None;
            };
            let borrowed = arguments
                .iter()
                .filter(|entity| matches!(entity, EntityId::Loan(_)))
                .filter(|entity| terminals(function, **entity) == removed)
                .copied()
                .collect::<Vec<_>>();
            (!borrowed.is_empty()).then_some((instruction, borrowed))
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.iter().map(|(_, loans)| loans.len()).sum::<usize>(), 2);
    let actual_views = calls
        .iter()
        .flat_map(|(_, loans)| loans)
        .map(|entity| {
            let EntityId::Loan(loan) = entity else {
                unreachable!()
            };
            let roots = proof_origins(function, *loan);
            assert_eq!(roots.len(), 1);
            *roots.first().unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual_views, views,
        "the first pending view crosses the second proof branch without swapping identities"
    );
    for (call, loans) in calls {
        let block = function.block(call.block).unwrap();
        let at = block
            .instructions
            .iter()
            .position(|id| *id == call.id)
            .unwrap();
        let ending = &block.instructions[at + 1..at + 1 + loans.len()];
        for (instruction, loan) in ending.iter().zip(loans.iter().rev()) {
            let EntityId::Loan(loan) = loan else {
                unreachable!()
            };
            assert_eq!(
                function.instruction(*instruction).unwrap().operation,
                Operation::BorrowEnd { loan: *loan },
                "call ends the currently rebound loan"
            );
        }
    }
    let headers = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::Compare {
                    operator: ComparisonOperator::LessThan,
                    ..
                }
            )
        })
        .map(|instruction| instruction.block)
        .collect::<Vec<_>>();
    let [header] = headers.as_slice() else {
        panic!("one actual provider header")
    };
    let header = function.block(*header).unwrap();
    let map_columns = header
        .parameters
        .iter()
        .enumerate()
        .filter_map(|(index, entity)| {
            terminals(function, *entity)
                .contains(&(remove.id, 0))
                .then_some(index)
        })
        .collect::<Vec<_>>();
    let [map_column] = map_columns.as_slice() else {
        panic!("one updated map header owner")
    };
    for parameter in &header.parameters {
        assert!(
            !terminals(function, *parameter).contains(&(remove.id, 1)),
            "removed owner and its call proofs are retired before the backedge"
        );
    }
    let incoming = function
        .blocks
        .iter()
        .flat_map(|block| block.terminator.iter().flat_map(|term| edges(&term.kind)))
        .filter(|edge| edge.target == header.id)
        .collect::<Vec<_>>();
    assert!(incoming.len() >= 2, "real preheader and body backedge");
    let backedges = incoming
        .iter()
        .filter(|edge| {
            terminals(function, edge.arguments[*map_column]) == BTreeSet::from([(remove.id, 0)])
        })
        .count();
    assert_eq!(
        backedges + 1,
        incoming.len(),
        "each non-entry edge carries the updated map, never its old seed"
    );
    let drops = function
        .instructions
        .iter()
        .filter(|instruction| match instruction.operation {
            Operation::Drop { owner } => terminals(function, EntityId::Value(owner)) == removed,
            _ => false,
        })
        .collect::<Vec<_>>();
    assert!(
        !drops.is_empty(),
        "the removed Resource keeps its lexical cleanup obligation"
    );
    assert!(!function.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::NullableTake { .. }
            | Operation::MapResultUnwrap { .. }
            | Operation::SharedRetain { .. }
    )));
    let ir =
        crate::llvm::render_verified_program(&program).expect("unchanged LLVM adapter verifies");
    let mut block = "";
    let mut slots = 0;
    let mut stores = 0;
    for line in ir.lines() {
        if !line.starts_with(' ') && line.contains(':') {
            block = line.split(':').next().unwrap();
        }
        if line.contains("map.payload.view") && line.contains(" = alloca ") {
            assert_eq!(
                block, "bb0",
                "static non-owning view storage is allocated in entry, not per iteration"
            );
            slots += 1;
        }
        if line.contains("map.payload.view") && line.trim_start().starts_with("store ") {
            stores += 1;
        }
    }
    assert_eq!(slots, 2);
    assert_eq!(
        stores, 2,
        "each static branch refreshes its current handle on every execution"
    );
    (program, id)
}

#[test]
fn resource_remove_loop_original_source_keeps_owner_proof_and_backedge_identity() {
    let source = analyzed(ORIGINAL);
    let OwnedProducts::Basic(typed, owned) = &source.products else {
        unreachable!()
    };
    let owned = owned.ownership();
    let [plan] = owned.iterations() else {
        panic!("one real provider plan")
    };
    let statement = source
        .parsed
        .ast()
        .statements()
        .get(plan.descriptor().statement().statement())
        .unwrap();
    let Statement::For { body, .. } = statement.payload() else {
        panic!("real for")
    };
    let body_point =
        UnitDropPoint::AfterStatement(lang_frontend::type_checking::UnitStatementId::new(
            plan.descriptor().statement().source_unit(),
            *body,
        ));
    let root = owned
        .loans()
        .iter()
        .find_map(|fact| match fact.target() {
            lang_frontend::ownership_checking::UnitLoanTarget::Place(place)
                if place.is_root()
                    && source.names.names().source_units()[place.root().source_unit().index()]
                        .resolution()
                        .symbols()[place.root().symbol().index()]
                    .name()
                        == "removed" =>
            {
                Some(place.root())
            }
            _ => None,
        })
        .expect("the actual published call-loan root, not a reconstructed UnitSymbolId");
    assert_eq!(
        typed
            .types()
            .is_resource_type(typed.types().symbol_type(root).unwrap()),
        Some(true)
    );
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert_eq!(
        drops[0].point(),
        body_point,
        "Resource does not become ASAP at the second observe"
    );
    let (program, id) = assert_loop_protocol(&source, "entry");
    let function = program.module(id.module()).unwrap().function(id).unwrap();
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::StringClone { .. }))
            .count(),
        2,
        "only the two explicit source clones exist"
    );
}

#[test]
fn resource_remove_loop_original_and_zero_iterations_have_exact_native_output() {
    assert_success(
        &emit_link_and_run("resource-loop-original.ko", ORIGINAL, "entry"),
        ORIGINAL_STDOUT,
    );
    let zero = ORIGINAL.replace("arrayOf(1, 2, 3)", "arrayOf<Int>()");
    analyzed(&zero);
    assert_success(
        &emit_link_and_run("resource-loop-zero.ko", &zero, "entry"),
        b"end\ndrop\ndrop\n",
    );
}

fn interrupted(jump: &str) -> String {
    format!(
        r#"
class Resource(val text: String) {{ deinit() {{ println("drop") }} }}
fun pair(first: Resource, second: Resource, own marker: Int) {{
 println(first.text); println(second.text)
}}
fun run(stop: Int) {{
 var values = mutableMapOf<Int, Resource>()
 values.put(1, Resource("first".clone()))
 for (key in arrayOf(1, 2, 3)) {{
   val removed = values.remove(key)
   if (removed != null) {{
     pair(removed, (removed), if (key == stop) {{ {jump} }} else {{ 0 }})
   }} else {{ println("missing") }}
   println("iteration")
 }}
 println("end")
}}
fun entry() {{ run(0); run(1); println("done") }}
"#
    )
}

#[test]
fn resource_remove_loop_two_pending_views_end_before_each_early_exit() {
    for jump in ["continue", "break", "return"] {
        let source = analyzed(&interrupted(jump));
        let OwnedProducts::Basic(_, owned) = &source.products else {
            unreachable!()
        };
        let owned = owned.ownership();
        let [plan] = owned.iterations() else {
            panic!("one real loop plan")
        };
        let exits = plan
            .exits()
            .iter()
            .filter(|exit| matches!(exit.point(), UnitDropPoint::ControlTransfer(_)))
            .collect::<Vec<_>>();
        let [exit] = exits.as_slice() else {
            panic!("one real early exit")
        };
        let ends = exit
            .actions()
            .iter()
            .enumerate()
            .filter_map(|(index, action)| matches!(action, Action::EndCallLoan(_)).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(
            ends.len(),
            2,
            "both successfully evaluated Borrow operands end on {jump}"
        );
        let drop = exit
            .actions()
            .iter()
            .position(|action| {
                matches!(action, Action::Drop(fact)
            if matches!(fact.target(), UnitDropTarget::Named(root)
                if source.names.names().source_units()[root.source_unit().index()].resolution()
                    .symbols()[root.symbol().index()].name() == "removed"))
            })
            .unwrap();
        let element = exit
            .actions()
            .iter()
            .position(|action| matches!(action, Action::EndElement(_)))
            .unwrap();
        assert!(
            ends[1] < drop && drop < element,
            "actual frontend cleanup order, not reconstructed order"
        );
        let (program, id) = assert_loop_protocol(&source, "run");
        let function = program.module(id.module()).unwrap().function(id).unwrap();
        let remove = function
            .instructions
            .iter()
            .find(|instruction| matches!(instruction.operation, Operation::MapRemove { .. }))
            .unwrap();
        let removed = BTreeSet::from([(remove.id, 1)]);
        let ending = function
            .instructions
            .iter()
            .filter(|instruction| {
                let Operation::BorrowEnd { loan } = instruction.operation else {
                    return false;
                };
                terminals(function, EntityId::Loan(loan)) == removed
                    && !function
                        .block(instruction.block)
                        .unwrap()
                        .instructions
                        .iter()
                        .any(|id| {
                            matches!(
                                function.instruction(*id).unwrap().operation,
                                Operation::DirectCall { .. }
                            )
                        })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ending.len(),
            2,
            "only the two current call views end on the interrupted path"
        );
        assert_eq!(ending[0].block, ending[1].block);
        let views = function
            .blocks
            .iter()
            .filter_map(
                |block| match block.terminator.as_ref().map(|term| &term.kind) {
                    Some(TerminatorKind::NullableBranch { view, .. }) => Some(*view),
                    _ => None,
                },
            )
            .collect::<Vec<_>>();
        for (instruction, expected) in ending.iter().zip(views.iter().rev()) {
            let Operation::BorrowEnd { loan } = instruction.operation else {
                unreachable!()
            };
            assert_eq!(
                proof_origins(function, loan),
                BTreeSet::from([*expected]),
                "early exit ends the actual rebound proofs in reverse source order"
            );
        }
        let end_spans = exit
            .actions()
            .iter()
            .filter_map(|action| match action {
                Action::EndCallLoan(fact) => Some(fact.end_span()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ending
                .iter()
                .map(|instruction| instruction.origin.span())
                .collect::<Vec<_>>(),
            end_spans
        );
        let block = function.block(ending[0].block).unwrap();
        let drop = block
            .instructions
            .iter()
            .position(|id| match function.instruction(*id).unwrap().operation {
                Operation::Drop { owner } => terminals(function, EntityId::Value(owner)) == removed,
                _ => false,
            })
            .expect("the same current removed owner drops on this path");
        let second_end = block
            .instructions
            .iter()
            .position(|id| *id == ending[1].id)
            .unwrap();
        assert!(
            second_end < drop,
            "LoanEnd precedes Resource cleanup, not a stale entry loan"
        );
    }
}

#[test]
fn resource_remove_loop_pending_early_exits_preserve_native_cleanup() {
    let normal = "first\nfirst\niteration\ndrop\nmissing\niteration\nmissing\niteration\nend\n";
    for (jump, exit) in [
        (
            "continue",
            "drop\nmissing\niteration\nmissing\niteration\nend\n",
        ),
        ("break", "drop\nend\n"),
        ("return", "drop\n"),
    ] {
        let text = interrupted(jump);
        let expected = format!("{normal}{exit}done\n");
        assert_success(
            &emit_link_and_run("resource-loop-jump.ko", &text, "entry"),
            expected.as_bytes(),
        );
    }
}

#[test]
fn resource_remove_loop_unqualified_loop_and_payload_remain_precise_boundaries() {
    for text in [
        ORIGINAL
            .replace("for (key in arrayOf(1, 2, 3))", "loop")
            .replace("values.remove(key)", "values.remove(1)")
            .replace("println(\"iteration\")", "println(\"iteration\"); break"),
        ORIGINAL.replace("deinit() { println(\"drop\") }", ""),
        interrupted("loop { break }; 0"),
    ] {
        let source = analyzed(&text);
        let error = source
            .lower_entry(source.entry())
            .err()
            .expect("outside the bounded Resource for slice");
        assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
        assert_eq!(
            source
                .sources
                .slice(error.span.expect("source-located boundary"))
                .unwrap(),
            "removed"
        );
    }
}

#[test]
fn resource_remove_loop_cancelled_frame_does_not_reappear_in_next_loop() {
    let text = r#"
class Resource(val text: String) { deinit() { println("drop") } }
fun observe(value: Resource) { println(value.text) }
fun pair(first: Resource, second: Resource, own marker: Int) {
 println(first.text); println(second.text)
}
fun entry() {
 var values = mutableMapOf<Int, Resource>()
 values.put(1, Resource("first".clone())); values.put(2, Resource("second".clone()))
 for (key in arrayOf(1)) {
   val removed = values.remove(key)
   if (removed != null) { pair(removed, removed, if (key == 1) { break } else { 0 }) }
 }
 println("between")
 for (key in arrayOf(2, 3)) {
   val removed = values.remove(key)
   if (removed != null) { observe(removed); observe(removed) } else { println("missing") }
   println("iteration")
 }
 println("end")
}
"#;
    let source = analyzed(text);
    let (program, _) = source
        .lower_entry(source.entry())
        .expect("no cancelled frame is restored into the following independent for");
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    assert_success(
        &emit_link_and_run("resource-loop-next.ko", text, "entry"),
        b"drop\nbetween\nsecond\nsecond\niteration\ndrop\nmissing\niteration\nend\n",
    );
}
