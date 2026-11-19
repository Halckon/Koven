//! Same-source caller cleanup identities. Literal owners do not imply heap allocations.
use crate::ssa::model::*;

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one matching identity");
    assert!(values.next().is_none(), "no duplicate identity or cleanup");
    value
}

fn value(instruction: &Instruction) -> ValueId {
    let [EntityId::Value(value)] = instruction.results.as_slice() else {
        panic!("one actual value result")
    };
    *value
}

fn loan(instruction: &Instruction) -> LoanId {
    let [EntityId::Loan(loan)] = instruction.results.as_slice() else {
        panic!("one actual loan result")
    };
    *loan
}

#[test]
fn shared_replacement_callback_keeps_both_sources_until_invocation_returns() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;
    // Byte-identical to the private frontend lifecycle fixture; no wrapper or extra read.
    const SOURCE: &str = "fun run(): Unit {\nvar a = \"old\"\nval old = replace(&a, \"new\")\nval callback = { a + old }\ncallback()\n}";
    let unit = UnitSource::analyze("root.ko", SOURCE).unwrap();
    unit.assert_frontend_clean();
    let (program, entry) = unit
        .lower_entry(unit.declaration("run", SymbolKind::Function))
        .unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    let module = program.module(entry.module()).unwrap();
    let caller = module.function(entry).unwrap();
    assert_eq!(
        caller.blocks.len(),
        1,
        "original source has no control split"
    );
    let literal = |text: &[u8]| {
        value(only(caller.instructions.iter().filter(|i| {
            matches!(&i.operation,
            Operation::StringLiteral { bytes, .. } if bytes == text)
        })))
    };
    let replacement = only(
        caller
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::RootReplace { .. })),
    );
    let Operation::RootReplace {
        owner,
        replacement: incoming,
        ..
    } = replacement.operation
    else {
        unreachable!()
    };
    assert_eq!(owner, literal(b"old"));
    assert_eq!(incoming, literal(b"new"));
    let [EntityId::Value(current), EntityId::Value(previous)] = replacement.results.as_slice()
    else {
        panic!("new root and detached old owner")
    };
    assert_ne!(current, previous);
    let formed = only(
        caller
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::ClosureConstruct { .. })),
    );
    let Operation::ClosureConstruct {
        thunk, captures, ..
    } = &formed.operation
    else {
        unreachable!()
    };
    assert_eq!(captures.len(), 2);
    let captures = captures
        .iter()
        .map(|capture| match capture {
            ClosureCaptureOperand::Shared(loan) => *loan,
            _ => panic!("original shared captures must not consume their sources"),
        })
        .collect::<Vec<_>>();
    for (capture, expected_owner) in captures.iter().zip([current, previous]) {
        let begin = only(
            caller
                .instructions
                .iter()
                .filter(|i| i.results == [EntityId::Loan(*capture)]),
        );
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            panic!("real shared source loan")
        };
        let root = only(
            caller
                .instructions
                .iter()
                .filter(|i| i.results == [EntityId::Place(place)]),
        );
        assert_eq!(
            root.operation,
            Operation::RootPlace {
                owner: *expected_owner
            }
        );
    }
    let closure = value(formed);
    let invoked = only(caller.instructions.iter().filter(|i| {
        matches!(i.operation, Operation::CallableInvoke { callable, .. } if callable == closure)
    }));
    let position = |instruction: &Instruction| {
        caller.blocks[0]
            .instructions
            .iter()
            .position(|id| *id == instruction.id)
            .unwrap()
    };
    let drop_position = |owner| {
        position(only(
            caller
                .instructions
                .iter()
                .filter(|i| i.operation == Operation::Drop { owner }),
        ))
    };
    assert!(position(replacement) < position(formed));
    assert!(position(formed) < position(invoked));
    let closure_end = drop_position(closure);
    assert!(position(invoked) < closure_end);
    // Closure Drop implicitly ends both capture loans; never emit a second explicit end.
    assert!(caller.instructions.iter().all(|i| !matches!(i.operation,
        Operation::BorrowEnd { loan } if captures.contains(&loan))));
    for owner in [*current, *previous] {
        assert!(closure_end < drop_position(owner));
    }
    assert!(position(invoked) < drop_position(value(invoked)));
    assert_eq!(
        caller
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::Drop { .. }))
            .count(),
        4
    );

    let body = module.function(*thunk).unwrap();
    assert_eq!(body.blocks.len(), 1);
    let [EntityId::Loan(environment)] = body.blocks[0].parameters.as_slice() else {
        panic!("borrowed environment parameter")
    };
    let reads = (0..2)
        .map(|field| {
            let projected = loan(only(body.instructions.iter().filter(|i| {
                i.operation
                    == Operation::SharedFieldLoan {
                        base: *environment,
                        field,
                    }
            })));
            loan(only(body.instructions.iter().filter(|i| {
                i.operation == Operation::SharedReferenceFollow { source: projected }
            })))
        })
        .collect::<Vec<_>>();
    let concat = only(
        body.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::StringConcat { .. })),
    );
    assert_eq!(
        concat.operation,
        Operation::StringConcat {
            left: EntityId::Loan(reads[0]),
            right: EntityId::Loan(reads[1])
        }
    );
    assert!(
        body.instructions
            .iter()
            .all(|i| !matches!(i.operation, Operation::Drop { .. })),
        "thunk borrows, never destroys captured owners"
    );
    let TerminatorKind::Return { values } = &body.blocks[0].terminator.as_ref().unwrap().kind
    else {
        panic!("original concatenation returns its new result")
    };
    assert_eq!(values, &[value(concat)]);
}

