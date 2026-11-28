//! Public single-file source path: real object emission, host link, and run.
use super::*;

#[test]
fn generic_bound_dispatch_preserves_override_default_and_qualified_super() {
    assert_both_native_entries(
        r#"interface Named { fun name(): Int = 1; fun through(): Int = this.name() }
class Default(): Named {}
class Actual(): Named {
    override fun name(): Int = 2
    fun original(): Int = super<Named>.name()
    deinit() { println("drop actual") }
}
interface Derived: Named { fun name(): Int = 3 }
class Inherited(): Derived {}
class Delegated(val target: Inherited): Derived by target {
    fun original(): Int = super<Named>.name()
}
interface Root { fun read(): Int }
interface Replaced: Root { fun read(): Int }
interface Other { fun read(): Int }
class Reader(): Other { override fun read(): Int = 4 }
class Shared(val target: Reader): Replaced, Other by target {}
fun <T: Named> count(value: T): Int = value.name()
fun <T: Named> countThrough(value: T): Int = value.through()
fun <T: Root> read(value: T): Int = value.read()
fun main(): Unit {
    val actual = Actual()
    val delegated = Delegated(Inherited())
    println("${count(actual)} ${count(Default())} ${actual.original()} ${count(Inherited())}")
    println("${count(delegated)} ${delegated.original()}")
    println("${countThrough(actual)} ${read(Shared(Reader()))}")
    println("done")
}
"#,
        b"2 1 1 3\n3 1\n2 4\ndone\ndrop actual\n",
    );
}

#[test]
fn generic_bound_dispatch_materializes_the_selected_override_body() {
    assert_both_native_entries(
        r#"class Extra<A>() {}
interface Action { fun <X> apply(input: X): Int = 1 }
class C(): Action {
    override fun <Y> apply(input: Y): Int {
        val extra = Extra<Y>()
        return 2
    }
}
fun <T: Action> invoke(value: T): Int = value.apply<Int>(1)
fun main(): Unit { println("${invoke(C())}") }
"#,
        b"2\n",
    );
}

#[test]
fn default_body_materializes_overrides_for_each_concrete_self() {
    assert_both_native_entries(
        r#"class FirstExtra<A>() {}
class SecondExtra<A>() {}
interface Action {
    fun <X> apply(input: X): Int = 1
    fun forward(): Int = this.apply<Int>(1)
    fun through(): Int = this.forward()
}
class First(): Action {
    override fun <Y> apply(input: Y): Int {
        val extra = FirstExtra<Y>()
        return 2
    }
    fun explicit(): Int = super<Action>.through()
    deinit() { println("drop first") }
}
class Second(): Action {
    override fun <Z> apply(input: Z): Int {
        val extra = SecondExtra<Z>()
        return 3
    }
    deinit() { println("drop second") }
}
fun <T: Action> invoke(value: T): Int = value.through()
fun main(): Unit {
    val first = First()
    val second = Second()
    println("${invoke(first)} ${invoke(second)} ${first.explicit()}")
}
"#,
        b"2 3 2\ndrop second\ndrop first\n",
    );
}

#[test]
fn delegated_default_materializes_the_endpoint_self() {
    assert_both_native_entries(
        r#"class Extra<A>() {}
interface Action {
    fun <X> apply(input: X): Int = 1
    fun through(): Int = this.apply<Int>(1)
}
class Endpoint(): Action {
    override fun <Y> apply(input: Y): Int {
        val extra = Extra<Y>()
        return 2
    }
    deinit() { println("drop endpoint") }
}
class Middle(val target: Endpoint): Action by target
class Outer(val target: Middle): Action by target
fun <T: Action> invoke(value: T): Int = value.through()
fun main(): Unit {
    val outer = Outer(Middle(Endpoint()))
    println("${invoke(outer)}")
}
"#,
        b"2\ndrop endpoint\n",
    );
}

