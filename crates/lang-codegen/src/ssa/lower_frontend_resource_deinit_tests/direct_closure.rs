//! Original direct resource closure through strict Unit, alongside the Legacy witness.

use super::{
    EntityId, EntityType, Operation, SsaTypeKind, lower_resource, render_verified_program,
};

#[test]
fn original_owned_resource_closure_keeps_body_and_environment_cleanup() {
    let source = "class Resource { deinit() {} }\nfun run(): Unit { val text = \"keep\"; val callback = move { println(text); val resource = Resource() }; callback() }";
    assert_cleanup(source, 1);
}

#[test]
fn grouped_alias_and_repeated_calls_share_the_original_resource_thunk() {
    assert_cleanup(
        "class Resource { deinit() {} }\nfun run(): Unit { val text = \"keep\"; val callback = move { println(text); val resource = Resource() }; val alias = callback; (alias)(); (alias)() }",
        2,
    );
}

fn assert_cleanup(source: &str, invocations: usize) {
    let unit = lower_resource(source, "run");
    assert!(unit.source.ownership().deferred().is_empty());
    assert_eq!(
        unit.source
            .sources()
            .source_text(unit.source.parsed().source_id()),
        Ok(source)
    );
    let module = unit.module();
    let entry = unit.entry();
    let constructions = entry
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::ClosureConstruct { closure, thunk, .. } => Some((closure, thunk)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(closure_type, thunk)] = constructions.as_slice() else {
        panic!("the original callback forms exactly one concrete owned closure");
    };
    let thunk = module
        .function(*thunk)
        .expect("the constructed closure selects its actual body thunk");
    assert_eq!(
        thunk
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
            .count(),
        1
    );
    let body_drops = thunk
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::Drop { owner } = instruction.operation {
                Some(thunk.entity(EntityId::Value(owner)).unwrap().ty)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [EntityType::Value(resource)] = body_drops.as_slice() else {
        panic!("the callback's local Resource must have one drop")
    };
    assert!(matches!(
        module.type_kind(*resource),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    assert_eq!(module.deinit(*resource), Some(unit.deinit("Resource").id()));
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::CallableInvoke { .. }))
            .count(),
        invocations
    );
    let entry_drops = entry
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::Drop { owner } = instruction.operation {
                Some(entry.entity(EntityId::Value(owner)).unwrap().ty)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [EntityType::Value(closure)] = entry_drops.as_slice() else {
        panic!("the closure must independently drop its owned String environment once")
    };
    assert_eq!(closure, closure_type);
    let Some(SsaTypeKind::ConcreteClosure { captures, .. }) = module.type_kind(*closure) else {
        panic!("the entry drops its concrete closure environment");
    };
    let [capture] = captures.as_slice() else {
        panic!("the closure retains exactly the original text capture");
    };
    assert_eq!(capture.mode, crate::ssa::model::ClosureCaptureMode::Owned);
    assert_eq!(
        module.type_kind(capture.ty),
        Some(&SsaTypeKind::StringOwner)
    );
    render_verified_program(&unit.program)
        .expect("body Resource and owned environment cleanup verify through LLVM");
}

#[test]
fn resource_lambdas_without_a_dispatch_keep_their_exact_existing_boundary() {
    use super::{LoweringErrorKind, UnitSource};
    use lang_frontend::name_resolution::SymbolKind;

    for source in [
        "class Resource { deinit() {} }\nfun run(): Unit { val callback = move { val resource = Resource() } }",
        "class Resource { deinit() {} }\nfun run(): Unit { val callback: move (Int) -> Unit = move { value -> val resource = Resource() }; callback(return) }",
    ] {
        let unit =
            UnitSource::analyze("lowering.ko", source).expect("original strict Unit frontend");
        unit.assert_frontend_clean();
        assert!(unit.ownership().deferred().is_empty());
        let failure = unit
            .lower_entry(unit.declaration("run", SymbolKind::Function))
            .err()
            .expect("unselected resource lambdas remain outside this bounded slice");
        assert_eq!(failure.kind, LoweringErrorKind::UnsupportedNode);
        let span = failure.span.expect("original resource construction span");
        let start = source.find("Resource()").unwrap();
        assert_eq!(span.source_id(), unit.parsed().source_id());
        assert_eq!(
            (span.start(), span.end()),
            (start, start + "Resource()".len())
        );
        assert_eq!(unit.sources().slice(span), Ok("Resource()"));
    }
}

#[test]
fn invoked_resource_closure_composes_with_borrow_initializer_prefix_exit() {
    for (argument, has_borrow_call) in
        [("return", false), ("if (flag) { return } else { 1 }", true)]
    {
        let source = format!(
            r#"class Resource {{ deinit() {{}} }}
fun view(source: String, own n: Int): borrow String from source = source
fun run(own flag: Boolean): Unit {{
    val text = "capture"
    val callback = move {{ println(text); val resource = Resource() }}
    callback()
    val owner = "kept".clone()
    borrow val item = view(owner, {argument})
    println(item)
}}"#
        );
        let unit = lower_resource(&source, "run");
        assert!(unit.source.ownership().deferred().is_empty());
        let entry = unit.entry();
        assert_eq!(
            entry
                .instructions
                .iter()
                .filter(|instruction| matches!(
                    instruction.operation,
                    Operation::CallableInvoke { .. }
                ))
                .count(),
            1
        );
        let thunk = entry
            .instructions
            .iter()
            .find_map(|instruction| match instruction.operation {
                Operation::ClosureConstruct { thunk, .. } => unit.module().function(thunk),
                _ => None,
            })
            .expect("the dispatched resource closure keeps its actual thunk");
        let resources = thunk
            .instructions
            .iter()
            .filter_map(|instruction| {
                if let Operation::Drop { owner } = instruction.operation {
                    Some(thunk.entity(EntityId::Value(owner)).unwrap().ty)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let [EntityType::Value(resource)] = resources.as_slice() else {
            panic!("the invoked closure must retain one Resource drop");
        };
        assert_eq!(
            unit.module().deinit(*resource),
            Some(unit.deinit("Resource").id())
        );
        assert_eq!(
            entry
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
                .count(),
            usize::from(has_borrow_call)
        );
        if has_borrow_call {
            assert_borrow_prefix_exit(entry);
        } else {
            assert!(
                unit.module().functions.iter().all(|function| {
                    function.origin
                        != crate::ssa::model::Origin::Source(unit.source.declaration_span(
                            unit.source.declaration(
                                "view",
                                lang_frontend::name_resolution::SymbolKind::Function,
                            ),
                        ))
                }),
                "an abandoned initializer must not instantiate its callee"
            );
        }
        render_verified_program(&unit.program).expect("resource closure and borrow prefix compose");
    }
}

#[test]
fn dispatched_resource_lambda_does_not_admit_its_unselected_sibling() {
    use super::{LoweringErrorKind, UnitSource};
    use lang_frontend::name_resolution::SymbolKind;

    for call in ["", "unselected(return)"] {
        let source = format!(
            r#"class Resource {{ deinit() {{}} }}
fun run(): Unit {{
    val selected = move {{ val first = Resource() }}
    selected()
    val unselected: move (Int) -> Unit = move {{ value -> val second = Resource() }}
    {call}
}}"#
        );
        let unit = UnitSource::analyze("sibling.ko", &source).expect("strict Unit frontend");
        unit.assert_frontend_clean();
        assert!(unit.ownership().deferred().is_empty());
        let error = unit
            .lower_entry(unit.declaration("run", SymbolKind::Function))
            .err()
            .expect("only the invoked lambda may contribute resource plans");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        let span = error.span.expect("unselected Resource construction");
        let start = source.rfind("Resource()").unwrap();
        assert_eq!(span.source_id(), unit.parsed().source_id());
        assert_eq!(
            (span.start(), span.end()),
            (start, start + "Resource()".len())
        );
        assert_eq!(unit.sources().slice(span), Ok("Resource()"));
    }
}

fn assert_borrow_prefix_exit(entry: &crate::ssa::model::Function) {
    use crate::ssa::model::TerminatorKind;

    let (return_edge, normal_edge) = entry
        .blocks
        .iter()
        .find_map(|block| {
            let TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } = &block.terminator.as_ref()?.kind
            else {
                return None;
            };
            Some((when_true, when_false))
        })
        .expect("borrow initializer argument conditional");
    assert_eq!(return_edge.arguments, normal_edge.arguments);
    let returning = &entry.blocks[return_edge.target.index()];
    let normal = &entry.blocks[normal_edge.target.index()];
    let [EntityId::Value(owner), EntityId::Loan(returning_loan)] = returning.parameters.as_slice()
    else {
        panic!("return arm receives the owner and its source loan")
    };
    let [EntityId::Value(_), EntityId::Loan(normal_loan)] = normal.parameters.as_slice() else {
        panic!("normal arm receives the owner and its source loan")
    };
    assert!(matches!(
        returning.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { .. }
    ));
    let dispatches = entry
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
        .collect::<Vec<_>>();
    let [dispatch] = dispatches.as_slice() else {
        panic!("only the normal arm dispatches the borrow-result call")
    };
    assert_eq!(dispatch.block, normal.id);
    assert!(
        matches!(dispatch.operation, Operation::BorrowCall { source, .. } if source == *normal_loan)
    );
    let exit = returning
        .instructions
        .iter()
        .map(|id| &entry.instructions[id.index()])
        .collect::<Vec<_>>();
    let loan_end = exit
        .iter()
        .position(|instruction| {
            instruction.operation
                == Operation::BorrowEnd {
                    loan: *returning_loan,
                }
        })
        .expect("return arm ends its exact source loan");
    let owner_drop = exit
        .iter()
        .position(|instruction| instruction.operation == Operation::Drop { owner: *owner })
        .expect("return arm drops its exact owner");
    assert!(loan_end < owner_drop);
}

pub(in crate::ssa) mod field_chain {
//! Strong source-to-SSA identity oracle shared by readonly field-copy and call tests.

use super::super::ResourceUnit;
use crate::ssa::model::{
    EntityId, EntityType, Function, LoanKind, Operation, PlaceAccess, SsaTypeKind,
};

pub(in crate::ssa) fn assert_nested_this_field_chain(
    unit: &ResourceUnit,
    body: &Function,
    expression: &str,
    fields: &[usize],
    callee: &Function,
    copied: bool,
) {
    let block = body.block(body.entry_block().unwrap()).unwrap();
    let [EntityId::Loan(receiver)] = block.parameters.as_slice() else {
        panic!("the destructor has only its original shared this loan");
    };
    let Some(EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    }) = body.receiver()
    else {
        panic!("the registered destructor has a shared receiver");
    };
    assert_eq!(
        body.entity(EntityId::Loan(*receiver)).unwrap().ty,
        body.receiver().unwrap()
    );
    assert_eq!(unit.module().deinit(target), Some(body.id()));
    let instructions = block
        .instructions
        .iter()
        .map(|id| body.instruction(*id).unwrap())
        .collect::<Vec<_>>();
    let calls = instructions.iter().enumerate().filter(|(_, instruction)| matches!(
        instruction.operation, Operation::DirectCall { callee: target, .. } if target == callee.id()
    )).collect::<Vec<_>>();
    let [(call_index, call)] = calls.as_slice() else {
        panic!("one actual call to the original source helper");
    };
    let start = call_index
        .checked_sub(if copied {
            fields.len() * 2 + 1
        } else {
            fields.len()
        })
        .expect("projection chain precedes its call");
    let mut parent = *receiver;
    let mut parent_type = target;
    let mut loans = Vec::new();
    for (offset, field) in fields.iter().enumerate() {
        let instruction = instructions[start + offset];
        assert!(matches!(instruction.operation,
            Operation::SharedHeapFieldLoan { base, field: actual } if base == parent && actual == *field));
        assert_eq!(
            unit.source.sources().slice(instruction.origin.span()),
            Ok(expression)
        );
        let [EntityId::Loan(child)] = instruction.results.as_slice() else {
            panic!("each projection creates exactly one actual child loan");
        };
        let Some(SsaTypeKind::HeapOwner {
            payload: Some(payload),
            ..
        }) = unit.module().type_kind(parent_type)
        else {
            panic!("each parent is the previous concrete ordinary class owner");
        };
        let Some(SsaTypeKind::Aggregate { fields, .. }) = unit.module().type_kind(*payload) else {
            panic!("ordinary class has its concrete payload");
        };
        parent_type = fields[*field];
        assert_eq!(
            body.entity(EntityId::Loan(*child)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: parent_type
            }
        );
        loans.push(*child);
        parent = *child;
    }
    let argument = if copied {
        let read = instructions[start + fields.len()];
        assert_eq!(
            read.operation,
            Operation::Read {
                source: PlaceAccess::Loan(parent)
            }
        );
        assert_eq!(
            unit.source.sources().slice(read.origin.span()),
            Ok(expression)
        );
        let [value @ EntityId::Value(_)] = read.results.as_slice() else {
            panic!("the leaf read alone produces the owned copy");
        };
        assert_eq!(
            body.entity(*value).unwrap().ty,
            EntityType::Value(parent_type)
        );
        *value
    } else {
        EntityId::Loan(parent)
    };
    assert!(
        matches!(&call.operation, Operation::DirectCall { receiver: None, arguments, .. }
        if arguments.as_slice() == [argument])
    );
    let end_start = if copied {
        start + fields.len() + 1
    } else {
        call_index + 1
    };
    for (offset, loan) in loans.iter().rev().enumerate() {
        assert_eq!(
            instructions[end_start + offset].operation,
            Operation::BorrowEnd { loan: *loan }
        );
        assert_eq!(
            body.instructions
                .iter()
                .filter(|instruction| instruction.operation == Operation::BorrowEnd { loan: *loan })
                .count(),
            1
        );
    }
    assert!(
        !body
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation,
        Operation::BorrowEnd { loan } if loan == *receiver))
    );
    assert!(!body.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::Drop { .. }
            | Operation::Consume { .. }
            | Operation::HeapAllocate { .. }
            | Operation::HeapPayloadPlace { .. }
    )));
    assert!(
        !body
            .instructions
            .iter()
            .flat_map(|instruction| &instruction.results)
            .any(
                |result| matches!(body.entity(*result).unwrap().ty, EntityType::Value(ty)
            if matches!(unit.module().type_kind(ty), Some(SsaTypeKind::HeapOwner { .. })))
            )
    );
}
}

