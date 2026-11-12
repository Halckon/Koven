use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

pub(super) struct TestDirectory(pub(super) PathBuf);

impl TestDirectory {
    pub(super) fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-unit-native-test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("test directory must be creatable");
        Self(path)
    }

    pub(super) fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("owned test directory must be removable");
    }
}

pub(crate) mod receiver_two_phase_tests {
use super::super::*;

#[test]
fn receiver_two_phase_nested_readonly_native_source_order() {
    for kind in ["class", "value class"] {
        let analysis = analyze_sources(
            &"package p\nCLASS Cell(var item: Int) {\n\
         fun read(): Int { println(\"read\")\nreturn item }\n\
         inout fun set(first: Int, own second: Int): Unit {\n\
         println(\"set\")\nitem = first + second }\n\
         inout fun relay(): Unit { val done = set(0, inspect((this))) }\n}\n\
         fun inspect(cell: Cell): Int { println(\"inspect\")\nreturn cell.read() + 1 }\n\
         fun make(): Cell { println(\"make\")\nreturn Cell(20) }\n\
         fun argument(own value: Int): Int { println(\"argument\")\nreturn value }"
                .replace("CLASS", kind),
            "package q\nimport p.Cell\nfun entry(): Unit {\n\
         var cell = p.make()\n\
         val ignored = cell.set(second = p.argument(cell.read() + 1), first = p.argument(cell.read()))\n\
         if (cell.read() != 41) { error(\"wrong result\") }\n\
         val relayed = cell.relay()\n\
         if (cell.read() != 42) { error(\"wrong reborrow result\") }\n}",
        );
        let inputs = analysis.inputs();
        let directory = TestDirectory::create();
        let object = directory.join("two-phase.o");
        let executable = directory.join("two-phase");
        emit_native_unit_object(
            &analysis.sources,
            &inputs,
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .expect("two-phase receiver must emit native object");
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).output().unwrap();
        assert!(run.status.success(), "{run:?}");
        assert_eq!(
            run.stdout,
            b"make\nread\nargument\nread\nargument\nset\nread\ninspect\nread\nset\nread\n"
        );
        assert_no_sibling_temporary(&directory.0);
    }
}
}

pub(crate) mod iteration_conditional_receivers {
//! StaticSelf cleanup must be on its lexical side of each provider, including nested providers.
use super::super::resource_exchange_tests::run_unit;
use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};

#[test]
fn unit_for_conditional_receiver_cleanup_tracks_inside_outside_and_nested_positions() {
    for receiver in ["consume", "this.consume"] {
        for copyable in [false, true] {
            for position in ["inside", "outside", "between"] {
                let body = match position {
                    "inside" => format!(
                        "for (_ in listOf(Leaf(\"outer\"))) {{ {receiver}(if(flag) {{ return }} else {{ return }}) }}"
                    ),
                    "outside" => format!(
                        "{receiver}(if(flag) {{ for (_ in listOf(Leaf(\"outer\"))) {{ return }}; return }} else {{ return }})"
                    ),
                    _ => format!(
                        "for (_ in listOf(Leaf(\"outer\"))) {{ {receiver}(if(flag) {{ for (_ in listOf(Leaf(\"inner\"))) {{ return }}; return }} else {{ return }}) }}"
                    ),
                };
                let field = if copyable { "Int" } else { "String" };
                let constructor = if copyable {
                    "0"
                } else {
                    "\"receiver\" + \"owner\""
                };
                let provider = format!(
                    "package p\nclass Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}\ninterface Relay {{ own fun consume(own flag: Boolean): Unit {{ println(\"unexpected call\") }}\nown fun run(own flag: Boolean): Unit {{ {body}; println(\"unexpected after\") }} }}\nvalue class Host(val item: {field}): Relay {{}}"
                );
                let consumer = format!(
                    "package q\nimport p.Host\nfun entry(): Unit {{ Host({constructor}).run(true); println(\"caller\") }}"
                );
                let order: &[usize] = match (copyable, position) {
                    (false, "inside") => &[0, 1, 2],
                    (false, "outside") => &[1, 2, 0],
                    (false, _) => &[3, 4, 0, 1, 2],
                    (true, "inside" | "outside") => &[0, 1],
                    (true, _) => &[2, 3, 0, 1],
                };
                let expected: &[u8] = if position == "between" {
                    b"inner\nouter\ncaller\n"
                } else {
                    b"outer\ncaller\n"
                };
                for constants in [false, true] {
                    let (run, ir) = run_unit(&provider, &consumer, constants);
                    assert_success(&run, expected);
                    assert_success(&run_counted_allocations_in_order(&ir, order), expected);
                }
            }
        }
    }
}
}

