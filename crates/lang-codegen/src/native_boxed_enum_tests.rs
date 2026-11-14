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
            let expected_prefix =
                format!("abort: {message}\n  at {case}.ko:{}:", operator_line + 1);
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