pub(in crate::ssa) mod nullable_return {
//! Pending nullable text receivers keep normal and early-return cleanup distinct.
use crate::ssa::text_copy_tests::*;

#[test]
fn public_text_nullable_receiver_argument_return_releases_the_pending_view() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun observe(stop: Boolean): Unit {
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return } else { "!" })
        if (copy == null) { error("unexpected overflow") }
    }
    println("completed")
}
fun main(): Unit { observe(true); observe(false) }
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    // Both calls allocate joined, but only the completed call allocates its result.
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 3);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"completed\n");
}

#[test]
fn public_text_nullable_receiver_both_argument_branches_cancel_the_call() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun observe(stop: Boolean): Unit {
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return } else { return })
    }
}
fun main(): Unit { observe(true); observe(false); println("cancelled") }
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 2);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"cancelled\n");
}

#[test]
fn public_text_nullable_receiver_return_preserves_prior_argument_and_copyable_result() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
import koven.text.sliceBytes
fun observe(input: String, stop: Boolean): Int {
    val joined = input.checkedPlus("b")
    var seen = 0
    if (joined != null) {
        val copy = joined.sliceBytes(
            if (stop) { seen = seen + 1; 0 } else { seen = seen + 2; 0 },
            if (stop) { return seen } else { 1 })
    }
    return seen
}
fun main(): Unit {
    val input = "a".clone()
    if (observe(input, true) != 1 || observe(input, false) != 2) { error("argument order") }
    println(input)
    println("ok")
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 4);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"a\nok\n");
}

