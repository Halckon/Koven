//! The tutorial's exact three-source program, with owner-identity checks in test-only LLVM.
use super::*;
use lang_frontend::ownership_checking::check_compilation_unit_constant_ownership;

#[test]
fn unit_for_report_releases_previous_final_parent_and_argv_owners_once_in_order() {
    for constants in [false, true] {
        let (llvm, string) = report_program(constants);
        for arguments in [
            vec![],
            vec!["alpha"],
            vec!["alpha", "你好", "tail"],
            vec![""],
        ] {
            let run = counted_report(&llvm, string, &arguments);
            let expected = format!(
                "{}processed\ndone\n",
                arguments
                    .iter()
                    .map(|argument| format!("{argument}\n"))
                    .collect::<String>()
            );
            crate::native_tests::boxed_enum_tests::assert_success(&run, expected.as_bytes());
        }
    }
}

fn tutorial_source(name: &str) -> &str {
    let tutorial = include_str!("../../../../docs/tutorials/koven-tour.md");
    let opener = format!("```koven {name}\n");
    let (_, rest) = tutorial
        .split_once(&opener)
        .expect("named tutorial fixture exists");
    let (source, _) = rest.split_once("\n```").expect("tutorial fixture closes");
    assert!(!source.is_empty());
    source
}

fn report_program(constants: bool) -> (String, usize) {
    let mut sources = SourceMap::new();
    let parts = [
        ("app/model.ko", "parameter-report-model"),
        ("app/processor.ko", "parameter-report-processor"),
        ("app/main.ko", "parameter-report"),
    ];
    let files = parts
        .iter()
        .map(|(path, name)| parsed(&mut sources, path, tutorial_source(name)))
        .collect::<Vec<_>>();
    let inputs = parts
        .iter()
        .zip(&files)
        .map(|((path, _), (source, file))| SourceUnitInput::new("root", path, *source, file))
        .collect::<Vec<_>>();
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "main")
        .unwrap()
        .id();
    let result = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
    let (program, function) = if constants {
        let typed = result.validate_constants().unwrap();
        let owned = check_compilation_unit_constant_ownership(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
        )
        .unwrap()
        .validate()
        .unwrap();
        crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .unwrap()
    } else {
        let typed = result.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                .unwrap()
                .validate()
                .unwrap();
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .unwrap()
    };
    let string = program.modules[0]
        .types
        .iter()
        .position(|ty| *ty == crate::ssa::model::SsaTypeKind::StringOwner)
        .unwrap();
    let plan = super::super::native_unit_entry_plan(
        &program,
        NativeUnitEntry::BorrowedArguments(entry),
        function,
    )
    .unwrap();
    (
        crate::llvm::render_verified_program_with_entry_plan(&program, plan).unwrap(),
        string,
    )
}

fn counted_report(llvm: &str, string: usize, arguments: &[&str]) -> std::process::Output {
    let bits = usize::BITS;
    let mut instrumented = String::new();
    let mut owner_parameter = None;
    let mut injected = false;
    for line in llvm.lines() {
        instrumented.push_str(
            &line
                .replace("@malloc(", "@counted_malloc(")
                .replace("@free(", "@counted_free("),
        );
        instrumented.push('\n');
        if line.starts_with("define ") && line.contains(&format!("@koven.drop.t{string}(")) {
            let parameters = line.split_once('(').unwrap().1.split_once(')').unwrap().0;
            owner_parameter = Some(parameters.to_owned());
        } else if owner_parameter.is_some() && line.ends_with(':') {
            let owner = owner_parameter.take().unwrap();
            instrumented.push_str(&format!("  %checked.bytes = extractvalue {owner}, 0\n  %checked.length = extractvalue {owner}, 1\n  %checked.capacity = extractvalue {owner}, 2\n  call void @counted_string_drop(ptr %checked.bytes, i{bits} %checked.length, i{bits} %checked.capacity)\n"));
            injected = true;
        }
    }
    assert!(injected, "counter must observe the String owner destructor");
    instrumented.push_str(&format!(
        "declare void @counted_string_drop(ptr, i{bits}, i{bits})\n"
    ));
    let nonempty = arguments
        .iter()
        .filter(|argument| !argument.is_empty())
        .count();
    let directory = TestDirectory::create();
    let ir = directory.join("report.ll");
    let counter = directory.join("counter.c");
    let executable = directory.join("report");
    fs::write(&ir, instrumented).unwrap();
    fs::write(
        &counter,
        format!(
            "#define ARGUMENTS {}\n#define NONEMPTY {nonempty}\n{COUNTER}",
            arguments.len()
        ),
    )
    .unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg(&counter)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    Command::new(executable).args(arguments).output().unwrap()
}

