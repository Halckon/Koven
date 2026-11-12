//! Newly rebuilt finite Map<String, Resource> normal-exhaustion pipelines.
//! Resource values remain container-owned; the loop creates Shared component loans.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
pub(super) mod continuation;
pub(super) mod continued {
    //! Resource continue retains provider/source and lexical owner across every backedge.
    use super::continuation::{Case, sources as continuations};
    use crate::test_support::diagnostics::assert_diagnostic;
    pub(crate) fn sources() -> Vec<Case> {
        let originals = continuations();
        let mut cases = vec![Case {
            label: "original-continue",
            owned: true,
            source: include_str!(
                "../../../../tests/fixtures/map-provider-resource-0331/boundary-continue.ko"
            )
            .to_owned(),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-resource-0331/map-resource.stdout"
            )
            .to_owned(),
        }];
        for (index, guard, label) in [
            (1, false, "continue-last-use"),
            (4, false, "continue-later-use"),
            (1, true, "guard-continue-last-use"),
            (4, true, "guard-continue-later-use"),
        ] {
            let original = &originals[index];
            let body = if guard {
                "val local = Guard(); read(key, value); continue"
            } else {
                "read(key, value); continue"
            };
            let mut source = original.source.replace("read(key, value)", body);
            let golden = if guard {
                source = "class Guard { deinit() { println(\"local-drop\") } }\n".to_owned() + &source;
                original
                    .golden
                    .replace("value-borrow\n", "value-borrow\nlocal-drop\n")
            } else {
                original.golden.clone()
            };
            cases.push(Case {
                label,
                owned: true,
                source,
                golden,
            });
        }
        for mutable in [false, true] {
            let container = if mutable { "MutableMap" } else { "Map" };
            let conversion = if mutable {
                "return source"
            } else {
                "val result=source.consume(); return result"
            };
            let mutation = if mutable {
                "populated.put(\"later\".clone(),Resource());"
            } else {
                ""
            };
            let size = if mutable { 3 } else { 2 };
            let source = format!(
                r#"
    class Resource {{ deinit() {{ println("resource-drop") }} }}
    fun read(key:String,value:Resource):Unit {{ println(key); println("value-borrow") }}
    fun makeMap():{container}<String,Resource> {{ var source=mutableMapOf<String,Resource>(); source.put("z".clone(),Resource()); source.put("a".clone(),Resource()); {conversion} }}
    fun scan(source:{container}<String,Resource>):Unit {{ for((key,value) in source){{read(key,value);continue}};println("exhausted") }}
    fun entry():Unit {{ println("nonempty"); var populated=makeMap(); scan(populated); println("after-scan"); {mutation}
     if(populated.size=={size}){{println("post-size")}}else{{error("bad size")}};
     println("empty");val empty={factory}<String,Resource>();scan(empty);println("entry-end") }}
    "#,
                factory = if mutable { "mutableMapOf" } else { "mapOf" }
            );
            let drops = "resource-drop\n".repeat(size as usize);
            cases.push(Case{label:if mutable {"borrow-mutable-continue"}else{"borrow-map-continue"},owned:false,source,golden:format!("nonempty\nz\nvalue-borrow\na\nvalue-borrow\nexhausted\nafter-scan\npost-size\nempty\nexhausted\nentry-end\n{drops}")});
        }
        cases
    }
    #[test]
    fn resource_continue_reaches_both_verified_pipelines() {
        let results = sources()
            .iter()
            .flat_map(|case| {
                [false, true].map(|unit| (case.label, unit, super::continuation::observe(case, unit)))
            })
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 14);
        assert_diagnostic!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
    }
}
mod identity;
pub(super) mod nullable {
    //! Existing nullable Map value representation, borrowed as a nullable handle without extraction.
    use super::super::fixture::{Failure, failure, need};
    use super::continuation::Case;
    use crate::ssa::model::*;
    use crate::test_support::diagnostics::assert_diagnostic;
    use lang_frontend::{
        parser::{Item, NameMarker, ParsedFile},
        source::SourceMap,
    };