#[test]
fn display_interpolation_materializes_the_contextual_default_body() {
    assert_both_native_entries(
        r#"class Extra<A>() {}
interface Action {
    fun <X> apply(input: X): Int = 1
    fun toString(): String = "${this.apply<Int>(1)}"
}
class Actual(): Display, Action {
    override fun <Y> apply(input: Y): Int {
        val extra = Extra<Y>()
        return 2
    }
    deinit() { println("drop actual") }
}
fun <T: Display> show(value: T): String = "${value}"
fun main(): Unit {
    val value = Actual()
    println("${value}")
    println(show(value))
}
"#,
        b"2\n2\ndrop actual\n",
    );
}

#[test]
fn display_in_unreachable_nested_interpolation_does_not_instantiate_a_default() {
    assert_both_native_entries(
        r#"interface Action {
    fun <X> apply(): Int = 1
    fun toString(): String = "${this.apply<Nothing>()}"
}
class Actual(): Display, Action {
    override fun <Y> apply(): Int {
        val items = listOf<Y>()
        return 2
    }
}
fun nested(): Unit { val text = "${return}${"${Actual()}"}" }
fun main(): Unit { nested(); println("stopped") }
"#,
        b"stopped\n",
    );
}

#[test]
fn explicit_display_borrows_receivers_and_returns_independent_text() {
    assert_both_native_entries(
        r#"class Marker(val text: String) { deinit() { println(text) } }
class Holder(val text: String) { deinit() { println("drop holder") } }
fun counter(): Int {
    val marker = Marker("drop receiver")
    println("receiver")
    return 3
}
fun view(borrow source: String): borrow String from source = source
fun <T : Display> convert(borrow value: T): String = value.toString()
fun rc(borrow value: Rc<String>): String = value.value.toString()
fun list(borrow value: List<String>): String = value[0].toString()
fun early(flag: Boolean): String {
    val marker = Marker("drop early")
    return (if (flag) { return "stop" } else { 7 }).toString()
}
fun element(flag: Boolean): String {
    return listOf("l" + "ive")[if (flag) { return "stop element" } else { 0 }].toString()
}
fun makeLabel(): String = "lab" + "el"
fun main(): Unit {
    println(counter().toString())
    println(convert(counter()))
    println(true.toString())
    println(convert(false))
    println('界'.toString())
    val owner = makeLabel()
    borrow val value = view(owner)
    val direct = value.toString()
    val generic = convert(value)
    println(direct)
    println(generic)
    println(value)
    println(rc(Rc("rc")))
    println(list(listOf("list")))
    println(listOf("temp")[0].toString())
    println((Holder("field").text).toString())
    println((Holder("parent")).text.toString())
    println(early(true))
    println(early(false))
    println(element(true))
    println(element(false))
}
"#,
        concat!(
            "receiver\ndrop receiver\n3\nreceiver\ndrop receiver\n3\n",
            "true\nfalse\n界\nlabel\nlabel\nlabel\nrc\nlist\ntemp\ndrop holder\nfield\ndrop holder\nparent\n",
            "drop early\nstop\ndrop early\n7\nstop element\nlive\n",
        )
        .as_bytes(),
    );
}

#[test]
fn explicit_display_lowers_inside_compatibility_control_flow() {
    assert_both_native_entries(
        r#"class Inner(val text: String?)
class Middle(val inner: Inner)
class Outer(val middle: Middle)
class Holder(val number: Int, val text: String) { deinit() { println("drop:" + text) } }
fun make(): Outer = Outer(Middle(Inner(null)))
fun missing(): Nothing? = null
class Reader(val text: String) {
    fun show(): Unit {
        for (label in arrayOf("member")) {
            if (make().middle.inner.text == missing()) {
                println(label)
                println(this.text.toString())
            }
        }
    }
}
fun main(): Unit {
    for (label in arrayOf("same")) {
        if (make().middle.inner.text == missing()) {
            println(label)
            println(3.toString())
            println(true.toString())
            println(label.toString())
            val holder = Holder(7, "holder")
            println(holder.number.toString())
            println(holder.text.toString())
            println((Holder(8, "temporary").text).toString())
            val boxed = Rc("boxed")
            println(boxed.value.toString())
        }
    }
    Reader("this field").show()
}
"#,
        b"same\n3\ntrue\nsame\n7\nholder\ndrop:temporary\ntemporary\nboxed\ndrop:holder\nmember\nthis field\n",
    );
}

