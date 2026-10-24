//! Pure source lifetime follows the frontend's actual future use, not lexical scope.
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
mod facts;
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
}
pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for exit in 0..3 {
            for form in 0..4 {
                let kind = if mutable { "MutableMap" } else { "Map" };
                let conversion = if mutable {
                    "return source"
                } else {
                    "val result=source.consume();return result"
                };
                let transfer = ["", "continue", "break"][exit];
                let after = match form {
                    0 => {
                        "println(\"middle\");val count=source.size;println(\"after-last-use\");if(count==0){println(\"zero\")}else{println(\"nonzero\")}"
                    }
                    1 => {
                        "readMap(source);println(\"middle\");readMap(source);println(\"after-last-use\")"
                    }
                    2 => {
                        "if(source.size==0){println(\"zero\")}else{println(\"nonzero\")};println(\"middle\");readMap(source);println(\"after-last-use\")"
                    }
                    _ => {
                        "println(\"second\");for((key,value) in source){val local=Guard();read(key,value)};println(\"after-last-use\")"
                    }
                };
                let source = format!(
                    r#"class Guard{{deinit(){{println("guard-drop")}}}}
fun read(key:String,value:String):Unit{{println(key);println(value)}}
fun readMap(source:{kind}<String,String>):Unit{{if(source.size==0){{println("borrow-zero")}}else{{println("borrow-nonzero")}}}}
fun make():{kind}<String,String>{{var source=mutableMapOf<String,String>();source.put("z".clone(),"first".clone());source.put("a".clone(),"second".clone());{conversion}}}
fun scan(own source:{kind}<String,String>):Unit{{for((key,value) in source){{val local=Guard();read(key,value);{transfer}}};{after};println("tail")}}
fun entry():Unit{{println("nonempty");val actual=make();scan(actual);println("empty");val empty={factory}<String,String>();scan(empty);println("entry-end")}}
"#,
                    factory = if mutable { "mutableMapOf" } else { "mapOf" }
                );
                let mut golden = String::from("nonempty\nz\nfirst\nguard-drop\n");
                if exit != 2 {
                    golden.push_str("a\nsecond\nguard-drop\n");
                }
                for empty in [false, true] {
                    if empty {
                        golden.push_str("empty\n");
                    }
                    golden.push_str(match (form, empty) {
                        (0, false) => "middle\nafter-last-use\nnonzero\n",
                        (0, true) => "middle\nafter-last-use\nzero\n",
                        (1, false) => "borrow-nonzero\nmiddle\nborrow-nonzero\nafter-last-use\n",
                        (1, true) => "borrow-zero\nmiddle\nborrow-zero\nafter-last-use\n",
                        (2, false) => "nonzero\nmiddle\nborrow-nonzero\nafter-last-use\n",
                        (2, true) => "zero\nmiddle\nborrow-zero\nafter-last-use\n",
                        (3, false) => {
                            "second\nz\nfirst\nguard-drop\na\nsecond\nguard-drop\nafter-last-use\n"
                        }
                        _ => "second\nafter-last-use\n",
                    });
                    golden.push_str("tail\n");
                }
                golden.push_str("entry-end\n");
                cases.push(Case {
                    label: format!(
                        "pure-future-{kind}-{}-{}",
                        ["normal", "continue", "break"][exit],
                        ["size", "calls", "cfg", "repeat"][form]
                    ),
                    source,
                    golden,
                    mutable,
                    form,
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
        "G9 INPUT case={} unit={unit} source={:?} golden={:?}",
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
    let result = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                facts::unit(case, &sources, &parsed, names, typed, owned, report);
                println!(
                    "G9 UNIT ORIGINAL plans={:#?} drops={:#?}",
                    owned.map_iterations(),
                    owned.drops()
                );
            },
        )
    } else {
        pipeline::memory_document_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                facts::unit(case, &sources, &parsed, names, typed, owned, report);
                println!(
                    "G9 UNIT ORIGINAL plans={:#?} drops={:#?}",
                    owned.map_iterations(),
                    owned.drops()
                );
            },
        )
    };
    println!("G9 REPORT {} unit={unit} {report:#?}", case.label);
    let (program, _) = result?;
    report.query(
        "Unit complete temporary source acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    if !report.passed() {
        return Err(failure("original FE query", report));
    }
    graph::check(case, &sources, &parsed, &program)?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}
#[test]
fn pure_owned_parameters_follow_actual_future_uses() {
    let mut outcomes = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            println!("G9 CASE {} unit={unit} result={result:?}", case.label);
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert!(
        outcomes.len() == 48 && outcomes.iter().all(|(_, _, r)| r.is_ok()),
        "{outcomes:#?}"
    );
}
