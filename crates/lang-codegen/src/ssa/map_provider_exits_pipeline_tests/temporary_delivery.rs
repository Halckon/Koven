//! Existing ordinary parameter delivery is shared by no-For controls and temporary sources.
mod ast;
mod facts;
mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Delivery {
    BorrowAndOwnedResource,
    SameMapBorrow,
    ReorderedMixed,
}

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub control: String,
    pub golden: String,
    pub control_golden: String,
    pub delivery: Delivery,
    pub mutable: bool,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for delivery in [
        Delivery::BorrowAndOwnedResource,
        Delivery::SameMapBorrow,
        Delivery::ReorderedMixed,
    ] {
        for mutable in [false, true] {
            let kind = if mutable { "MutableMap" } else { "Map" };
            let conversion = if mutable {
                "return result"
            } else {
                "val output=result.consume();return output"
            };
            let resource =
                "class Resource(val tag:String){deinit(){println(\"deinit\");println(this.tag)}}";
            let (tag, declarations, parameters, before, value_type, value, call, prefix, read) =
                match delivery {
                    Delivery::BorrowAndOwnedResource => (
                        "borrow-owned-resource",
                        format!("{resource}\nfun borrowedArgument():Resource{{println(\"borrow-argument\");return Resource(\"borrow-drop\".clone())}}\nfun ownedArgument():Resource{{println(\"owned-argument\");return Resource(\"owned-drop\".clone())}}"),
                        "input:Resource,own payload:Resource",
                        "println(input.tag)",
                        "Resource",
                        "payload",
                        "produce(borrowedArgument(),ownedArgument())",
                        "borrow-argument\nowned-argument\nfactory\nborrow-drop\ndeinit\nborrow-drop\n",
                        "println(value.tag)",
                    ),
                    Delivery::SameMapBorrow => (
                        "same-map-borrow",
                        "fun borrowedArgument():Map<String,String>{println(\"borrow-argument\");var source=mutableMapOf<String,String>();source.put(\"input\".clone(),\"input-value\".clone());val output=source.consume();return output}".to_owned(),
                        "input:Map<String,String>",
                        "if(input.size!=1){error(\"bad-input-size\")};println(\"input-size\")",
                        "String",
                        "\"value\".clone()",
                        "produce(borrowedArgument())",
                        "borrow-argument\nfactory\ninput-size\n",
                        "println(value)",
                    ),
                    Delivery::ReorderedMixed => (
                        "reordered-mixed",
                        format!("{resource}\nfun leftArgument():String{{println(\"left-argument\");return \"left\".clone()}}\nfun rightArgument():String{{println(\"right-argument\");return \"right\".clone()}}\nfun ownedArgument():Resource{{println(\"owned-argument\");return Resource(\"owned-drop\".clone())}}"),
                        "left:String,right:String,own payload:Resource",
                        "println(left);println(right)",
                        "Resource",
                        "payload",
                        "produce(right=rightArgument(),payload=ownedArgument(),left=leftArgument())",
                        "right-argument\nowned-argument\nleft-argument\nfactory\nleft\nright\n",
                        "println(value.tag)",
                    ),
                };
            let common = format!(
                "{declarations}\nfun produce({parameters}):{kind}<String,{value_type}>{{println(\"factory\");{before};var result=mutableMapOf<String,{value_type}>();result.put(\"key\".clone(),{value});{conversion}}}\nfun read(key:String,value:{value_type}):Unit{{println(key);{read}}}\n"
            );
            let source = format!(
                "{common}fun entry():Unit{{println(\"begin\");for((key,value) in {call}){{read(key,value)}};println(\"after\")}}\n"
            );
            let control = format!(
                "{common}fun entry():Unit{{println(\"begin\");val source={call};if(source.size==1){{println(\"control\")}}else{{error(\"bad-output-size\")}};println(\"after\")}}\n"
            );
            let (body, drop) = match delivery {
                Delivery::SameMapBorrow => ("key\nvalue\n", ""),
                _ => ("key\nowned-drop\n", "deinit\nowned-drop\n"),
            };
            cases.push(Case {
                label: format!("temporary-delivery-{tag}-{kind}"),
                source,
                control,
                golden: format!("begin\n{prefix}{body}{drop}after\n"),
                control_golden: format!("begin\n{prefix}control\nafter\n{drop}"),
                delivery,
                mutable,
            });
        }
    }
    assert_eq!(cases.len(), 6);
    cases
}

fn observe(case: &Case, unit: bool, control: bool) -> Result<(), Failure> {
    let (text, golden) = if control {
        (&case.control, &case.control_golden)
    } else {
        (&case.source, &case.golden)
    };
    println!(
        "DELIVERY INPUT case={} unit={unit} control={control} source={text:?} golden={golden:?}",
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
                    "delivery qualified selected arguments/cleanup",
                    &facts::unit(case, &sources, &parsed, n, t, o, control),
                );
            },
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |n, t, o, r| {
            source_forms::facts::single(&sources, &parsed, n, t, o, r);
            r.query(
                "delivery selected arguments/cleanup",
                &facts::single(case, &sources, &parsed, n, t, o, control),
            );
        })
    };
    println!(
        "DELIVERY LOWER {} unit={unit} control={control} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    let deep = graph::check(case, &sources, &parsed, &program, control);
    report.query("delivery actual SSA owner/argument graph", &deep);
    println!(
        "DELIVERY GRAPH {} unit={unit} control={control} deep={deep:?}",
        case.label
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if !report.passed() {
        return Err(failure("original FE/SSA queries", report));
    }
    Ok(())
}

fn check(control: bool) {
    let mut outcomes = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit, control);
            println!(
                "DELIVERY CASE {} unit={unit} control={control} result={result:?}",
                case.label
            );
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert_eq!(outcomes.len(), 12);
    assert!(outcomes.iter().all(|(_, _, r)| r.is_ok()), "{outcomes:#?}");
}

#[test]
fn ordinary_borrow_and_owned_delivery_reaches_both_pipelines() {
    check(true);
}

#[test]
fn temporary_source_uses_ordinary_borrow_and_owned_delivery() {
    check(false);
}
