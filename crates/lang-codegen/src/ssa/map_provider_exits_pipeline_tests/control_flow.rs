//! Ordinary If and return delivery use the same source and cleanup facts for every source mode.
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

mod query;

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
    let result = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |_, _, owned, report| {
            use lang_frontend::{
                ownership_checking::{UnitDropPoint, UnitIterationExitKind as Exit},
                parser::{Expression, Statement},
            };
            let original_fors = parsed
                .ast()
                .statements()
                .iter()
                .filter(|(_, node)| matches!(node.payload(), Statement::For { .. }))
                .count();
            report.check(
                "ordinary CFG complete original For ownership inventory",
                owned.map_iterations().len() == original_fors,
                owned.map_iterations(),
            );
            for plan in owned.map_iterations() {
                let bridge = (|| -> Result<(), Failure> {
                    let descriptor = plan.descriptor();
                    let node = parsed
                        .ast()
                        .statements()
                        .get(descriptor.statement().statement())
                        .map_err(|e| failure("ordinary CFG original Unit For", e))?;
                    let Statement::For { source, body, .. } = node.payload() else {
                        return Err(failure("ordinary CFG descriptor is original For", node));
                    };
                    let body = parsed
                        .ast()
                        .statements()
                        .get(*body)
                        .map_err(|e| failure("ordinary CFG original For body", e))?
                        .span();
                    super::fixture::need(
                        descriptor.source().source_unit() == descriptor.statement().source_unit()
                            && descriptor.source().expression() == *source
                            && node.span().source_id() == parsed.source_id(),
                        "ordinary CFG exact Unit descriptor/source/For identity",
                        descriptor,
                    )?;
                    let expected = parsed
                        .ast()
                        .expressions()
                        .iter()
                        .filter_map(|(id, expression)| {
                            let span = expression.span();
                            if span.source_id() != body.source_id()
                                || span.start() < body.start()
                                || span.end() > body.end()
                            {
                                return None;
                            }
                            let kind = match expression.payload() {
                                Expression::Continue { .. } => 0,
                                Expression::Break { .. } => 1,
                                Expression::Return { .. } => 2,
                                _ => return None,
                            };
                            Some((id, kind))
                        })
                        .collect::<Vec<_>>();
                    let mut actual = Vec::new();
                    for exit in plan.exits() {
                        let (id, kind) = match exit.kind() {
                            Exit::Continue(id) => (id, 0),
                            Exit::Break(id) => (id, 1),
                            Exit::Return(id) => (id, 2),
                            _ => continue,
                        };
                        super::fixture::need(
                            id.source_unit() == descriptor.statement().source_unit()
                                && exit.point() == UnitDropPoint::ControlTransfer(id)
                                && !actual.contains(&(id.expression(), kind)),
                            "ordinary CFG exact unique original Unit control-transfer point",
                            exit,
                        )?;
                        actual.push((id.expression(), kind));
                    }
                    super::fixture::need(
                        actual.len() == expected.len()
                            && expected.iter().all(|transfer| actual.contains(transfer)),
                        "ordinary CFG all and only this original For body's transfer IDs/kinds",
                        (&actual, &expected),
                    )
                })();
                report.query(
                    "ordinary CFG Unit exact body transfers retain original AST spans",
                    &bridge,
                );
            }
            println!(
                "ORDINARY CFG FACTS case={label} unit=true plans={:#?} drops={:#?}",
                owned.map_iterations(),
                owned.drops()
            );
        },
    );
    println!(
        "ORDINARY CFG LOWER case={label} rooted={unit} report={report:#?} outcome={:?}",
        result.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = result?;
    let identity = query::check(&sources, &parsed, &program, true);
    println!("ORDINARY CFG QUERY case={label} rooted={unit} result={identity:?}");
    report.query("ordinary CFG complete graph identities", &identity);
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    println!(
        "ORDINARY CFG SSA case={label} rooted={unit}\n{}",
        crate::ssa::render::render_program(&program)
    );
    if !report.passed() {
        return Err(failure("original CFG FE and graph identities", report));
    }
    Ok(())
}

