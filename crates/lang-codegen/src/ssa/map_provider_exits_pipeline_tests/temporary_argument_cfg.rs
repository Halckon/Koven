//! Legal argument CFG is an explicit pending provider capability, never missing FE facts.
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

fn source(mutable: bool, control: bool) -> String {
    let kind = if mutable { "MutableMap" } else { "Map" };
    let factory = if mutable { "mutableMapOf" } else { "mapOf" };
    let body = if control {
        "val source=produce(if(flag){1}else{2});if(source.size==0){println(\"control\")}else{error(\"bad-size\")}"
    } else {
        "for((key,value) in produce(if(flag){1}else{2})){read(key,value)}"
    };
    format!(
        "fun produce(input:Int):{kind}<String,String>{{if(input==1){{println(\"one\")}}else{{println(\"two\")}};return {factory}<String,String>()}}\nfun read(key:String,value:String):Unit{{println(key);println(value)}}\nfun entry(flag:Boolean):Unit{{{body}}}\n"
    )
}

fn observe(mutable: bool, unit: bool, control: bool) -> Result<(), Failure> {
    let text = source(mutable, control);
    println!("ARGUMENT CFG INPUT mutable={mutable} unit={unit} control={control} source={text:?}");
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", &text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |n, t, o, r| source_forms::facts::unit(&sources, &parsed, n, t, o, r),
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |n, t, o, r| {
            source_forms::facts::single(&sources, &parsed, n, t, o, r);
        })
    };
    println!(
        "ARGUMENT CFG LOWER mutable={mutable} unit={unit} control={control} report={report:#?} outcome={:?}",
        lowered.as_ref().map(|(_, entry)| entry)
    );
    if !report.passed() {
        return Err(failure("original FE queries", report));
    }
    if control {
        let (program, _) = lowered?;
        crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
        crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
        Ok(())
    } else {
        let Err(stopped) = lowered else {
            return Err(failure(
                "CFG provider pending boundary unexpectedly admitted",
                (mutable, unit),
            ));
        };
        if stopped.stage != "lower"
            || !stopped.original.contains("kind: UnsupportedNode")
            || !stopped
                .original
                .contains("for((key,value) in produce(if(flag){1}else{2}))")
        {
            return Err(failure(
                "CFG pending boundary must retain original For UnsupportedNode",
                stopped,
            ));
        }
        Ok(())
    }
}

fn check(control: bool) {
    let mut outcomes = Vec::new();
    for mutable in [false, true] {
        for unit in [false, true] {
            let result = observe(mutable, unit, control);
            println!(
                "ARGUMENT CFG CASE mutable={mutable} unit={unit} control={control} result={result:?}"
            );
            outcomes.push(result);
        }
    }
    assert_eq!(outcomes.len(), 4);
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
}

#[test]
fn ordinary_call_with_argument_cfg_reaches_both_pipelines() {
    check(true);
}

#[test]
fn pending_provider_argument_cfg_is_unsupported_not_missing_fact() {
    check(false);
}