pub(crate) mod owned_source_continue_tests {
//! SPEC-0312 unit controls: unchanged ordinary/constant implementations, real native outputs.
use super::super::resource_exchange_tests::run_unit;
use crate::native_tests::boxed_enum_tests::assert_success;

fn run(constants: bool, live: bool) {
    let after = if live {
        "if (keys.size == 3) { println(\"live\") }"
    } else {
        ""
    };
    let provider = format!(
        "package p\nfun main(): Unit {{ val keys = arrayOf(1, 2, 3); for (key in keys) {{ if (key == 2) {{ continue }}; println(\"tail\") }}; {after} }}"
    );
    let (output, _) = run_unit(
        &provider,
        "package q\nfun entry(): Unit { p.main() }",
        constants,
    );
    assert_success(
        &output,
        if live {
            b"tail\ntail\nlive\n"
        } else {
            b"tail\ntail\n"
        },
    );
}
#[test]
fn owned_source_continue_unit_native_basic_live() {
    run(false, true);
}
#[test]
fn owned_source_continue_unit_native_basic_dead() {
    run(false, false);
}
#[test]
fn owned_source_continue_unit_native_constant_live() {
    run(true, true);
}
#[test]
fn owned_source_continue_unit_native_constant_dead() {
    run(true, false);
}

#[test]
fn owned_source_continue_unit_native_post_loop_array_replacement() {
    let provider = r#"package p
fun main(): Unit {
    var keys = arrayOf(1, 2, 3)
    for (key in keys) { if (key == 2) { continue } }
    keys[0] = 9
    if (keys[0] == 9) { println("replaced") }
}
"#;
    for constants in [false, true] {
        let (output, _) = run_unit(
            provider,
            "package q\nfun entry(): Unit { p.main() }",
            constants,
        );
        assert_success(&output, b"replaced\n");
    }
}
}

