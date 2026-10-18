//! Resource continue retains provider/source and lexical owner across every backedge.
use super::continuation::{Case, sources as continuations};
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
    assert!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
}

#[test]
fn resource_break_return_neighbors_keep_original_for_boundary() {
    use super::super::{
        Report,
        fixture::{clean, failure},
        pipeline,
    };
    use lang_frontend::{
        lexer::lex,
        parser::{Statement, parse_file},
        source::SourceMap,
    };
    let cases = sources();
    let mut observed = Vec::new();
    for index in [0, 5] {
        for returning in [false, true] {
            let original = &cases[index].source;
            let text = if returning {
                let start = original.find("fun scan(").unwrap();
                let end = original.find("fun entry(").unwrap();
                let mut scan = original[start..end]
                    .replacen("Unit", "Int", 1)
                    .replace("continue", "return result(key,value)");
                let last = scan.rfind('}').unwrap();
                scan.insert_str(last, "\nreturn 0\n");
                format!(
                    "{}{}fun result(key:String,value:Resource):Int{{read(key,value);return 7}}\n{}",
                    &original[..start],
                    scan,
                    &original[end..]
                )
            } else {
                original.replace("continue", "break")
            };
            for unit in [false, true] {
                let result = (|| {
                    let mut sources = SourceMap::new();
                    let id = sources
                        .add_source("provider.ko", &text)
                        .map_err(|e| failure("source", e))?;
                    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
                    clean(&sources, "lex", lexed.diagnostics())?;
                    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
                    clean(&sources, "parse", parsed.diagnostics())?;
                    let spans = parsed
                        .ast()
                        .statements()
                        .iter()
                        .filter_map(|(_, n)| {
                            matches!(n.payload(), Statement::For { .. }).then_some(n.span())
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(spans.len(), 1);
                    let mut report = Report::default();
                    let actual = if unit {
                        pipeline::unit_observed(
                            &sources,
                            &parsed,
                            "provider.ko",
                            &mut report,
                            |_, _, _, _| {},
                        )
                    } else {
                        pipeline::single_observed(&sources, &parsed, &mut report, |_, _, _, _| {})
                    };
                    let expected = format!(
                        "(LoweringError {{ kind: UnsupportedNode, span: Some({:?}) }},",
                        spans[0]
                    );
                    println!(
                        "RESOURCE EXIT BOUNDARY shared={} return={returning} unit={unit} report={report:#?} actual={:?}",
                        !cases[index].owned,
                        actual.as_ref().map(|(_, entry)| entry)
                    );
                    Ok::<_, super::super::fixture::Failure>(
                        report.passed()
                            && matches!(actual,Err(ref e) if e.stage=="lower" && e.original.starts_with(&expected)),
                    )
                })();
                observed.push((index, returning, unit, result));
            }
        }
    }
    assert_eq!(observed.len(), 8);
    assert!(
        observed.iter().all(|(_, _, _, r)| matches!(r, Ok(true))),
        "{observed:#?}"
    );
}
