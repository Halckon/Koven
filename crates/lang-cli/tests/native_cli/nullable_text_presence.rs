use super::assert_both_native_entries;

#[test]
fn nullable_field_observer_keeps_lexical_borrow_across_array_for() {
    assert_both_native_entries(
        r#"class Item(val label: String) { deinit() { println(label) } }
class Inner(val text: String?) { deinit() { println("drop inner") } }
class Outer(val inner: Inner) { deinit() { println("drop outer") } }
fun view(borrow owner: Outer): borrow Outer from owner = owner
fun missing(): Nothing? { println("missing"); return null }
fun scan(own owner: Outer): Unit {
    borrow val alias = view(owner)
    for (label in arrayOf("first", "second")) {
        if (alias.inner.text == missing()) { println(label); continue }
        println("unexpected")
    }
    if (alias.inner.text == null) { println("after loop") }
}
fun stop(own owner: Outer): Unit {
    borrow val alias = view(owner)
    val labels = arrayOf(Item("drop named 1"), Item("drop named 2"))
    for (label in labels) {
        if (missing() == alias.inner.text) { println("break"); break }
    }
    if (alias.inner.text == null) { println("after break") }
}
fun exit(own owner: Outer): Unit {
    borrow val alias = view(owner)
    borrow val child = alias
    for (label in arrayOf(Item("drop temporary 1"), Item("drop temporary 2"))) {
        if (child.inner.text == missing()) { println("return"); return }
    }
    println("unexpected")
}
fun main(): Unit {
    scan(Outer(Inner(null)))
    stop(Outer(Inner(null)))
    exit(Outer(Inner(null)))
    println("done")
}
"#,
        concat!(
            "missing\nfirst\nmissing\nsecond\nafter loop\ndrop outer\ndrop inner\n",
            "missing\nbreak\nafter break\ndrop named 2\ndrop named 1\ndrop outer\ndrop inner\n",
            "missing\nreturn\ndrop temporary 2\ndrop temporary 1\ndrop outer\ndrop inner\ndone\n",
        )
        .as_bytes(),
    );
}

#[test]
fn nullable_text_presence_releases_the_borrow_result_on_operand_returns() {
    assert_both_native_entries(
        r#"class Section(val text: String?) { deinit() { println("drop section") } }
class Owner(val selected: Section) { deinit() { println("drop owner") } }
fun branch(borrow owner: Owner): borrow Section from owner = owner.selected
fun missing(): Nothing? { println("missing"); return null }
fun inspect(own owner: Owner, early: Boolean): Unit {
    borrow val alias = branch(owner)
    if (alias.text == (if (early) { return } else { missing() })) { println("equal") }
    println("done")
}
fun result(own owner: Owner, early: Boolean, nested: Boolean): String {
    borrow val alias = branch(owner)
    if ((if (early) { return "early" } else if (nested) { return "nested" } else { missing() }) != alias.text) {
        return "present"
    }
    return "absent"
}
fun main(): Unit {
    inspect(Owner(Section(null)), true)
    inspect(Owner(Section(null)), false)
    println(result(Owner(Section("")), true, false))
    println(result(Owner(Section("")), false, true))
    println(result(Owner(Section("")), false, false))
}
"#,
        concat!(
            "drop owner\ndrop section\n",
            "missing\nequal\ndone\ndrop owner\ndrop section\n",
            "drop owner\ndrop section\nearly\n",
            "drop owner\ndrop section\nnested\n",
            "missing\ndrop owner\ndrop section\npresent\n",
        )
        .as_bytes(),
    );
}