pub(crate) mod map_word_frequency_tests {
//! SPEC-0295: ordinary-source Map owners through borrowed argv-style loops.
use super::super::resource_exchange_tests::run_unit;
use crate::native_tests::boxed_enum_tests::assert_success;

#[test]
fn unit_map_word_frequency_two_passes_keep_first_occurrence_order() {
    let provider = r#"package p
fun printWord(word: String, count: Int): Unit {
    println(word)
    if (count == 1) { println("one") } else if (count == 3) { println("three") } else { error("unexpected count") }
}
fun countWords(args: Array<String>): Unit {
    var counts = mutableMapOf<String, Int>()
    for (word in args) {
        val previous = counts.get(word)
        val count = if (previous == null) { 1 } else { previous + 1 }
        counts.put(word.clone(), count)
    }
    for (word in args) {
        val count = counts.remove(word)
        if (count != null) { printWord(word, count) }
    }
}
"#;
    let consumer = r#"package q
fun entry(): Unit {
    p.countWords(arrayOf<String>())
    p.countWords(arrayOf("beta", "alpha", "beta", "界", "beta"))
    println("after")
}
"#;
    for constants in [false, true] {
        let (run, _) = run_unit(provider, consumer, constants);
        assert_success(&run, "beta\nthree\nalpha\none\n界\none\nafter\n".as_bytes());
    }
}

const NARROWED_BORROW_SOURCE: &str = r#"
value class Count(val n: Int)
fun zero(value: Int): Unit { if (value == 0) { println("zero") } else { error("wrong zero") } }
fun flag(value: Boolean): Unit { if (!value) { println("false") } else { error("wrong flag") } }
fun count(value: Count): Unit { if (value.n == 7) { println("seven") } else { error("wrong count") } }
fun exercise(): Unit {
    var numbers = mutableMapOf<Int, Int>()
    numbers.put(1, 0)
    val gotZero = numbers.get(1)
    if (gotZero != null) { zero(gotZero); zero((gotZero)) }
    val removedZero = numbers.remove(1)
    if (removedZero != null) { zero(removedZero) }
    var flags = mutableMapOf<Int, Boolean>()
    flags.put(1, false)
    val gotFlag = flags.get(1)
    if (gotFlag != null) { flag(gotFlag); flag((gotFlag)) }
    val removedFlag = flags.remove(1)
    if (removedFlag != null) { flag(removedFlag) }
    if (flags.get(1) != null) { error("flag still present") }
    var counts = mutableMapOf<Boolean, Count>()
    counts.put(true, Count(7))
    val gotCount = counts.get(true)
    if (gotCount != null) { count(gotCount); count((gotCount)) }
    val removedCount = counts.remove(true)
    if (removedCount != null) { count(removedCount) }
    if (counts.get(true) != null) { error("count still present") }
}
"#;

#[test]
fn unit_map_word_frequency_narrowed_borrow_is_generic_for_get_and_remove() {
    let provider = format!("package p\n{NARROWED_BORROW_SOURCE}");
    let consumer = "package q\nfun entry(): Unit { p.exercise() }";
    for constants in [false, true] {
        let (run, _) = run_unit(&provider, consumer, constants);
        assert_success(
            &run,
            b"zero\nzero\nzero\nfalse\nfalse\nfalse\nseven\nseven\nseven\n",
        );
    }
}

#[test]
fn single_map_word_frequency_narrowed_borrow_is_generic_for_get_and_remove() {
    let source = format!("{NARROWED_BORROW_SOURCE}\nfun main(): Unit {{ exercise() }}");
    let run = crate::native_tests::emit_link_and_run("map-narrowed-borrow.ko", &source, "main");
    assert_success(
        &run,
        b"zero\nzero\nzero\nfalse\nfalse\nfalse\nseven\nseven\nseven\n",
    );
}

#[test]
fn unit_map_word_frequency_owner_carry_remains_generic() {
    let provider = r#"package p
class Resource(val word: String) { deinit() { println(this.word) } }
fun exercise(args: Array<String>): Unit {
    var texts = mutableMapOf<String, String>()
    for (word in args) { texts.put(word.clone(), word.clone()) }
    val value = texts.remove("key")!!
    println(value)
    var resources = mutableMapOf<String, Resource>()
    for (word in args) { resources.put(word.clone(), Resource(word.clone())) }
    val removed = resources.remove("key")
    println("done")
}
"#;
    let consumer = "package q\nfun entry(): Unit { p.exercise(arrayOf(\"key\", \"key\")) }";
    for constants in [false, true] {
        let (run, _) = run_unit(provider, consumer, constants);
        assert_success(&run, b"key\nkey\ndone\nkey\n");
    }
}
}

