//! Existing represented payloads and Int keys retain a real temporary owner per For.
mod ast;
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
    pub representation: usize,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for representation in 0..5 {
        for mutable in [false, true] {
            let tag = [
                "field",
                "structural",
                "int-resource",
                "nullable-field",
                "int-string",
            ][representation];
            let kind = if mutable { "MutableMap" } else { "Map" };
            let integer = matches!(representation, 2 | 4);
            let key = if integer { "Int" } else { "String" };
            let payload = match representation {
                1 => "Payload",
                3 => "Resource?",
                4 => "String",
                _ => "Resource",
            };
            let resource = if representation == 0 || representation == 3 {
                "class Resource(val tag:String){deinit(){println(this.tag)}}"
            } else if representation == 4 {
                ""
            } else {
                "class Resource{deinit(){println(\"resource-drop\")}}"
            };
            let structural = if representation == 1 {
                "value class Payload(val resource:Resource)"
            } else {
                ""
            };
            let construct = match representation {
                0 | 3 => "Resource(\"field-drop\".clone())",
                1 => "Payload(Resource())",
                4 => "\"value\".clone()",
                _ => "Resource()",
            };
            let first = if integer { "1" } else { "\"z\".clone()" };
            let second = if integer { "2" } else { "\"a\".clone()" };
            let middle = if representation == 3 {
                "source.put(\"n\".clone(),null);"
            } else {
                ""
            };
            let read_key = if integer {
                "val actualKey:Int=key;if(actualKey==1){println(\"int-one\")}else{if(actualKey==2){println(\"int-two\")}else{error(\"bad-int-key\")}}"
            } else {
                "println(key)"
            };
            let read_value = match representation {
                3 => "val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}",
                4 => "println(value)",
                _ => "println(\"value\")",
            };
            let conversion = if mutable {
                "return source"
            } else {
                "val result=source.consume();return result"
            };
            let empty = if mutable { "mutableMapOf" } else { "mapOf" };
            let source = format!(
                r#"{resource}
{structural}
fun read(key:{key},value:{payload}):Unit{{{read_key};{read_value}}}
fun produce():{kind}<{key},{payload}>{{println("factory-start");var source=mutableMapOf<{key},{payload}>();source.put({first},{construct});{middle}source.put({second},{construct});println("factory-ready");{conversion}}}
fun produceEmpty():{kind}<{key},{payload}>{{println("empty-start");val result={empty}<{key},{payload}>();println("empty-ready");return result}}
fun entry():Unit{{println("nonempty");for((key,value) in produce()){{read(key,value)}};println("after-nonempty");println("empty");for((key,value) in produceEmpty()){{read(key,value)}};println("after-empty")}}
"#
            );
            let mut golden = String::from("nonempty\nfactory-start\nfactory-ready\n");
            let keys = if integer {
                vec!["int-one", "int-two"]
            } else if representation == 3 {
                vec!["z", "n", "a"]
            } else {
                vec!["z", "a"]
            };
            for (index, key) in keys.into_iter().enumerate() {
                golden.push_str(key);
                golden.push('\n');
                golden.push_str(if representation == 3 {
                    if index == 1 { "none\n" } else { "some\n" }
                } else {
                    "value\n"
                });
            }
            if representation != 4 {
                let drop = if representation == 0 || representation == 3 {
                    "field-drop\n"
                } else {
                    "resource-drop\n"
                };
                golden.push_str(drop);
                golden.push_str(drop);
            }
            golden.push_str("after-nonempty\nempty\nempty-start\nempty-ready\nafter-empty\n");
            cases.push(Case {
                label: format!("temporary-{tag}-{kind}-normal"),
                source,
                golden,
                mutable,
                representation,
            });
        }
    }
    assert_eq!(cases.len(), 10);
    cases
}

fn observe(case: &Case, rooted: bool) -> Result<(), Failure> {
    println!(
        "TEMPORARY FORMS INPUT case={} rooted={rooted} source={:?} golden={:?}",
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
        |names, typed, owned, report| {
            source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
            report.query(
                "represented temporary original qualified FE identities",
                &facts::unit(case, &sources, &parsed, names, typed, owned),
            );
        },
    );
    println!(
        "TEMPORARY FORMS LOWER {} rooted={rooted} report={report:#?} outcome={:?}",
        case.label,
        result.as_ref().map(|(_, id)| id)
    );
    let (program, _) = result?;
    report.query(
        "Unit original temporary entry and actual canonical owner acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    report.query(
        "represented temporary actual SSA identities and storage",
        &graph::check(case, &sources, &parsed, &program),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if !report.passed() {
        return Err(failure("original FE/SSA observations", report));
    }
    Ok(())
}

#[test]
fn existing_temporary_representations_reach_both_verified_pipelines() {
    let mut results = Vec::new();
    for case in sources() {
        for rooted in [false, true] {
            let result = observe(&case, rooted);
            println!(
                "TEMPORARY FORMS CASE {} rooted={rooted} result={result:?}",
                case.label
            );
            results.push((case.label.clone(), rooted, result));
        }
    }
    assert_eq!(results.len(), 20);
    assert!(
        results.iter().all(|(_, _, result)| result.is_ok()),
        "{results:#?}"
    );
}
