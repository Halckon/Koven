//! SPEC-0313: ordinary source through original object/link/run helpers.
use super::{assert_success, emit_link_and_run};

#[test]
fn single_map_return_native_cardinality() {
    let run = emit_link_and_run(
        "map-return-cardinality.ko",
        include_str!("../../tests/fixtures/map-conditional-return-0313/live-dead.ko"),
        "main",
    );
    let expected = concat!(
        "head\ntail\nhead\ntail\nhead\nreturn\n",
        "dead\n",
        "head\nreturn\n",
        "head\ntail\nhead\ntail\nhead\ntail\ndead\n",
        "tail absent\nafter present\nlive\n",
        "head\ntail\ntail present\nafter present\nlive\n",
        "head\nreturn\n",
        "head\ntail\nhead\nreturn\n",
        "dead\n",
        "present\nreturn\n",
        "present\ntail removed\nmissing\nreturn\n",
        "head absent\nafter missing\nlive\n",
        "present\nreturn\n",
        "present\ntail removed\nmissing\ntail removed\nmissing\ntail removed\nhead absent\nafter missing\nlive\n",
    );
    assert_success(&run, expected.as_bytes());
}

#[test]
fn single_map_return_native_distinct_owners() {
    let run = emit_link_and_run(
        "map-return-distinct.ko",
        include_str!("../../tests/fixtures/map-conditional-return-0313/distinct-owners.ko"),
        "main",
    );
    assert_success(
        &run,
        concat!(
            "parameter empty\nparameter no tail\nparameter missing\n",
            "return\n",
            "return\n",
            "first no last\nfirst no tail\nsecond tail\nfirst after\nsecond after removed\n",
            "second removed\nreturn\n",
            "second removed\nsecond removed\nsecond removed\nreturn\n",
        )
        .as_bytes(),
    );
}

#[test]
fn single_map_return_native_providers() {
    let run = emit_link_and_run(
        "map-return-providers.ko",
        include_str!(
            "../../tests/fixtures/map-conditional-return-0313/borrowed-temporary-providers.ko"
        ),
        "main",
    );
    assert_success(
        &run,
        b"tail\nreturn\ntail\nreturn\ntail\nreturn\ntail\nreturn\ntail\nreturn\ntail\nreturn\n",
    );
}

#[test]
fn single_map_return_native_operand_cleanup() {
    let run = emit_link_and_run(
        "map-return-cleanup.ko",
        include_str!("../../tests/fixtures/map-conditional-return-0313/operand-cleanup.ko"),
        "main",
    );
    assert_success(&run, concat!(
        "first\noperand\nlater\nearlier\nreturned\n",
        "fallthrough\nlater\nearlier\nfallthrough\nlater\nearlier\ntail kept\nexhausted\ncompleted\n",
        "copy returned\ncopy exhausted\n",
    ).as_bytes());
}

fn unit_source(live: bool, constants: bool) {
    let after = if live {
        "if (cache.contains(9)) { println(\"current\") } else { println(\"lost\") }"
    } else {
        ""
    };
    let provider = format!(
        r#"package p
fun main(): Unit {{
    var cache = mutableMapOf<Int, String>()
    for (key in arrayOf(1, 2, 3)) {{
        cache.put(key, "head".clone())
        println("head")
        if (key == 1) {{ return }}
        cache.put(9, "tail".clone())
        println("tail")
    }}
    {after}
    println("done")
}}
"#
    );
    let (run, _) = super::super::resource_exchange_tests::run_unit(
        &provider,
        "package q\nfun entry(): Unit { p.main(); println(\"caller\") }",
        constants,
    );
    assert_success(&run, b"head\ncaller\n");
}
#[test]
fn unit_map_return_native_basic_live() {
    unit_source(true, false);
}
#[test]
fn unit_map_return_native_basic_dead() {
    unit_source(false, false);
}
#[test]
fn unit_map_return_native_constant_live() {
    unit_source(true, true);
}
#[test]
fn unit_map_return_native_constant_dead() {
    unit_source(false, true);
}

