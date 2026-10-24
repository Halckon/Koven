//! Ordinary invalid aliasing stays separate from the supported local Borrow-result continuation.
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
    temporary_delivery::{Delivery, sources},
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

fn observe(text: &str, unit: bool) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let result = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |_, _, _, _| {},
        )
    } else {
        super::legacy_boundaries::observe(&sources, &parsed, "entry", &mut report, |_, _, _, _| {})
    };
    println!(
        "DELIVERY BOUNDARY INPUT unit={unit} source={text:?} report={report:#?} outcome={:?}",
        result.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = result?;
    if !report.passed() {
        return Err(failure("original boundary FE queries", report));
    }
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}

#[test]
fn same_root_borrow_then_owned_delivery_is_rejected_before_lowering() {
    let mut outcomes = Vec::new();
    for case in sources()
        .into_iter()
        .filter(|case| matches!(case.delivery, Delivery::BorrowAndOwnedResource))
    {
        for control in [false, true] {
            let source = if control {
                case.control.clone()
            } else {
                case.source.clone()
            };
            let call = "produce(borrowedArgument(),ownedArgument())";
            assert_eq!(source.matches(call).count(), 1);
            let text = source
                .replace(
                    "println(\"begin\");",
                    "println(\"begin\");val shared=borrowedArgument();",
                )
                .replace(call, "produce(shared,shared)");
            for unit in [false, true] {
                let result = observe(&text, unit);
                println!(
                    "DELIVERY BOUNDARY SAME ROOT {} unit={unit} control={control} result={result:?}",
                    case.label
                );
                outcomes.push(result.is_err_and(|e| {
                    e.stage.ends_with("ownership diagnostics")
                        && e.original.matches("code: L0135").count() == 1
                        && e.original.contains("move conflicts with an active loan")
                }));
            }
        }
    }
    assert_eq!(outcomes.len(), 8);
    assert!(outcomes.iter().all(|&passed| passed), "{outcomes:?}");
}

// This observer is exclusive to the retired Legacy-rejection contract below.
// Each layout keeps its own original SourceMap/parsed/name/type/ownership chain.
fn observe_local_result_unit(text: &str, rooted: bool) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let result = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        rooted,
        |_, _, _, _| {},
    );
    let input = super::input_label(rooted);
    println!(
        "DELIVERY LOCAL RESULT INPUT input={input} stages_passed={} outcome={:?}",
        report.passed(),
        result.as_ref().map(|(_, entry)| entry)
    );
    if result.is_err() || !report.passed() {
        eprintln!("DELIVERY LOCAL RESULT FAILURE input={input} source={text:?} report={report:#?}");
    }
    let (program, _) = result?;
    if !report.passed() {
        return Err(failure(
            "local Borrow-result Unit FE and entry identity",
            report,
        ));
    }
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}

// Historical exact name retained for CI identity. The retired Legacy pipeline
// rejected `result.consume()` as a non-entry slot; this is not a Unit boundary.
// Both original single-source layouts must pass full FE admission and SSA/LLVM.
#[test]
fn local_borrow_result_is_not_an_original_entry_slot() {
    let source = "class Resource(val tag:String)\nfun view(input:Resource):borrow Resource from input=input\nfun observe(input:Resource):Unit{println(input.tag)}\nfun entry():Unit{val input=Resource(\"input\".clone());borrow val alias=view(input);var result=mutableMapOf<String,String>();result.put(\"key\".clone(),\"value\".clone());val output=result.consume();observe(alias);println(\"done\")}\n";
    let mut outcomes = Vec::new();
    for rooted in [false, true] {
        let result = observe_local_result_unit(source, rooted);
        let input = super::input_label(rooted);
        println!("DELIVERY BOUNDARY LOCAL RESULT input={input} result={result:?}");
        outcomes.push(result.is_ok());
    }
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().all(|&passed| passed), "{outcomes:?}");
}