#[test]
fn nullable_text_presence_observes_fields_of_a_lexical_borrow_result() {
    assert_both_native_entries(
        r#"class Inner(val text: String?) { deinit() { println("drop inner") } }
class Outer(val inner: Inner) { deinit() { println("drop outer") } }
class Pair(val first: Outer, val second: Outer) { deinit() { println("drop pair") } }
fun view(borrow outer: Outer): borrow Outer from outer = outer
fun second(borrow pair: Pair): borrow Outer from pair = pair.second
fun missing(label: String): Nothing? { println(label); return null }
fun observe(own owner: Outer): Unit {
    borrow val alias = view(owner)
    if (alias.inner.text == missing("right")) { println("null") }
    if (missing("left") != (alias).inner.text) { println("present") }
    if (!(alias.inner.text != null)) { println("literal null") }
    println("after")
}
fun early(own owner: Outer): Unit {
    borrow val alias = view(owner)
    if (alias.inner.text == missing("early")) { return }
    println("unexpected")
}
fun projected(own pair: Pair): Unit {
    borrow val alias = second(pair)
    borrow val child = alias
    var again = true
    while (again && child.inner.text == missing("projected")) {
        again = false
        println("selected null")
    }
}
fun main(): Unit {
    observe(Outer(Inner(null)))
    observe(Outer(Inner("")))
    early(Outer(Inner(null)))
    projected(Pair(Outer(Inner("")), Outer(Inner(null))))
    println("done")
}
"#,
        concat!(
            "right\nnull\nleft\nliteral null\nafter\ndrop outer\ndrop inner\n",
            "right\nleft\npresent\nafter\ndrop outer\ndrop inner\n",
            "early\ndrop outer\ndrop inner\n",
            "projected\nselected null\ndrop pair\ndrop outer\ndrop inner\ndrop outer\ndrop inner\ndone\n",
        )
        .as_bytes(),
    );
}

#[test]
fn nullable_text_presence_lowers_nested_class_fields_in_compatibility_loops() {
    assert_both_native_entries(
        r#"class Inner(val text: String?) { deinit() { println("drop inner") } }
class Middle(val inner: Inner) { deinit() { println("drop middle") } }
class Outer(val middle: Middle) { deinit() { println("drop outer") } }
fun make(label: String, present: Boolean): Outer {
    println(label)
    val text: String? = if (present) { "" } else { null }
    return Outer(Middle(Inner(text)))
}
fun missing(): Nothing? { println("missing"); return null }
fun borrowed(borrow outer: Outer): Unit {
    for (label in arrayOf("borrowed:null")) {
        if (outer.middle.inner.text == missing()) { println(label) }
    }
}
fun temporary(stop: Boolean): Unit {
    for (label in arrayOf("temporary:null")) {
        if (((make("temporary", false)).middle).inner.text ==
            (if (stop) { return } else { missing() })) { println(label) }
        if (missing() != make("empty", true).middle.inner.text) { println("temporary:present") }
    }
}
fun loopExits(stop: Boolean): Unit {
    var first = true
    for (label in arrayOf("continue", "break")) {
        if (make(label, false).middle.inner.text ==
            (if (first) { first = false; continue } else if (stop) { break } else { missing() })) {
            println("unexpected")
        }
    }
    println("done")
}
fun main(): Unit {
    temporary(false)
    temporary(true)
    loopExits(true)
    for (label in arrayOf("owned:null")) {
        val outer = make("owned", false)
        if (outer.middle.inner.text == missing()) { println(label) }
        borrowed(outer)
    }
}
"#,
        concat!(
            "temporary\nmissing\ndrop outer\ndrop middle\ndrop inner\ntemporary:null\n",
            "missing\nempty\ndrop outer\ndrop middle\ndrop inner\ntemporary:present\n",
            "temporary\ndrop outer\ndrop middle\ndrop inner\n",
            "continue\ndrop outer\ndrop middle\ndrop inner\nbreak\ndrop outer\ndrop middle\ndrop inner\ndone\n",
            "owned\nmissing\nowned:null\nmissing\nborrowed:null\ndrop outer\ndrop middle\ndrop inner\n",
        )
        .as_bytes(),
    );
}

