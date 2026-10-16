//! SPEC-0330: ordinary-source downstream observations; no native/object execution.
use super::{integer_range_value_tests::collect_sources, verify::verify_program};

#[test]
fn while_refined_enum_and_nullable_sources_reach_verified_llvm() {
    let sources = [
        // Each successful condition is made false before the normal backedge.
        r#"enum class Shape { A(value: Int), B }
class Guard { deinit() { println("normal-drop") } }
fun entry(): Unit {
    var current: Shape = Shape.A(41)
    var total: Int = 0
    while (current is Shape.A) {
        val guard = Guard()
        val seen: Int = current.value
        total = total + seen
        current = Shape.B
    }
    if (total == 41) { println("enum-normal") } else { println("bad-value") }
}"#,
        r#"class Holder(val value: Int)
class Guard { deinit() { println("continue-drop") } }
fun entry(): Unit {
    var current: Holder? = Holder(42)
    var total: Int = 0
    while (current != null) {
        val guard = Guard()
        val seen: Int = current.value
        total = total + seen
        current = null
        continue
    }
    if (total == 42) { println("nullable-continue") } else { println("bad-value") }
}"#,
        // The first iteration unconditionally exits; outer owner remains after the loop.
        r#"enum class Shape { A(value: Int), B }
class Guard(val label: String) { deinit() { println(this.label) } }
fun entry(): Unit {
    val outer = Guard("outer-drop")
    var current: Shape = Shape.A(43)
    while (current is Shape.A) {
        val inner = Guard("break-drop")
        val seen: Int = current.value
        if (seen == 43) { println("enum-break") } else { println("bad-value") }
        break
    }
    println("after-break")
}"#,
        r#"class Holder(val value: Int)
class Guard(val label: String) { deinit() { println(this.label) } }
fun work(own initial: Holder?): Unit {
    val outer = Guard("outer-drop")
    var current: Holder? = initial
    while (current != null) {
        val inner = Guard("return-drop")
        val seen: Int = current.value
        if (seen == 44) { println("nullable-return") } else { println("bad-value") }
        return
    }
    println("zero-iteration")
}
fun entry(): Unit {
    val initial: Holder? = Holder(44)
    work(initial)
    println("after-return")
}"#,
    ]
    .map(str::to_owned);
    let observations = collect_sources(&sources);
    assert_eq!(observations.len(), 8);
    let outcomes = observations
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            let result = result.and_then(|program| {
                verify_program(&program).map_err(|error| format!("SSA verify: {error:?}"))?;
                let llvm = crate::llvm::render_verified_program(&program)
                    .map_err(|error| format!("LLVM: {error:?}"))?;
                println!(
                    "WHILE LLVM case={} unit={} {llvm}",
                    index / 2,
                    index % 2 == 1
                );
                Ok(())
            });
            println!(
                "WHILE OUTCOME case={} unit={} {result:?}",
                index / 2,
                index % 2 == 1
            );
            result
        })
        .collect::<Vec<_>>();
    // Every ordinary-source phase outcome has been retained before any oracle.
    for result in outcomes {
        result.expect("refined while body reaches verified SSA and LLVM");
    }
}

#[test]
fn observe_nullable_storage_and_while_boundaries() {
    let sources = [
        r#"class Holder(val value: Int)
fun entry(): Unit {
    var current: Holder? = Holder(42)
}"#,
        r#"class Holder(val value: Int)
fun entry(own initial: Holder?): Unit {
    var current: Holder? = initial
    current = null
}"#,
        r#"class Holder(val value: Int)
fun entry(own initial: Holder?): Unit {
    var current: Holder? = initial
    while (current != null) { current = null }
}"#,
        r#"class Holder(val value: Int)
fun entry(own initial: Holder?): Unit {
    var current: Holder? = initial
    while (current != null) {
        val seen: Int = current.value
        println(seen)
        return
    }
}"#,
    ]
    .map(str::to_owned);
    let observations = collect_sources(&sources);
    assert_eq!(observations.len(), 8);
    for (index, result) in observations.iter().enumerate() {
        println!(
            "NULLABLE BOUNDARY case={} unit={} stage={}",
            index / 2,
            index % 2 == 1,
            match result {
                Ok(_) => "verified SSA",
                Err(error) => error.as_str(),
            }
        );
    }
    // This exact records actual phase boundaries; success means observation only.
}