const COUNTER: &str = r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#define HAS_BUFFER (ARGUMENTS > 0)
#define REPORT (HAS_BUFFER + NONEMPTY)
#define CLONES (REPORT + 1)
#define ALLOCATIONS (HAS_BUFFER + NONEMPTY * 2 + 1)
static void *live[ALLOCATIONS];
static int allocations, releases, dropped[ALLOCATIONS], start_drops, empty_drops, processed_drops, done_drops;
void *counted_malloc(size_t size) {
    assert(allocations < ALLOCATIONS);
    void *pointer = malloc(size);
    assert(pointer);
    live[allocations++] = pointer;
    return pointer;
}
void counted_string_drop(void *pointer, size_t length, size_t capacity) {
    if (capacity) {
        for (int i = 0; i < allocations; ++i) {
            if (live[i] == pointer) {
                assert((i >= HAS_BUFFER && i < REPORT) || i >= CLONES);
                assert(dropped[i]++ == 0);
                return;
            }
        }
        abort();
    }
    if (!length) { ++empty_drops; return; }
    if (length == 5 && !memcmp(pointer, "start", 5)) { ++start_drops; return; }
    if (length == 9 && !memcmp(pointer, "processed", 9)) { ++processed_drops; return; }
    if (length == 4 && !memcmp(pointer, "done", 4)) { ++done_drops; return; }
    abort();
}
void counted_free(void *pointer) {
    /* Previous fields release in iteration order, then final field, Report,
       argv strings in reverse order, and finally the argv element buffer. */
    int expected;
    if (releases < NONEMPTY) expected = CLONES + releases;
    else if (releases == NONEMPTY) expected = REPORT;
    else if (releases <= NONEMPTY * 2) expected = REPORT - (releases - NONEMPTY);
    else expected = 0;
    assert(releases < ALLOCATIONS && expected < allocations && live[expected] == pointer);
    if (expected != REPORT && !(HAS_BUFFER && expected == 0)) assert(dropped[expected] == 1);
    live[expected] = 0;
    ++releases;
    free(pointer);
}
__attribute__((destructor)) static void verify_owners(void) {
    assert(allocations == ALLOCATIONS && releases == ALLOCATIONS);
    for (int i = 0; i < ALLOCATIONS; ++i) assert(!live[i]);
    assert(start_drops == 1 && processed_drops == 1 && done_drops == 1);
    assert(empty_drops == (ARGUMENTS - NONEMPTY) * 2);
}
"#;

pub(crate) mod resource_deinit_tests {
    //! Resource bodies and lexical cleanup through both public compilation-unit entries.

    use super::super::*;
    use lang_frontend::ownership_checking::check_compilation_unit_constant_ownership;

    const LEAF: &str = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
fun observe(value: String): Unit { println(value) }
"#;

