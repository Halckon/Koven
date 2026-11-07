//! Native cleanup and reachability witnesses for an exit inside a comparison operand.
use super::counted;

#[test]
fn comparison_operand_return_releases_pending_string_and_named_owner_once() {
    let source = r#"fun left(): String { println("left string"); return "temporary".clone() }
fun measure(value: String): Int { println(value); return 5 }
fun finish(): Int {
    val owner = "named".clone()
    left() == (return measure(owner))
}
fun main(): Unit {
    if (finish() == 5) { println("returned") }
    else { error("wrong return") }
}
"#;
    counted(source, 2, b"left string\nnamed\nreturned\n");
}

#[test]
fn comparison_operand_return_cleans_the_same_left_owner_on_each_return_edge() {
    let source = r#"fun left(): String { println("left string"); return "temporary".clone() }
fun finish(flag: Boolean): Int {
    val live = "two exits".clone()
    println(live)
    left() == (if (flag) { return 5 } else { return 6 })
}
fun nested(flag: Boolean): Int {
    val live = "nested exit".clone()
    println(live)
    left() == (return if (flag || ((return 7) == 0)) { return 8 } else { 9 })
}
fun main(): Unit {
    if (finish(true) == 5 && finish(false) == 6 && nested(false) == 7 && nested(true) == 8) {
        println("returned")
    } else { error("wrong return") }
}
"#;
    counted(
        source,
        8,
        b"two exits\nleft string\ntwo exits\nleft string\nnested exit\nleft string\nnested exit\nleft string\nreturned\n",
    );
}

#[test]
fn comparison_operand_return_does_not_require_drops_on_unreachable_arm() {
    let source = r#"fun finish(flag: Boolean): Int {
    val live = "condition".clone()
    println(live)
    if (flag || ((return 6) == 0)) { return 7 }
    else { val dead = "dead".clone(); println(dead); return 8 }
}
fun choose(flag: Boolean): Int {
    return when (flag) {
        ((return 9) == 0) -> 0
        else -> { val dead = "dead candidate".clone(); println(dead); 0 }
    }
}
fun chooseWithoutSubject(): Int {
    return when {
        ((return 11) == 0) -> 0
        else -> { val dead = "dead alternative".clone(); println(dead); 0 }
    }
}
fun exitsInHeader(): Int {
    val live = "header".clone()
    println(live)
    while ((return 10) == 0) {}
    val dead = "dead tail".clone()
    println(dead)
    return 0
}
fun main(): Unit {
    val live = "returned".clone()
    if (finish(true) == 7 && finish(false) == 6 && choose(true) == 9
        && exitsInHeader() == 10 && chooseWithoutSubject() == 11) {
        println(live)
    }
    else { error("wrong return") }
}
"#;
    counted(source, 4, b"condition\ncondition\nheader\nreturned\n");
}

#[test]
fn comparison_operand_return_restores_pending_call_loan_on_the_skip_edge() {
    let source = r#"fun use(text: String, flag: Boolean, value: Int): Int = error("unreachable call")
fun measure(text: String): Int { println(text); return 5 }
fun finish(flag: Boolean): Int {
    val owner = "pending".clone()
    return use(owner, flag || ((return measure(owner)) == 0), (return measure(owner)))
}
fun main(): Unit {
    if (finish(true) == 5 && finish(false) == 5) { println("returned") }
    else { error("wrong return") }
}
"#;
    counted(source, 2, b"pending\npending\nreturned\n");
}