pub(crate) mod container_append_tests {
//! SPEC-0280: native MutableList.add sequential appending and dynamic capacity growth.

use super::super::*;
use crate::test_support::diagnostics::assert_diagnostic;

#[test]
fn unit_container_append_native_growth_and_ordering() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf<Int>()
            if (list.size != 0) { error("initial size not zero") }
            list.add(10)
            list.add(20)
            list.add(30)
            list.add(40)
            list.add(50)
            list.add(60)
            list.add(70)
            list.add(80)
            list.add(90)
            if (list.size != 9) { error("final size mismatch") }
            if (list[0] != 10) { error("element 0 mismatch") }
            if (list[1] != 20) { error("element 1 mismatch") }
            if (list[2] != 30) { error("element 2 mismatch") }
            if (list[3] != 40) { error("element 3 mismatch") }
            if (list[4] != 50) { error("element 4 mismatch") }
            if (list[5] != 60) { error("element 5 mismatch") }
            if (list[6] != 70) { error("element 6 mismatch") }
            if (list[7] != 80) { error("element 7 mismatch") }
            if (list[8] != 90) { error("element 8 mismatch") }
            println("append-growth-ok")
        }"#,
    );
    run(&analysis, b"append-growth-ok\n");
}

#[test]
fn unit_container_append_native_nonempty_start() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf(1, 2, 3)
            if (list.size != 3) { error("start size mismatch") }
            list.add(4)
            list.add(5)
            if (list.size != 5) { error("size mismatch") }
            if (list[0] != 1) { error("elem 0") }
            if (list[1] != 2) { error("elem 1") }
            if (list[2] != 3) { error("elem 2") }
            if (list[3] != 4) { error("elem 3") }
            if (list[4] != 5) { error("elem 4") }
            println("nonempty-append-ok")
        }"#,
    );
    run(&analysis, b"nonempty-append-ok\n");
}

#[test]
fn unit_container_append_native_move_only_elements_deinit_in_reverse_order() {
    let analysis = analyze_sources(
        r#"package p
        class Resource(val name: String) {
            deinit() {
                println(this.name)
            }
        }"#,
        r#"package q
        import p.Resource
        fun buildAndGrow(): Unit {
            var list = mutableListOf<Resource>()
            list.add(Resource("r1"))
            list.add(Resource("r2"))
            list.add(Resource("r3"))
            list.add(Resource("r4"))
            list.add(Resource("r5"))
            println("done-append")
        }
        fun entry(): Unit {
            buildAndGrow()
            println("done-all")
        }"#,
    );
    run(&analysis, b"done-append\nr5\nr4\nr3\nr2\nr1\ndone-all\n");
}

fn run(analysis: &UnitAnalysis, expected: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("append.o");
    let executable = directory.join("append");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &object,
    )
    .expect("source append emits a verified native object");
    crate::test_support::assert_native_object(&fs::read(&object).unwrap());
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("native link runs");
    assert_diagnostic!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let output = Command::new(&executable)
        .output()
        .expect("native program runs");
    assert_diagnostic!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty());
}
}

pub(crate) mod field_resource_replace_tests {
//! Cross-file direct class-field resource replacement through both native unit entries.

use super::super::resource_exchange_tests::run_unit;
use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations};

const PROVIDER: &str = r#"package p
class Leaf(val name: String) { deinit() { println(this.name) } }
class Guard(val name: String, val first: Leaf, val second: Leaf) {
    deinit() { println(this.name) }
}
fun replacement(): Guard {
    println("evaluate new")
    return Guard("new body", Leaf("new first"), Leaf("new second"))
}
fun inspect(guard: Guard): Unit { println("inspect old") }
"#;

#[test]
fn unit_field_resource_replace_native_commit_preserves_parent_and_lexical_owners() {
    let consumer = r#"package q
import p.Leaf
import p.Guard
class Holder(var state: Guard, val sibling: Leaf) {
    deinit() { println("parent body") }
}
fun entry(): Unit {
    val holder = Holder(Guard("old body", Leaf("old first"), Leaf("old second")), Leaf("sibling"))
    val old = replace(&holder.state, p.replacement())
    println("after replace")
    p.inspect(old)
    println("after inspect")
}
"#;
    let expected = b"evaluate new\nafter replace\ninspect old\nafter inspect\nold body\nold second\nold first\nparent body\nsibling\nnew body\nnew second\nnew first\n";
    for constants in [false, true] {
        let (run, ir) = run_unit(PROVIDER, consumer, constants);
        assert_success(&run, expected);
        assert_success(&run_counted_allocations(&ir, 8), expected);
    }
}

