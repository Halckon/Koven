//! Ordinary If and return delivery use the same source and cleanup facts for every source mode.
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Flow {
    Continue,
    Break,
    Return,
    BothLive,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Source {
    Borrow,
    Owned,
    Temporary,
}

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub flow: Flow,
    pub mode: Source,
    pub mutable: bool,
}

fn common(mutable: bool) -> String {
    let kind = if mutable { "MutableMap" } else { "Map" };
    let finish = if mutable {
        "return result"
    } else {
        "val output=result.consume();return output"
    };
    format!(
        "class Guard{{deinit(){{println(\"guard-drop\")}}}}\n\
         fun make(own filled:Boolean):{kind}<String,String>{{println(\"factory\");var result=mutableMapOf<String,String>();if(filled){{result.put(\"keep\".clone(),\"first\".clone());result.put(\"skip\".clone(),\"middle\".clone());result.put(\"tail\".clone(),\"last\".clone())}};{finish}}}\n\
         fun chosen(key:String):Boolean{{return key==\"skip\"}}\n\
         fun read(key:String,value:String):Unit{{println(key);println(value)}}\n\
         fun operand(value:String):Int{{println(\"operand\");println(value);return 7}}\n"
    )
}

fn check_result(call: &str, suffix: &str) -> String {
    format!(
        "val result{suffix}={call};if(result{suffix}==7){{println(\"returned\")}}else{{if(result{suffix}==0){{println(\"zero\")}}else{{error(\"bad-result\")}}}}"
    )
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        let kind = if mutable { "MutableMap" } else { "Map" };
        for mode in [Source::Borrow, Source::Owned, Source::Temporary] {
            for flow in [Flow::Continue, Flow::Break, Flow::Return, Flow::BothLive] {
                let body = match flow {
                    Flow::Continue => {
                        "if(chosen(key)){println(\"continue\");continue};read(key,value)"
                    }
                    Flow::Break => "if(chosen(key)){println(\"break\");break};read(key,value)",
                    Flow::Return => "if(chosen(key)){return operand(value)};read(key,value)",
                    Flow::BothLive => {
                        "if(chosen(key)){println(\"yes\");read(key,value)}else{println(\"no\");read(value,key)};println(\"joined\");read(key,value)"
                    }
                };
                let (parameters, source) = match mode {
                    Source::Borrow => (format!("input:{kind}<String,String>"), "input"),
                    Source::Owned => (format!("own input:{kind}<String,String>"), "input"),
                    Source::Temporary => ("own filled:Boolean".to_owned(), "make(filled)"),
                };
                let mut entry = String::new();
                for (filled, suffix) in [(true, "Nonempty"), (false, "Empty")] {
                    let tag = if filled { "nonempty" } else { "empty" };
                    entry.push_str(&format!("println(\"{tag}\");"));
                    let call = if matches!(mode, Source::Temporary) {
                        format!("scan({filled})")
                    } else {
                        entry.push_str(&format!("val source{suffix}=make({filled});"));
                        format!("scan(source{suffix})")
                    };
                    entry.push_str(&check_result(&call, suffix));
                    entry.push_str(&format!(";println(\"after-{tag}\");"));
                }
                let source = format!(
                    "{}fun scan({parameters}):Int{{for((key,value) in {source}){{val guard=Guard();{body}}};return 0}}\nfun entry():Unit{{{entry}}}\n",
                    common(mutable)
                );
                let first = "keep\nfirst\nguard-drop\n";
                let middle = match flow {
                    Flow::Continue => format!("{first}continue\nguard-drop\ntail\nlast\nguard-drop\nzero\n"),
                    Flow::Break => format!("{first}break\nguard-drop\nzero\n"),
                    Flow::Return => format!("{first}operand\nmiddle\nguard-drop\nreturned\n"),
                    Flow::BothLive => "no\nfirst\nkeep\njoined\nkeep\nfirst\nguard-drop\nyes\nskip\nmiddle\njoined\nskip\nmiddle\nguard-drop\nno\nlast\ntail\njoined\ntail\nlast\nguard-drop\nzero\n".to_owned(),
                };
                cases.push(Case {
                    label: format!("ordinary-CFG-{kind}-{mode:?}-{flow:?}"),
                    source,
                    golden: format!("nonempty\nfactory\n{middle}after-nonempty\nempty\nfactory\nzero\nafter-empty\n"),
                    flow,
                    mode,
                    mutable,
                });
            }
        }
    }
    assert_eq!(cases.len(), 24);
    cases
}

