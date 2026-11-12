//! SPEC-0230: enum payloads behind unique Box handles reach native execution.
use super::{Command, Output, TestDirectory, emit_link_and_run, fs};

#[path = "native_unit_source_test_support.rs"]
pub(crate) mod unit_source;

pub(crate) const SIMPLE_DECLARATIONS: &str = r#"
enum class Token { Number(item: Int), Empty }
fun make(): Box<Token> {
    val token: Token = Token.Number(37)
    return Box(token)
}
fun makeEmpty(): Box<Token> {
    val token: Token = Token.Empty
    return Box(token)
}
fun take(own token: Box<Token>): Unit { println("boxed-enum") }
"#;

const RECURSIVE_DECLARATIONS: &str = r#"
enum class Expr { Num(item: Int), Add(left: Box<Expr>, right: Box<Expr>) }
fun makeNum(item: Int): Box<Expr> {
    val node: Expr = Expr.Num(item)
    return Box(node)
}
fun makeAdd(own left: Box<Expr>, own right: Box<Expr>): Box<Expr> {
    val node: Expr = Expr.Add(left, right)
    return Box(node)
}
fun makePair(): Box<Expr> = makeAdd(makeNum(1), makeNum(2))
fun makeTree(): Box<Expr> = makeAdd(makePair(), makePair())
fun makeDeepTree(): Box<Expr> = makeAdd(makeTree(), makeTree())
fun makeInline(): Expr = Expr.Add(makeDeepTree(), makeDeepTree())
fun takeBox(own tree: Box<Expr>): Unit { println("boxed-tree") }
fun takeInline(own tree: Expr): Unit { println("inline-tree") }
"#;

pub(crate) const SIMPLE_STDOUT: &[u8] = b"boxed-enum\nboxed-enum\n";
pub(crate) const RECURSIVE_STDOUT: &[u8] = b"boxed-tree\ninline-tree\n";

// One boxed tree has 15 allocations; the inline root owns two more such trees.
// The inline enum root must not introduce a forty-sixth allocation.
pub(crate) const RECURSIVE_ALLOCATIONS: usize = 45;

fn simple_source() -> String {
    format!(
        "{SIMPLE_DECLARATIONS}\nfun entry(): Unit {{\n\
         val first = take(make())\n\
         take(makeEmpty())\n\
         }}"
    )
}

pub(crate) fn recursive_declarations(enum_first: bool) -> String {
    if enum_first {
        // An inline enum signature precedes all Box signatures in this source order.
        let inline = "fun makeInline(): Expr = Expr.Add(makeDeepTree(), makeDeepTree())\n";
        let declaration =
            "enum class Expr { Num(item: Int), Add(left: Box<Expr>, right: Box<Expr>) }\n";
        format!(
            "{declaration}{inline}{}",
            RECURSIVE_DECLARATIONS
                .replace(inline, "")
                .replace(declaration, "")
        )
    } else {
        RECURSIVE_DECLARATIONS.to_owned()
    }
}

fn recursive_source(enum_first: bool) -> String {
    let declarations = recursive_declarations(enum_first);
    format!(
        "{declarations}\nfun entry(): Unit {{\n\
         val boxed = makeDeepTree()\n\
         val first = takeBox(boxed)\n\
         val inline = makeInline()\n\
         takeInline(inline)\n\
         }}"
    )
}

#[test]
fn boxed_enum_return_and_value_delivery_run_natively() {
    let run = emit_link_and_run("boxed-enum.ko", &simple_source(), "entry");
    assert_success(&run, SIMPLE_STDOUT);
}

#[test]
fn recursive_boxed_enum_tree_and_inline_root_run_natively() {
    for enum_first in [false, true] {
        let run = emit_link_and_run(
            "recursive-boxed-enum.ko",
            &recursive_source(enum_first),
            "entry",
        );
        assert_success(&run, RECURSIVE_STDOUT);
    }
}