    fn nullable(text: &str) -> String {
        text.replace("String, Resource>", "String, Resource?>")
            .replace("String,Resource>", "String,Resource?>")
            .replace("value: Resource)", "value: Resource?)")
            .replace("value:Resource)", "value:Resource?)")
    }
    fn presence(text: &str) -> String {
        text.replace(
            "println(\"value-borrow\")",
            "val absent = value == null; if(absent){println(\"none\")}else{println(\"some\")}",
        )
    }
    pub(crate) fn sources() -> Vec<Case> {
        let base =
            include_str!("../../../tests/fixtures/map-provider-resource-0331/boundary-nullable.ko");
        let normal =
            include_str!("../../../tests/fixtures/map-provider-resource-0331/map-resource.stdout");
        let mut cases = vec![Case {
            label: "original-nullable",
            owned: true,
            source: base.to_owned(),
            golden: normal.to_owned(),
        }];
        cases.push(Case{label:"nullable-mixed",owned:true,source:presence(&base.replace("  source.put(\"a\".clone(), Resource())","  source.put(\"n\".clone(), null)\n  source.put(\"a\".clone(), Resource())")),golden:"nonempty\nz\nsome\nn\nnone\na\nsome\nexhausted\nresource-drop\nresource-drop\nempty\nexhausted\nentry-end\n".to_owned()});
        cases.push(Case {
            label: "nullable-all-null",
            owned: true,
            source: presence(&base.replace("Resource())", "null)")),
            golden: "nonempty\nz\nnone\na\nnone\nexhausted\nempty\nexhausted\nentry-end\n".to_owned(),
        });
        let continued = super::continued::sources();
        for (index, label) in [
            (2, "nullable-own-continue"),
            (5, "nullable-borrow-map-continue"),
            (6, "nullable-borrow-mutable-continue"),
        ] {
            let case = &continued[index];
            cases.push(Case {
                label,
                owned: case.owned,
                source: presence(&nullable(&case.source)),
                golden: case.golden.replace("value-borrow\n", "some\n"),
            });
        }
        let own = &cases[3];
        cases.push(Case {
            label: "nullable-null-continue",
            owned: true,
            source: own.source.replace("Resource())", "null)"),
            golden: own
                .golden
                .replace("some\n", "none\n")
                .replace("resource-drop\n", ""),
        });
        assert_eq!(cases.len(), 7);
        cases
    }

