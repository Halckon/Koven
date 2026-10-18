//! Real local formation and Move sources; red observations precede capability changes.
mod boundaries;
pub(crate) use boundaries::{alias_rebound_source, sources as alternate_sources};
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod facts;
mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub mutable: bool,
    pub form: usize,
    pub exit: usize,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for form in 0..3 {
            for exit in 0..4 {
                let kind = if mutable { "MutableMap" } else { "Map" };
                let conversion = if mutable {
                    "return source"
                } else {
                    "val result=source.consume();return result"
                };
                let factory = if mutable { "mutableMapOf" } else { "mapOf" };
                let transfer = ["", "continue", "break", "return result(key,value)"][exit];
                let mut scans = String::new();
                for empty in [false, true] {
                    let scan = if empty { "scanEmpty" } else { "scan" };
                    let producer = if empty { "makeEmpty" } else { "make" };
                    let declaration = match form {
                        0 => format!("val source={producer}()"),
                        1 => format!("var source={producer}()"),
                        _ => format!("val original={producer}();val source=original"),
                    };
                    scans.push_str(&format!(
                        "fun {scan}():Int{{{declaration};println(\"formed\");for((key,value) in source){{val local=Guard();read(key,value);{transfer}}};println(\"after\");return 0}}\n"
                    ));
                }
                let source = format!(
                    r#"class Guard{{deinit(){{println("guard-drop")}}}}
fun read(key:String,value:String):Unit{{println(key);println(value)}}
fun result(key:String,value:String):Int{{println("return-value");return 7}}
fun make():{kind}<String,String>{{var source=mutableMapOf<String,String>();source.put("z".clone(),"first".clone());source.put("a".clone(),"second".clone());{conversion}}}
fun makeEmpty():{kind}<String,String>{{return {factory}<String,String>()}}
{scans}
fun entry():Unit{{println("nonempty");val first=scan();if(first==7){{println("seven")}}else{{println("zero")}};println("empty");val second=scanEmpty();if(second==7){{println("seven")}}else{{println("zero")}};println("entry-end")}}
"#
                );
                let mut golden = String::from("nonempty\nformed\nz\nfirst\n");
                if exit == 3 {
                    golden.push_str("return-value\n");
                }
                golden.push_str("guard-drop\n");
                if exit < 2 {
                    golden.push_str("a\nsecond\nguard-drop\n");
                }
                if exit == 3 {
                    golden.push_str("seven\n");
                } else {
                    golden.push_str("after\nzero\n");
                }
                golden.push_str("empty\nformed\nafter\nzero\nentry-end\n");
                cases.push(Case {
                    label: format!(
                        "local-{kind}-{}-{}",
                        ["val", "var", "moved-alias"][form],
                        ["normal", "continue", "break", "return"][exit]
                    ),
                    source,
                    golden,
                    mutable,
                    form,
                    exit,
                });
            }
        }
    }
    assert_eq!(cases.len(), 24);
    assert_eq!(
        cases
            .iter()
            .map(|c| &c.label)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        24
    );
    cases
}

fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "G2 INPUT case={} unit={unit} source={:?} golden={:?}",
        case.label, case.source, case.golden
    );
    // The locked metadata belongs to source construction; identity queries use actual AST/FE.
    println!(
        "G2 FORM mutable={} form={} exit={}",
        case.mutable, case.form, case.exit
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
    let result = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                facts::unit(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "G2 exactly two original qualified Map descriptors",
                    typed.map_iterations().len() == 2,
                    typed.map_iterations(),
                );
                println!(
                    "G2 UNIT ORIGINAL formed={:#?} plans={:#?} drops={:#?} deliveries={:#?}",
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.value_deliveries()
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
                facts::single(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "G2 exactly two original Map descriptors",
                    typed.map_iterations().len() == 2,
                    typed.map_iterations(),
                );
                println!(
                    "G2 SINGLE ORIGINAL formed={:#?} plans={:#?} drops={:#?} conditions={:#?}",
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.cleanup_conditions()
                );
            },
        )
    };
    println!("G2 REPORT {} unit={unit} {report:#?}", case.label);
    let (program, _) = result?;
    if !report.passed() {
        return Err(failure("original FE query", report));
    }
    graph::check(case, &sources, &parsed, &program)?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}

#[test]
fn owned_locals_follow_real_formation_and_move() {
    let mut outcomes = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            println!("G2 CASE {} unit={unit} result={result:?}", case.label);
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert!(
        outcomes.len() == 48 && outcomes.iter().all(|(_, _, r)| r.is_ok()),
        "{outcomes:#?}"
    );
}