fn observe(label: &str, text: &str, unit: bool) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
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
            |_, _, owned, _| {
                println!(
                    "ORDINARY CFG FACTS case={label} unit=true plans={:#?} drops={:#?}",
                    owned.map_iterations(),
                    owned.drops()
                );
            },
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |_, _, owned, _| {
            println!(
                "ORDINARY CFG FACTS case={label} unit=false plans={:#?} conditions={:#?} steps={:#?}",
                owned.map_iterations(),
                owned.cleanup_conditions(),
                owned.cleanup_steps()
            );
        })
    };
    println!(
        "ORDINARY CFG LOWER case={label} unit={unit} report={report:#?} outcome={:?}",
        result.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = result?;
    if !report.passed() {
        return Err(failure("original CFG FE identities", report));
    }
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    println!(
        "ORDINARY CFG SSA case={label} unit={unit}\n{}",
        crate::ssa::render::render_program(&program)
    );
    Ok(())
}

/// Return operands differ in delivery, without changing the provider protocol.
pub(crate) fn return_sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        let kind = if mutable { "MutableMap" } else { "Map" };
        for tag in [
            "literal-int",
            "bare-unit",
            "call-unit",
            "owned-string",
            "copyable-payload",
            "int-key-checked",
            "move-inline-payload",
        ] {
            let (declaration, key, value, put, read, actual) = match tag {
                "copyable-payload" => (
                    "",
                    "Int",
                    "Int",
                    "result.put(1,11);result.put(2,22)",
                    "if(key==1){println(\"key-one\")}else{if(key==2){println(\"key-two\")}else{error(\"bad-key\")}};if(value==11){println(\"value-eleven\")}else{if(value==22){println(\"value-twenty-two\")}else{error(\"bad-value\")}}",
                    "key-one\nvalue-eleven\n",
                ),
                "int-key-checked" => (
                    "",
                    "Int",
                    "String",
                    "result.put(1,\"first\".clone());result.put(2,\"second\".clone())",
                    "if(key==1){println(\"key-one\")}else{if(key==2){println(\"key-two\")}else{error(\"bad-key\")}};println(value)",
                    "key-one\nfirst\n",
                ),
                "move-inline-payload" => (
                    "value class Payload(val text:String)\n",
                    "String",
                    "Payload",
                    "result.put(\"keep\".clone(),Payload(\"first\".clone()));result.put(\"tail\".clone(),Payload(\"second\".clone()))",
                    "println(key);println(value.text)",
                    "keep\nfirst\n",
                ),
                _ => (
                    "",
                    "String",
                    "String",
                    "result.put(\"keep\".clone(),\"first\".clone());result.put(\"tail\".clone(),\"second\".clone())",
                    "println(key);println(value)",
                    "keep\nfirst\n",
                ),
            };
            let (return_type, operand, tail, marker) = match tag {
                "bare-unit" => ("Unit", "return", "", ""),
                "call-unit" => ("Unit", "unitOperand();return", "", "operand-unit\n"),
                "owned-string" | "move-inline-payload" => (
                    "String",
                    "return \"delivered\".clone()",
                    "return \"empty-result\".clone()",
                    "",
                ),
                "int-key-checked" => ("Int", "return key+6", "return 0", ""),
                _ => ("Int", "return 7", "return 0", ""),
            };
            let finish = if mutable {
                "return result"
            } else {
                "val output=result.consume();return output"
            };
            let mut entry = String::new();
            for (filled, suffix) in [(true, "Nonempty"), (false, "Empty")] {
                let state = if filled { "nonempty" } else { "empty" };
                entry.push_str(&format!(
                    "println(\"{state}\");val source{suffix}=make({filled});"
                ));
                let call = format!("scan(source{suffix})");
                match return_type {
                    "Unit" => entry.push_str(&format!("{call};println(\"unit-returned\")")),
                    "String" => entry.push_str(&format!(
                        "val result{suffix}={call};println(result{suffix})"
                    )),
                    _ => entry.push_str(&check_result(&call, suffix)),
                }
                entry.push_str(&format!(";println(\"after-{state}\");"));
            }
            let returned = match return_type {
                "Unit" => "unit-returned\n",
                "String" => "delivered\n",
                _ => "returned\n",
            };
            let empty = match return_type {
                "Unit" => "unit-returned\n",
                "String" => "empty-result\n",
                _ => "zero\n",
            };
            let source = format!(
                "class Guard{{deinit(){{println(\"guard-drop\")}}}}\n\
                 {declaration}fun make(own filled:Boolean):{kind}<{key},{value}>{{println(\"factory\");var result=mutableMapOf<{key},{value}>();if(filled){{{put}}};{finish}}}\n\
                 fun read(key:{key},value:{value}):Unit{{{read}}}\n\
                 fun unitOperand():Unit{{println(\"operand-unit\")}}\n\
                 fun scan(input:{kind}<{key},{value}>):{return_type}{{for((key,value) in input){{val guard=Guard();read(key,value);{operand}}};{tail}}}\n\
                 fun entry():Unit{{{entry}}}\n"
            );
            cases.push(Case {
                label: format!("ordinary-return-{kind}-{tag}"),
                source,
                golden: format!(
                    "nonempty\nfactory\n{actual}{marker}guard-drop\n{returned}after-nonempty\nempty\nfactory\n{empty}after-empty\n"
                ),
                flow: Flow::Return,
                mode: Source::Borrow,
                mutable,
            });
        }
    }
    assert_eq!(cases.len(), 14);
    cases
}