/// Additional source-only neighbours; the original 24 + 14 inputs stay byte-identical.
pub(crate) fn neighbor_sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        let kind = if mutable { "MutableMap" } else { "Map" };
        for tag in ["two-continues", "two-breaks", "return-if", "return-checked"] {
            if tag == "two-continues" {
                let source = format!(
                    "{}fun scan(input:{kind}<String,String>):Int{{for((key,value) in input){{val guard=Guard();if(chosen(key)){{println(\"continue-one\");continue}};if(key==\"keep\"){{println(\"continue-two\");continue}};read(key,value)}};return 0}}\n\
                     fun entry():Unit{{println(\"nonempty\");val populated=make(true);scan(populated);println(\"after-nonempty\");println(\"empty\");val empty=make(false);scan(empty);println(\"after-empty\")}}\n",
                    common(mutable)
                );
                cases.push(Case {
                    label: format!("ordinary-neighbor-{kind}-{tag}"),
                    source,
                    golden: "nonempty\nfactory\ncontinue-two\nguard-drop\ncontinue-one\nguard-drop\ntail\nlast\nguard-drop\nafter-nonempty\nempty\nfactory\nafter-empty\n".to_owned(),
                    flow: Flow::Continue,
                    mode: Source::Borrow,
                    mutable,
                });
                continue;
            }
            let finish = if mutable {
                "return result"
            } else {
                "val output=result.consume();return output"
            };
            let common = format!(
                "class Guard{{deinit(){{println(\"guard-drop\")}}}}\n\
                 class Resource(val name:String){{deinit(){{println(\"resource-drop\")}}}}\n\
                 fun make(own filled:Boolean):{kind}<String,Resource>{{println(\"factory\");var result=mutableMapOf<String,Resource>();if(filled){{result.put(\"keep\".clone(),Resource(\"first\".clone()));result.put(\"skip\".clone(),Resource(\"middle\".clone()));result.put(\"tail\".clone(),Resource(\"last\".clone()))}};{finish}}}\n\
                 fun chosen(key:String):Boolean{{return key==\"skip\"}}\n\
                 fun read(key:String,value:Resource):Unit{{println(key);println(value.name)}}\n\
                 fun operand(value:Resource):Int{{println(\"operand\");println(value.name);return 7}}\n"
            );
            let drops = "resource-drop\nresource-drop\nresource-drop\n";
            let (mode, parameters, provider, body) = match tag {
                "two-breaks" => (
                    Source::Owned,
                    format!("own input:{kind}<String,Resource>,own later:Boolean"),
                    "input",
                    "if(chosen(key)){if(later){println(\"pass-first\")}else{println(\"break-one\");break}};if(key==\"tail\"){println(\"break-two\");break};read(key,value)",
                ),
                "return-if" => (
                    Source::Temporary,
                    "own filled:Boolean,own higher:Boolean".to_owned(),
                    "make(filled)",
                    "read(key,value);return if(higher){operand(value)+1}else{operand(value)+0}",
                ),
                _ => (
                    Source::Borrow,
                    format!("input:{kind}<String,Resource>"),
                    "input",
                    "read(key,value);return operand(value)+0",
                ),
            };
            let mut entry = String::new();
            let states = if tag == "two-breaks" {
                vec![
                    (true, false, "early"),
                    (true, true, "late"),
                    (false, false, "empty"),
                ]
            } else if tag == "return-if" {
                vec![
                    (true, true, "if-true"),
                    (true, false, "if-false"),
                    (false, true, "empty"),
                ]
            } else {
                vec![(true, false, "nonempty"), (false, false, "empty")]
            };
            for (index, (filled, later, state)) in states.iter().enumerate() {
                entry.push_str(&format!("println(\"{state}\");"));
                let call = if tag == "return-if" {
                    format!("scan({filled},{later})")
                } else if matches!(mode, Source::Temporary) {
                    format!("scan({filled})")
                } else {
                    entry.push_str(&format!("val source{index}=make({filled});"));
                    if tag == "two-breaks" {
                        format!("scan(source{index},{later})")
                    } else {
                        format!("scan(source{index})")
                    }
                };
                if tag == "return-if" {
                    let (expected, marker) = if !filled {
                        (0, "zero")
                    } else if *later {
                        (8, "eight")
                    } else {
                        (7, "seven")
                    };
                    entry.push_str(&format!("val result{index}={call};if(result{index}=={expected}){{println(\"{marker}\")}}else{{error(\"bad-result\")}}"));
                } else {
                    entry.push_str(&check_result(&call, &index.to_string()));
                }
                entry.push_str(&format!(";println(\"after-{state}\");"));
            }
            let golden = match tag {
                "two-breaks" => format!(
                    "early\nfactory\nkeep\nfirst\nguard-drop\nbreak-one\nguard-drop\n{drops}zero\nafter-early\nlate\nfactory\nkeep\nfirst\nguard-drop\npass-first\nskip\nmiddle\nguard-drop\nbreak-two\nguard-drop\n{drops}zero\nafter-late\nempty\nfactory\nzero\nafter-empty\n"
                ),
                "return-if" => format!(
                    "if-true\nfactory\nkeep\nfirst\noperand\nfirst\nguard-drop\n{drops}eight\nafter-if-true\nif-false\nfactory\nkeep\nfirst\noperand\nfirst\nguard-drop\n{drops}seven\nafter-if-false\nempty\nfactory\nzero\nafter-empty\n"
                ),
                _ => format!(
                    "nonempty\nfactory\nkeep\nfirst\noperand\nfirst\nguard-drop\nreturned\nafter-nonempty\nempty\nfactory\nzero\nafter-empty\n{drops}"
                ),
            };
            cases.push(Case {
                label: format!("ordinary-neighbor-{kind}-{tag}"),
                source: format!(
                    "{common}fun scan({parameters}):Int{{for((key,value) in {provider}){{val guard=Guard();{body}}};return 0}}\nfun entry():Unit{{{entry}}}\n"
                ),
                golden,
                flow: if tag == "two-breaks" { Flow::Break } else { Flow::Return },
                mode,
                mutable,
            });
        }
    }
    for mutable in [false, true] {
        let kind = if mutable { "MutableMap" } else { "Map" };
        for unit_return in [true, false] {
            let (tag, return_type, returned, tail, nonempty, empty) = if unit_return {
                (
                    "bare-unit",
                    "Unit",
                    "return",
                    "",
                    "unit-returned\n",
                    "unit-returned\n",
                )
            } else {
                (
                    "owned-string",
                    "String",
                    "return \"delivered\".clone()",
                    "return \"empty-result\".clone()",
                    "delivered\n",
                    "empty-result\n",
                )
            };
            let mut entry = String::new();
            for (filled, state) in [(true, "nonempty"), (false, "empty")] {
                entry.push_str(&format!("println(\"{state}\");"));
                if unit_return {
                    entry.push_str(&format!("scan({filled});println(\"unit-returned\");"));
                } else {
                    entry.push_str(&format!("val {state}=scan({filled});println({state});"));
                }
                entry.push_str(&format!("println(\"after-{state}\");"));
            }
            cases.push(Case {
                label: format!("ordinary-neighbor-{kind}-temporary-{tag}"),
                source: format!(
                    "{}fun scan(own filled:Boolean):{return_type}{{for((key,value) in make(filled)){{val guard=Guard();read(key,value);{returned}}};{tail}}}\nfun entry():Unit{{{entry}}}\n",
                    common(mutable)
                ),
                golden: format!("nonempty\nfactory\nkeep\nfirst\nguard-drop\n{nonempty}after-nonempty\nempty\nfactory\n{empty}after-empty\n"),
                flow: Flow::Return,
                mode: Source::Temporary,
                mutable,
            });
        }
    }
    for mutable in [false, true] {
        let kind = if mutable { "MutableMap" } else { "Map" };
        let finish = if mutable {
            "return result"
        } else {
            "val output=result.consume();return output"
        };
        let mut entry = String::new();
        for (mode, tag) in [(1, "returned"), (2, "broken"), (0, "empty")] {
            entry.push_str(&format!(
                "println(\"{tag}\");val source{mode}=make({mode});{};println(\"after-{tag}\");",
                check_result(&format!("scan(source{mode})"), &mode.to_string())
            ));
        }
        cases.push(Case {
            label: format!("ordinary-neighbor-{kind}-owned-checked-return-break"),
            source: format!(
                "class Guard{{deinit(){{println(\"guard-drop\")}}}}\n\
                 fun make(own mode:Int):{kind}<Int,String>{{println(\"factory\");var result=mutableMapOf<Int,String>();if(mode==1){{result.put(1,\"first\".clone())}}else{{if(mode==2){{result.put(2,\"second\".clone())}}}};{finish}}}\n\
                 fun read(key:Int,value:String):Unit{{if(key==1){{println(\"key-one\")}}else{{println(\"key-two\")}};println(value)}}\n\
                 fun scan(own input:{kind}<Int,String>):Int{{for((key,value) in input){{val guard=Guard();read(key,value);if(key==1){{return key+6}};if(key==2){{break}}}};return 0}}\n\
                 fun entry():Unit{{{entry}}}\n"
            ),
            golden: "returned\nfactory\nkey-one\nfirst\nguard-drop\nreturned\nafter-returned\nbroken\nfactory\nkey-two\nsecond\nguard-drop\nzero\nafter-broken\nempty\nfactory\nzero\nafter-empty\n".to_owned(),
            flow: Flow::Return,
            mode: Source::Owned,
            mutable,
        });
    }
    assert_eq!(cases.len(), 14);
    cases
}