#[test]
fn unit_field_resource_replace_native_return_and_borrow_transfer_old_once() {
    let provider = r#"package p
class Leaf(val name: String) { deinit() { println(this.name) } }
class Holder(var state: Leaf)
fun takeOld(): Leaf {
    val holder = Holder(Leaf("returned old"))
    return replace(&holder.state, Leaf("returned replacement"))
}
fun inspect(leaf: Leaf): Unit { println("inspect old") }
"#;
    let consumer = r#"package q
import p.Leaf
class Holder(var state: Leaf)
fun entry(): Unit {
    val returned = p.takeOld()
    println("received old")
    val holder = Holder(Leaf("borrowed old"))
    p.inspect(replace(&holder.state, Leaf("borrowed replacement")))
    println("after inspect")
}
"#;
    let expected = b"returned replacement\nreceived old\ninspect old\nborrowed old\nafter inspect\nborrowed replacement\nreturned old\n";
    for constants in [false, true] {
        let (run, ir) = run_unit(provider, consumer, constants);
        assert_success(&run, expected);
        assert_success(&run_counted_allocations(&ir, 6), expected);
    }
}

#[test]
fn unit_field_resource_replace_native_pending_control_exit_keeps_old_field() {
    for exit in ["return", "break", "continue"] {
        let provider = format!(
            r#"package p
class Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}
class Holder(var state: Leaf) {{ deinit() {{ println("parent body") }} }}
fun exercise(own flag: Boolean): Unit {{
    val holder = Holder(Leaf("old"))
    var once = true
    loop {{
        if (!once) {{ break }}
        once = false
        val old = replace(&holder.state, if (flag) {{ {exit} }} else {{ Leaf("new") }})
        println("committed")
        break
    }}
    println("after loop")
}}
"#
        );
        let consumer = "package q\nfun entry(): Unit { p.exercise(true); p.exercise(false); println(\"done\") }";
        // Preserve the base entry's existing argument-control-transfer boundary.
        let (run, ir) = run_unit(&provider, consumer, true);
        let expected = if exit == "return" {
            "parent body\nold\ncommitted\nold\nafter loop\nparent body\nnew\ndone\n"
        } else {
            "after loop\nparent body\nold\ncommitted\nold\nafter loop\nparent body\nnew\ndone\n"
        };
        assert_success(&run, expected.as_bytes());
        assert_success(&run_counted_allocations(&ir, 5), expected.as_bytes());
    }
}

#[test]
fn unit_field_resource_replace_native_abort_does_not_commit_or_run_deinit() {
    let provider = r#"package p
class Leaf { deinit() { println("unexpected leaf deinit") } }
"#;
    let consumer = r#"package q
import p.Leaf
class Holder(var state: Leaf) { deinit() { println("unexpected parent deinit") } }
fun entry(): Unit {
    val holder = Holder(Leaf())
    println("before abort")
    val old = replace(&holder.state, error("stop"))
    println("unexpected commit")
}
"#;
    for constants in [false, true] {
        let (run, _) = run_unit(provider, consumer, constants);
        assert!(!run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"before abort\n");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(run.status.signal(), Some(6), "{run:?}");
        }
    }
}
}

pub(crate) mod container_clear_tests {
//! SPEC-0281: native MutableList.clear element reverse cleanup and buffer reuse.

use super::super::*;
use crate::test_support::diagnostics::assert_diagnostic;

#[test]
fn unit_container_clear_native_primitive_and_reappend() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf(1, 2, 3)
            if (list.size != 3) { error("initial size mismatch") }
            list.clear()
            if (list.size != 0) { error("size after clear mismatch") }
            list.add(10)
            list.add(20)
            if (list.size != 2) { error("size after re-add mismatch") }
            if (list[0] != 10) { error("elem 0 mismatch") }
            if (list[1] != 20) { error("elem 1 mismatch") }
            println("clear-primitive-readd-ok")
        }"#,
    );
    run(&analysis, b"clear-primitive-readd-ok\n");
}