#[test]
fn single_map_return_native_independent_owned_results() {
    let run = emit_link_and_run(
        "map-return-owned-results.ko",
        include_str!("../../tests/fixtures/map-conditional-return-0313/owned-results.ko"),
        "main",
    );
    assert_success(
        &run,
        "returned\ntail\ntail\nreturned\ntail\ntail\ntail\nfallback\n"
            .repeat(4)
            .as_bytes(),
    );
}

pub(crate) mod packed_runtime_7 {
//! CI-only whole: original Resource sources, native counters and the explicit Abort companion.
//! Never select any part of this mixed whole for local execution. No ignore/environment skip.
//! The original sources and their empty deinit bodies remain byte-for-byte unchanged.

use crate::test_support::diagnostics::assert_diagnostic;
use std::{fs, os::unix::process::ExitStatusExt, path::Path, process::Command};

use inkwell::{
    context::Context,
    memory_buffer::MemoryBuffer,
    values::{AnyValue, InstructionOpcode},
};
use lang_frontend::{
    name_resolution::{DeclarationId, SymbolKind},
    ownership_checking::OwnedCompilationUnitView,
};

use crate::{
    native_tests::{
        TestDirectory,
        boxed_enum_tests::{
            assert_success, unit_source::llvm_definition, write_counted_allocations,
        },
    },
    ssa::model::{EntityType, FunctionId, LoanKind, Operation, Program},
};

use crate::ssa::nullable_map_consume_lowering_tests::resource::{CASES, LowerOutcome, Trace, contract, memory, unit};

// Independent companions, not rewrites/replacements of any frozen R0–R3 input.
const MAP_NULL_PATH: &str = "resource_map_null_companion.ko";
const MAP_NULL: &str = r#"class Resource { deinit() {} }
fun main(): Unit {
  val ordinary: Map<Int, Resource>? = null
}
"#;

const MISSING_PATH: &str = "resource_missing_remove_companion.ko";
const MISSING: &str = r#"class Resource { deinit() {} }
fun main(): Unit {
  var inner = mutableMapOf<Int, Resource>()
  inner.put(1, Resource())
  var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
  outer.put(1, inner)
  val got = outer.remove(2)!!
  val count = got.size
}
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Exit {
    Present,
    Null,
    Abort,
}

#[test]
fn original_resource_sources_native_counts_ci_only() {
    assert_eq!(
        CASES.iter().map(|case| case.id).collect::<Vec<_>>(),
        ["R0", "R1", "R2", "R3"]
    );
    for case in CASES {
        let exit = if case.id == "R3" {
            Exit::Null
        } else {
            Exit::Present
        };
        run_source(
            &format!("{}/original-single", case.id),
            case.path,
            case.single,
            None,
            case.counts[5],
            exit,
        );
        run_source(
            &format!("{}/rooted-pq", case.id),
            "p/provider.ko",
            case.provider,
            Some(case.consumer),
            case.counts[5],
            exit,
        );
    }
}

#[test]
fn resource_map_null_and_missing_remove_companions_native_ci_only() {
    run_source(
        "Map null companion",
        MAP_NULL_PATH,
        MAP_NULL,
        None,
        0,
        Exit::Null,
    );
    run_source(
        "missing-key companion",
        MISSING_PATH,
        MISSING,
        None,
        1,
        Exit::Abort,
    );
}

fn run_source(
    label: &str,
    path: &str,
    text: &str,
    consumer: Option<&str>,
    assertions: usize,
    exit: Exit,
) {
    let mut trace = Trace::default();
    let inspect_lower = |view: &OwnedCompilationUnitView<'_, '_>,
                         entry: DeclarationId,
                         program: &Program,
                         lowered_entry: FunctionId,
                         trace: &mut Trace| {
        contract::inspect(view, entry, program, lowered_entry, trace);
        assert!(trace.input_failures.is_empty(), "{label}: {trace:#?}");
        crate::ssa::verify::verify_program(program)
            .expect("same real Unit Program must verify before native execution");
        let (deinit, main) = resource_functions(view, program, lowered_entry);
        assert_allocation_sequence(program, main, deinit, exit);
        // For rooted sources this remains the returned q.entry, never p.main.
        let raw =
            crate::llvm::render_verified_program_with_debug(program, view.sources(), lowered_entry)
                .expect("same real Unit Program and SourceMap must produce verified LLVM");
        let directory = TestDirectory::create();
        let raw_path = directory.join("original.ll");
        fs::write(&raw_path, &raw).expect("write unmodified production LLVM");
        let raw_run = compile_and_run(&directory, "original", &raw_path, None);
        assert_run(&raw_run, path, text, exit, false);

        // Instrument only after the untouched artifact has executed successfully
        // or produced the companion's exact real null-assertion Abort.
        let observed = observe_deinit(&raw, program, deinit);
        let ir = directory.join("observed.ll");
        let counter = directory.join("counter.c");
        if exit == Exit::Null {
            let context = Context::create();
            let llvm = context
                .create_module_from_ir(MemoryBuffer::create_from_memory_range_copy(
                    raw.as_bytes(),
                    "resource-null.raw.ll",
                ))
                .expect("parse real null-only LLVM");
            assert!(llvm.get_function("malloc").is_none());
            fs::write(&ir, observed.replace("@free(", "@resource_free("))
                .expect("write zero-owner deinit/free observer");
            fs::write(&counter, NULL_COUNTER).expect("write zero-owner counter");
        } else {
            // The same established counter checks every allocation identity,
            // including both Map buffers, with no new allocation harness.
            write_counted_allocations(&observed, 3, None, &ir, &counter);
            let observed = fs::read_to_string(&ir)
                .expect("read existing allocator-counter product")
                .replace("@counted_free(", "@resource_free(");
            let observed = if exit == Exit::Abort {
                observed.replace("@abort(", "@resource_abort(")
            } else {
                observed
            };
            fs::write(&ir, observed).expect("write Resource-only observation calls");
            let existing = fs::read_to_string(&counter).expect("read existing counter");
            fs::write(
                &counter,
                format!(
                    "{existing}\n#define EXPECT_ABORT {}\n{RESOURCE_COUNTER}",
                    u8::from(exit == Exit::Abort)
                ),
            )
            .expect("append Resource-specific deinit observation");
        }
        let observed_run = compile_and_run(&directory, "observed", &ir, Some(&counter));
        assert_run(&observed_run, path, text, exit, true);
    };
    let outcome = if let Some(consumer) = consumer {
        assert_eq!(path, "p/provider.ko");
        unit::observe_sources_with_lower(
            text,
            consumer,
            |_, _, _, _, _| {},
            |_, _, _, _, _, _| {},
            assertions,
            inspect_lower,
            &mut trace,
        )
    } else {
        memory::observe_source(
            path,
            text,
            |_, _, _, _, _| {},
            |_, _, _, _, _, _| {},
            assertions,
            inspect_lower,
            &mut trace,
        )
    };
    assert_diagnostic!(
        matches!(outcome, Ok(ref result) if matches!(result.outcome, LowerOutcome::VerifiedLlvm)),
        "{label}: {outcome:#?}; {trace:#?}"
    );
    assert!(trace.input_failures.is_empty(), "{label}: {trace:#?}");
}

fn resource_functions(
    view: &OwnedCompilationUnitView<'_, '_>,
    program: &Program,
    entry: FunctionId,
) -> (FunctionId, FunctionId) {
    let descriptors = view
        .types()
        .signatures()
        .declarations()
        .iter()
        .filter_map(|signature| signature.nominal().and_then(|nominal| nominal.deinit()))
        .collect::<Vec<_>>();
    let [descriptor] = descriptors.as_slice() else {
        panic!("one real Resource descriptor is required, including the null-only source");
    };
    let source_unit = descriptor.item().source_unit();
    let source = view
        .names()
        .names()
        .index()
        .source_units()
        .iter()
        .find(|source| source.id() == source_unit)
        .expect("deinit descriptor retains its actual source unit");
    let inputs = view
        .inputs()
        .iter()
        .filter(|input| input.source_id() == source.source_id())
        .collect::<Vec<_>>();
    let [input] = inputs.as_slice() else {
        panic!("one original input matches the exact deinit source unit");
    };
    let span = input
        .parsed()
        .ast()
        .items()
        .get(descriptor.item().item())
        .expect("same source deinit item")
        .span();
    assert_eq!(span.source_id(), input.source_id());
    let module = program.module(entry.module()).expect("actual entry module");
    let functions = module
        .functions
        .iter()
        .filter(|function| function.origin.span() == span)
        .collect::<Vec<_>>();
    let [function] = functions.as_slice() else {
        panic!("exact source deinit origin must have one actual Unit function");
    };
    let Some(EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    }) = function.receiver()
    else {
        panic!("Resource deinit must retain the real Borrow receiver ABI");
    };
    assert_eq!(module.deinit(target), Some(function.id()));
    assert!(function.return_types.is_empty());
    let declarations = view
        .names()
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "main"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == source_unit
        })
        .collect::<Vec<_>>();
    let [main] = declarations.as_slice() else {
        panic!("one real main declaration in the original Resource source unit");
    };
    let main_span = input
        .parsed()
        .ast()
        .items()
        .get(main.root())
        .expect("same-source main item")
        .span();
    let mains = module
        .functions
        .iter()
        .filter(|candidate| candidate.origin.span() == main_span)
        .collect::<Vec<_>>();
    let [main] = mains.as_slice() else {
        panic!("one exact source-origin main body, not the q.entry bridge");
    };
    (function.id(), main.id())
}