#[test]
fn nullable_text_presence_preserves_operand_effects_and_branch_exits() {
    assert_both_native_entries(
        r#"class Mark(val label: String) { deinit() { println(label) } }
fun missing(label: String): Nothing? {
    val mark = Mark("drop:missing")
    println(label)
    return null
}

fun text(flag: Boolean): String? {
    println("text")
    if (flag) { return "" }
    return null
}
fun same(borrow value: String?): Boolean = value == (missing("right"))
fun view(borrow source: String?): borrow String? from source = source
fun borrowed(own source: String?): Unit {
    borrow val value = view(source)
    if (value == missing("borrow:right")) { println("borrow:null") }
    if (missing("borrow:left") != value) { println("borrow:present") }
}
fun opposite(): Boolean = ((missing("left"))) != text(true)
fun <T> identity(own value: T?): T? = value
fun <T> generic(own value: T?): Boolean = identity<T>(value) == missing("generic")
fun early(flag: Boolean): Boolean {
    return text(false) == (if (flag) { return false } else { missing("finish") })
}
fun compatible(borrow value: String?, flag: Boolean): Unit {
    for (label in arrayOf("compatible")) {
        if (value == (if (flag) { missing("compat:right") } else { return })) {
            println(label)
        }
        if (missing("compat:left") != text(true)) { println("compat:present") }
    }
}
fun main(): Unit {
    if (same(null)) { println("missing") }
    if (!same("")) { println("empty") }
    if (opposite()) { println("present") }
    if (text(false) == missing("temporary")) { println("temporary:missing") }
    if (!early(true)) { println("early") }
    if (early(false)) { println("complete") }
    compatible(null, true)
    borrowed(null)
    borrowed("")
    val textValue: String? = null
    if (generic<String>(textValue)) { println("generic:string") }
    val floatValue: Float? = 1f
    if (!generic<Float>(floatValue)) { println("generic:float") }
}
"#,
        concat!(
            "right\ndrop:missing\nmissing\nright\ndrop:missing\nempty\n",
            "left\ndrop:missing\ntext\npresent\ntext\ntemporary\ndrop:missing\ntemporary:missing\n",
            "text\nearly\ntext\nfinish\ndrop:missing\ncomplete\n",
            "compat:right\ndrop:missing\ncompatible\ncompat:left\ndrop:missing\ntext\ncompat:present\n",
            "borrow:right\ndrop:missing\nborrow:null\nborrow:left\ndrop:missing\n",
            "borrow:right\ndrop:missing\nborrow:left\ndrop:missing\nborrow:present\n",
            "generic\ndrop:missing\ngeneric:string\ngeneric\ndrop:missing\ngeneric:float\n",
        )
        .as_bytes(),
    );
}

#[test]
fn nullable_text_presence_holds_the_temporary_field_receiver_through_the_comparison() {
    assert_both_native_entries(
        r#"class Holder(val text: String?) { deinit() { println("drop:holder") } }
fun source(flag: Boolean): Holder {
    println("holder")
    if (flag) { return Holder("") }
    return Holder(null)
}
fun missing(): Nothing? { println("missing"); return null }
fun compatibleFields(flag: Boolean): Unit {
    for (label in arrayOf("compat:field")) {
        val stable = Holder(null)
        if (stable.text == missing()) { println(label) }
        if (source(flag).text == (if (flag) { return } else { missing() })) {
            println("compat:temporary")
        }
    }
}
fun early(flag: Boolean): Boolean {
    return (source(false).text) == (if (flag) { return false } else { missing() })
}
fun main(): Unit {
    if (((source(false))).text == missing()) { println("left null") }
    if (missing() != source(true).text) { println("right present") }
    if (!early(true)) { println("early") }
    val stable = Holder(null)
    if ((stable.text) == missing() && missing() == stable.text) { println("stable") }
    compatibleFields(false)
    compatibleFields(true)
}
"#,
        concat!(
            "holder\nmissing\ndrop:holder\nleft null\n",
            "missing\nholder\ndrop:holder\nright present\n",
            "holder\ndrop:holder\nearly\nmissing\nmissing\nstable\n",
            "missing\ncompat:field\nholder\nmissing\ndrop:holder\ncompat:temporary\ndrop:holder\n",
            "missing\ncompat:field\nholder\ndrop:holder\ndrop:holder\ndrop:holder\n",
        )
        .as_bytes(),
    );
}

#[test]
fn nullable_text_presence_allows_a_disjoint_field_write_in_the_right_operand() {
    assert_both_native_entries(
        r#"class Holder(var text: String?, var spare: Int) {}
fun missing(): Nothing? { println("missing"); return null }
fun test(inout holder: Holder, flag: Boolean): Boolean {
    return holder.text == (if (flag) { holder.spare = 1; missing() } else { missing() })
}
fun main(): Unit {
    var holder = Holder(null, 0)
    if (test(&holder, true) && holder.spare == 1) { println("equal") }
}
"#,
        b"missing\nequal\n",
    );
}
