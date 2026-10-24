//! The installation event and its complete RHS share an end byte, but are different epochs.
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
mod ast;
mod facts;
mod graph;
use super::super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub mutable: bool,
    pub grouped: bool,
}
pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for grouped in [false, true] {
            let kind = if mutable { "MutableMap" } else { "Map" };
            let mut factories = String::new();
            for (name, tag, empty) in [
                ("make", "old", false),
                ("other", "new", false),
                ("makeEmpty", "old", true),
                ("otherEmpty", "new", true),
            ] {
                let put = if empty {
                    String::new()
                } else {
                    format!("source.put(\"{tag}-key\".clone(),\"{tag}-value\".clone());")
                };
                factories.push_str(&format!("fun {name}():{kind}<String,String>{{println(\"{name}-start\");var source=mutableMapOf<String,String>();{put}println(\"{name}-ready\");{}}}\n",if mutable{"return source"}else{"val result=source.consume();return result"}));
            }
            let mut scanners = String::new();
            for empty in [false, true] {
                let (scan, make, other) = if empty {
                    ("scanEmpty", "makeEmpty", "otherEmpty")
                } else {
                    ("scan", "make", "other")
                };
                let rhs = if grouped {
                    format!("((({other}())))")
                } else {
                    format!("{other}()")
                };
                scanners.push_str(&format!("fun {scan}():Unit{{var source={make}();println(\"formed\");for((key,value) in source){{val local=Guard();read(key,value)}};println(\"between\");source={rhs};println(\"tail\")}}\n"));
            }
            let source = format!(
                "class Guard{{deinit(){{println(\"guard-drop\")}}}}\nfun read(key:String,value:String):Unit{{println(key);println(value)}}\n{factories}{scanners}fun entry():Unit{{println(\"nonempty\");scan();println(\"empty\");scanEmpty();println(\"entry-end\")}}\n"
            );
            let golden="nonempty\nmake-start\nmake-ready\nformed\nold-key\nold-value\nguard-drop\nbetween\nother-start\nother-ready\ntail\nempty\nmakeEmpty-start\nmakeEmpty-ready\nformed\nbetween\notherEmpty-start\notherEmpty-ready\ntail\nentry-end\n".to_owned();
            cases.push(Case {
                label: format!(
                    "completed-epoch-{kind}-{}",
                    if grouped { "triple-group" } else { "plain" }
                ),
                source,
                golden,
                mutable,
                grouped,
            });
        }
    }
    assert_eq!(cases.len(), 4);
    cases
}
fn observed(case: &Case, unit_mode: bool) -> Result<(), Failure> {
    println!(
        "COMPLETED EPOCH INPUT {} mutable={} grouped={} source={:?} golden={:?}",
        case.label, case.mutable, case.grouped, case.source, case.golden
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
    let lowered = if unit_mode {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |n, t, o, r| {
                println!(
                    "COMPLETED EPOCH UNIT RAW plans={:#?} drops={:#?}",
                    o.map_iterations(),
                    o.drops()
                );
                r.query(
                    "completed event qualified FE identity",
                    &facts::unit(case, &sources, &parsed, n, t, o),
                );
            },
        )
    } else {
        pipeline::memory_document_observed(&sources, &parsed, &mut report, |n, t, o, r| {
            println!(
                "COMPLETED EPOCH UNIT RAW plans={:#?} drops={:#?}",
                o.map_iterations(),
                o.drops()
            );
            r.query(
                "completed event qualified FE identity",
                &facts::unit(case, &sources, &parsed, n, t, o),
            );
        })
    };
    println!(
        "COMPLETED EPOCH LOWER {} unit={unit_mode} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    let (program, _) = lowered?;
    report.query(
        "completed event actual SSA identity",
        &graph::check(case, &sources, &parsed, &program),
    );
    report.query(
        "completed event SSA verifier",
        &crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e)),
    );
    report.query(
        "completed event LLVM verifier",
        &crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e)),
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("completed event query", report))
    }
}
#[test]
fn completed_assignment_event_keeps_its_own_epoch() {
    let mut results = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observed(&case, unit);
            println!(
                "COMPLETED EPOCH CASE {} unit={unit} result={result:?}",
                case.label
            );
            results.push((case.label.clone(), unit, result));
        }
    }
    assert_eq!(results.len(), 8);
    assert!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
}