#[test]
fn ordinary_cfg_neighbors_reach_both_pipelines() {
    let mut outcomes = Vec::new();
    for case in neighbor_sources() {
        for unit in [false, true] {
            println!(
                "ORDINARY CFG NEIGHBOR INPUT case={} rooted={unit} source={:?} golden={:?}",
                case.label, case.source, case.golden
            );
            let result = observe(&case.label, &case.source, unit);
            println!(
                "ORDINARY CFG NEIGHBOR CASE case={} rooted={unit} result={result:?}",
                case.label
            );
            outcomes.push(result);
        }
    }
    assert_eq!(outcomes.len(), 28);
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
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
                "ORDINARY RETURN INPUT case={} rooted={unit} source={:?} golden={:?}",
                case.label, case.source, case.golden
            );
            let result = observe(&case.label, &case.source, unit);
            println!(
                "ORDINARY RETURN CASE case={} rooted={unit} result={result:?}",
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
                "ORDINARY CFG INPUT case={} rooted={unit} source={:?} golden={:?} axes={:?}",
                case.label,
                case.source,
                case.golden,
                (case.flow, case.mode, case.mutable)
            );
            let result = observe(&case.label, &case.source, unit);
            println!(
                "ORDINARY CFG CASE case={} rooted={unit} result={result:?}",
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
            println!("ORDINARY CFG INPUT case=noFor-{mutable} rooted={unit} source={text:?}");
            let result = observe("noFor", &text, unit);
            println!("ORDINARY CFG CONTROL mutable={mutable} rooted={unit} result={result:?}");
            outcomes.push(result);
        }
    }
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
}

