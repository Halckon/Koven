//! A real selected factory takes an evaluated argument through the ordinary call ABI.
pub(super) mod ast;
pub(super) mod facts;
pub(super) mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub control: String,
    pub golden: String,
    pub control_golden: String,
    pub mutable: bool,
}

pub(crate) fn sources() -> Vec<Case> {
    [false, true]
        .into_iter()
        .map(|mutable| {
            let kind = if mutable { "MutableMap" } else { "Map" };
            let factory = if mutable { "mutableMapOf" } else { "mapOf" };
            let conversion = if mutable {
                "return result"
            } else {
                "val output=result.consume();return output"
            };
            let common = format!(
                r#"fun argument():Int{{println("argument");return 1}}
fun produce(input:Int):{kind}<String,String>{{if(input!=1){{error("bad-input")}};println("factory");var result=mutableMapOf<String,String>();result.put("key".clone(),"value".clone());{conversion}}}
fun produce(input:Boolean):{kind}<String,String>{{println("wrong-overload");return {factory}<String,String>()}}
fun read(key:String,value:String):Unit{{println(key);println(value)}}
"#
            );
            let source = format!(
                "{common}fun entry():Unit{{println(\"begin\");for((key,value) in produce(argument())){{read(key,value)}};println(\"after\")}}\n"
            );
            let control = format!(
                "{common}fun entry():Unit{{println(\"begin\");val source=produce(argument());if(source.size==1){{println(\"control\")}}else{{error(\"bad-control-size\")}};println(\"after\")}}\n"
            );
            Case {
                label: format!("temporary-argument-{kind}-borrow-int-overload"),
                source,
                control,
                golden: "begin\nargument\nfactory\nkey\nvalue\nafter\n".to_owned(),
                control_golden: "begin\nargument\nfactory\ncontrol\nafter\n".to_owned(),
                mutable,
            }
        })
        .collect()
}

fn observe(case: &Case, unit: bool, control: bool) -> Result<(), Failure> {
    let text = if control { &case.control } else { &case.source };
    let golden = if control {
        &case.control_golden
    } else {
        &case.golden
    };
    println!(
        "TEMPORARY ARGUMENT INPUT case={} unit={unit} control={control} source={text:?} golden={golden:?}",
        case.label
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |n, t, o, r| {
                source_forms::facts::unit(&sources, &parsed, n, t, o, r);
                r.query(
                    "argument original qualified overload and Borrow facts",
                    &facts::unit(case, &sources, &parsed, n, t, o, control),
                );
            },
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |n, t, o, r| {
            source_forms::facts::single(&sources, &parsed, n, t, o, r);
            r.query(
                "argument original overload and Borrow facts",
                &facts::single(case, &sources, &parsed, n, t, o, control),
            );
        })
    };
    println!(
        "TEMPORARY ARGUMENT LOWER {} unit={unit} control={control} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    report.query(
        "argument independent selected Call and current owner graph",
        &graph::check(case, &sources, &parsed, &program, control),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if !report.passed() {
        return Err(failure("original FE queries", report));
    }
    Ok(())
}

fn check(control: bool) {
    let mut outcomes = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit, control);
            println!(
                "TEMPORARY ARGUMENT CASE {} unit={unit} control={control} result={result:?}",
                case.label
            );
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert_eq!(outcomes.len(), 4);
    assert!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}

#[test]
fn selected_argument_factory_reaches_both_provider_pipelines() {
    check(false);
}

#[test]
fn same_selected_argument_call_without_for_reaches_both_pipelines() {
    check(true);
}