    #[test]
    fn unit_resource_deinit_native_body_then_reverse_fields_and_lexical_owners() {
        for constants in [false, true] {
            let provider = format!(
                r#"package p
{LEAF}
class Guard(val first: Leaf, val second: Leaf) {{
    deinit() {{ observe(BANNER); println(this.first.name); println(this.second.name) }}
}}
"#
            );
            let consumer = r#"package q
import p.Leaf
import p.Guard
fun entry(): Unit {
    val earlier = Leaf("earlier")
    val guard = Guard(Leaf("first"), Leaf("second"))
    println("scope")
}
"#;
            let (run, llvm) = run_unit(&provider, consumer, constants);
            let expected = b"scope\nbody\nfirst\nsecond\nsecond\nfirst\nearlier\n";
            assert!(run.status.success(), "{run:?}");
            assert_eq!(run.stdout, expected);
            let counted = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 4);
            crate::native_tests::boxed_enum_tests::assert_success(&counted, expected);
        }
    }

    #[test]
    fn unit_resource_deinit_native_transfer_return_and_temporary_drop_once() {
        for constants in [false, true] {
            let provider = format!(
                r#"package p
{LEAF}
fun make(): Leaf {{ val local = Leaf("returned"); return local }}
fun consume(own item: Leaf): Unit {{ println(BANNER) }}
fun inspect(item: Leaf): Unit {{ println("inspect") }}
fun branch(flag: Boolean): Unit {{
    val outer = Leaf("outer")
    if (flag) {{ val inner = Leaf("inner"); println("return"); return }}
    println("fallthrough")
}}
"#
            );
            let consumer = r#"package q
import p.Leaf
fun entry(): Unit {
    val source = p.make()
    val destination = source
    println("before")
    p.consume(destination)
    p.inspect(Leaf("temporary"))
    p.branch(true)
    p.branch(false)
    println("done")
}
"#;
            let (run, _) = run_unit(&provider, consumer, constants);
            assert!(run.status.success(), "{run:?}");
            assert_eq!(run.stdout, b"before\nbody\nreturned\ninspect\ntemporary\nreturn\ninner\nouter\nfallthrough\nouter\ndone\n");
        }
    }

    #[test]
    fn unit_resource_deinit_native_loop_exits_preserve_outer_resource() {
        for constants in [false, true] {
            let provider = format!("package p\n{LEAF}");
            let consumer = r#"package q
import p.Leaf
fun entry(): Unit {
    val outer = Leaf("outer")
    var i: Int = 0
    while (i < 3) {
        val inner = Leaf("inner")
        i = i + 1
        if (i == 1) { println("continue"); continue }
        if (i == 2) { println("break"); break }
    }
    println("after loop")
}
"#;
            let (run, _) = run_unit(&provider, consumer, constants);
            assert!(run.status.success(), "{run:?}");
            assert_eq!(
                run.stdout,
                b"continue\ninner\nbreak\ninner\nafter loop\nouter\n"
            );
        }
    }

    #[test]
    fn unit_resource_deinit_native_body_return_cleans_fields_but_abort_does_not() {
        for constants in [false, true] {
            for abort in [false, true] {
                let body_exit = if abort { "error(\"stop\")" } else { "return" };
                let provider = format!(
                    r#"package p
{LEAF}
class Guard(val child: Leaf) {{ deinit() {{ observe(BANNER); {body_exit} }} }}
"#
                );
                let consumer = r#"package q
import p.Leaf
import p.Guard
fun entry(): Unit { val guard = Guard(Leaf("field")); println("scope") }
"#;
                let (run, _) = run_unit(&provider, consumer, constants);
                if abort {
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::ExitStatusExt;
                        assert_eq!(run.status.signal(), Some(6), "{run:?}");
                    }
                    assert!(!run.status.success(), "{run:?}");
                    assert_eq!(run.stdout, b"scope\nbody\n");
                } else {
                    assert!(run.status.success(), "{run:?}");
                    assert_eq!(run.stdout, b"scope\nbody\nfield\n");
                }
            }
        }
    }

    #[test]
    fn unit_resource_deinit_native_readonly_reborrow_clone_and_pending_field_chain() {
        for constants in [false, true] {
            let provider = format!(
                r#"package p
{LEAF}
fun inspect(resource: Guard): Unit {{ println("inspect") }}
fun show(text: String, own number: Int): Unit {{ println(text) }}
class Guard(val child: Leaf) {{
    deinit() {{
        inspect(this)
        show(this.child.name, if (true) {{ 1 }} else {{ 2 }})
        println(this.child.name.clone())
        val local = Leaf("local")
        println(BANNER)
        return
    }}
}}
"#
            );
            let consumer = r#"package q
import p.Leaf
import p.Guard
fun entry(): Unit { val guard = Guard(Leaf("field")); println("scope") }
"#;
            let (run, _) = run_unit(&provider, consumer, constants);
            assert!(run.status.success(), "{run:?}");
            assert_eq!(
                run.stdout,
                b"scope\ninspect\nfield\nfield\nbody\nlocal\nfield\n"
            );
        }
    }

    #[test]
    fn unit_destructuring_native_move_only_and_copyable_execution() {
        for constants in [false, true] {
            let provider = format!(
                r#"package p
{LEAF}
value class ResourcePair(val left: Leaf, val right: Leaf)
value class IntPair(val first: Int, val second: Int)
fun makeResources(): ResourcePair = ResourcePair(Leaf("left"), Leaf("right"))
fun makeInts(): IntPair = IntPair(10, 32)
"#
            );
            let consumer = r#"package q
import p.Leaf
import p.ResourcePair
import p.IntPair
fun entry(): Unit {
    val (second, first) = p.makeInts()
    if (first == 10 && second == 32) {
        println("42")
    }
    val (right, left) = p.makeResources()
    println("destructured")
}
"#;
            let (run, llvm) = run_unit(&provider, consumer, constants);
            let expected = b"42\ndestructured\nleft\nright\n";
            assert!(run.status.success(), "{run:?}");
            assert_eq!(run.stdout, expected);
            let counted = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 2);
            crate::native_tests::boxed_enum_tests::assert_success(&counted, expected);
        }
    }

    fn run_unit(provider: &str, consumer: &str, constants: bool) -> (std::process::Output, String) {
        let provider = if constants {
            provider.replacen(
                "package p",
                "package p\nconst val BANNER: String = \"body\"",
                1,
            )
        } else {
            provider.replace("BANNER", "\"body\"")
        };
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", &provider);
        let (consumer_source, consumer) = parsed(&mut sources, "q/consumer.ko", consumer);
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let (name_environment, environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .unwrap()
            .validate()
            .unwrap();
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "entry")
            .unwrap()
            .id();
        let result = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
        let directory = TestDirectory::create();
        let object = directory.join("resource.o");
        let executable = directory.join("resource");
        let mut first_ir = None;
        let mut first_object = None;
        if constants {
            let typed = result
                .validate_constants()
                .expect("constant-capable resource types");
            let owned = check_compilation_unit_constant_ownership(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
            )
            .unwrap()
            .validate()
            .expect("constant-capable resource ownership");
            for order in [inputs, [inputs[1], inputs[0]]] {
                let (program, function) =
                    crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
                        &sources,
                        &order,
                        &names,
                        &environment,
                        &typed,
                        &owned,
                        entry,
                    )
                    .expect("constant unit resource SSA");
                let ir = crate::llvm::render_verified_program_with_entry(&program, function)
                    .expect("constant unit resource LLVM");
                crate::emit_native_constant_unit_object(
                    &sources,
                    &order,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    entry,
                    &object,
                )
                .expect("public constant unit object");
                assert_stable_output(ir, &object, &mut first_ir, &mut first_object);
            }
        } else {
            let typed = result.validate().expect("resource types");
            let owned =
                check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                    .unwrap()
                    .validate()
                    .expect("resource ownership");
            for order in [inputs, [inputs[1], inputs[0]]] {
                let (program, function) = lower_scalar_unit_with_entry(
                    &sources,
                    &order,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    entry,
                )
                .expect("unit resource SSA");
                let ir = crate::llvm::render_verified_program_with_entry(&program, function)
                    .expect("unit resource LLVM");
                emit_native_unit_object(
                    &sources,
                    &order,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    entry,
                    &object,
                )
                .expect("public unit object");
                assert_stable_output(ir, &object, &mut first_ir, &mut first_object);
            }
        }
        assert_no_sibling_temporary(&directory.0);
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).output().unwrap();
        (run, first_ir.expect("lowered unit LLVM"))
    }

    #[test]
    fn unit_temporary_field_parent_drops_after_call_or_prefix_exit() {
        for (parameter, check) in [
            ("Int", r#"if (number == 7) { println("read") }"#),
            (
                "Int?",
                r#"if (number != null) { if (number == 7) { println("read") } }"#,
            ),
        ] {
            let source = format!(
                r#"class Holder(val number: Int) {{ deinit() {{ println("release") }} }}
fun make_number(): Int {{ println("construct"); return 7 }}
fun read(number: {parameter}, own n: Int): Unit {{ {check} }}
fun exercise(own stop: Boolean): Unit {{
    read((Holder(make_number())).number, if (stop) {{ return }} else {{ 0 }})
    println("after")
}}
fun main(): Unit {{
    exercise(false)
    exercise(true)
    println("finished")
}}"#
            );
            let output = crate::native_tests::emit_link_and_run(
                "temporary_field_parent.ko",
                &source,
                "main",
            );
            crate::native_tests::boxed_enum_tests::assert_success(
                &output,
                b"construct\nread\nrelease\nafter\nconstruct\nrelease\nfinished\n",
            );
        }
    }

    #[test]
    fn unit_borrow_result_temporary_field_auxiliary_drops_before_consumer() {
        for parameter in ["Int", "Int?"] {
            let source = format!(
                r#"class Holder(val number: Int) {{ deinit() {{ println("release") }} }}
fun view(source: Int, auxiliary: {parameter}): borrow Int from source = source
fun observe(value: Int): Unit {{ if (value == 7) {{ println("read") }} }}
fun main(): Unit {{
    val n: Int = 7
    observe(view(auxiliary = (Holder(1)).number, source = n))
}}"#
            );
            let output = crate::native_tests::emit_link_and_run(
                "temporary_field_auxiliary.ko",
                &source,
                "main",
            );
            crate::native_tests::boxed_enum_tests::assert_success(&output, b"release\nread\n");
        }
    }

    #[test]
    fn unit_non_dispatching_nullable_prefix_needs_no_callee_instance() {
        for marker in ["", "own "] {
            let source = format!(
                r#"fun make_number(): Int {{ println("prefix"); return 7 }}
fun after(): Int {{ println("unexpected suffix"); return 0 }}
fun read({marker}number: Int?, own n: Int, own last: Int): Unit {{ println("unexpected call") }}
fun main(): Unit {{ read(make_number(), return, after()) }}"#
            );
            let output =
                crate::native_tests::emit_link_and_run("nullable_prefix.ko", &source, "main");
            crate::native_tests::boxed_enum_tests::assert_success(&output, b"prefix\n");
        }
    }

    #[test]
    fn unit_temporary_field_prefix_abandons_the_call_on_return_or_abort() {
        for parameter in ["Int", "Int?"] {
            for exit in ["return", "error(\"stop\")"] {
                let source = format!(
                    r#"class Holder(val number: Int) {{ deinit() {{ println("release") }} }}
fun make_number(): Int {{ println("construct"); return 7 }}
fun read(number: {parameter}, own n: Int): Unit {{ println("unexpected") }}
fun main(): Unit {{ read((Holder(make_number())).number, {exit}) }}"#
                );
                let output =
                    crate::native_tests::emit_link_and_run("field_prefix.ko", &source, "main");
                if exit == "return" {
                    crate::native_tests::boxed_enum_tests::assert_success(
                        &output,
                        b"construct\nrelease\n",
                    );
                } else {
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::ExitStatusExt;
                        assert_eq!(output.status.signal(), Some(6), "{output:?}");
                    }
                    assert!(!output.status.success(), "{output:?}");
                    assert_eq!(output.stdout, b"construct\n");
                }
            }
        }
    }

    fn assert_stable_output(
        ir: String,
        object: &Path,
        first_ir: &mut Option<String>,
        first_object: &mut Option<Vec<u8>>,
    ) {
        if let Some(first) = first_ir {
            crate::test_support::diagnostics::assert_text_eq!(first, &ir);
        } else {
            *first_ir = Some(ir);
        }
        let bytes = fs::read(object).unwrap();
        if let Some(first) = first_object {
            assert_eq!(first, &bytes);
        } else {
            *first_object = Some(bytes);
        }
    }
}