#[test]
fn public_text_nullable_if_both_returns_preserve_the_null_edge() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
import koven.text.byteLength
fun choose(present: Boolean, stop: Boolean): Int {
    val joined = if (present) { "a".checkedPlus("b") } else { null }
    if (joined != null) {
        if (stop) { return joined.byteLength() + 15 } else { return joined.byteLength() + 27 }
    }
    return 0
}
fun main(): Unit {
    if (choose(true, true) != 17 || choose(true, false) != 29 ||
        choose(false, true) != 0 || choose(false, false) != 0) { error("return edge") }
    println("ok")
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 2);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"ok\n");
}

#[test]
fn public_text_nullable_if_returns_retire_only_the_taken_path() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
import koven.text.byteLength
fun unitExit(input: String, stop: Boolean): Unit {
    val joined = input.checkedPlus("b")
    if (joined != null) {
        if (stop) { return }
        println(joined)
    }
    println("completed")
}
fun intExit(input: String, stop: Boolean): Int {
    val joined = input.checkedPlus("b")
    if (joined != null) {
        if (stop) { return joined.byteLength() }
        println(joined)
        return 29
    }
    return 0
}
fun main(): Unit {
    val input = "a".clone()
    unitExit(input, true)
    unitExit(input, false)
    if (intExit(input, true) != 2 || intExit(input, false) != 29) { error("return value") }
    println(input)
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 5);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"ab\ncompleted\nab\na\n");
}

