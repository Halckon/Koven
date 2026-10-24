//! Ordinary resource witnesses make source evaluation and cleanup visible without fault instrumentation.

#[derive(Clone, Copy, Debug)]
pub(crate) enum History {
    ParameterMove,
    PrefixPut,
    AliasRebound,
    OwnerTransfer,
}

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub history: History,
    pub mutable: bool,
    pub populated: bool,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        let kind = if mutable { "MutableMap" } else { "Map" };
        let finish = if mutable {
            "return result"
        } else {
            "val output=result.consume();return output"
        };
        for history in [
            History::ParameterMove,
            History::PrefixPut,
            History::AliasRebound,
            History::OwnerTransfer,
        ] {
            if matches!(history, History::PrefixPut) && !mutable {
                continue;
            }
            for populated in [false, true] {
                let prefix_put = matches!(history, History::PrefixPut);
                let put = "result.put(\"key\".clone(),R(\"old\".clone()));";
                let factory_put = if populated && !prefix_put { put } else { "" };
                let common = format!(
                    "class R(val name:String){{deinit(){{println(\"drop\");println(name)}}}}\n\
                     class Guard{{deinit(){{println(\"guard\")}}}}\n\
                     fun make():{kind}<String,R>{{println(\"factory\");var result=mutableMapOf<String,R>();{factory_put}{finish}}}\n\
                     fun other():{kind}<String,R>{{println(\"other-factory\");var result=mutableMapOf<String,R>();result.put(\"new-key\".clone(),R(\"new\".clone()));{finish}}}\n\
                     fun read(key:String,value:R):Unit{{println(key);println(value.name)}}\n\
                     fun readMap(source:{kind}<String,R>):Unit{{if(source.size==1){{println(\"other-one\")}}else{{error(\"wrong-other\")}}}}\n\
                     fun sink(own value:{kind}<String,R>):Unit{{println(\"sink\");if(value.size==0){{println(\"empty\")}}else{{if(value.size==1){{println(\"one\")}}else{{error(\"wrong-size\")}}}}}}\n"
                );
                let (parameters, formation, after, entry) = match history {
                    History::ParameterMove => (
                        format!("own incoming:{kind}<String,R>"),
                        "val source=incoming;".to_owned(),
                        "println(\"after\");",
                        "val input=make();scan(input)",
                    ),
                    History::PrefixPut => (
                        String::new(),
                        format!(
                            "var source=make();{}",
                            if populated {
                                "source.put(\"key\".clone(),R(\"old\".clone()));"
                            } else {
                                ""
                            }
                        ),
                        "println(\"after\");",
                        "scan()",
                    ),
                    History::AliasRebound => (
                        String::new(),
                        "var old=make();val source=old;old=other();".to_owned(),
                        "readMap(old);println(\"after\");",
                        "scan()",
                    ),
                    History::OwnerTransfer => (
                        format!("own source:{kind}<String,R>"),
                        String::new(),
                        "sink(source);println(\"after\");",
                        "val input=make();scan(input)",
                    ),
                };
                let source = format!(
                    "{common}fun scan({parameters}):Unit{{{formation}println(\"gap\");for((key,value) in source){{val guard=Guard();read(key,value)}};{after}}}\n\
                     fun entry():Unit{{{entry};println(\"done\")}}\n"
                );
                let mut golden = "factory\n".to_owned();
                if matches!(history, History::AliasRebound) {
                    golden.push_str("other-factory\n");
                }
                golden.push_str("gap\n");
                if populated {
                    golden.push_str("key\nold\nguard\n");
                }
                if matches!(history, History::OwnerTransfer) {
                    golden.push_str(if populated {
                        "sink\none\n"
                    } else {
                        "sink\nempty\n"
                    });
                    if populated {
                        golden.push_str("drop\nold\n");
                    }
                    golden.push_str("after\n");
                } else {
                    if matches!(history, History::AliasRebound) {
                        golden.push_str("other-one\n");
                    }
                    golden.push_str("after\n");
                    if populated {
                        golden.push_str("drop\nold\n");
                    }
                    if matches!(history, History::AliasRebound) {
                        golden.push_str("drop\nnew\n");
                    }
                }
                golden.push_str("done\n");
                cases.push(Case {
                    label: format!(
                        "formation-resource-{kind}-{history:?}-{}",
                        if populated { "populated" } else { "empty" }
                    ),
                    source,
                    golden,
                    history,
                    mutable,
                    populated,
                });
            }
        }
    }
    assert_eq!(cases.len(), 14);
    cases
}

#[test]
fn ordinary_resource_formations_reach_verified_llvm() {
    use super::{
        Report,
        fixture::{Failure, clean, failure},
        local_exits::formation,
        pipeline,
    };
    use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
    let mut outcomes = vec![];
    for case in sources() {
        for rooted in [false, true] {
            println!(
                "FORMATION RESOURCE INPUT label={} rooted={rooted} axes={:?} source={:?} golden={:?}",
                case.label,
                (case.history, case.mutable, case.populated),
                case.source,
                case.golden
            );
            let observed = (|| -> Result<(), Failure> {
                let mut sources = SourceMap::new();
                let id = sources
                    .add_source("provider.ko", &case.source)
                    .map_err(|e| failure("source", e))?;
                let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
                clean(&sources, "lex", lexed.diagnostics())?;
                let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
                clean(&sources, "parse", parsed.diagnostics())?;
                let mut report = Report::default();
                let mut identity = None;
                let result = pipeline::unit_input_observed(
                    &sources,
                    &parsed,
                    "provider.ko",
                    &mut report,
                    rooted,
                    |names, typed, owned, report| {
                        let original = formation::unit(&sources, &parsed, names, typed, owned);
                        report.query("formation runtime qualified original facts", &original);
                        identity = Some(original);
                    },
                );
                if let Ok((program, _)) = &result {
                    if let Some(Ok(input)) = &identity {
                        report.query(
                            "formation runtime actual exact owner/provider graph",
                            &formation::check(&sources, &parsed, program, input),
                        );
                    }
                    report.query(
                        "formation runtime actual SSA verifier",
                        &crate::ssa::verify::verify_program(program).map_err(|e| failure("SSA", e)),
                    );
                    report.query(
                        "formation runtime actual LLVM renderer",
                        &crate::llvm::render_verified_program(program)
                            .map(|_| ())
                            .map_err(|e| failure("LLVM", e)),
                    );
                }
                println!(
                    "FORMATION RESOURCE OBSERVED label={} rooted={rooted} report={report:#?} result={:?}",
                    case.label,
                    result.as_ref().map(|(_, entry)| entry)
                );
                if !report.passed() {
                    return Err(failure("formation runtime independent queries", report));
                }
                result?;
                Ok(())
            })();
            println!(
                "FORMATION RESOURCE CASE label={} rooted={rooted} result={observed:?}",
                case.label
            );
            outcomes.push(observed);
        }
    }
    assert_eq!(outcomes.len(), 28);
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
}