#[test]
fn display_interpolation_reaches_default_and_skips_exiting_suffix() {
    assert_both_native_entries(
        r#"class Extra<A>() {}
interface Action {
    fun <X> apply(input: X): Int = 1
    fun toString(): String = "${this.apply<Int>(1)}"
}
class Actual(): Display, Action {
    override fun <Y> apply(input: Y): Int {
        val extra = Extra<Y>()
        return 2
    }
    deinit() { println("drop actual") }
}
interface InvalidAction {
    fun <X> apply(): Int = 1
    fun toString(): String = "${this.apply<Nothing>()}"
}
class Unreached(): Display, InvalidAction {
    override fun <Y> apply(): Int {
        val xs = listOf<Y>()
        return 5
    }
}
const val ALWAYS = true
interface OuterAction {
    fun toString(): String {
        println("[${if (ALWAYS && ((return "outer") == "")) { "yes" } else { "no" }}|${this.later()}]")
        return "late"
    }
    fun later(): String = "${Unreached()}"
}
class OuterValue(): Display, OuterAction
fun prefix(): Int {
    println("[${if (ALWAYS && ((return 7) == 0)) { "yes" } else { "no" }}|${show(Unreached())}]")
    return 8
}
fun nested(): Unit { val text = "${return}${"${Unreached()}"}" }
fun <T: Display> show(value: T): String = "${value}"
fun main(): Unit {
    val value = Actual()
    println("${value}")
    println(show(value))
    println("${OuterValue()}")
    nested()
    if (prefix() == 7) { println("stopped") }
}
"#,
        b"2\n2\nouter\nstopped\ndrop actual\n",
    );
}

fn check(name: &str, text: &str, expected: &[u8]) {
    let directory = TestDirectory::create();
    let source = directory.join(&format!("{name}.ko"));
    let executable = directory.join("program");
    fs::write(&source, text).unwrap();
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("entry"),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{name}: {built:?}");
    assert!(
        built.stdout.is_empty() && built.stderr.is_empty(),
        "{name}: {built:?}"
    );
    let launched = Command::new(&executable).output().unwrap();
    assert_eq!(launched.status.code(), Some(0), "{name}: {launched:?}");
    assert_eq!(launched.stdout, expected, "{name}: {launched:?}");
    assert!(launched.stderr.is_empty(), "{name}: {launched:?}");
    let executed = run([
        OsStr::new("run"),
        source.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("entry"),
    ]);
    assert_eq!(executed.status.code(), Some(0), "{name}: {executed:?}");
    assert_eq!(executed.stdout, expected, "{name}: {executed:?}");
    assert!(executed.stderr.is_empty(), "{name}: {executed:?}");
}

#[test]
fn public_single_readonly_methods_literal_and_constant_use_real_fields() {
    for (name, fixture, expected) in [
        (
            "int",
            include_str!("../../../lang-codegen/tests/fixtures/readonly_methods/int.ko"),
            &b"seven\n"[..],
        ),
        (
            "bool",
            include_str!("../../../lang-codegen/tests/fixtures/readonly_methods/bool.ko"),
            &b"enabled\n"[..],
        ),
    ] {
        check(name, fixture, expected);
        let text = format!(
            "const val SEVEN:Int=7\nconst val ENABLED:Boolean=true\n{}",
            fixture
                .replace("Reading(7)", "Reading(SEVEN)")
                .replace("Switch(true)", "Switch(ENABLED)")
        );
        check(&format!("{name}-constant"), &text, expected);
    }
}

#[test]
fn public_single_readonly_methods_named_and_moveonly_sources() {
    check(
        "named",
        include_str!("../../../lang-codegen/tests/fixtures/readonly_methods/named.ko"),
        b"receiver\nsecond\nfirst\nfourteen\n",
    );
    check(
        "moveonly",
        include_str!("../../../lang-codegen/tests/fixtures/readonly_methods/moveonly.ko"),
        b"seven\nseven\nseven\n",
    );
    check(
        "moveonly-temporary",
        include_str!("../../../lang-codegen/tests/fixtures/readonly_methods/moveonly_temporary.ko"),
        b"seven\n",
    );
}