#[test]
fn ordinary_return_delivery_reaches_both_pipelines() {
    let mut outcomes = Vec::new();
    for case in return_sources() {
        for unit in [false, true] {
            println!(
                "ORDINARY RETURN INPUT case={} unit={unit} source={:?} golden={:?}",
                case.label, case.source, case.golden
            );
            let result = observe(&case.label, &case.source, unit);
            println!(
                "ORDINARY RETURN CASE case={} unit={unit} result={result:?}",
                case.label
            );
            outcomes.push(result);
        }
    }
    assert_eq!(outcomes.len(), 28);
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
}

#[test]
fn ordinary_conditional_sources_reach_both_pipelines() {
    let mut outcomes = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            println!(
                "ORDINARY CFG INPUT case={} unit={unit} source={:?} golden={:?} axes={:?}",
                case.label,
                case.source,
                case.golden,
                (case.flow, case.mode, case.mutable)
            );
            let result = observe(&case.label, &case.source, unit);
            println!(
                "ORDINARY CFG CASE case={} unit={unit} result={result:?}",
                case.label
            );
            outcomes.push(result);
        }
    }
    assert_eq!(outcomes.len(), 48);
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
}

#[test]
fn ordinary_factory_without_for_reaches_both_pipelines() {
    let mut outcomes = Vec::new();
    for mutable in [false, true] {
        let text = format!(
            "{}fun entry():Unit{{val nonempty=make(true);if(nonempty.size!=3){{error(\"bad-nonempty\")}};val empty=make(false);if(empty.size!=0){{error(\"bad-empty\")}};println(\"control\")}}\n",
            common(mutable)
        );
        for unit in [false, true] {
            println!("ORDINARY CFG INPUT case=noFor-{mutable} unit={unit} source={text:?}");
            let result = observe("noFor", &text, unit);
            println!("ORDINARY CFG CONTROL mutable={mutable} unit={unit} result={result:?}");
            outcomes.push(result);
        }
    }
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
}