fn assert_allocation_sequence(program: &Program, main: FunctionId, deinit: FunctionId, exit: Exit) {
    let module = program.module(main.module()).unwrap();
    let resource = module
        .function(deinit)
        .unwrap()
        .receiver()
        .unwrap()
        .semantic_type();
    let sequence = module
        .function(main)
        .unwrap()
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::MapConstruct { .. } => Some("map"),
            Operation::HeapAllocate { owner, .. } => {
                assert_eq!(owner, resource);
                Some("resource")
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if exit == Exit::Null {
        assert!(sequence.is_empty());
    } else {
        // Source order fixes the Resource as allocation identity 1. Empty Map
        // construction allocates its buffer; the single put does not rehash.
        assert_eq!(sequence, ["map", "resource", "map"]);
    }
}

fn observe_deinit(raw: &str, program: &Program, deinit: FunctionId) -> String {
    let context = Context::create();
    let llvm = context
        .create_module_from_ir(MemoryBuffer::create_from_memory_range_copy(
            raw.as_bytes(),
            "resource.raw.ll",
        ))
        .expect("parse untouched production LLVM");
    llvm.verify().expect("untouched LLVM verifies");
    let function = program
        .module(deinit.module())
        .unwrap()
        .function(deinit)
        .unwrap();
    let target = llvm_definition(&llvm, &(deinit.index(), function.name.clone()));
    assert_eq!(target.count_params(), 1);
    assert!(target.get_type().get_return_type().is_none());
    assert!(!target.get_type().is_var_arg());
    let receiver = target.get_nth_param(0).expect("real Borrow receiver");
    assert!(receiver.is_pointer_value());
    assert!(llvm.get_function("resource_deinit").is_none());
    let original = target
        .get_basic_blocks()
        .into_iter()
        .flat_map(|block| block.get_instructions())
        .map(|instruction| instruction.print_to_string().to_string())
        .collect::<Vec<_>>();
    let observer = llvm.add_function("resource_deinit", target.get_type(), None);
    let first = target
        .get_first_basic_block()
        .unwrap()
        .get_instructions()
        .find(|instruction| instruction.get_opcode() != InstructionOpcode::Phi)
        .expect("actual deinit has an executable instruction");
    let builder = context.create_builder();
    builder.position_before(&first);
    let inserted = builder
        .build_call(observer, &[receiver.into()], "")
        .expect("one entry observation on the real source deinit")
        .try_as_basic_value()
        .unwrap_instruction();
    let retained = target
        .get_basic_blocks()
        .into_iter()
        .flat_map(|block| block.get_instructions())
        .filter(|instruction| *instruction != inserted)
        .map(|instruction| instruction.print_to_string().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        retained, original,
        "all original deinit instructions remain intact"
    );
    llvm.verify().expect("deinit observer LLVM verifies");
    llvm.print_to_string().to_string()
}

// Same synchronous compiler/run protocol as the existing native count helpers.
fn compile_and_run(
    directory: &TestDirectory,
    label: &str,
    ir: &Path,
    counter: Option<&Path>,
) -> std::process::Output {
    let executable = directory.join(label);
    let mut command = Command::new(crate::test_support::ir_clang());
    command.arg(ir);
    if let Some(counter) = counter {
        command.arg(counter);
    }
    let built = command
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("existing native Clang");
    assert!(built.status.success(), "{label}: {built:?}");
    Command::new(executable)
        .output()
        .expect("run actual native artifact")
}

fn assert_run(run: &std::process::Output, path: &str, text: &str, exit: Exit, observed: bool) {
    if exit != Exit::Abort {
        assert_success(run, b"");
        return;
    }
    let anchor = "outer.remove(2)!!";
    assert_eq!(text.matches(anchor).count(), 1);
    let offset = text.find(anchor).unwrap();
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = text[line_start..offset].chars().count() + 1;
    let marker = if observed {
        "resource-abort-no-unwind\n"
    } else {
        ""
    };
    assert_eq!(run.status.signal(), Some(6), "{path}: {run:?}");
    assert!(run.stdout.is_empty(), "{path}: {run:?}");
    assert_eq!(
        run.stderr,
        format!("abort: null assertion failed\n  at {path}:{line}:{column}\n{marker}").as_bytes(),
        "{path}: {run:?}"
    );
}

const NULL_COUNTER: &str = r#"
#include <assert.h>
static int deinits, frees;
void resource_deinit(void *receiver) {
    (void)receiver;
    ++deinits;
    assert(0 && "null must not read a Resource payload or execute deinit");
}
void resource_free(void *pointer) {
    (void)pointer;
    ++frees;
    assert(0 && "null must not release an absent Map or Resource payload");
}
__attribute__((destructor)) static void verify_null(void) {
    assert(deinits == 0);
    assert(frees == 0);
}
"#;

// Appended to the established identity counter, not an alternative allocator.
const RESOURCE_COUNTER: &str = r#"
#include <stdio.h>
static int resource_deinits, resource_frees;
static void *deinitialized_resource;
void resource_deinit(void *receiver) {
    /* Source deinit receives a pointer to the owner handle, not its payload. */
    void *pointer = *(void **)receiver;
    assert(allocations == 3);
    assert(pointer && pointer == live[1]);
    assert(resource_deinits == 0);
    deinitialized_resource = pointer;
    ++resource_deinits;
}
void resource_free(void *pointer) {
    if (pointer == deinitialized_resource) {
        assert(resource_deinits == 1);
        assert(resource_frees == 0);
        ++resource_frees;
    }
    counted_free(pointer);
}
__attribute__((destructor)) static void verify_resource(void) {
    assert(!EXPECT_ABORT);
    assert(resource_deinits == 1);
    assert(resource_frees == 1);
}
_Noreturn void resource_abort(void) {
    assert(EXPECT_ABORT);
    assert(allocations == 3);
    assert(releases == 0);
    assert(resource_deinits == 0);
    assert(resource_frees == 0);
    for (int i = 0; i < allocations; ++i) assert(live[i]);
    fputs("resource-abort-no-unwind\n", stderr);
    fflush(stderr);
    abort();
}
"#;
}
