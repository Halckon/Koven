//! Factory-formed Resource Map locals retain their original owner through all exits.
mod boundaries;
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
    pub exit: usize,
    pub nullable: bool,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for form in 0..3 {
            for exit in 0..4 {
                for nullable in [false, true] {
                    cases.push(source(mutable, form, exit, nullable));
                }
            }
        }
    }
    assert_eq!(cases.len(), 48);
    assert_eq!(
        cases
            .iter()
            .map(|c| c.label.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        48
    );
    for mutable in [false, true] {
        for form in 0..3 {
            for exit in 0..4 {
                assert_eq!(
                    cases
                        .iter()
                        .filter(|c| c.mutable == mutable && c.form == form && c.exit == exit)
                        .count(),
                    2
                );
            }
        }
    }
    cases
}

fn source(mutable: bool, form: usize, exit: usize, nullable: bool) -> Case {
    let kind = if mutable { "MutableMap" } else { "Map" };
    let payload = if nullable { "Resource?" } else { "Resource" };
    let factory = if mutable { "mutableMapOf" } else { "mapOf" };
    let conversion = if mutable {
        "return source"
    } else {
        "val result=source.consume();return result"
    };
    let middle = if nullable {
        "source.put(\"n\".clone(),null);"
    } else {
        ""
    };
    let value_read = if nullable {
        "val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}"
    } else {
        "println(\"value\")"
    };
    let transfer = ["", "continue", "break", "return result(key,value)"][exit];
    let mut scanners = String::new();
    for empty in [false, true] {
        let scan = if empty { "scanEmpty" } else { "scan" };
        let producer = if empty { "makeEmpty" } else { "make" };
        let declaration = match form {
            0 => format!("val source={producer}()"),
            1 => format!("var source={producer}()"),
            _ => format!("val original={producer}();val source=original"),
        };
        scanners.push_str(&format!(
            "fun {scan}():Int{{{declaration};println(\"formed\");for((key,value) in source){{val local=Guard();read(key,value);{transfer}}};println(\"after\");return 0}}\n"
        ));
    }
    let source = format!(
        r#"class Resource(val tag:String){{deinit(){{println(this.tag)}}}}
class Guard{{deinit(){{println("guard-drop")}}}}
fun read(key:String,value:{payload}):Unit{{println(key);{value_read}}}
fun result(key:String,value:{payload}):Int{{println("return-value");read(key,value);return 7}}
fun make():{kind}<String,{payload}>{{var source=mutableMapOf<String,{payload}>();source.put("z".clone(),Resource("resource-drop".clone()));{middle}source.put("a".clone(),Resource("resource-drop".clone()));{conversion}}}
fun makeEmpty():{kind}<String,{payload}>{{return {factory}<String,{payload}>()}}
{scanners}
fun entry():Unit{{println("nonempty");val first=scan();if(first==7){{println("seven")}}else{{println("zero")}};println("empty");val second=scanEmpty();if(second==7){{println("seven")}}else{{println("zero")}};println("entry-end")}}
"#
    );
    let mut golden = String::from("nonempty\nformed\n");
    let entries: &[(&str, &str)] = if nullable {
        &[("z", "some"), ("n", "none"), ("a", "some")]
    } else {
        &[("z", "value"), ("a", "value")]
    };
    for &(key, value) in entries
        .iter()
        .take(if exit >= 2 { 1 } else { entries.len() })
    {
        golden.push_str(&format!("{key}\n{value}\n"));
        if exit == 3 {
            golden.push_str(&format!("return-value\n{key}\n{value}\n"));
        }
        golden.push_str("guard-drop\n");
    }
    if exit != 3 {
        golden.push_str("after\n");
    }
    golden.push_str("resource-drop\nresource-drop\n");
    golden.push_str(if exit == 3 { "seven\n" } else { "zero\n" });
    golden.push_str("empty\nformed\nafter\nzero\nentry-end\n");
    Case {
        label: format!(
            "local-resource-{kind}-{}-{}-{}",
            ["val", "var", "moved-alias"][form],
            ["normal", "continue", "break", "return"][exit],
            if nullable { "nullable" } else { "direct" }
        ),
        source,
        golden,
        mutable,
        form,
        exit,
        nullable,
    }
}

fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "G2 RESOURCE INPUT case={} unit={unit} source={:?} golden={:?}",
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
                report.check(
                    "G2 Resource two actual qualified Map plans",
                    typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
                    (typed.map_iterations(), owned.map_iterations()),
                );
                println!(
                    "G2 RESOURCE UNIT case={} nullable={} formed={:#?} plans={:#?} drops={:#?}",
                    case.label,
                    case.nullable,
                    owned.formed_owned_locals(),
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
                facts::single(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "G2 Resource two actual single Map plans",
                    typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
                    (typed.map_iterations(), owned.map_iterations()),
                );
                println!(
                    "G2 RESOURCE SINGLE case={} nullable={} formed={:#?} plans={:#?} drops={:#?} conditions={:#?}",
                    case.label,
                    case.nullable,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.cleanup_conditions()
                );
            },
        )
    };
    println!(
        "G2 RESOURCE REPORT {} unit={unit} report={report:#?}",
        case.label
    );
    let (program, _) = result?;
    if !report.passed() {
        return Err(failure("original Resource FE queries", report));
    }
    graph::check(case, &sources, &parsed, &program, unit)?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}

#[test]
fn resource_locals_keep_actual_formation_and_cleanup() {
    let mut outcomes = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            println!(
                "G2 RESOURCE CASE {} unit={unit} result={result:?}",
                case.label
            );
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert!(
        outcomes.len() == 96 && outcomes.iter().all(|(_, _, r)| r.is_ok()),
        "{outcomes:#?}"
    );
}