#[test]
fn public_text_nullable_if_returns_independent_string_owners() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun direct(stop: Boolean): String {
    val result = "direct".clone()
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        if (stop) { return result }
        println(joined)
    }
    return result
}
fun argument(stop: Boolean): String {
    val result = "argument".clone()
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return result } else { "!" })
        if (copy == null) { error("overflow") }
    }
    return result
}
fun main(): Unit {
    println(direct(true))
    println(direct(false))
    println(argument(true))
    println(argument(false))
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 9);
    crate::native_tests::boxed_enum_tests::assert_success(
        &run,
        b"direct\nab\ndirect\nargument\nargument\n",
    );
}

#[test]
fn public_text_nullable_if_returns_literals_through_existing_nullable_wrap() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun direct(stop: Boolean): String? {
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        if (stop) { return "early" }
        println(joined)
    }
    return "late"
}
fun argument(stop: Boolean): String? {
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return "early" } else { "!" })
        if (copy == null) { error("overflow") }
    }
    return "late"
}
fun main(): Unit {
    val first = direct(true)
    if (first != null) { println(first) }
    val second = direct(false)
    if (second != null) { println(second) }
    val third = argument(true)
    if (third != null) { println(third) }
    val fourth = argument(false)
    if (fourth != null) { println(fourth) }
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 5);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"early\nab\nlate\nearly\nlate\n");
}

