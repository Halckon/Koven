//! Real local formation plus future uses: preserve every public FE/lowering observation.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod facts;
mod graph;
mod sources {
    //! Fixed source matrix; each case executes a populated and a truly empty local producer.
    pub(crate) struct Case {
        pub label: String,
        pub source: String,
        pub golden: String,
        pub mutable: bool,
        pub binding: usize,
        pub exit: usize,
        pub form: usize,
    }

    pub(crate) fn sources() -> Vec<Case> {
        let mut cases = Vec::new();
        for mutable in [false, true] {
            for binding in 0..3 {
                for exit in 0..3 {
                    for form in 0..3 {
                        cases.push(source(mutable, binding, exit, form));
                    }
                }
            }
        }
        assert_eq!(cases.len(), 54);
        assert_eq!(
            cases
                .iter()
                .map(|c| &c.label)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            54
        );
        cases
    }

    fn source(mutable: bool, binding: usize, exit: usize, form: usize) -> Case {
        let kind = if mutable { "MutableMap" } else { "Map" };
        let factory = if mutable { "mutableMapOf" } else { "mapOf" };
        let conversion = if mutable {
            "return source"
        } else {
            "val result=source.consume();return result"
        };
        let transfer = ["", "continue", "break"][exit];
        let continuation = match form {
            0 => {
                "readMap(source);println(\"between\");readMap(source);val count=source.size;println(\"after-last-use\");if(count==0){println(\"zero\")}else{println(\"nonzero\")}"
            }
            1 => {
                "if(source.size==0){println(\"zero\")}else{println(\"nonzero\")};println(\"between\");readMap(source);println(\"after-last-use\")"
            }
            _ => {
                "println(\"second\");for((key,value) in source){val local=Guard();read(key,value)};readMap(source);println(\"after-last-use\")"
            }
        };
        let mut scanners = String::new();
        for empty in [false, true] {
            let scan = if empty { "scanEmpty" } else { "scan" };
            let producer = if empty { "makeEmpty" } else { "make" };
            let declaration = match binding {
                0 => format!("val source={producer}()"),
                1 => format!("var source={producer}()"),
                _ => format!("val original={producer}();val source=original"),
            };
            scanners.push_str(&format!("fun {scan}():Unit{{{declaration};println(\"formed\");for((key,value) in source){{val local=Guard();read(key,value);{transfer}}};{continuation};println(\"tail\")}}\n"));
        }
        let source = format!(
            r#"class Guard{{deinit(){{println("guard-drop")}}}}
    fun read(key:String,value:String):Unit{{println(key);println(value)}}
    fun readMap(source:{kind}<String,String>):Unit{{if(source.size==0){{println("borrow-zero")}}else{{println("borrow-nonzero")}}}}
    fun make():{kind}<String,String>{{var source=mutableMapOf<String,String>();source.put("z".clone(),"first".clone());source.put("a".clone(),"second".clone());{conversion}}}
    fun makeEmpty():{kind}<String,String>{{return {factory}<String,String>()}}
    {scanners}
    fun entry():Unit{{println("nonempty");scan();println("empty");scanEmpty();println("entry-end")}}
    "#
        );
        let mut golden = String::new();
        for empty in [false, true] {
            golden.push_str(if empty {
                "empty\nformed\n"
            } else {
                "nonempty\nformed\n"
            });
            if !empty {
                golden.push_str("z\nfirst\nguard-drop\n");
                if exit != 2 {
                    golden.push_str("a\nsecond\nguard-drop\n");
                }
            }
            let borrowed = if empty {
                "borrow-zero\n"
            } else {
                "borrow-nonzero\n"
            };
            let size = if empty { "zero\n" } else { "nonzero\n" };
            match form {
                0 => {
                    golden.push_str(borrowed);
                    golden.push_str("between\n");
                    golden.push_str(borrowed);
                    golden.push_str("after-last-use\n");
                    golden.push_str(size);
                }
                1 => {
                    golden.push_str(size);
                    golden.push_str("between\n");
                    golden.push_str(borrowed);
                    golden.push_str("after-last-use\n");
                }
                _ => {
                    golden.push_str("second\n");
                    if !empty {
                        golden.push_str("z\nfirst\nguard-drop\na\nsecond\nguard-drop\n");
                    }
                    golden.push_str(borrowed);
                    golden.push_str("after-last-use\n");
                }
            }
            golden.push_str("tail\n");
        }
        golden.push_str("entry-end\n");
        Case {
            label: format!(
                "local-future-{kind}-{}-{}-{}",
                ["val", "var", "moved-alias"][binding],
                ["normal", "continue", "break"][exit],
                ["calls-size", "cfg-borrow", "repeat-borrow"][form]
            ),
            source,
            golden,
            mutable,
            binding,
            exit,
            form,
        }
    }
}
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) use sources::{Case, sources};

fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "LOCAL FUTURE INPUT case={} unit={unit} source={:?} golden={:?}",
        case.label,
        case.source,
        case.golden
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
    // Queries record failures in Report; neither path turns a failed identity
    // query into an artificial lowering failure or prevents ordinary lowering.
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                facts::unit(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "local future original unit descriptor count",
                    typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 },
                    typed.map_iterations(),
                );
                diagnostic!(
                    "LOCAL FUTURE UNIT FACTS case={} formed={:#?} plans={:#?} drops={:#?} deliveries={:#?} loans={:#?}",
                    case.label,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.value_deliveries(),
                    owned.loans()
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
                report.check(
                    "local future original unit descriptor count",
                    typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 },
                    typed.map_iterations(),
                );
                diagnostic!(
                    "LOCAL FUTURE UNIT FACTS case={} formed={:#?} plans={:#?} drops={:#?} deliveries={:#?} loans={:#?}",
                    case.label,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.value_deliveries(),
                    owned.loans()
                );
            },
        )
    };
    diagnostic!(
        "LOCAL FUTURE LOWER case={} unit={unit} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    report.query(
        "Unit complete temporary source acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    let identity = graph::check(case, &sources, &parsed, &program);
    report.query("local future independent SSA identities", &identity);
    let verified =
        crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA verifier", e));
    report.query("local future actual SSA verifier", &verified);
    let llvm =
        crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM verifier", e));
    report.query("local future actual LLVM verifier", &llvm);
    diagnostic!(
        "LOCAL FUTURE FINAL case={} unit={unit} report={report:#?}",
        case.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("local future observed stage failures", report))
    }
}

#[test]
fn local_sources_follow_actual_future_uses() {
    let mut observations = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            diagnostic!(
                "LOCAL FUTURE CASE {} unit={unit} result={result:?}",
                case.label
            );
            observations.push((case.label.clone(), unit, result));
        }
    }
    assert_eq!(observations.len(), 108);
    assert_diagnostic!(
        observations.iter().all(|(_, _, result)| result.is_ok()),
        "{observations:#?}"
    );
}