#[test]
fn heap_replacement_shared_callback_preserves_contents_and_release_order() {
    use crate::native_tests::boxed_enum_tests::{
        assert_success, lower_to_llvm, run_counted_allocations_in_order,
    };
    // A separate runtime variant, not a modification of the original literal fixture.
    // Parameters prevent frontend constant materialization of the two source strings.
    const SOURCE: &str = r#"fun run(oldPrefix: String, newPrefix: String): Unit {
        var a = oldPrefix + "ld"
        println(a)
        val old = replace(&a, newPrefix + "ew")
        val callback = { a + old }
        val result = callback()
        println(result)
        println(old)
        println(a)
    }
    fun entry(): Unit { run("o", "n") }
    "#;
    const STDOUT: &[u8] = b"old\nnewold\nold\nnew\n";
    // First exercise the ordinary emitter/linker without allocator substitution.
    let ordinary =
        crate::native_tests::emit_link_and_run("heap-replacement-callback.ko", SOURCE, "entry");
    assert_success(&ordinary, STDOUT);
    let llvm = lower_to_llvm("heap-replacement-callback-counted.ko", SOURCE);
    // Allocation ordinals follow source evaluation: old concatenation (0), incoming
    // concatenation (1), callback result (2). RootReplace transfers, and shared
    // capture uses inline/stack representation; neither allocates another heap owner.
    // Result, detached old, and current root reach their final reads in that order.
    // Both last-use cleanup and reverse lexical cleanup therefore require [2,0,1].
    // The helper rejects duplicate/untracked frees and checks every allocation freed.
    assert_success(&run_counted_allocations_in_order(&llvm, &[2, 0, 1]), STDOUT);
}

#[test]
fn heap_replacement_shared_callback_cleans_uninvoked_sources_on_return() {
    use crate::native_tests::boxed_enum_tests::{
        assert_success, lower_to_llvm, run_counted_allocations_in_order,
    };
    // Separate from the original fixture: stop is a parameter, so both paths remain
    // in run. The explicit marker distinguishes non-invocation from a lost result.
    const BODY: &str = r#"fun run(oldPrefix: String, newPrefix: String, stop: Boolean): Unit {
        var a = oldPrefix + "ld"
        println(a)
        val old = replace(&a, newPrefix + "ew")
        val callback = { println("invoked"); a + old }
        if (stop) { println("early"); return }
        val result = callback()
        println(result)
        println(old)
        println(a)
    }
    "#;
    for (stop, stdout, order) in [
        (true, b"old\nearly\n".as_slice(), &[0, 1][..]),
        (
            false,
            b"old\ninvoked\nnewold\nold\nnew\n".as_slice(),
            &[2, 0, 1][..],
        ),
    ] {
        let source = format!("{BODY}\nfun entry(): Unit {{ run(\"o\", \"n\", {stop}) }}");
        assert_success(
            &crate::native_tests::emit_link_and_run(
                "shared-callback-early-return.ko",
                &source,
                "entry",
            ),
            stdout,
        );
        let llvm = lower_to_llvm("shared-callback-early-return-counted.ko", &source);
        // Original/incoming strings allocate 0/1 before the branch. Early return
        // drops the shared closure (ending its loans), then detached old, then a.
        // It never allocates result 2. Fallthrough first releases that result.
        // The shared environment is a real closure owner, but owns no source heap.
        assert_success(&run_counted_allocations_in_order(&llvm, order), stdout);
    }
}