#[test]
fn public_text_nullable_if_returns_independent_array_and_result_owners() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
import koven.text.toBytes
import koven.text.sliceBytes
import koven.text.TextError
fun bytes(stop: Boolean): Array<UByte> {
    val result = "é".toBytes()
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        if (stop) { return result }
        println(joined)
    }
    return result
}
fun sliced(stop: Boolean): Result<String, TextError> {
    val result = "slice".sliceBytes(0, 5)
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return result } else { "!" })
        if (copy == null) { error("overflow") }
    }
    return result
}
fun main(): Unit {
    val first = bytes(true)
    val second = bytes(false)
    val lead: UByte = 195u
    val continuation: UByte = 169u
    if (first.size != 2 || first[0] != lead || first[1] != continuation ||
        second.size != 2 || second[0] != lead || second[1] != continuation) { error("bytes") }
    val third = sliced(true)
    when (third) {
        is Ok -> println(third.success)
        is Err -> error("slice")
    }
    val fourth = sliced(false)
    when (fourth) {
        is Ok -> println(fourth.success)
        is Err -> error("slice")
    }
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 9);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"ab\nslice\nslice\n");
}

#[test]
fn public_text_nullable_if_wraps_allocated_returned_strings() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun direct(stop: Boolean): String? {
    val result = "direct".clone()
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        if (stop) { return result }
        println(joined)
    }
    return result
}
fun argument(stop: Boolean): String? {
    val result = "argument".clone()
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return result } else { "!" })
        if (copy == null) { error("overflow") }
    }
    return result
}
fun main(): Unit {
    val first = direct(true)
    if (first != null) { println(first) }
    val second = direct(false)
    if (second != null) { println(second) }
    val third = argument(true)
    if (third != null) { println(third) }
    val fourth = argument(false)
    if (fourth != null) { println(fourth) }
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 9);
    crate::native_tests::boxed_enum_tests::assert_success(
        &run,
        b"direct\nab\ndirect\nargument\nargument\n",
    );
}

