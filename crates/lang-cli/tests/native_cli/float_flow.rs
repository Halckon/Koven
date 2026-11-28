use super::assert_both_native_entries;

#[test]
fn native_float_operations_preserve_width_and_ieee_results() {
    assert_both_native_entries(
        r#"fun remDouble(a: Double, b: Double): Double = a % b
fun remFloat(a: Float, b: Float): Float = a % b
fun doubleOps(a: Double, b: Double): Boolean {
    val n = b / b
    val negativeZero = -b
    return a * 2.0 == 3.0 && (a + 2.0) - 0.5 == 3.0 &&
        remDouble(7.5, 2.0) == 1.5 && 1.0 / b > a && 1.0 / negativeZero < b &&
        n != n && !(n == n) && !(n < a) && !(n <= a) && !(n > a) && !(n >= a)
}
fun floatOps(own a: Float): Boolean {
    val b: Float = a + 1f
    return b == a && a - 1f < a && 3_0.0f / 4f == 7.5f && remFloat(7.5f, 2f) == 1.5f
}
fun main(): Unit {
    if (doubleOps(1.5, 0.0)) { println("double") }
    if (floatOps(16777216f)) { println("float") }
}
"#,
        b"double\nfloat\n",
    );
}

#[test]
fn native_nullable_float_preserves_presence_width_and_negative_zero() {
    assert_both_native_entries(
        r#"fun toDouble(flag: Boolean): Double? = if (flag) { -0.0 } else { null }
fun toFloat(flag: Boolean): Float? = if (flag) { 16777216f } else { null }
fun doubleKind(value: Double?): String {
    if (value == null) { return "missing double" }
    if (1.0 / value < 0.0) { return "negative zero" }
    return "present double"
}
fun floatKind(own value: Float?): String {
    when (value) {
        null -> return "missing float"
        else -> if (value + 1f == value) { return "float32" }
    }
    return "present float"
}
fun main(): Unit {
    println(doubleKind(toDouble(false)))
    println(doubleKind(toDouble(true)))
    println(doubleKind(0.0))
    println(floatKind(toFloat(false)))
    println(floatKind(toFloat(true)))
    println(floatKind(0f))
    val narrowed = toDouble(true)!!
    if (1.0 / narrowed < 0.0) { println("asserted negative zero") }
}
"#,
        b"missing double\nnegative zero\npresent double\nmissing float\nfloat32\npresent float\nasserted negative zero\n",
    );
}

#[test]
fn native_nullable_float_equality_uses_presence_then_ieee_payloads() {
    assert_both_native_entries(
        r#"fun sameDouble(a: Double?, b: Double?): Boolean = a == b
fun differentFloat(a: Float?, b: Float?): Boolean = a != b
fun sameMixed(a: Float?, b: Float): Boolean = a == b && b == a
fun first(): Double? { println("left"); return -0.0 }
fun second(): Double? { println("right"); return 0.0 }
fun early(): Boolean { return first() == (return true) }
fun wrapDouble(value: Double): Double? = value
fun loopComparison(a: Double?, b: Double): Unit {
    for (s in arrayOf("loop")) {
        if (b == a && wrapDouble(b) != null && null != wrapDouble(b)) { println(s) }
    }
}
fun nanDouble(zero: Double): Double? = zero / zero
fun nanFloat(zero: Float): Float? = zero / zero
fun main(): Unit {
    if (sameDouble(null, null) && !sameDouble(null, 0.0) && !sameDouble(0.0, null)) {
        println("presence")
    }
    if (sameDouble(first(), second()) && !sameDouble(2.0, 3.0)) {
        println("double")
    }
    if (!differentFloat(null, null) && differentFloat(null, 0f) &&
        differentFloat(0f, null) && !differentFloat(-0f, 0f) && differentFloat(1f, 2f)) {
        println("float")
    }
    if (sameMixed(-0f, 0f) && !sameMixed(null, 0f)) { println("mixed") }
    val d = nanDouble(0.0)
    val f = nanFloat(0f)
    if (!sameDouble(d, d) && differentFloat(f, f)) { println("nan") }
    if (early()) { println("early") }
    loopComparison(-0.0, 0.0)
    loopComparison(null, 0.0)
}
"#,
        b"presence\nleft\nright\ndouble\nfloat\nmixed\nnan\nleft\nearly\nloop\n",
    );
}

