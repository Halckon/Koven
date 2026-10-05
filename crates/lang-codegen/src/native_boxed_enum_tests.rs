//! SPEC-0230: enum payloads behind unique Box handles reach native execution.
use super::{Command, Output, SymbolKind, TestDirectory, analyze, emit_link_and_run, fs, symbol};

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
    let analysis = analyze(name, source);
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let lower = || {
        crate::ssa::lower_scalar_file_with_entry(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
            symbol(&analysis, "entry", SymbolKind::Function),
        )
        .expect("boxed enum source must lower to verified SSA")
    };
    let (program, entry) = lower();
    let (repeated, repeated_entry) = lower();
    assert_eq!(entry.index(), repeated_entry.index());
    assert_eq!(entry.module().index(), repeated_entry.module().index());
    assert_eq!(
        crate::ssa::render_program(&program),
        crate::ssa::render_program(&repeated),
        "recursive type registration must be deterministic"
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("boxed enum LLVM must verify");
    assert_eq!(
        llvm,
        crate::llvm::render_verified_program_with_entry(&repeated, repeated_entry)
            .expect("repeated boxed enum LLVM must verify")
    );
    llvm
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