#[test]
fn unit_container_clear_native_empty_list() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf<Int>()
            if (list.size != 0) { error("empty list size mismatch") }
            list.clear()
            if (list.size != 0) { error("empty list size after clear mismatch") }
            list.add(42)
            if (list.size != 1) { error("size after add mismatch") }
            if (list[0] != 42) { error("elem 0 mismatch") }
            println("clear-empty-ok")
        }"#,
    );
    run(&analysis, b"clear-empty-ok\n");
}

#[test]
fn unit_container_clear_native_move_only_elements_reverse_deinit() {
    let analysis = analyze_sources(
        r#"package p
        class Resource(val name: String) {
            deinit() {
                println(this.name)
            }
        }"#,
        r#"package q
        import p.Resource
        fun clearAndAdd(): Unit {
            var list = mutableListOf<Resource>()
            list.add(Resource("r1"))
            list.add(Resource("r2"))
            list.add(Resource("r3"))
            println("before-clear")
            list.clear()
            println("after-clear")
            if (list.size != 0) { error("size after clear mismatch") }
            list.add(Resource("r4"))
            list.add(Resource("r5"))
            println("after-readd")
        }
        fun entry(): Unit {
            clearAndAdd()
            println("done-all")
        }"#,
    );
    run(
        &analysis,
        b"before-clear\nr3\nr2\nr1\nafter-clear\nafter-readd\nr5\nr4\ndone-all\n",
    );
}

#[test]
fn unit_container_clear_native_multiple_cycles_buffer_reuse() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf<Int>()
            list.add(1)
            list.add(2)
            list.add(3)
            list.add(4)
            list.clear()
            if (list.size != 0) { error("cycle 1 clear mismatch") }
            list.add(100)
            list.add(200)
            if (list[0] != 100) { error("cycle 2 elem 0 mismatch") }
            if (list[1] != 200) { error("cycle 2 elem 1 mismatch") }
            list.clear()
            if (list.size != 0) { error("cycle 2 clear mismatch") }
            list.add(999)
            if (list[0] != 999) { error("cycle 3 elem 0 mismatch") }
            println("clear-cycles-ok")
        }"#,
    );
    run(&analysis, b"clear-cycles-ok\n");
}

fn run(analysis: &UnitAnalysis, expected: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("clear.o");
    let executable = directory.join("clear");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &object,
    )
    .expect("source clear emits a verified native object");
    crate::test_support::assert_native_object(&fs::read(&object).unwrap());
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("native link runs");
    assert_diagnostic!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let output = Command::new(&executable)
        .output()
        .expect("native program runs");
    assert_diagnostic!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty());
}
}

