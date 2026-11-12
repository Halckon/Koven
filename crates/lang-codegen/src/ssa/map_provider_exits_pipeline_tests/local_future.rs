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

pub(in crate::ssa) mod history {
//! Ordinary storage histories consumed by the actual Map provider, never instrumented IR.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
use super::super::{
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
                    pipeline::memory_document_observed(
                        &sources,
                        &parsed,
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
                };
                super::super::super::verify::verify_program(&program)
                    .map_err(|e| failure("SSA verifier", e))?;
                crate::llvm::render_verified_program(&program)
                    .map_err(|e| failure("LLVM verifier", e))?;
                Ok::<(), super::super::fixture::Failure>(())
            })();
            diagnostic!(
                "HISTORY case={} unit={unit} source={:?} outcome={outcome:?} report={report:#?}",
                case.label,
                case.source
            );
            results.push((case.label.clone(), unit, report.passed() && outcome.is_ok()));
        }
    }
    assert_eq!(results.len(), 42);
    assert_diagnostic!(results.iter().all(|r| r.2), "{results:#?}");
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
        pipeline::memory_document_observed(&sources, &parsed, &mut report, |_, typed, owned, r| {
            r.check(
                "one actual Resource unit provider",
                typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1,
                owned.map_iterations(),
            );
        })
    }
    .expect("complete real provider pipeline");
    assert_diagnostic!(report.passed(), "{report:#?}");
    super::super::super::verify::verify_program(&program).expect("original Resource SSA");
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("original Resource LLVM with actual entry")
}
}

pub(in crate::ssa) mod ci_contracts {
//! CI-only: corrupt one dimension of real lowered Map SSA. Never execute locally.
use super::super::ci_baseline::baseline;
use crate::ssa::{
    model::*,
    verify::{VerifyErrorKind, VerifyLocation, verify_program},
};
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};

fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(id) = entity else {
        panic!("actual loan result")
    };
    id
}
fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(id) = entity else {
        panic!("actual value result")
    };
    id
}

// Preserve IDs, block membership, origins and terminators. Only the requested operation is added.
fn insert_before(
    f: &mut Function,
    before: InstructionId,
    op: Operation,
    results: Vec<EntityType>,
) -> Vec<EntityId> {
    let original = f.instruction(before).unwrap().clone();
    let block = original.block;
    let position = f.blocks[block.index()]
        .instructions
        .iter()
        .position(|id| *id == before)
        .unwrap();
    let terminator = f.blocks[block.index()].terminator.take();
    let (id, entities) = f
        .append_instruction(block, op, results, original.origin)
        .unwrap();
    let instructions = &mut f.blocks[block.index()].instructions;
    assert_eq!(instructions.pop(), Some(id));
    instructions.insert(position, id);
    f.blocks[block.index()].terminator = terminator;
    entities
}