#[test]
fn boxed_enum_cases_allocate_and_free_each_owner_once() {
    let llvm = lower_to_llvm("boxed-enum-counts.ko", &simple_source());
    let run = run_counted_allocations(&llvm, 2);
    assert_success(&run, SIMPLE_STDOUT);
}

#[test]
fn recursive_boxed_enum_drop_frees_all_descendants_once() {
    for enum_first in [false, true] {
        let llvm = lower_to_llvm(
            "recursive-boxed-enum-counts.ko",
            &recursive_source(enum_first),
        );
        let run = run_counted_allocations(&llvm, RECURSIVE_ALLOCATIONS);
        assert_success(&run, RECURSIVE_STDOUT);
    }
}

pub(crate) fn lower_to_llvm(name: &str, source: &str) -> String {
    unit_source::UnitSource::analyze(name, source)
        .expect("source must pass complete Unit frontend qualification")
        .lower_to_llvm()
}

pub(crate) fn assert_success(run: &Output, stdout: &[u8]) {
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, stdout);
    assert!(run.stderr.is_empty(), "{run:?}");
}

/// Reuse the native owner tests' LLVM-only allocator substitution: libc I/O is not counted.
pub(crate) fn run_counted_allocations(llvm: &str, expected: usize) -> Output {
    let directory = TestDirectory::create();
    run_counted_in(llvm, expected, &directory.0, false, None)
}

/// Assert each release against independently specified allocation identities.
pub(crate) fn run_counted_allocations_in_order(llvm: &str, order: &[usize]) -> Output {
    let directory = TestDirectory::create();
    run_counted_in(llvm, order.len(), &directory.0, false, Some(order))
}

/// The caller owns this directory and decides when retained evidence is removed.
pub(crate) fn run_counted_allocations_with_artifacts(
    llvm: &str,
    expected: usize,
    directory: &std::path::Path,
) -> Output {
    run_counted_in(llvm, expected, directory, true, None)
}

fn run_counted_in(
    llvm: &str,
    expected: usize,
    directory: &std::path::Path,
    retain: bool,
    order: Option<&[usize]>,
) -> Output {
    let ir = directory.join("boxed-enum-counts.ll");
    let counter = directory.join("counter.c");
    let executable = directory.join("boxed-enum-counts");
    write_counted_allocations(llvm, expected, order, &ir, &counter);
    let evidence = retain.then_some(directory);
    if retain {
        let version = counted_output(
            Command::new(crate::test_support::ir_clang()).arg("--version"),
            evidence,
            "version",
        );
        assert!(version.status.success(), "{version:?}");
    }
    let linked = counted_output(
        Command::new(crate::test_support::ir_clang())
            .arg(&ir)
            .arg(&counter)
            .arg(format!("-DEXPECTED_ALLOCATIONS={expected}"))
            .arg("-o")
            .arg(&executable),
        evidence,
        "compile",
    );
    assert!(linked.status.success(), "{linked:?}");
    counted_output(&mut Command::new(&executable), evidence, "run")
}

