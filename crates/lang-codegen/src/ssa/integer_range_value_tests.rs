//! Ordinary-source Range value ABI probes; no fabricated facts or native execution.
use super::{
    lower_frontend::orchestrate::lower_scalar_file,
    model::Program,
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{declaration, parsed},
    verify::verify_program,
};
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};
fn lower_source(text: &str, unit: bool) -> Result<Program, String> {
    let mut sources = SourceMap::new();
    let (environment, types) = standard_environments();
    let result = if unit {
        println!("STAGE unit=true parse");
        let text = format!("package p\n{text}");
        let (source, file) = parsed(&mut sources, "p/range.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/range.ko", source, &file)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        println!("STAGE unit=true names");
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .map_err(|error| format!("names API: {error:?}"))?
            .validate()
            .map_err(|error| format!("names diagnostics: {error:?}"))?;
        println!("STAGE unit=true types");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .map_err(|error| format!("types API: {error:?}"))?;
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            .map_err(|error| format!("complete: {error:?}"))?;
        let typed = typed
            .validate()
            .map_err(|error| format!("types validation: {error:?}"))?;
        println!("STAGE unit=true ownership");
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .map_err(|error| format!("ownership API: {error:?}"))?
            .validate()
            .map_err(|error| format!("ownership diagnostics: {error:?}"))?;
        println!("STAGE unit=true lower");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            declaration(&names, "p", "entry"),
        )
        .map(|(program, _)| program)
    } else {
        println!("STAGE unit=false parse");
        let source = sources.add_source("range.ko", text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let file = parse_file(&sources, &lexed).unwrap();
        assert!(file.diagnostics().is_empty());
        println!("STAGE unit=false names");
        let names = resolve_names(&sources, &file, &environment).unwrap();
        if !names.diagnostics().is_empty() {
            return Err(format!("names diagnostics: {:?}", names.diagnostics()));
        }
        println!("STAGE unit=false types");
        let typed = check_types(&sources, &file, &names, &types).unwrap();
        complete_file_types(&sources, &file, &names, &types, &typed)
            .map_err(|error| format!("complete: {error:?}"))?;
        println!("STAGE unit=false ownership");
        let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
        if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
            return Err(format!(
                "ownership: {:?}, deferred: {:?}",
                owned.diagnostics(),
                owned.deferred()
            ));
        }
        println!("STAGE unit=false lower");
        lower_scalar_file(&sources, &file, &names, &typed, &owned)
    }
    .map_err(|error| format!("lower: {error:?}"));
    match &result {
        Ok(program) => println!(
            "MODE unit={unit} SOURCE {text:?} SSA {}",
            render_program(program)
        ),
        Err(error) => println!("MODE unit={unit} SOURCE {text:?} LOWER {error:?}"),
    }
    result
}

fn collect_sources(sources: &[String]) -> Vec<Result<Program, String>> {
    let mut results = Vec::new();
    for source in sources {
        for unit in [false, true] {
            println!("BEGIN unit={unit} SOURCE {source:?}");
            let result = lower_source(source, unit);
            if let Err(error) = &result {
                println!("STOP unit={unit} SOURCE {source:?} {error}");
            }
            results.push(result);
        }
    }
    results
}

fn verified(result: Result<Program, String>) -> Program {
    let program = result.expect("real Range value source must reach SSA");
    verify_program(&program).expect("ordinary source SSA verifies");
    program
}

#[test]
fn range_construction_membership_and_equality_reach_verified_ssa() {
    let mut sources = Vec::new();
    for (integer, range, min, max) in [
        ("Int", "IntRange", "-2147483648", "2147483647"),
        (
            "Long",
            "LongRange",
            "-9223372036854775808L",
            "9223372036854775807L",
        ),
    ] {
        for root in ["left..right", "left..<right"] {
            sources.push(format!(
                "fun entry(left: {integer}, right: {integer}): {range} = {root}"
            ));
        }
        for root in [
            "value in (left..right)",
            "value !in (left..<right)",
            "(left..right) == (left..<right)",
            "(left..right) != (right..left)",
        ] {
            sources.push(format!(
                "fun entry(left: {integer}, right: {integer}, value: {integer}): Boolean = {root}"
            ));
        }
        for root in [
            format!("{min}..{max}"),
            format!("{max}..{max}"),
            format!("{min}..<{min}"),
            format!("{max}..<{min}"),
        ] {
            sources.push(format!("fun entry(): {range} = {root}"));
        }
    }
    for result in collect_sources(&sources) {
        verified(result);
    }
}

#[test]
fn range_operand_calls_and_outer_join_reach_verified_llvm() {
    let mut sources = Vec::new();
    for (integer, suffix) in [("Int", ""), ("Long", "L")] {
        sources.push(format!(
            "fun first(): {integer} {{ println(\"first\"); return 1{suffix} }}\nfun last(): {integer} {{ println(\"last\"); return 3{suffix} }}\nfun entry(): Boolean = first() in (first()..<last())"
        ));
    }
    for (integer, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        sources.push(format!("fun entry(a: {integer}, b: {integer}, flag: Boolean): {range} {{ val live = \"live\".clone(); val first = a..<b; val second = b..<a; val chosen = if (flag) first else second; println(live); return chosen }}"));
    }
    for result in collect_sources(&sources) {
        let program = verified(result);
        let rendered = render_program(&program);
        println!("CALL ORDER SSA {rendered}");
        // Initial lower-gap oracle; exact call/result identities will be locked with the new op.
        assert!(rendered.contains("call"));
        println!("STAGE LLVM");
        let llvm =
            crate::llvm::render_verified_program(&program).expect("real Range LLVM verifies");
        println!("RANGE LLVM {llvm}");
        assert!(llvm.contains("define"));
    }
}

#[test]
fn range_values_cross_fields_containers_and_consumed_map_results() {
    let mut sources = Vec::new();
    for (integer, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        sources.push(format!("fun entry(a: {integer}, b: {integer}): Boolean {{ val original = a..b; val copy = original; return original == copy }}"));
        sources.push(format!("fun pass(own value: {range}): {range} = value\nfun entry(a: {integer}, b: {integer}): Boolean = pass(a..b) == (a..b)"));
        for class in ["class", "value class"] {
            sources.push(format!("{class} Holder(val value: {range})\nfun entry(a: {integer}, b: {integer}): Boolean {{ val holder = Holder(a..b); return a in holder.value }}"));
        }
        sources.push(format!("fun entry(a: {integer}, b: {integer}): Boolean {{ val values = listOf(a..b); val first = values[0]; return (a in first) && (a in values[0]) }}"));
        sources.push(format!("fun entry(a: {integer}, b: {integer}): Boolean {{ var values = mutableMapOf<Int, {range}>(); values.put(1, a..b); borrow val item = values.requireValue(1); val copy = item; return (a in copy) && values.contains(1) }}"));
        for operation in ["get", "remove"] {
            sources.push(format!("fun entry(a: {integer}, b: {integer}): Boolean {{ var values = mutableMapOf<Int, {range}>(); values.put(1, a..b); val found = values.{operation}(1); return if (found != null) (a in found) else false }}"));
        }
    }
    for result in collect_sources(&sources) {
        verified(result);
    }
}
