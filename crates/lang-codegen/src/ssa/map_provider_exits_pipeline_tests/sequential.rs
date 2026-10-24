//! Ordinary Borrow parameter reuse across complete, sequential provider lifetimes.
mod identity;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    slots: [usize; 2],
    parameters: usize,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for order in ["distinct", "repeat", "reverse"] {
            cases.push(case(mutable, order, "normal"));
        }
    }
    for shape in ["empty", "break", "continue", "int-key"] {
        cases.push(case(false, "distinct", shape));
    }
    assert_eq!(cases.len(), 10);
    cases
}

fn case(mutable: bool, order: &str, shape: &str) -> Case {
    let container = if mutable { "MutableMap" } else { "Map" };
    let repeat = order == "repeat";
    let reverse = order == "reverse";
    let integer = shape == "int-key";
    let slots = if repeat {
        [0, 0]
    } else if reverse {
        [1, 0]
    } else {
        [0, 1]
    };
    let names = slots.map(|slot| if slot == 0 { "first" } else { "second" });
    let second_type = if integer {
        "Map<Int,String>".to_owned()
    } else {
        format!("{container}<String,String>")
    };
    let parameters = if repeat {
        String::new()
    } else {
        format!(", second: {second_type}")
    };
    let first_body = if shape == "empty" {
        String::new()
    } else {
        "source.put(\"a0\".clone(),\"A0\".clone())\nsource.put(\"a1\".clone(),\"A1\".clone())"
            .into()
    };
    let conversion = if mutable {
        "return source"
    } else {
        "val result=source.consume(); return result"
    };
    let second_body = if integer {
        "var source=mutableMapOf<Int,String>(); source.put(7,\"B0\".clone()); source.put(9,\"B1\".clone()); val result=source.consume(); return result".into()
    } else {
        format!(
            "var source=mutableMapOf<String,String>(); source.put(\"b0\".clone(),\"B0\".clone()); source.put(\"b1\".clone(),\"B1\".clone()); {conversion}"
        )
    };
    let body = |index| {
        if index == 0 && matches!(shape, "break" | "continue") {
            format!("val local=Guard(); read(key,value); {shape}")
        } else if index == 1 && integer {
            "println(value)".into()
        } else {
            "read(key,value)".into()
        }
    };
    let second_local = if repeat {
        String::new()
    } else {
        "val b=makeB();".into()
    };
    let arguments = if repeat { "a" } else { "a,b" };
    let size = if mutable {
        3
    } else if shape == "empty" {
        0
    } else {
        2
    };
    let mutation = if mutable {
        "a.put(\"later\".clone(),\"tail\".clone());"
    } else {
        ""
    };
    let after = format!(
        "{mutation} if (a.size == {size}) {{ println(\"size-a\") }} else {{ error(\"bad a size\") }}"
    );
    let second_size = if repeat {
        ""
    } else {
        "if (b.size == 2) { println(\"size-b\") } else { error(\"bad b size\") }"
    };
    let source = format!(
        r#"
class Guard {{ deinit() {{ println("local-drop") }} }}
fun read(key:String,value:String):Unit {{ println(key); println(value) }}
fun makeA():{container}<String,String> {{ var source=mutableMapOf<String,String>(); {first_body}; {conversion} }}
fun makeB():{second_type} {{ {second_body} }}
fun scan(first:{container}<String,String>{parameters}):Unit {{
 for ((key,value) in {}) {{ {} }}
 println("between")
 for ((key,value) in {}) {{ {} }}
 println("finished")
}}
fun entry():Unit {{ var a=makeA(); {second_local} scan({arguments}); println("after-scan"); {after}; {second_size} }}
"#,
        names[0],
        body(0),
        names[1],
        body(1)
    );
    // The full observable oracle is specified from the source contract, never emitted output.
    let a = if shape == "empty" {
        ""
    } else {
        "a0\nA0\na1\nA1\n"
    };
    let b = "b0\nB0\nb1\nB1\n";
    let first = match shape {
        "break" => "a0\nA0\nlocal-drop\n",
        "continue" => "a0\nA0\nlocal-drop\na1\nA1\nlocal-drop\n",
        _ if reverse => b,
        _ => a,
    };
    let second = if integer {
        "B0\nB1\n"
    } else if repeat || reverse {
        a
    } else {
        b
    };
    let golden = format!(
        "{first}between\n{second}finished\nafter-scan\nsize-a\n{}",
        if repeat { "" } else { "size-b\n" }
    );
    Case {
        label: format!(
            "{}-{order}-{shape}",
            if mutable { "mutable" } else { "map" }
        ),
        source,
        golden,
        slots,
        parameters: if repeat { 1 } else { 2 },
    }
}

fn observe(case: &Case, unit: bool, report: &mut Report) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", &case.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex diagnostics", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse diagnostics", parsed.diagnostics())?;
    let (program, entry) = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        report,
        unit,
        |names, typed, owned, r| {
            r.query(
                "unit two original Borrow roots and complete cleanup",
                &identity::unit(case, &sources, &parsed, names, typed, owned),
            );
        },
    )?;
    report.query(
        "all actual incoming Borrow identities and provider cleanup",
        &identity::ssa(case, &sources, &parsed, &program, true),
    );
    super::super::verify::verify_program(&program).map_err(|e| failure("SSA verifier", e))?;
    let ir =
        crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM verify", e))?;
    report.check("SSA and LLVM verified", !ir.is_empty(), entry);
    Ok(())
}

#[test]
fn sequential_borrowed_maps_keep_current_roots_and_cleanup_in_both_entries() {
    let mut results = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let mut report = Report::default();
            let outcome = observe(&case, unit, &mut report);
            println!(
                "SEQUENTIAL case={} rooted={unit} outcome={outcome:?} report={report:#?}",
                case.label
            );
            results.push((case.label.clone(), unit, report.passed() && outcome.is_ok()));
        }
    }
    assert!(
        results.len() == 20 && results.iter().all(|r| r.2),
        "{results:#?}"
    );
}