    fn representation(
        program: &Program,
        input: &super::identity::Input,
        checked_presence: bool,
        sources: &SourceMap,
        parsed: &ParsedFile,
    ) -> Result<(), Failure> {
        use super::super::query_inputs::one;
        let module = one(program.modules.iter(), "one nullable program module")?;
        let scan = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(input.scan)),
            "actual nullable scan",
        )?;
        let value = one(
            scan.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
            "actual nullable value loan",
        )?;
        let [EntityId::Loan(loan)] = value.results.as_slice() else {
            return Err(failure("nullable shared value", value));
        };
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = scan.entity(EntityId::Loan(*loan)).unwrap().ty
        else {
            return Err(failure("shared nullable loan type", value));
        };
        let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(target) else {
            return Err(failure("existing nullable handle ABI", target));
        };
        need(
            matches!(
                module.type_kind(*inner),
                Some(SsaTypeKind::HeapOwner { .. })
            ) && module.deinits.contains_key(inner),
            "nullable inner retains actual resource drop glue",
            inner,
        )?;
        let class = one(
            parsed.roots().iter().filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                let Item::Classifier(class) = n.payload() else {
                    return None;
                };
                matches!(class.name, NameMarker::Present(name) if sources.slice(name) == Ok("Resource"))
                    .then_some(class)
            }),
            "original nullable Resource class",
        )?;
        let body = class
            .body
            .as_ref()
            .ok_or_else(|| failure("Resource body", class))?;
        let member = one(
            body.members.iter().filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                matches!(n.payload(), Item::Deinit { .. }).then_some(n)
            }),
            "original Resource deinit",
        )?;
        let actual = module
            .deinits
            .get(inner)
            .and_then(|id| module.function(*id))
            .ok_or_else(|| failure("actual nullable Resource drop glue", inner))?;
        need(
            actual.origin == Origin::Source(member.span()),
            "nullable inner uses original Resource deinit identity",
            actual,
        )?;
        let read = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(input.read)),
            "actual nullable reader",
        )?;
        let parameters = &read.block(read.entry_block().unwrap()).unwrap().parameters;
        need(
            parameters.len() == 2
                && read.entity(parameters[1]).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    },
            "reader uses exact nullable component type",
            read,
        )?;
        if checked_presence {
            let checks = read
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::NullableLoanIsNull { .. }))
                .collect::<Vec<_>>();
            need(
                matches!(checks.as_slice(),[check] if matches!(check.operation,Operation::NullableLoanIsNull{source} if EntityId::Loan(source)==parameters[1])),
                "presence reads original borrowed nullable, no owner copy",
                &checks,
            )?;
            need(
                !read.instructions.iter().any(|i| {
                    matches!(
                        i.operation,
                        Operation::NullableTake { .. }
                            | Operation::NullableWrap { .. }
                    )
                }) && !read.instructions.iter().any(|i|matches!(i.operation,Operation::Drop{owner} if matches!(read.entity(EntityId::Value(owner)).unwrap().ty,EntityType::Value(t) if t==target || t==*inner))),
                "presence test does not acquire or drop Resource owner",
                read,
            )?;
        }
        Ok(())
    }

    #[test]
    fn nullable_resource_components_reach_both_verified_pipelines() {
        let results = sources()
            .iter()
            .flat_map(|case| {
                [false, true].map(|unit| {
                    (
                        case.label,
                        unit,
                        super::continuation::observe_with(
                            case,
                            unit,
                            |program, input, sources, parsed| {
                                representation(
                                    program,
                                    input,
                                    case.label != "original-nullable",
                                    sources,
                                    parsed,
                                )
                            },
                        ),
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 14);
        assert_diagnostic!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
    }
}
pub(super) mod terminated;

use super::{
    Report,
    fixture::{Failure, clean, failure, verify_failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) fn sources() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            "map-resource",
            include_str!("../../../tests/fixtures/map-provider-resource-0331/map-resource.ko"),
            include_str!("../../../tests/fixtures/map-provider-resource-0331/map-resource.stdout"),
        ),
        (
            "mutable-map-resource",
            include_str!(
                "../../../tests/fixtures/map-provider-resource-0331/mutable-map-resource.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-resource-0331/mutable-map-resource.stdout"
            ),
        ),
    ]
}

fn observe(label: &str, text: &str, unit: bool, report: &mut Report) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex diagnostics", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse diagnostics", parsed.diagnostics())?;
    let input = identity::ast(&sources, &parsed, label == "map-resource")?;
    let (program, entry) = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        report,
        unit,
        |names, typed, owned, report| {
            report.query(
                "resource original unit FE obligations",
                &identity::unit(&input, &parsed, names, typed, owned),
            );
            report.check(
                "unit one actual Resource Map provider",
                typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1,
                (typed.map_iterations(), owned.map_iterations()),
            );
        },
    )?;
    report.query(
        "resource independent SSA incoming/current lexical drop",
        &identity::ssa(&program, &input, true),
    );
    super::super::verify::verify_program(&program).map_err(|e| verify_failure(&sources, e))?;
    report.check("SSA verifier", true, entry);
    let llvm = crate::llvm::render_verified_program(&program)
        .map_err(|e| failure("LLVM render/verify", e))?;
    report.check("LLVM render/verify", true, llvm.len());
    Ok(())
}

#[test]
fn resource_map_normal_sources_reach_verified_llvm_in_both_entries() {
    let mut results = Vec::new();
    for (label, text, _) in sources() {
        for unit in [false, true] {
            let mut report = Report::default();
            let outcome = observe(label, text, unit, &mut report);
            diagnostic!(
                "SPEC0331 RESOURCE case={label} rooted={unit} outcome={outcome:?} report={report:#?}"
            );
            results.push((label, unit, report.passed() && outcome.is_ok()));
        }
    }
    assert_diagnostic!(
        results.len() == 4 && results.iter().all(|(_, _, passed)| *passed),
        "all four original Resource Map pipelines must pass: {results:?}"
    );
}
