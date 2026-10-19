//! Legal alternate formation histories must not be mistaken for missing frontend facts.
mod alias;
pub(in super::super) mod formation;
use super::super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use crate::ssa::verify::verify_program;
use lang_frontend::{
    lexer::lex,
    parser::{Statement, parse_file},
    source::SourceMap,
};

pub(crate) fn sources() -> Vec<(&'static str, String)> {
    let common = r#"fun read(key:String,value:String):Unit{println(key);println(value)}
fun readMap(source:Map<String,String>):Unit{if(source.size==0){println("zero")}else{println("nonzero")}}
fun make():Map<String,String>{return mapOf<String,String>()}
fun other():Map<String,String>{return mapOf<String,String>()}
fun makeMutable():MutableMap<String,String>{return mutableMapOf<String,String>()}
"#;
    let histories = [
        (
            "reassigned",
            "",
            "var source=make();source=other()",
            "",
            "scan()",
        ),
        (
            "parameter-move",
            "own incoming:Map<String,String>",
            "val source=incoming",
            "",
            "val input=make();scan(input)",
        ),
        (
            "old-alias-rebound",
            "",
            "var old=make();val source=old;old=other()",
            "readMap(old)",
            "scan()",
        ),
        (
            "prefix-cfg",
            "",
            "val source=make();if(source.size==0){println(\"zero\")}else{println(\"nonzero\")}",
            "",
            "scan()",
        ),
        (
            "prefix-put",
            "",
            "var source=makeMutable();source.put(\"key\".clone(),\"value\".clone())",
            "",
            "scan()",
        ),
    ];
    histories.into_iter().map(|(label,parameters,formation,after,entry)|{
        (label,format!("{common}\nfun scan({parameters}):Unit{{{formation};println(\"gap\");for((key,value) in source){{read(key,value)}};{after}}}\nfun entry():Unit{{{entry}}}\n"))
    }).collect()
}

fn observe(label: &str, source: &str, unit: bool) -> Result<&'static str, Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let spans = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, n)| matches!(n.payload(), Statement::For { .. }).then_some(n.span()))
        .collect::<Vec<_>>();
    assert_eq!(spans.len(), 1);
    println!(
        "G2 ALTERNATE ORIGINAL FOR case={label} unit={unit} span={:?}",
        spans[0]
    );
    let mut report = Report::default();
    let mut identity = None;
    let result = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                let observed = formation::unit(&sources, &parsed, names, typed, owned);
                report.query(
                    "alternate original qualified formation identities",
                    &observed,
                );
                identity = Some(observed);
                if label == "old-alias-rebound" {
                    report.query(
                        "rebound alias original Unit identities",
                        &alias::unit_facts(&sources, &parsed, names, typed, owned),
                    );
                }
            },
        )
    } else {
        pipeline::single_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::single(&sources, &parsed, names, typed, owned, report);
                if label == "old-alias-rebound" {
                    println!(
                        "ALTERNATE SINGLE OLD ALIAS plans={:#?} drops={:#?} conditions={:#?}",
                        owned.map_iterations(),
                        owned.drops(),
                        owned.cleanup_conditions()
                    );
                }
                let observed = formation::single(&sources, &parsed, names, typed, owned);
                report.query("alternate original Single formation identities", &observed);
                identity = Some(observed);
            },
        )
    };
    println!(
        "G2 ALTERNATE unit={unit} source={source:?} report={report:#?} result={:?}",
        result.as_ref().map(|(_, id)| id)
    );
    if label == "old-alias-rebound"
        && let Ok((program, _)) = &result
    {
        report.query(
            "rebound alias actual SSA owner separation",
            &identity
                .as_ref()
                .ok_or_else(|| failure("alias original input bridge", "absent"))
                .and_then(|v| {
                    v.as_ref()
                        .map_err(|e| failure("alias same-analysis FE bridge", e))
                })
                .and_then(formation::alias_drop_origins)
                .and_then(|origins| alias::graph(&sources, &parsed, program, origins)),
        );
    }
    if let (Some(Ok(input)), Ok((program, _))) = (&identity, &result) {
        report.query(
            "alternate exact current owners and complete graph",
            &formation::check(&sources, &parsed, program, input),
        );
    }
    if let Ok((program, _)) = &result {
        report.query(
            "alternate actual SSA verifier",
            &verify_program(program).map_err(|e| failure("SSA", e)),
        );
        report.query(
            "alternate actual LLVM renderer",
            &crate::llvm::render_verified_program(program)
                .map(|_| ())
                .map_err(|e| failure("LLVM", e)),
        );
    }
    if !report.passed() {
        return Err(failure("alternate original FE", report));
    }
    result?;
    Ok("verified LLVM")
}

#[test]
fn legal_alternate_histories_reach_verified_llvm() {
    let mut outcomes = Vec::new();
    for (label, source) in super::alternate_sources() {
        for unit in [false, true] {
            let result = observe(label, &source, unit);
            println!("G2 ALTERNATE CASE {label} unit={unit} result={result:?}");
            outcomes.push((label, unit, result));
        }
    }
    assert!(
        outcomes.len() == 10 && outcomes.iter().all(|(_, _, r)| r.is_ok()),
        "{outcomes:#?}"
    );
}

pub(crate) fn alias_rebound_source() -> (&'static str, String, &'static str) {
    let selected = sources()
        .into_iter()
        .filter(|(label, _)| *label == "old-alias-rebound")
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1);
    let (label, source) = selected.into_iter().next().unwrap();
    (label, source, "gap\nzero\n")
}
