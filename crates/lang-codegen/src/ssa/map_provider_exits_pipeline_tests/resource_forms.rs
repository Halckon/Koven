//! Existing Resource storage must not be filtered by nominal shape or key syntax.
mod representation;
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
    for representation in 0..4 {
        for mutable in [false, true] {
            for exit in 0..4 {
                let tag = ["field", "structural", "int-key", "nullable-field"][representation];
                let kind = if mutable { "MutableMap" } else { "Map" };
                let key = if representation == 2 { "Int" } else { "String" };
                let payload = match representation {
                    1 => "Payload",
                    3 => "Resource?",
                    _ => "Resource",
                };
                let class = if representation == 0 || representation == 3 {
                    "class Resource(val tag:String){deinit(){println(this.tag)}}"
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
                    _ => "Resource()",
                };
                let first = if key == "Int" { "1" } else { "\"z\".clone()" };
                let second = if key == "Int" { "2" } else { "\"a\".clone()" };
                let middle = if representation == 3 {
                    "source.put(\"n\".clone(),null);"
                } else {
                    ""
                };
                let read_key = if key == "Int" {
                    "val actualKey:Int=key; println(\"int-key\")"
                } else {
                    "println(key)"
                };
                let read_value = if representation == 3 {
                    "val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}"
                } else {
                    "println(\"value\")"
                };
                let conversion = if mutable {
                    "return source"
                } else {
                    "val result=source.consume();return result"
                };
                let transfer = ["", "continue", "break", "return 7"][exit];
                let returns = if exit == 3 { ":Int" } else { ":Unit" };
                let fallback = if exit == 3 { "return 0" } else { "" };
                let caller = if exit == 3 {
                    "val result=scan(actual);if(result==7){println(\"returned\")}else{println(\"fallback\")}"
                } else {
                    "scan(actual);println(\"done\")"
                };
                let empty_caller = if exit == 3 {
                    "val emptyResult=scan(empty);if(emptyResult==0){println(\"fallback\")}else{println(\"unexpected\")}"
                } else {
                    "scan(empty);println(\"done\")"
                };
                let source = format!(
                    r#"{class}
{structural}
class Guard{{deinit(){{println("guard-drop")}}}}
fun read(key:{key},value:{payload}):Unit{{{read_key};{read_value}}}
fun readMap(source:{kind}<{key},{payload}>):Unit{{if(source.size==0){{println("zero")}}else{{println("nonzero")}}}}
fun make():{kind}<{key},{payload}>{{var source=mutableMapOf<{key},{payload}>();source.put({first},{construct});{middle}source.put({second},{construct});{conversion}}}
fun scan(own source:{kind}<{key},{payload}>){returns}{{for((key,value) in source){{val local=Guard();read(key,value);{transfer}}};println("after");readMap(source);{fallback}}}
fun entry():Unit{{println("nonempty");val actual=make();{caller};println("empty");val empty={factory}<{key},{payload}>();{empty_caller};println("entry-end")}}
"#,
                    factory = if mutable { "mutableMapOf" } else { "mapOf" }
                );
                let mut golden = String::from("nonempty\n");
                let keys = if representation == 2 {
                    vec!["int-key", "int-key"]
                } else if representation == 3 {
                    vec!["z", "n", "a"]
                } else {
                    vec!["z", "a"]
                };
                for (i, key) in keys
                    .iter()
                    .enumerate()
                    .take(if exit >= 2 { 1 } else { keys.len() })
                {
                    golden.push_str(key);
                    golden.push('\n');
                    golden.push_str(if representation == 3 {
                        if i == 1 { "none\n" } else { "some\n" }
                    } else {
                        "value\n"
                    });
                    golden.push_str("guard-drop\n");
                }
                if exit != 3 {
                    golden.push_str("after\nnonzero\n");
                }
                for _ in 0..2 {
                    golden.push_str(if representation == 0 || representation == 3 {
                        "field-drop\n"
                    } else {
                        "resource-drop\n"
                    });
                }
                golden.push_str(if exit == 3 { "returned\n" } else { "done\n" });
                golden.push_str("empty\nafter\nzero\n");
                golden.push_str(if exit == 3 { "fallback\n" } else { "done\n" });
                golden.push_str("entry-end\n");
                cases.push(Case {
                    label: format!(
                        "{tag}-{kind}-{}",
                        ["normal", "continue", "break", "return"][exit]
                    ),
                    source,
                    golden,
                    mutable,
                    representation,
                });
            }
        }
    }
    assert_eq!(cases.len(), 32);
    assert_eq!(
        cases
            .iter()
            .map(|c| c.label.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        32
    );
    for representation in 0..4 {
        for mutable in [false, true] {
            assert_eq!(
                cases
                    .iter()
                    .filter(|c| c.representation == representation && c.mutable == mutable)
                    .count(),
                4
            );
        }
    }

    cases
}
fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "RESOURCE FORMS INPUT case={} rooted={unit} source={:?} golden={:?}",
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
        unit,
        |names, typed, owned, report| {
            source_forms::facts::unit(&sources, &parsed, names, typed, owned, report)
        },
    );
    println!(
        "RESOURCE FORMS {} rooted={unit} report={report:#?} outcome={:?}",
        case.label,
        result.as_ref().map(|(_, id)| id)
    );
    let (program, _) = result?;
    report.query(
        "Unit temporary Entry keeps outer source and actual producer/owner transport",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    if !report.passed() {
        return Err(failure("original frontend observations", report));
    }
    source_forms::graph::owned_map(&sources, &parsed, &program, case.mutable)?;
    representation::check(case, &sources, &parsed, &program)?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}
#[test]
fn existing_resource_representations_reach_verified_llvm() {
    let mut outcomes = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            println!(
                "RESOURCE FORMS CASE {} rooted={unit} result={result:?}",
                case.label
            );
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert!(
        outcomes.len() == 64 && outcomes.iter().all(|(_, _, r)| r.is_ok()),
        "{outcomes:#?}"
    );
}