#[test]
fn native_nothing_nullable_preserves_effects_through_float_comparisons_and_calls() {
    assert_both_native_entries(
        r#"class Guard(val label: String) { deinit() { println(label) } }
fun missing(flag: Boolean): Nothing? {
    val resource = Guard("drop")
    if (flag) { println("called:true"); return null }
    println("called:false")
    return null
}
fun pass(own value: Nothing?): Nothing? = value
fun observe(value: Nothing?): Nothing? = value
fun empty(): Nothing? = null
fun seesNull(value: Double?): Boolean = value == null
fun absence(value: Nothing?): Boolean {
    if (value == null) { return value == empty() }
    return false
}
fun right(x: Double?, flag: Boolean): Boolean =
    x == (if (flag) { println("if:true"); null } else { println("if:false"); null })
fun left(x: Float?, flag: Boolean): Boolean =
    (if (flag) { println("left:true"); null } else { println("left:false"); null }) != x
fun compareCall(x: Double?, flag: Boolean): Boolean = x == observe(pass(missing(flag)))
fun exits(x: Double?, flag: Boolean): Boolean =
    x == (if (flag) { println("exit"); return true } else { null })
fun outsideExpected(x: Nothing?, flag: Boolean): Double? {
    return if (x == (if (flag) { println("nested:true"); null } else { println("nested:false"); null })) {
        1.0
    } else { null }
}
fun joinedResult(flag: Boolean): Double? {
    return if (flag) { empty() } else { pass(empty()) }
}
fun joinedWhen(flag: Boolean): Float? {
    return when {
        flag -> empty()
        else -> pass(empty())
    }
}
fun consumeText(own value: String?, own label: String): Unit { println(label) }
fun textArgument(): Nothing? { println("text:argument"); return null }
fun deliverText(flag: Boolean): Unit {
    consumeText(textArgument(), if (flag) { return } else { "text:delivered" })
}
fun loopCheck(x: Double?, flag: Boolean): Unit {
    for (label in arrayOf("loop")) {
        if (seesNull(empty()) && x == (if (flag) { println("loop:true"); null } else { println("loop:false"); null })) {
            println(label)
        }
    }
}
fun main(): Unit {
    if (right(null, true) && !right(0.0, false)) { println("right") }
    if (left(0f, true) && !left(null, false)) { println("left") }
    if (compareCall(null, true) && !compareCall(-0.0, false)) { println("calls") }
    if (exits(0.0, true) && !exits(0.0, false)) { println("early") }
    val local: Nothing? = observe(empty())
    if (absence(local) && null == local) { println("singleton") }
    val widened: Double? = local
    if (widened == null) { println("widened") }
    loopCheck(null, true)
    loopCheck(0.0, false)
    if (outsideExpected(local, true) == 1.0 && outsideExpected(local, false) == 1.0) {
        println("nested")
    }
    if (joinedResult(true) == null && joinedResult(false) == null) { println("joined") }
    if (joinedWhen(true) == null && joinedWhen(false) == null) { println("when") }
    deliverText(true)
    deliverText(false)
}
"#,
        b"if:true\nif:false\nright\nleft:true\nleft:false\nleft\ncalled:true\ndrop\ncalled:false\ndrop\ncalls\nexit\nearly\nsingleton\nwidened\nloop:true\nloop\nloop:false\nnested:true\nnested:false\nnested\njoined\nwhen\ntext:argument\ntext:argument\ntext:delivered\n",
    );
}