pub(crate) mod owned_map_borrow_tests {
//! SPEC-0299 normal source → unmodified LLVM/object → native, without instrumentation.
use super::super::resource_exchange_tests::run_unit;
use crate::native_owned_map_borrow_cases as cases;
use crate::native_tests::{boxed_enum_tests::assert_success, emit_link_and_run};
use crate::test_support::diagnostics::diagnostic;

fn run(source: &str, expected: &[u8], unit: bool) {
    diagnostic!("owned-map-borrow native unit={unit}; source={source}");
    if unit {
        let (provider, entry) = source.split_once("fun entry()").unwrap();
        let provider = format!("package p\n{provider}");
        let consumer = format!("package q\nimport p.*\nfun entry(){entry}");
        for constants in [false, true] {
            let (run, _) = run_unit(&provider, &consumer, constants);
            assert_success(&run, expected);
        }
    } else {
        assert_success(
            &emit_link_and_run("owned-map-borrow.ko", source, "entry"),
            expected,
        );
    }
}
#[test]
fn owned_map_borrow_native_same_source_core() {
    for unit in [false, true] {
        for grouped in [false, true] {
            for (payload, expected) in [
                ("String", "value\nvalue\nvalue\nend\n"),
                (
                    "Text",
                    "borrowed inline\nborrowed inline\nowned inline\nend\n",
                ),
                ("Resource", "value\nvalue\nvalue\nend\ndrop\n"),
            ] {
                run(&cases::core(payload, grouped), expected.as_bytes(), unit);
            }
        }
    }
}
#[test]
fn owned_map_borrow_native_prefix_scopes_missing_and_resource_lifetime() {
    for unit in [false, true] {
        for payload in ["String", "Text", "Resource"] {
            run(&cases::missing(payload, false), b"missing\nend\n", unit);
        }
        run(
            &cases::independent_map(),
            b"value\nvalue\nvalue\nend\n",
            unit,
        );
        for (source, expected) in [
            (cases::PAIR, "prefix\nvalue\nvalue\nvalue\n"),
            (cases::BORROWED_PREFIX, "prefix\nvalue\nvalue\nvalue\n"),
            (
                cases::SCOPES,
                "first\nsecond\nfirst\nsecond\nfirst\nsecond\n",
            ),
            (cases::MISSING, "missing\nend\n"),
            (
                cases::RESOURCE_SCOPE,
                "removed drop\nremoved drop\nend\nremoved drop\nouter drop\n",
            ),
        ] {
            run(source, expected.as_bytes(), unit);
        }
    }
}
#[test]
fn owned_map_borrow_native_last_use_and_separate_inline_read_helpers() {
    for unit in [false, true] {
        for (payload, expected) in [
            ("String", "value\nvalue\nend\n"),
            ("Text", "borrowed inline\nborrowed inline\nend\n"),
            ("Resource", "value\nvalue\nend\ndrop\n"),
        ] {
            run(&cases::last_use(payload), expected.as_bytes(), unit);
        }
        // These helpers exercise different pre-existing callee field-reading paths.
        run(
            &cases::inline_field(unit),
            b"borrowed inline\nborrowed inline\nowned inline\nend\n",
            unit,
        );
        // Explicit source clone reads the String field; it is not an implicit bridge clone.
        run(
            &cases::inline_explicit_clone(),
            b"value\nvalue\nowned inline\nend\n",
            unit,
        );
    }
}
#[test]
fn owned_map_borrow_native_missing_assertion_has_defined_abort() {
    for payload in ["String", "Text", "Resource"] {
        let source = cases::missing(payload, true);
        let run = emit_link_and_run("owned-map-missing.ko", &source, "entry");
        assert!(!run.status.success());
        assert!(run.stdout.is_empty());
        let (provider, entry) = source.split_once("fun entry()").unwrap();
        for constants in [false, true] {
            let (run, _) = run_unit(
                &format!("package p\n{provider}"),
                &format!("package q\nimport p.*\nfun entry(){entry}"),
                constants,
            );
            assert!(!run.status.success());
            assert!(run.stdout.is_empty());
        }
    }
}

#[test]
fn owned_map_borrow_resource_plain_native_baseline_probe() {
    run(
        &cases::core("Resource", false),
        b"value\nvalue\nvalue\nend\ndrop\n",
        false,
    );
}

#[test]
fn owned_map_borrow_non_map_nullable_handle_storage_native_regression() {
    let source = r#"
class Node(val n: Int)
fun observe(node: Node) { if (node.n == 7) { println("node") } else { error("bad node") } }
fun entry() { val node: Node? = Node(7); if (node != null) { observe(node); observe(node) }; val taken = node!!; observe(taken) }
"#;
    // Existing single NullableHandle path, deliberately not the new unit Map bridge.
    run(source, b"node\nnode\nnode\n", false);
}

#[test]
fn owned_map_borrow_single_resource_loop_reuses_static_proof_slot() {
    let source = r#"
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
    run(source, b"first\nfirst\niteration\ndrop\nsecond\nsecond\niteration\ndrop\nmissing\niteration\nend\n", false);
}
}