#[test]
fn public_text_nullable_if_delivers_its_narrowed_owner_on_each_return_edge() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun direct(present: Boolean, stop: Boolean): String {
    val joined = if (present) { "a".checkedPlus("b") } else { null }
    if (joined != null) {
        if (stop) { return ((joined)) }
        println(joined)
        return joined
    }
    return "absent"
}
fun wrapped(present: Boolean, stop: Boolean): String? {
    val joined = if (present) { "c".checkedPlus("d") } else { null }
    if (joined != null) {
        if (stop) { return ((joined)) }
        println(joined)
        return joined
    }
    return joined
}
fun main(): Unit {
    println(direct(true, true))
    println(direct(true, false))
    println(direct(false, true))
    println(direct(false, false))
    val first = wrapped(true, true)
    if (first != null) { println(first) } else { error("missing first") }
    val second = wrapped(true, false)
    if (second != null) { println(second) } else { error("missing second") }
    val third = wrapped(false, true)
    if (third != null) { error("unexpected third") }
    val fourth = wrapped(false, false)
    if (fourth != null) { error("unexpected fourth") }
    println("null")
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 4);
    crate::native_tests::boxed_enum_tests::assert_success(
        &run,
        b"ab\nab\nab\nabsent\nabsent\ncd\ncd\ncd\nnull\n",
    );
}

#[test]
fn public_text_nullable_roots_return_without_a_preceding_borrow() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun owned(own text: String?): String {
    if (text != null) { return ((text)) }
    return "absent"
}
fun preserved(own text: String?): String? {
    if (text == null) { return text } else { return ((text)) }
}
fun local(present: Boolean): String {
    val text = if (present) { "a".checkedPlus("b") } else { null }
    if (text != null) { return text }
    return "missing"
}
fun forked(own text: String?, stop: Boolean): String {
    if (text != null) {
        if (stop) { return text } else { return ((text)) }
    }
    return "empty"
}
fun main(): Unit {
    val first: String? = "owned".clone()
    println(owned(first))
    val absent: String? = null
    println(owned(absent))
    val second: String? = "wrapped".clone()
    val result = preserved(second)
    if (result != null) { println(result) }
    val none: String? = null
    val empty = preserved(none)
    if (empty != null) { error("unexpected") }
    println(local(true))
    println(local(false))
    val left: String? = "branch".clone()
    println(forked(left, true))
    val right: String? = "branch".clone()
    println(forked(right, false))
    val nullLeft: String? = null
    println(forked(nullLeft, true))
    val nullRight: String? = null
    println(forked(nullRight, false))
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 5);
    crate::native_tests::boxed_enum_tests::assert_success(
        &run,
        b"owned\nabsent\nwrapped\nab\nmissing\nbranch\nbranch\nempty\nempty\n",
    );
}
}
