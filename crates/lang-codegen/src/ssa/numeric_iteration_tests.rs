//! SPEC-0329 ordinary source probes; no object emission or native execution.
use super::{integer_range_value_tests::collect_sources, verify::verify_program};

fn require_verified_sources(sources: &[String], llvm: bool) {
    // Observe both entries for every source before asserting the first missing stage.
    let observations = collect_sources(sources);
    assert_eq!(observations.len(), sources.len() * 2);
    for result in observations {
        let program = result.expect("numeric value iteration must reach SSA");
        verify_program(&program).expect("real source SSA verifies");
        if llvm {
            let rendered = crate::llvm::render_verified_program(&program)
                .expect("numeric iteration LLVM verifies in memory");
            println!("NUMERIC ITERATION LLVM {rendered}");
        }
    }
}

#[test]
fn numeric_sources_and_extreme_exits_reach_verified_ssa() {
    let mut sources = Vec::new();
    for (integer, range, minimum, maximum, previous) in [
        ("Int", "IntRange", "-2147483648", "2147483647", "2147483646"),
        (
            "Long",
            "LongRange",
            "-9223372036854775808L",
            "9223372036854775807L",
            "9223372036854775806L",
        ),
    ] {
        sources.push(format!(
            "fun entry(value:{range}):Unit{{for(item in value){{val copy:{integer}=item}}}}"
        ));
        sources.push(format!("class Holder(val value:{range})\nfun entry(holder:Holder):Unit{{for(item in holder.value){{val copy:{integer}=item}}}}"));
        for (source, exit) in [
            (format!("{minimum}..{maximum}"), "break"),
            (format!("{maximum}..{maximum}"), "continue"),
            (format!("{previous}..{maximum}"), "continue"),
            (format!("{minimum}..<{minimum}"), ""),
            (format!("{maximum}..<{maximum}"), ""),
            (format!("{maximum}..{minimum}"), ""),
        ] {
            sources.push(format!("fun entry():Unit{{for(_ in {source}){{{exit}}}}}"));
        }
    }
    require_verified_sources(&sources, false);
}

#[test]
fn numeric_source_snapshot_and_call_order_reach_verified_ssa() {
    let mut sources = Vec::new();
    for (integer, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        sources.push(format!(
            "fun make(a:{integer},b:{integer}):{range}{{println(\"source\");return a..b}}\nfun entry(a:{integer},b:{integer}):Unit{{for(item in make(a,b)){{val copy:{integer}=item;break}}}}"
        ));
        sources.push(format!(
            "fun entry(input:{range},replacement:{range},fallback:{integer}):{integer}{{var current=input;for(item in current){{current=replacement;val copy:{integer}=item;continue}};val after=current;return fallback}}"
        ));
        sources.push(format!(
            "fun entry(input:{range},fallback:{integer}):{integer}{{for(item in input){{return item}};return fallback}}"
        ));
    }
    require_verified_sources(&sources, false);
}

#[test]
fn numeric_resource_exits_and_nested_borrow_reach_verified_llvm() {
    let mut sources = Vec::new();
    for range in ["IntRange", "LongRange"] {
        for exit in ["", "continue", "break", "return"] {
            sources.push(format!(
                "class Guard(val name:String){{deinit(){{println(this.name)}}}}\nfun entry(input:{range}):Unit{{for(_ in input){{val first=Guard(\"first\");val second=Guard(\"second\");{exit}}}}}"
            ));
        }
        sources.push(format!(
            "fun entry(input:{range},values:List<Int>):Unit{{for(value in values){{for(item in input){{val copy=value;continue}};val after=value}}}}"
        ));
        sources.push(format!(
            "fun entry(input:{range},values:List<Int>):Unit{{for(item in input){{for(value in values){{val copy=value;break}};continue}}}}"
        ));
    }
    require_verified_sources(&sources, true);
}