pub(crate) fn future_transfer_sources() -> Vec<(bool, String)> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        let kind = if mutable { "MutableMap" } else { "Map" };
        let transfer = if mutable {
            "sinkMutable(source)"
        } else {
            "sinkMap(source)"
        };
        let text = format!(
            "fun sinkMap(own value:Map<String,String>):Unit{{println(\"sink\")}}\n\
             fun sinkMutable(own value:MutableMap<String,String>):Unit{{println(\"sink\")}}\n\
             fun scan(own source:{kind}<String,String>):Unit{{for((key,value) in source){{if(true){{println(key)}}else{{println(value)}}}};{transfer}}}\n\
             fun entry():Unit{{val input={factory}<String,String>();scan(input)}}\n",
            factory = if mutable { "mutableMapOf" } else { "mapOf" }
        );
        cases.push((mutable, text));
    }
    cases
}

fn observe_future_transfer(mutable: bool, text: &str, unit: bool) -> Result<(), Failure> {
    use lang_frontend::{ownership_checking::UnitDropTarget, parser::Statement};

    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let for_span =
        super::query_inputs::one(
            parsed.ast().statements().iter().filter_map(|(_, n)| {
                matches!(n.payload(), Statement::For { .. }).then_some(n.span())
            }),
            "actual future-transfer For",
        )?;
    let mut report = Report::default();
    let mut identity = None;
    let result = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |names, typed, owned, report| {
            let observed =
                super::local_exits::formation::unit(&sources, &parsed, names, typed, owned);
            report.query("future-transfer qualified original identities", &observed);
            identity = Some(observed);
            let resolution = names.names().source_units().first().unwrap().resolution();
            let symbols = resolution
                .symbols()
                .iter()
                .filter(|s| s.name() == "source")
                .map(|s| s.id())
                .collect::<Vec<_>>();
            report.check("future-transfer original owner has no Drop", symbols.len()==1 && !owned.drops().iter().any(|d|
                        matches!(d.target(), UnitDropTarget::Named(s) if symbols.contains(&s.symbol()))), (&symbols, owned.drops()));
        },
    );
    if let (Some(Ok(input)), Ok((program, _))) = (&identity, &result) {
        report.query(
            "future-transfer exact live current Value delivered once",
            &super::local_exits::formation::check(&sources, &parsed, program, input),
        );
    }
    if let Ok((program, _)) = &result {
        report.query(
            "future-transfer actual SSA verifier",
            &crate::ssa::verify::verify_program(program).map_err(|e| failure("SSA", e)),
        );
        report.query(
            "future-transfer actual LLVM renderer",
            &crate::llvm::render_verified_program(program)
                .map(|_| ())
                .map_err(|e| failure("LLVM", e)),
        );
    }
    let sealed = "unit full ownership validator";
    println!(
        "ORDINARY CFG FUTURE TRANSFER ORIGINAL mutable={mutable} rooted={unit} for_span={for_span:?} report={report:#?} result={:?}",
        result.as_ref().map(|(_, entry)| entry)
    );
    if !report.passed()
        || !report
            .checks
            .iter()
            .any(|(name, pass, _)| *name == sealed && *pass)
    {
        return Err(failure("future-transfer sealed original FE", &report));
    }
    result?;
    Ok(())
}