#[test]
fn map_real_lowered_ssa_private_damage_has_precise_rejections() {
    const TYPE_ERROR: &str = "operand and result types do not match the operation contract";
    for unit in [false, true] {
        let mut valid = baseline(unit);
        let functions = valid
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .filter(|f| {
                f.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. }))
                    .count()
                    == 1
            })
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            panic!("one actual provider function")
        };
        let function = (*function).clone();
        let fid = function.id();
        let guards = function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::MapIterationHasNext { .. }))
            .collect::<Vec<_>>();
        let guard = guards[0];
        let key = function
            .instructions
            .iter()
            .find(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. }))
            .unwrap();
        let val = function
            .instructions
            .iter()
            .find(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. }))
            .unwrap();
        let next = function
            .instructions
            .iter()
            .find(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
            .unwrap();
        let Operation::MapIterationKeyLoan { source, .. } = key.operation else {
            unreachable!()
        };
        let boolean = match function.entity(guard.results[0]).unwrap().ty {
            EntityType::Value(t) => t,
            _ => unreachable!(),
        };
        for fault in 0..8 {
            let f = valid
                .module_mut(fid.module())
                .unwrap()
                .function_mut(fid)
                .unwrap();
            *f = function.clone();
            let (at, expected) = match fault {
                0 | 1 => {
                    let Operation::MapIterationHasNext { cursor, .. } = guard.operation else {
                        unreachable!()
                    };
                    let ty = if fault == 0 {
                        EntityType::Value(boolean)
                    } else {
                        f.entity(EntityId::Value(cursor)).unwrap().ty
                    };
                    let constant = if fault == 0 {
                        ScalarConstant::Boolean(true)
                    } else {
                        ScalarConstant::Integer(0)
                    };
                    let replacement = value(
                        insert_before(f, guard.id, Operation::Constant(constant), vec![ty])[0],
                    );
                    let Operation::MapIterationHasNext { cursor, .. } =
                        &mut f.instructions[guard.id.index()].operation
                    else {
                        unreachable!()
                    };
                    *cursor = replacement;
                    (
                        guard.id,
                        VerifyErrorKind::OperationContract {
                            reason: if fault == 0 {
                                TYPE_ERROR
                            } else {
                                "Map cursor must originate from First/Next and explicit CFG transport"
                            },
                        },
                    )
                }
                2 => {
                    // A real, dominating active String loan cannot serve as a Map provider source.
                    let wrong = loan(key.results[0]);
                    let Operation::MapIterationValueLoan { source, .. } =
                        &mut f.instructions[val.id.index()].operation
                    else {
                        unreachable!()
                    };
                    *source = wrong;
                    (
                        val.id,
                        VerifyErrorKind::OperationContract { reason: TYPE_ERROR },
                    )
                }
                3 => {
                    insert_before(f, key.id, Operation::BorrowEnd { loan: source }, vec![]);
                    (key.id, VerifyErrorKind::LoanInactive { loan: source })
                }
                4 | 5 => {
                    let component = loan(if fault == 4 {
                        key.results[0]
                    } else {
                        val.results[0]
                    });
                    let end = f
                        .instructions
                        .iter_mut()
                        .find(|i| i.operation == Operation::BorrowEnd { loan: component })
                        .expect("actual component End before Next");
                    end.operation = Operation::PrintString { value: component };
                    let Operation::MapIterationNext { source: parent, .. } = next.operation else {
                        unreachable!()
                    };
                    (
                        next.id,
                        VerifyErrorKind::LoanDependencyActive {
                            parent,
                            dependent: component,
                        },
                    )
                }
                6 => {
                    let component = loan(key.results[0]);
                    let EntityType::Loan { target, .. } = &mut f.loans[component.index()].ty else {
                        unreachable!()
                    };
                    *target = boolean;
                    (
                        key.id,
                        VerifyErrorKind::OperationContract { reason: TYPE_ERROR },
                    )
                }
                7 => {
                    // A complete, current-block same-type reborrow is a distinct lease.
                    let ty = f.entity(EntityId::Loan(source)).unwrap().ty;
                    let caller = loan(
                        insert_before(f, key.id, Operation::SharedReborrow { source }, vec![ty])[0],
                    );
                    insert_before(f, next.id, Operation::BorrowEnd { loan: caller }, vec![]);
                    verify_program(&valid).expect(
                        "extra balanced lease is valid before the single source substitution",
                    );
                    let f = valid
                        .module_mut(fid.module())
                        .unwrap()
                        .function_mut(fid)
                        .unwrap();
                    let Operation::MapIterationKeyLoan { source, .. } =
                        &mut f.instructions[key.id.index()].operation
                    else {
                        unreachable!()
                    };
                    *source = caller;
                    (
                        key.id,
                        VerifyErrorKind::OperationContract {
                            reason: "Map cursor and source must belong to the same live provider lease",
                        },
                    )
                }
                _ => unreachable!(),
            };
            let errors = verify_program(&valid).expect_err("private damage must not reach LLVM");
            let origin = function.instruction(at).unwrap().origin.clone();
            assert_diagnostic!(
                errors
                    .errors
                    .iter()
                    .any(|e| e.location == VerifyLocation::Instruction(at)
                        && e.kind == expected
                        && e.origin == Some(origin.clone())),
                "unit={unit} fault={fault} expected={expected:?} at={at:?} original={origin:?}: {errors:#?}"
            );
            diagnostic!(
                "SPEC0331 CI MAP SSA unit={unit} fault={fault} exact={expected:?} at={at:?} origin={origin:?}"
            );
        }
    }
}
}