/// Export the same identity counter for a caller that owns bounded child execution.
pub(crate) fn write_counted_allocations(
    llvm: &str,
    expected: usize,
    order: Option<&[usize]>,
    ir: &std::path::Path,
    counter: &std::path::Path,
) {
    if let Some(order) = order {
        let mut identities = order.to_vec();
        identities.sort_unstable();
        assert_eq!(
            identities,
            (0..expected).collect::<Vec<_>>(),
            "expected release order must name every allocation exactly once"
        );
    }
    assert!(llvm.contains("@malloc("), "fixture must actually allocate");
    assert!(
        llvm.contains("@free("),
        "fixture must contain owner drop glue"
    );
    let instrumented = llvm
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@counted_free(");
    fs::write(ir, instrumented).expect("write allocator-instrumented LLVM");
    let order_values = order
        .map(|order| {
            order
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_else(|| "0".to_owned());
    let prefix = format!(
        "#ifndef EXPECTED_ALLOCATIONS\n#define EXPECTED_ALLOCATIONS {expected}\n#endif\n\
         #define CHECK_ORDER {}\nstatic const int release_order[] = {{{order_values}}};\n",
        u8::from(order.is_some())
    );
    fs::write(
        counter,
        prefix
            + r#"
#include <stdlib.h>
#include <assert.h>
static void *live[EXPECTED_ALLOCATIONS];
static int allocations, releases;
void *counted_malloc(size_t size) {
    assert(allocations < EXPECTED_ALLOCATIONS);
    void *pointer = malloc(size);
    assert(pointer);
    live[allocations++] = pointer;
    return pointer;
}
void counted_free(void *pointer) {
    assert(pointer);
    for (int i = 0; i < allocations; ++i) {
        if (live[i] == pointer) {
            live[i] = 0;
            if (CHECK_ORDER) assert(i == release_order[releases]);
            ++releases;
            free(pointer);
            return;
        }
    }
    /* An untracked or already-freed pointer cannot mask a leaked descendant. */
    abort();
}
__attribute__((destructor)) static void verify_counts(void) {
    assert(allocations == EXPECTED_ALLOCATIONS);
    assert(releases == EXPECTED_ALLOCATIONS);
    for (int i = 0; i < allocations; ++i) assert(!live[i]);
}
"#,
    )
    .expect("write allocator identity counter");
}

/// Keep exact binary argv as well as readable commands; UTF-8 quoting must not
/// change paths. Normal counter callers retain their existing temporary lifetime.
fn counted_output(command: &mut Command, evidence: Option<&std::path::Path>, name: &str) -> Output {
    if let Some(directory) = evidence {
        let mut argv = Vec::new();
        for argument in std::iter::once(command.get_program()).chain(command.get_args()) {
            argv.extend_from_slice(argument.as_encoded_bytes());
            argv.push(0);
        }
        fs::write(directory.join(format!("{name}.argv.nul")), argv).unwrap();
        fs::write(
            directory.join(format!("{name}.command.txt")),
            format!("{command:?}\n"),
        )
        .unwrap();
        fs::write(
            directory.join(format!("{name}.cwd")),
            std::env::current_dir()
                .unwrap()
                .as_os_str()
                .as_encoded_bytes(),
        )
        .unwrap();
    }
    let result = command.output();
    if let Some(directory) = evidence {
        match &result {
            Ok(output) => {
                fs::write(directory.join(format!("{name}.stdout")), &output.stdout).unwrap();
                fs::write(directory.join(format!("{name}.stderr")), &output.stderr).unwrap();
                fs::write(
                    directory.join(format!("{name}.status")),
                    format!("{:?}\n", output.status),
                )
                .unwrap();
            }
            Err(error) => {
                fs::write(
                    directory.join(format!("{name}.status")),
                    format!("spawn failed: {error}\n"),
                )
                .unwrap();
            }
        }
    }
    result.expect("counter child must launch; retained callers preserve its evidence")
}

pub(crate) mod borrow_result_tests {
//! 普通源码的借用结果 ABI，不做故障或 IR 注入。
use super::super::*;

#[test]
fn native_borrow_last_use_single_preserves_permissions_and_owner_cleanup() {
    check_normal_cases(crate::native_borrow_last_use_cases::cases());
}

#[test]
fn native_borrow_direct_alias_single_preserves_real_storage_and_owner_cleanup() {
    check_normal_cases(crate::native_borrow_last_use_cases::alias_cases());
}

#[test]
fn native_borrow_stable_place_single_preserves_storage_and_cleanup() {
    check_normal_cases(crate::native_borrow_place_cases::cases());
}

#[test]
fn native_nullable_map_promotion_single_transfers_and_drops_generic_payloads() {
    check_normal_cases(crate::native_map_promotion_cases::cases());
}

#[test]
fn native_borrowed_inline_nullable_presence_preserves_empty_string_and_storage() {
    let path = "borrowed-inline-nullable-presence.ko";
    let source = r#"
fun inspect(value: String?): Unit {
    if (value != null) { println("present") } else { println("absent") }
    if (null == value) { println("equal") } else { println("different") }
}
fun entry(): Unit {
    inspect(null)
    inspect("")
    val text: String? = "owned"
    inspect(text)
}
"#;
    let run = emit_link_and_run(path, source, "entry");
    super::super::boxed_enum_tests::assert_success(
        &run,
        b"absent\nequal\npresent\ndifferent\npresent\ndifferent\n",
    );
    assert_presence_field_reads(path, source, "ptr", 2);
}

#[test]
fn native_borrowed_inline_nullable_presence_reads_the_tag_of_a_moveonly_value() {
    let path = "borrowed-tagged-nullable-presence.ko";
    let source = r#"
value class Payload(val text: String)
fun inspect(value: Payload?): Unit {
    if (value != null) { println("present") } else { println("absent") }
}
fun entry(): Unit {
    inspect(null)
    inspect(Payload("text"))
}
"#;
    let run = emit_link_and_run(path, source, "entry");
    super::super::boxed_enum_tests::assert_success(&run, b"absent\npresent\n");
    assert_presence_field_reads(path, source, "i1", 1);
}

fn assert_presence_field_reads(path: &str, source: &str, ty: &str, reads: usize) {
    let ir = super::super::boxed_enum_tests::lower_to_llvm(path, source);
    let signature = ir
        .lines()
        .find(|line| line.starts_with("define ") && line.contains(".inspect."))
        .expect("definition for the source inspect function");
    let (_, tail) = ir.split_once(signature).expect("inspect definition start");
    let (body, _) = tail.split_once("\n}").expect("inspect definition end");
    // Printing loads local literal temporaries; ignore those exact alloca slots.
    let local_slots = body
        .lines()
        .filter_map(|line| line.trim().split_once(" = alloca ").map(|(slot, _)| slot))
        .collect::<Vec<_>>();
    let loan_loads = body
        .lines()
        .filter_map(|line| line.split_once(" = load ").map(|(_, load)| load))
        .filter(|load| {
            let (_, address) = load.split_once(", ptr ").expect("load source address");
            let (address, _) = address.split_once(", ").expect("load source operand");
            !local_slots.contains(&address)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        loan_loads.len(),
        reads,
        "inspect loads only presence: {body}"
    );
    let expected = format!("{ty}, ptr %nullable.borrowed.presence.slot");
    assert!(
        loan_loads.iter().all(|load| load.starts_with(&expected)),
        "{body}"
    );
    let slots = body
        .lines()
        .filter(|line| line.contains(" = getelementptr "))
        .collect::<Vec<_>>();
    assert_eq!(slots.len(), reads, "{body}");
    assert!(
        slots.iter().all(|slot| slot.ends_with(", i32 0, i32 0")),
        "{body}"
    );
}

fn check_normal_cases(cases: impl IntoIterator<Item = crate::native_borrow_last_use_cases::Case>) {
    for case in cases {
        let run = emit_link_and_run(case.name, case.source, "entry");
        super::super::boxed_enum_tests::assert_success(&run, case.stdout);
        let llvm = super::super::boxed_enum_tests::lower_to_llvm(case.name, case.source);
        let counted = super::super::boxed_enum_tests::run_counted_allocations(&llvm, case.allocations);
        super::super::boxed_enum_tests::assert_success(&counted, case.stdout);
    }
}

#[test]
fn native_borrow_storage_single_generic_projection_and_nullable() {
    for callable in ["view", "wrap"] {
        for (name, text, expected) in crate::native_borrow_storage_cases::cases(callable) {
            let run = emit_link_and_run(name, &text, "entry");
            assert!(run.status.success(), "{name}/{callable}: {run:?}");
            assert_eq!(run.stdout, expected, "{name}/{callable}");
        }
    }
}

#[test]
fn native_borrow_result_single_direct_and_wrapper_keep_source_until_scope_end() {
    for callable in ["view", "wrap"] {
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)\nfun consume(own source: String) {{ println(source) }}\nfun entry() {{ val source = \"kept\".clone(); {{ borrow val item = {callable}(source); println((item)) }}; consume(source) }}"
        );
        let run = emit_link_and_run("borrow_result.ko", &text, "entry");
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"kept\nkept\n");
    }
}

#[test]
fn native_borrow_result_single_scalar_pointer_reads_copyable_storage() {
    let text = "fun view(source: Int): borrow Int from source = source\nfun wrap(source: Int): borrow Int from source = view(source)\nfun observe(own source: Int) { if (source == 7) { println(\"seven\") } else { error(\"wrong value\") } }\nfun entry() { val source = 7; { borrow val item = wrap(source); observe(item) }; observe(source) }";
    let run = emit_link_and_run("borrow_int.ko", text, "entry");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"seven\nseven\n");
}

#[test]
fn native_map_require_single_borrows_storage_and_aborts_on_missing() {
    for (name, text, expected) in crate::native_map_require_cases::cases() {
        let run = emit_link_and_run(name, text, "entry");
        assert!(run.status.success(), "{name}: {run:?}");
        assert_eq!(run.stdout, expected, "{name}");
    }
    let run = emit_link_and_run(
        "require-missing",
        crate::native_map_require_cases::MISSING,
        "entry",
    );
    assert!(!run.status.success(), "a missing required key must abort");
}

#[test]
fn native_map_with_single_synchronously_borrows_slot_and_handles_missing() {
    let text = "fun entry() { var m = mutableMapOf<String, String>(); m.put(\"key\", \"value\".clone()); if (m.withValue(\"key\", { value -> println(value); return })) { println(\"found\") }; if (m.withValue(\"missing\", { value -> error(\"must not call\") })) { error(\"must be false\") }; m.put(\"next\", \"after\") }";
    let run = emit_link_and_run("map-with.ko", text, "entry");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"value\nfound\n");
}

#[test]
fn native_map_with_single_generic_matrix() {
    for (name, text, expected) in crate::native_map_with_cases::cases() {
        let run = emit_link_and_run(name, &text, "entry");
        assert!(run.status.success(), "{name}: {run:?}");
        assert_eq!(run.stdout, expected, "{name}");
    }
}

#[test]
fn native_map_with_single_key_and_capture_owners_release_exactly_once() {
    for (name, text, expected) in crate::native_map_with_cases::cases()
        .into_iter()
        .filter(|(name, _, _)| matches!(*name, "capture" | "resource"))
    {
        // capture: buffer + 7 String owners; resource: buffer + 3 keys + 2 Resource owners.
        let count = if name == "capture" { 8 } else { 6 };
        let llvm = super::super::boxed_enum_tests::lower_to_llvm(name, &text);
        let run = super::super::boxed_enum_tests::run_counted_allocations(&llvm, count);
        assert!(run.status.success(), "{name}: {run:?}");
        assert_eq!(run.stdout, expected, "{name}");
    }
}

#[test]
fn borrow_call_lowering_single_groups_and_field_forwarding_run_without_owner_leaks() {
    for (name, provider, body, allocations, expected) in [
        (
            "grouped-binding",
            "fun view(source: String): borrow String from source = source",
            "val source = \"kept\".clone(); borrow val item = ((view(source))); println(item)",
            1,
            &b"kept\n"[..],
        ),
        (
            "grouped-return",
            "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = ((view(source)))",
            "val source = \"kept\".clone(); borrow val item = wrap(source); println(item)",
            1,
            &b"kept\n"[..],
        ),
        (
            "field-forwarding",
            "class Packet(val text: String)\nfun view(source: String): borrow String from source = source\nfun wrap(source: Packet): borrow String from source = view(source.text)\nfun consume(own source: Packet) {}",
            "val source = Packet(\"kept\".clone()); borrow val item = wrap(source); println(item); consume(source)",
            2,
            &b"kept\n"[..],
        ),
    ] {
        let text = format!("{provider}\nfun entry() {{ {body} }}");
        let run = emit_link_and_run(name, &text, "entry");
        super::super::boxed_enum_tests::assert_success(&run, expected);
        let llvm = super::super::boxed_enum_tests::lower_to_llvm(name, &text);
        let counted = super::super::boxed_enum_tests::run_counted_allocations(&llvm, allocations);
        super::super::boxed_enum_tests::assert_success(&counted, expected);
    }
}

#[test]
fn multi_parameter_borrow_single_native_matrix() {
    for case in crate::native_multi_borrow_cases::cases() {
        let run = emit_link_and_run(case.name, case.source, "entry");
        assert!(run.status.success(), "{}: {run:?}", case.name);
        assert_eq!(run.stdout, case.stdout, "{}", case.name);
        assert!(run.stderr.is_empty(), "{}: {run:?}", case.name);
    }
}

#[test]
fn mixed_value_borrow_single_native_matrix() {
    for case in crate::native_mixed_borrow_cases::cases()
        .into_iter()
        .chain([crate::native_mixed_borrow_cases::generic_consumers()])
    {
        let run = emit_link_and_run(case.name, case.source, "entry");
        assert!(run.status.success(), "{}: {run:?}", case.name);
        assert_eq!(run.stdout, case.stdout, "{}", case.name);
        assert!(run.stderr.is_empty(), "{}: {run:?}", case.name);
    }
}

#[test]
fn nested_borrow_argument_single_native_matrix() {
    for case in crate::native_nested_borrow_cases::cases() {
        let run = emit_link_and_run(case.name, case.source, "entry");
        assert!(run.status.success(), "{}: {run:?}", case.name);
        assert_eq!(run.stdout, case.stdout, "{}", case.name);
        assert!(run.stderr.is_empty(), "{}: {run:?}", case.name);
    }
}

#[test]
fn borrow_value_field_single_native_matrix() {
    for (name, source, stdout) in crate::native_borrow_value_field_cases::cases() {
        let run = emit_link_and_run(name, source, "entry");
        assert!(run.status.success(), "{name}: {run:?}");
        assert_eq!(run.stdout, stdout, "{name}");
        assert!(run.stderr.is_empty(), "{name}: {run:?}");
    }
}
}

pub(crate) mod abort_output_tests {
//! SPEC-0355: Guide v0.46 abort output contract on real native executables.
use super::super::emit_link_and_run;
use std::os::unix::process::ExitStatusExt;

/// Runs `entry` and checks stdout, the exact two-line stderr report and SIGABRT.
fn assert_abort(case: &str, source: &str, stdout: &str, message: &str, at: &str) {
    let name = format!("{case}.ko");
    let run = emit_link_and_run(&name, source, "entry");
    assert_eq!(run.status.signal(), Some(6), "{case}: {run:?}");
    assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{case}");
    let (line, column) = position(source, at);
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        format!("abort: {message}\n  at {name}:{line}:{column}\n"),
        "{case}"
    );
}

/// 1-based line and Unicode-scalar column of the unique `at` text.
fn position(source: &str, at: &str) -> (usize, usize) {
    assert_eq!(source.matches(at).count(), 1, "unique anchor {at:?}");
    let offset = source.find(at).unwrap();
    let before = &source[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    (line, source[line_start..offset].chars().count() + 1)
}

#[test]
fn error_writes_its_message_verbatim_after_flushed_stdout() {
    assert_abort(
        "error-message",
        "fun entry(): Unit {\n  println(\"before\")\n  /* 界 */ error(\"boom 界 \\\"quoted\\\"\")\n}\n",
        "before\n",
        "boom 界 \"quoted\"",
        "error(",
    );
}

#[test]
fn named_nothing_call_enters_the_callee_and_aborts_during_an_argument_prefix() {
    let check = || {
        assert_abort(
            "named-nothing-prefix",
            r#"
class Ticket { deinit() { println("unexpected drop") } }
fun prefix(): Ticket { println("prefix"); return Ticket() }
fun stop(): Nothing { println("entered stop"); error("from stop") }
fun suffix(): Int { println("unexpected suffix"); return 1 }
fun dormant(own ticket: Ticket, middle: Int, last: Int): Unit { println("unexpected dispatch") }
fun entry(): Unit { dormant(prefix(), stop(), suffix()) }
"#,
            "prefix\nentered stop\n",
            "from stop",
            "error(",
        )
    };
    check();
    // SPEC-0397 retires the legacy function-value fallback; the refinement layer is
    // the only lowering route for an indirect `Nothing` call.
}

#[test]
fn named_nothing_method_evaluates_its_receiver_before_its_arguments() {
    assert_abort(
        "named-nothing-method",
        r#"
class Worker {
    deinit() { println("unexpected drop") }
    fun stop(own value: Int): Nothing { println("entered method"); error("method stop") }
}
fun worker(): Worker { println("receiver"); return Worker() }
fun argument(): Int { println("argument"); return 1 }
fun entry(): Unit { worker().stop(argument()); println("unexpected continuation") }
"#,
        "receiver\nargument\nentered method\n",
        "method stop",
        "error(",
    );
}

#[test]
fn indirect_nothing_function_pointer_has_no_normal_continuation() {
    let check = || {
        assert_abort(
            "indirect-nothing-pointer",
            r#"
fun entry(): Unit {
    val stop: () -> Nothing = { println("pointer"); error("pointer stop") }
    stop()
    println("unexpected continuation")
}
"#,
            "pointer\n",
            "pointer stop",
            "error(",
        );
    };
    check();
    // SPEC-0397 retires the legacy function-value fallback; the refinement layer is
    // the only lowering route for an indirect `Nothing` call.
}

#[test]
fn indirect_nothing_capturing_closure_has_no_normal_continuation() {
    assert_abort(
        "indirect-nothing-capture",
        r#"
fun entry(): Unit {
    val message = "captured stop"
    val stop: () -> Nothing = { println("capture"); error(message) }
    stop()
    println("unexpected continuation")
}
"#,
        "capture\n",
        "captured stop",
        "error(",
    );
}

#[test]
fn indirect_nothing_borrowed_callback_has_no_normal_continuation() {
    assert_abort(
        "indirect-nothing-callback",
        r#"
fun invoke(stop: () -> Nothing): Int {
    println("callback")
    stop()
}
fun entry(): Unit {
    val stop: () -> Nothing = { error("callback stop") }
    val result = invoke(stop)
    println("unexpected continuation")
}
"#,
        "callback\n",
        "callback stop",
        "error(",
    );
}

#[test]
fn indirect_nothing_while_keeps_borrowed_operands_off_the_bypass() {
    assert_indirect_nothing_while_bypass(r#"stop("inner")"#);
    assert_indirect_nothing_while_bypass(r#"sink("outer", stop("inner"))"#);
}

#[test]
fn indirect_nothing_container_prefix_keeps_size_loan_off_the_bypass() {
    assert_indirect_nothing_while_bypass(r#"val xs = List<Int>(1, stop("inner"))"#);
}

#[test]
fn indirect_nothing_map_prefix_keeps_key_and_receiver_loans_off_the_bypass() {
    assert_indirect_nothing_while_bypass(
        r#"val m = mapOf<String, String>(); m.withValue("key", stop("inner"))"#,
    );
}

fn assert_indirect_nothing_while_bypass(body: &str) {
    let source = format!(
        r#"
fun sink(outer: String, inner: Int): Unit {{ println("unexpected sink") }}
fun work(flag: Boolean): Unit {{
    val stop: (borrow String) -> Nothing = {{ text -> error(text) }}
    while (flag) {{ {body} }}
    return
}}
fun entry(): Unit {{ work(false); println("done") }}
"#
    );
    super::super::boxed_enum_tests::assert_success(
        &emit_link_and_run("indirect-nothing-while-bypass.ko", &source, "entry"),
        b"done\n",
    );
}

#[test]
fn null_assertion_points_at_the_asserted_expression() {
    assert_abort(
        "null-assertion",
        "class Node(val item: Int)\nfun take(own node: Node): Unit {}\nfun entry(): Unit {\n  val node: Node? = null\n  take(node!!)\n}\n",
        "",
        "null assertion failed",
        "node!!",
    );
}

#[test]
fn checked_arithmetic_names_the_failed_operator() {
    let prelude = "fun one(): Int = 1\nfun zero(): Int = 0\nfun minusOne(): Int = -1\n";
    for (case, expression, message) in [
        ("add", "2147483647 + one()", "integer overflow in +"),
        (
            "subtract",
            "-2147483647 - one() - one()",
            "integer overflow in -",
        ),
        (
            "multiply",
            "1073741824 * (one() + one())",
            "integer overflow in *",
        ),
        ("divide-zero", "10 / zero()", "division by zero"),
        ("remainder-zero", "10 % zero()", "division by zero"),
        (
            "divide-overflow",
            "(-2147483647 - one()) / minusOne()",
            "integer overflow in /",
        ),
    ] {
        let source = format!(
            "{prelude}fun entry(): Unit {{\n  val result = {expression}\n  println(\"unreachable\")\n}}\n"
        );
        let operator_line = source
            .lines()
            .position(|line| line.contains("val result"))
            .unwrap();
        let run = emit_link_and_run(&format!("{case}.ko"), &source, "entry");
        assert_eq!(run.status.signal(), Some(6), "{case}: {run:?}");
        assert!(run.stdout.is_empty(), "{case}");
        let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
        let expected_prefix = format!("abort: {message}\n  at {case}.ko:{}:", operator_line + 1);
        assert!(stderr.starts_with(&expected_prefix), "{case}: {stderr:?}");
        assert!(
            stderr.ends_with('\n') && stderr.lines().count() == 2,
            "{case}: {stderr:?}"
        );
    }
}

#[test]
fn checked_conversion_names_the_target_type() {
    assert_abort(
        "conversion",
        "fun wide(): Int = 300\nfun entry(): Unit {\n  val narrow = wide().toByte()\n}\n",
        "",
        "value out of range for Byte",
        "wide().toByte()",
    );
}

#[test]
fn list_index_reports_the_index_and_size() {
    for (case, index, text) in [
        ("index-beyond", "5", "index 5 out of bounds for size 3"),
        ("index-negative", "-1", "index -1 out of bounds for size 3"),
    ] {
        let source = format!(
            "fun pick(): Int = {index}\nfun entry(): Unit {{\n  val items = listOf(1, 2, 3)\n  val item = items[pick()]\n}}\n"
        );
        assert_abort(case, &source, "", text, "items[pick()]");
    }
}

#[test]
fn collection_and_map_aborts_use_the_guide_text() {
    assert_abort(
        "list-empty",
        "fun entry(): Unit {\n  var items = mutableListOf<Int>()\n  val last = items.removeLast()\n}\n",
        "",
        "list is empty",
        "items.removeLast()",
    );
    assert_abort(
        "map-assert",
        "fun entry(): Unit {\n  val ages = mutableMapOf<Int, Int>()\n  val age = ages.get(3)!!\n}\n",
        "",
        "null assertion failed",
        "ages.get(3)!!",
    );
}
}
