//! Direct Resource Map exits, built from ordinary sources without synthetic facts.
mod fe;
mod graph;
pub(in super::super) mod returning;
use super::super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use super::continuation::Case;
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for (label, kind, payload, later, guard) in [
        ("own-break-resource-later", 0, 0, true, true),
        ("own-break-mixed-later", 0, 1, true, true),
        ("own-break-null-later", 0, 2, true, true),
        ("borrow-map-break-resource", 1, 0, true, true),
        ("borrow-map-break-mixed", 1, 1, true, true),
        ("borrow-map-break-null", 1, 2, true, true),
        ("borrow-mutable-break-resource", 2, 0, true, true),
        ("borrow-mutable-break-mixed", 2, 1, true, true),
        ("borrow-mutable-break-null", 2, 2, true, true),
        ("own-break-resource-last", 0, 0, false, true),
        ("own-break-mixed-last", 0, 1, false, true),
        ("own-break-null-last", 0, 2, false, true),
        ("own-break-no-guard", 0, 0, false, false),
    ] {
        cases.push(source(label, kind, payload, later, guard));
    }
    cases
}
fn source(label: &'static str, kind: usize, payload: usize, later: bool, guard: bool) -> Case {
    let nullable = payload != 0;
    let ty = if nullable { "Resource?" } else { "Resource" };
    let container = if kind == 2 { "MutableMap" } else { "Map" };
    let own = if kind == 0 { "own " } else { "" };
    let (puts, size, resources) = match payload {
        0 => (
            "source.put(\"z\".clone(),Resource());source.put(\"a\".clone(),Resource());",
            2,
            2,
        ),
        1 => (
            "source.put(\"z\".clone(),Resource());source.put(\"n\".clone(),null);source.put(\"a\".clone(),Resource());",
            3,
            2,
        ),
        _ => (
            "source.put(\"z\".clone(),null);source.put(\"a\".clone(),null);",
            2,
            0,
        ),
    };
    let conversion = if kind == 2 {
        "return source"
    } else {
        "val result=source.consume();return result"
    };
    let read = if nullable {
        "val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}"
    } else {
        "println(\"value-borrow\")"
    };
    let local = if guard { "val local=Guard();" } else { "" };
    let tail = if later {
        "if(source.size==0){println(\"zero\")}else{println(\"many\")};readMap(source);println(\"tail\")"
    } else {
        "println(\"exhausted\")"
    };
    let mutation = if kind == 2 {
        "populated.put(\"later\".clone(),Resource());"
    } else {
        ""
    };
    let post_size = size + usize::from(kind == 2);
    let source = format!(
        r#"
class Resource {{ deinit() {{ println("resource-drop") }} }}
class Guard {{ deinit() {{ println("local-drop") }} }}
fun read(key:String,value:{ty}):Unit {{println(key);{read}}}
fun readMap(source:{container}<String,{ty}>):Unit {{println("later-borrow")}}
fun makeMap():{container}<String,{ty}> {{var source=mutableMapOf<String,{ty}>();{puts}{conversion}}}
fun scan({own}source:{container}<String,{ty}>):Unit {{for((key,value) in source){{{local}read(key,value);break}};{tail}}}
fun entry():Unit {{println("nonempty");var populated=makeMap();scan(populated);println("after-scan");{mutation}
{post_check}
println("empty");val empty={factory}<String,{ty}>();scan(empty);println("entry-end")}}
"#,
        factory = if kind == 2 { "mutableMapOf" } else { "mapOf" },
        post_check = if kind == 0 {
            String::new()
        } else {
            format!(
                "if(populated.size=={post_size}){{println(\"post-size\")}}else{{error(\"bad size\")}};"
            )
        }
    );
    let mut golden = String::from("nonempty\nz\n");
    golden.push_str(if payload == 0 {
        "value-borrow\n"
    } else if payload == 1 {
        "some\n"
    } else {
        "none\n"
    });
    if guard {
        golden.push_str("local-drop\n");
    }
    golden.push_str(if later {
        "many\nlater-borrow\ntail\n"
    } else {
        "exhausted\n"
    });
    if kind == 0 {
        golden.push_str(&"resource-drop\n".repeat(resources));
    }
    golden.push_str("after-scan\n");
    if kind != 0 {
        golden.push_str("post-size\n");
    }
    golden.push_str("empty\n");
    golden.push_str(if later {
        "zero\nlater-borrow\ntail\n"
    } else {
        "exhausted\n"
    });
    golden.push_str("entry-end\n");
    if kind != 0 {
        golden.push_str(&"resource-drop\n".repeat(resources + usize::from(kind == 2)));
    }
    Case {
        label,
        owned: kind == 0,
        source,
        golden,
    }
}
fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", &case.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let input = super::identity::ast(&sources, &parsed, case.owned)?;
    let mut report = Report::default();
    let outcome = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |names, typed, owned, report| {
            report.query(
                "terminal original unit FE",
                &fe::unit(&input, &parsed, names, typed, owned),
            )
        },
    );
    println!(
        "RESOURCE TERMINAL case={} rooted={unit} report={report:#?} outcome={:?}",
        case.label,
        outcome.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = outcome?;
    assert!(report.passed(), "{report:#?}");
    graph::check(&program, &input, &sources, &parsed, true)?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA verifier", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM verifier", e))?;
    Ok(())
}
#[test]
fn resource_break_reaches_both_verified_pipelines() {
    let results = sources()
        .iter()
        .flat_map(|case| [false, true].map(|unit| (case.label, unit, observe(case, unit))))
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 26);
    assert!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
}
