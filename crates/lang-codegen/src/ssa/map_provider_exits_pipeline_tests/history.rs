//! Ordinary storage histories consumed by the actual Map provider, never instrumented IR.
use super::{
    Report,
    fixture::{clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
}

struct History {
    name: &'static str,
    body: String,
    expected: Vec<(&'static str, &'static str)>,
    discarded: usize,
}

fn histories() -> Vec<History> {
    let growth = [
        "k19", "k00", "k18", "k01", "k17", "k02", "k16", "k03", "k15", "k04", "k14", "k05", "k13",
        "k06", "k12", "k07", "k11", "k08", "k10", "k09",
    ];
    vec![
        History {name:"singleton", body:"source.put(\"only\".clone(), PAYLOAD(\"one\"))".into(), expected:vec![("only","one")], discarded:0},
        History {name:"overwrite",body:"source.put(\"z\".clone(), PAYLOAD(\"old\"))\nsource.put(\"a\".clone(), PAYLOAD(\"second\"))\nsource.put(\"z\".clone(), PAYLOAD(\"new\"))".into(),expected:vec![("z","new"),("a","second")],discarded:1},
        History {name:"reinsert",body:"source.put(\"z\".clone(), PAYLOAD(\"first\"))\nsource.put(\"a\".clone(), PAYLOAD(\"second\"))\nsource.remove(\"z\")\nsource.put(\"z\".clone(), PAYLOAD(\"last\"))".into(),expected:vec![("a","second"),("z","last")],discarded:1},
        History {name:"growth",body:growth.iter().map(|k|format!("source.put(\"{k}\".clone(), PAYLOAD(\"value\"))")).collect::<Vec<_>>().join("\n"),expected:growth.map(|k|(k,"value")).to_vec(),discarded:0},
        History {name:"tombstone",body:"source.put(\"a\".clone(), PAYLOAD(\"first\"))\nsource.put(\"q\".clone(), PAYLOAD(\"second\"))\nsource.put(\"A\".clone(), PAYLOAD(\"third\"))\nsource.put(\"tail\".clone(), PAYLOAD(\"fourth\"))\nsource.remove(\"a\")\nsource.remove(\"A\")".into(),expected:vec![("q","second"),("tail","fourth")],discarded:2},
    ]
}

/// Manual expected logical sequences are separate from the generated operations.
/// Resource remove yields nullable results and remains outside this finite slice.
pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for resource in [false, true] {
        for history in histories() {
            if resource && matches!(history.name, "reinsert" | "tombstone") {
                continue;
            }
            // Ordinary source construction; no typed/owned/SSA product is modified.
            let body = history
                .body
                .lines()
                .map(|line| {
                    if let Some((prefix, tail)) = line.split_once("PAYLOAD(") {
                        let argument = tail.strip_suffix("))").expect("fixed source put payload");
                        format!(
                            "{prefix}{})",
                            if resource {
                                "Resource()".into()
                            } else {
                                format!("{argument}.clone()")
                            }
                        )
                    } else {
                        line.into()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            for mode in ["own-map", "borrow-map", "borrow-mutable"] {
                if resource && mode == "borrow-map" {
                    continue;
                }
                let mutable = mode == "borrow-mutable";
                let container = if mutable { "MutableMap" } else { "Map" };
                let ty = if resource { "Resource" } else { "String" };
                let own = if mode == "own-map" { "own " } else { "" };
                let conversion = if mutable {
                    "return source"
                } else {
                    "val result = source.consume()\nreturn result"
                };
                let definition = if resource {
                    "class Resource { deinit() { println(\"resource-drop\") } }"
                } else {
                    ""
                };
                let read = if resource {
                    "println(\"resource-value\")"
                } else {
                    "println(value)"
                };
                let source = format!(
                    "{definition}\nfun read(key: String, value: {ty}): Unit {{ println(key); {read} }}\nfun factory(): {container}<String, {ty}> {{\nvar source = mutableMapOf<String, {ty}>()\n{body}\n{conversion}\n}}\nfun scan({own}source: {container}<String, {ty}>): Unit {{\nfor ((key, value) in source) {{ read(key, value) }}\nprintln(\"exhausted\")\n}}\nfun entry(): Unit {{\nprintln(\"before-factory\")\nval source = factory()\nprintln(\"before-scan\")\nscan(source)\nprintln(\"after-scan\")\n}}\n"
                );
                let mut golden = String::from("before-factory\n");
                if resource {
                    golden.push_str(&"resource-drop\n".repeat(history.discarded));
                }
                golden.push_str("before-scan\n");
                for (key, value) in &history.expected {
                    golden.push_str(key);
                    golden.push('\n');
                    golden.push_str(if resource { "resource-value" } else { value });
                    golden.push('\n');
                }
                golden.push_str("exhausted\n");
                if resource && !mutable {
                    golden.push_str(&"resource-drop\n".repeat(history.expected.len()));
                }
                golden.push_str("after-scan\n");
                if resource && mutable {
                    golden.push_str(&"resource-drop\n".repeat(history.expected.len()));
                }
                cases.push(Case {
                    label: format!(
                        "{}-{mode}{}",
                        history.name,
                        if resource { "-resource" } else { "" }
                    ),
                    source,
                    golden,
                });
            }
        }
    }
    assert_eq!(cases.len(), 21);
    cases
}

#[test]
fn storage_histories_reach_both_real_provider_pipelines() {
    let mut results = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let mut report = Report::default();
            let outcome = (|| {
                let mut sources = SourceMap::new();
                let id = sources
                    .add_source("provider.ko", &case.source)
                    .map_err(|e| failure("source", e))?;
                let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
                clean(&sources, "lex diagnostics", lexed.diagnostics())?;
                let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
                clean(&sources, "parse diagnostics", parsed.diagnostics())?;
                let (program, _) = if unit {
                    pipeline::unit_observed(
                        &sources,
                        &parsed,
                        "provider.ko",
                        &mut report,
                        |_, typed, owned, r| {
                            r.check(
                                "one actual unit Map plan",
                                typed.map_iterations().len() == 1
                                    && owned.map_iterations().len() == 1,
                                owned.map_iterations(),
                            )
                        },
                    )?
                } else {
                    pipeline::single_observed(
                        &sources,
                        &parsed,
                        &mut report,
                        |_, typed, owned, r| {
                            r.check(
                                "one actual single Map plan",
                                typed.map_iterations().len() == 1
                                    && owned.map_iterations().len() == 1,
                                owned.map_iterations(),
                            )
                        },
                    )?
                };
                super::super::verify::verify_program(&program)
                    .map_err(|e| failure("SSA verifier", e))?;
                crate::llvm::render_verified_program(&program)
                    .map_err(|e| failure("LLVM verifier", e))?;
                Ok::<(), super::fixture::Failure>(())
            })();
            println!(
                "HISTORY case={} unit={unit} source={:?} outcome={outcome:?} report={report:#?}",
                case.label, case.source
            );
            results.push((case.label.clone(), unit, report.passed() && outcome.is_ok()));
        }
    }
    assert_eq!(results.len(), 42);
    assert!(results.iter().all(|r| r.2), "{results:#?}");
}

/// CI sanitizer exporter consumes exactly these original Resource cases through both real entries.
pub(crate) fn sanitizer_ir(case: &Case, unit: bool) -> String {
    let mut sources = SourceMap::new();
    let id = sources.add_source("provider.ko", &case.source).unwrap();
    let lexed = lex(&sources, id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let mut report = Report::default();
    let (program, entry) = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |_, typed, owned, r| {
                r.check(
                    "one actual Resource unit provider",
                    typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1,
                    owned.map_iterations(),
                );
            },
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |_, typed, owned, r| {
            r.check(
                "one actual Resource single provider",
                typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1,
                owned.map_iterations(),
            );
        })
    }
    .expect("complete real provider pipeline");
    assert!(report.passed(), "{report:#?}");
    super::super::verify::verify_program(&program).expect("original Resource SSA");
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("original Resource LLVM with actual entry")
}