#[test]
fn ordinary_future_source_transfer_reaches_verified_llvm() {
    let mut outcomes = Vec::new();
    for (mutable, text) in future_transfer_sources() {
        for unit in [false, true] {
            let observed = observe_future_transfer(mutable, &text, unit);
            println!(
                "ORDINARY CFG FUTURE TRANSFER mutable={mutable} rooted={unit} source={text:?} result={observed:?}"
            );
            outcomes.push(observed);
        }
    }
    assert_eq!(outcomes.len(), 4);
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
}

/// The original scanner and sinks are retained byte for byte. Only the entry
/// now constructs one real pair, so the same conditional body executes natively.
pub(crate) fn populated_future_transfer_sources() -> Vec<(bool, String)> {
    future_transfer_sources()
        .into_iter()
        .map(|(mutable, text)| {
            let (scanner, _) = text.split_once("fun entry():Unit").unwrap();
            let transfer = if mutable {
                "scan(filled)"
            } else {
                "val input=filled.consume();scan(input)"
            };
            let entry=format!("fun entry():Unit{{var filled=mutableMapOf<String,String>();filled.put(\"key\".clone(),\"value\".clone());{transfer}}}\n");
            (mutable, format!("{scanner}{entry}"))
        })
        .collect()
}

#[test]
fn populated_future_source_transfer_reaches_verified_llvm() {
    let mut outcomes = vec![];
    for (mutable, text) in populated_future_transfer_sources() {
        for unit in [false, true] {
            println!(
                "POPULATED FUTURE INPUT mutable={mutable} rooted={unit} source={text:?} golden={:?}",
                "key\nsink\n"
            );
            let observed = observe_future_transfer(mutable, &text, unit);
            println!("POPULATED FUTURE CASE mutable={mutable} rooted={unit} result={observed:?}");
            outcomes.push(observed);
        }
    }
    assert_eq!(outcomes.len(), 4);
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
}
