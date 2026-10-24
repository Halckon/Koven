//! Execute both argument CFG edges through the original empty-Map source and ordinary call.
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms, temporary_argument_cfg,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for control in [false, true] {
            for flag in [false, true] {
                let original = temporary_argument_cfg::source(mutable, control);
                let declaration = "fun entry(flag:Boolean)";
                assert_eq!(original.matches(declaration).count(), 1);
                let old_guard = "if(input==1){println(\"one\")}else{println(\"two\")}";
                assert_eq!(original.matches(old_guard).count(), 1);
                let scan = original
                    .replace(declaration, "fun scan(flag:Boolean)")
                    .replace(old_guard, "if(input==1){println(\"one\")}else{if(input==2){println(\"two\")}else{error(\"bad-input\")}}");
                let source = format!(
                    "{scan}fun entry():Unit{{println(\"begin\");scan({flag});println(\"after\")}}\n"
                );
                let kind = if mutable { "MutableMap" } else { "Map" };
                let branch = if flag { "one" } else { "two" };
                let middle = if control { "control\n" } else { "" };
                cases.push(Case {
                    label: format!("temporary-argument-CFG-{kind}-flag-{flag}-control-{control}"),
                    source,
                    golden: format!("begin\n{branch}\n{middle}after\n"),
                });
            }
        }
    }
    assert_eq!(cases.len(), 8);
    cases
}

fn observe(case: &Case, rooted: bool) -> Result<(), Failure> {
    println!(
        "ARGUMENT CFG RUNTIME INPUT case={} rooted={rooted} source={:?} golden={:?}",
        case.label, case.source, case.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", &case.source)
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
        |n, t, o, r| {
            source_forms::facts::unit(&sources, &parsed, n, t, o, r);
        },
    );
    println!(
        "ARGUMENT CFG RUNTIME LOWER {} rooted={rooted} report={report:#?} outcome={:?}",
        case.label,
        result.as_ref().map(|(_, entry)| entry)
    );
    if !report.passed() {
        return Err(failure("original runtime FE queries", report));
    }
    let (program, _) = result?;
    report.query(
        "Unit original temporary entry and actual canonical owner acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    report.query(
        "actual runtime argument CFG and hidden owner identity",
        &super::temporary_argument_cfg_query::check(&sources, &parsed, &program),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if !report.passed() {
        return Err(failure(
            "original runtime CFG argument and owner queries",
            report,
        ));
    }
    Ok(())
}

#[test]
fn both_argument_cfg_edges_reach_both_pipelines() {
    let mut outcomes = Vec::new();
    for case in sources() {
        for rooted in [false, true] {
            let result = observe(&case, rooted);
            println!(
                "ARGUMENT CFG RUNTIME CASE {} rooted={rooted} result={result:?}",
                case.label
            );
            outcomes.push(result);
        }
    }
    assert_eq!(outcomes.len(), 16);
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
}
