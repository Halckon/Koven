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
        if (seen == 42) { println("seen") }
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

#[test]
fn enum_type_tests_preserve_last_use_cleanup_for_owned_roots() {
    let sources = ["subject is Shape.A", "subject !is Shape.A", "(subject) is Shape.A", "(subject) !is Shape.A"]
        .map(|condition| format!(
            "enum class Shape {{ A(value: String), B }}\nfun entry(own subject: Shape): Boolean = {condition}"
        ));
    let observations = collect_sources(&sources);
    assert_eq!(observations.len(), 8);
    let outcomes = observations
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            let result = result.and_then(|program| {
                verify_program(&program).map_err(|error| format!("SSA verify: {error:?}"))?;
                verify_last_enum_read_precedes_drop(&program)?;
                let llvm = crate::llvm::render_verified_program(&program)
                    .map_err(|error| format!("LLVM: {error:?}"))?;
                println!("ENUM LAST USE LLVM case={index} {llvm}");
                Ok(())
            });
            println!("ENUM LAST USE OUTCOME case={index} {result:?}");
            result
        })
        .collect::<Vec<_>>();
    for result in outcomes {
        result.expect("ordinary enum test reads tag before final owner cleanup");
    }
}

#[test]
fn direct_nullable_construction_storage_preserves_reachable_demand() {
    let sources = [
        r#"class Holder(val value: Int)
fun entry(): Unit {
    val current: Holder? = Holder(42)
    if (current == null) { println("bad") } else { println("present") }
}"#,
        r#"class Holder(val value: Int)
fun entry(): Unit {
    println("done")
    return
    val unused: Holder? = Holder(42)
}"#,
        r#"class Holder(val value: Int)
fun entry(own current: Holder?): Unit {
    if (current == null) { println("absent") } else { println("present") }
}"#,
        r#"class Holder(val value: Int)
fun entry(): Unit { val _ = Holder(42) }"#,
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
                println!("LOCAL STORAGE LLVM case={index} {llvm}");
                Ok(())
            });
            println!("LOCAL STORAGE OUTCOME case={index} {result:?}");
            result
        })
        .collect::<Vec<_>>();
    for (index, result) in outcomes.into_iter().enumerate() {
        if index < 6 {
            result.expect("direct nullable storage and reachability controls lower normally");
        } else {
            // The original f85 baseline has the same discard-owner cleanup gap.
            let start = if index == 6 { 53 } else { 63 };
            assert_eq!(
                result,
                Err(format!(
                    "lower: LoweringError {{ kind: MissingFact, span: Some(Span {{ source_id: SourceId(0), start: {start}, end: {} }}) }}",
                    start + 1
                ))
            );
        }
    }
}

fn verify_last_enum_read_precedes_drop(program: &super::model::Program) -> Result<(), String> {
    use super::model::Operation;
    let tests = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .flat_map(|function| {
            function.instructions.iter().filter_map(move |instruction| {
                match instruction.operation {
                    Operation::TaggedDiscriminant { owner } => Some((function, instruction, owner)),
                    _ => None,
                }
            })
        })
        .collect::<Vec<_>>();
    if tests.len() != 1 {
        return Err(format!("expected one actual tag read, got {}", tests.len()));
    }
    let (function, tag, owner) = tests[0];
    let drops = function.instructions.iter().filter(|instruction|
        matches!(instruction.operation, Operation::Drop { owner: dropped } if dropped == owner)
    ).collect::<Vec<_>>();
    if drops.len() != 1 {
        return Err(format!(
            "expected one drop of tag owner, got {}",
            drops.len()
        ));
    }
    let drop = drops[0];
    if tag.block != drop.block {
        return Err("tag read and final owner drop must share the straight-line block".to_owned());
    }
    let block = function
        .blocks
        .iter()
        .find(|block| block.id == tag.block)
        .ok_or_else(|| "tag block missing".to_owned())?;
    let tag_position = block
        .instructions
        .iter()
        .position(|id| *id == tag.id)
        .ok_or_else(|| "tag instruction absent from block".to_owned())?;
    let drop_position = block
        .instructions
        .iter()
        .position(|id| *id == drop.id)
        .ok_or_else(|| "drop instruction absent from block".to_owned())?;
    if tag_position >= drop_position {
        return Err("owner dropped before discriminator read".to_owned());
    }
    Ok(())
}

#[path = "while_flow_tests/proof_cases.rs"]
mod proof_cases;

#[test]
fn bounded_while_fixture_reaches_verified_llvm_before_native() {
    let source =
        include_str!("../../../lang-cli/tests/fixtures/while-flow-0330/success.ko").to_owned();
    let outcomes = collect_sources(&[source])
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            let result = result.and_then(|program| {
                verify_program(&program).map_err(|error| format!("SSA verify: {error:?}"))?;
                crate::llvm::render_verified_program(&program)
                    .map_err(|error| format!("LLVM: {error:?}"))?;
                Ok(())
            });
            println!("BOUNDED WHILE unit={} {result:?}", index == 1);
            result
        })
        .collect::<Vec<_>>();
    assert_eq!(outcomes.len(), 2);
    for outcome in outcomes {
        outcome.expect("complete bounded fixture reaches verified LLVM");
    }
}

#[test]
fn bounded_pending_child_fixture_reaches_verified_llvm_before_native() {
    let source =
        include_str!("../../../lang-cli/tests/fixtures/while-flow-0330/pending.ko").to_owned();
    let outcomes = collect_sources(&[source])
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            let result = result.and_then(|program| {
                verify_program(&program).map_err(|error| format!("SSA verify: {error:?}"))?;
                crate::llvm::render_verified_program(&program)
                    .map_err(|error| format!("LLVM: {error:?}"))?;
                Ok(())
            });
            println!("BOUNDED PENDING unit={} {result:?}", index == 1);
            result
        })
        .collect::<Vec<_>>();
    assert_eq!(outcomes.len(), 2);
    for outcome in outcomes {
        outcome.expect("complete bounded pending fixture reaches verified LLVM");
    }
}
