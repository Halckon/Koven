//! Ordinary legal neighbors distinguish transparent RHS groups from unproved epoch arity.
use super::*;
use lang_frontend::parser::{ParsedFile, Statement};

fn grouped_sources() -> Vec<Case> {
    let cases = super::sources()
        .into_iter()
        .filter(|case| case.history == 1)
        .map(|mut case| {
            assert_eq!(case.source.matches("source=other()").count(), 1);
            assert_eq!(case.source.matches("source=otherEmpty()").count(), 1);
            case.source = case
                .source
                .replace("source=other()", "source=(((other())))")
                .replace("source=otherEmpty()", "source=(((otherEmpty())))");
            case.label.push_str("-three-rhs-groups");
            case
        })
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 6);
    cases
}

fn arity_sources() -> Vec<(&'static str, String)> {
    let prefix = r#"class Resource{deinit(){println("resource-drop")}}
class Guard{deinit(){println("guard-drop")}}
fun read(key:String,value:Resource):Unit{println(key);println("value")}
fun make():Map<String,Resource>{var source=mutableMapOf<String,Resource>();source.put("k".clone(),Resource());val result=source.consume();return result}
fun makeWithArg(number:Int):Map<String,Resource>{val actual:Int=number;println("arity");return make()}
"#;
    [
        ("future-arity-epoch", "var source=make();for((key,value) in source){val local=Guard();read(key,value)};println(\"between\");source=makeWithArg(1);println(\"tail\")"),
        ("previous-arity-epoch", "var source=makeWithArg(1);println(\"formed\");source=make();for((key,value) in source){val local=Guard();read(key,value)};println(\"tail\")"),
    ].into_iter().map(|(label, body)| {
        (label, format!("{prefix}fun scan():Unit{{{body}}}\nfun entry():Unit{{scan()}}\n"))
    }).collect()
}

fn observe(label: &str, source: &str, golden: Option<&str>, unit: bool) -> Result<(), Failure> {
    println!(
        "LOCAL REASSIGN NEIGHBOR INPUT case={label} unit={unit} source={source:?} golden={golden:?}"
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let expected = if golden.is_some() { 4 } else { 1 };
    // Keep public validation and ordinary lowering independent: a query result
    // is recorded, never returned early instead of the actual lowering outcome.
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                report.check(
                    "neighbor original qualified descriptor count",
                    typed.map_iterations().len() == expected
                        && owned.map_iterations().len() == expected,
                    (typed.map_iterations(), owned.map_iterations()),
                );
                println!(
                    "LOCAL REASSIGN NEIGHBOR UNIT case={label} plans={:#?} drops={:#?}",
                    owned.map_iterations(),
                    owned.drops()
                );
            },
        )
    } else {
        pipeline::single_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::single(&sources, &parsed, names, typed, owned, report);
                report.check(
                    "neighbor original descriptor count",
                    typed.map_iterations().len() == expected
                        && owned.map_iterations().len() == expected,
                    (typed.map_iterations(), owned.map_iterations()),
                );
                println!(
                    "LOCAL REASSIGN NEIGHBOR SINGLE case={label} plans={:#?} drops={:#?} conditions={:#?}",
                    owned.map_iterations(),
                    owned.drops(),
                    owned.cleanup_conditions()
                );
            },
        )
    };
    println!(
        "LOCAL REASSIGN NEIGHBOR LOWER case={label} unit={unit} report={report:#?} outcome={:?}",
        lowered.as_ref().map(|(_, entry)| entry)
    );
    if golden.is_none() {
        let expected_span = original_for(&parsed)?;
        let precise = match &lowered {
            Err(error) => {
                error.stage == "lower"
                    && error.original.contains("kind: UnsupportedNode")
                    && error
                        .original
                        .contains(&format!("span: Some({expected_span:?})"))
                    && !error.original.contains("MissingFact")
            }
            Ok(_) => false,
        };
        report.check(
            "legal unproved other-epoch arity is Unsupported at original For",
            precise,
            (&lowered.as_ref().map(|(_, entry)| entry), expected_span),
        );
    } else {
        let (program, _) = lowered?;
        let verified =
            crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA verifier", e));
        report.query("grouped RHS actual SSA verifier", &verified);
        let llvm =
            crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM verifier", e));
        report.query("grouped RHS actual LLVM verifier", &llvm);
    }
    println!("LOCAL REASSIGN NEIGHBOR FINAL case={label} unit={unit} report={report:#?}");
    if report.passed() {
        Ok(())
    } else {
        Err(failure("neighbor observation failed", report))
    }
}

fn original_for(parsed: &ParsedFile) -> Result<lang_frontend::source::Span, Failure> {
    super::super::query_inputs::one(
        parsed.ast().statements().iter().filter_map(|(_, node)| {
            matches!(node.payload(), Statement::For { .. }).then_some(node.span())
        }),
        "one actual original arity-neighbor For",
    )
}

#[test]
fn grouped_replacement_rhs_preserves_both_verified_pipelines() {
    let mut observations = Vec::new();
    for case in grouped_sources() {
        for unit in [false, true] {
            let result = observe(&case.label, &case.source, Some(&case.golden), unit);
            println!(
                "LOCAL REASSIGN GROUP CASE {} unit={unit} result={result:?}",
                case.label
            );
            observations.push((case.label.clone(), unit, result));
        }
    }
    assert_eq!(observations.len(), 12);
    assert!(
        observations.iter().all(|(_, _, result)| result.is_ok()),
        "{observations:#?}"
    );
}

#[test]
fn legal_other_epoch_arity_remains_precise_unsupported() {
    let mut observations = Vec::new();
    for (label, source) in arity_sources() {
        for unit in [false, true] {
            let result = observe(label, &source, None, unit);
            println!("LOCAL REASSIGN ARITY CASE {label} unit={unit} result={result:?}");
            observations.push((label, unit, result));
        }
    }
    assert_eq!(observations.len(), 4);
    assert!(
        observations.iter().all(|(_, _, result)| result.is_ok()),
        "{observations:#?}"
    );
}
