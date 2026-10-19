//! Return operand delivery precedes existing complete provider/scope cleanup.
use super::super::super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use super::super::continuation::Case;
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) fn sources() -> Vec<Case> {
    let bases = super::sources();
    let labels = [
        "own-return-resource",
        "own-return-mixed",
        "own-return-null",
        "borrow-map-return-resource",
        "borrow-map-return-mixed",
        "borrow-map-return-null",
        "borrow-mutable-return-resource",
        "borrow-mutable-return-mixed",
        "borrow-mutable-return-null",
    ];
    let mut cases = Vec::new();
    for (index, label) in labels.into_iter().enumerate() {
        cases.push(make(&bases[index], label, index, 0));
    }
    for (label, kind) in [
        ("own-return-unit", 1),
        ("own-return-unit-prefix", 2),
        ("own-return-literal", 3),
        ("own-return-binary", 4),
        ("own-return-string", 5),
    ] {
        cases.push(make(&bases[0], label, 0, kind));
    }
    cases.push(make(&bases[12], "own-return-no-guard", 0, 0));
    cases
}
fn make(base: &Case, label: &'static str, index: usize, kind: usize) -> Case {
    let ty = if base.source.contains("value:Resource?") {
        "Resource?"
    } else {
        "Resource"
    };
    let start = base.source.find("fun scan(").unwrap();
    let end = base.source.find("fun entry(").unwrap();
    let (return_ty, returned, empty) = match kind {
        1 | 2 => ("Unit", "", ""),
        5 => ("String", "returned", "empty-result"),
        _ => ("Int", "seven", "zero-result"),
    };
    let operand = match kind {
        1 => "return".to_owned(),
        2 => "finish(key,value);return".to_owned(),
        3 => "return 7".to_owned(),
        4 => "return 3+4".to_owned(),
        5 => "return \"returned\"".to_owned(),
        _ => "return finish(key,value)".to_owned(),
    };
    let mut scan = base.source[start..end]
        .replacen("):Unit", &format!("):{return_ty}"), 1)
        .replace("break", &operand);
    if return_ty != "Unit" {
        let at = scan.rfind('}').unwrap();
        scan.insert_str(
            at,
            if kind == 5 {
                ";return \"empty-result\""
            } else {
                ";return 0"
            },
        );
    }
    let finish = if kind == 2 {
        format!(
            "fun finish(key:String,value:{ty}):Unit{{println(\"return-value\");read(key,value)}}\n"
        )
    } else {
        format!(
            "fun finish(key:String,value:{ty}):Int{{println(\"return-value\");read(key,value);return 7}}\n"
        )
    };
    let mut entry = base.source[end..].to_owned();
    if return_ty == "String" {
        entry = entry
            .replace("scan(populated)", "println(scan(populated))")
            .replace("scan(empty)", "println(scan(empty))");
    }
    if return_ty == "Int" {
        entry = entry
            .replace("scan(populated)", "checkSeven(scan(populated))")
            .replace("scan(empty)", "checkZero(scan(empty))");
    }
    let checks = "fun checkSeven(actual:Int):Unit{if(actual==7){println(\"seven\")}else{error(\"bad return\")}}\nfun checkZero(actual:Int):Unit{if(actual==0){println(\"zero-result\")}else{error(\"bad empty return\")}}\n";
    let source = format!(
        "{}{}{}{}{}",
        &base.source[..start],
        finish,
        checks,
        scan,
        entry
    );
    let payload = index % 3;
    let resources = if payload == 2 { 0 } else { 2 };
    let mutable = index >= 6;
    let value = match payload {
        0 => "value-borrow\n",
        1 => "some\n",
        _ => "none\n",
    };
    let mut golden = format!("nonempty\nz\n{value}");
    if kind == 0 || kind == 2 {
        golden.push_str(&format!("return-value\nz\n{value}"));
    }
    if base.source.contains("val local=Guard()") {
        golden.push_str("local-drop\n");
    }
    if base.owned {
        golden.push_str(&"resource-drop\n".repeat(resources));
    }
    if !returned.is_empty() {
        golden.push_str(returned);
        golden.push('\n');
    }
    golden.push_str("after-scan\n");
    if !base.owned {
        golden.push_str("post-size\n");
    }
    golden.push_str("empty\n");
    golden.push_str(if base.source.contains("if(source.size==0)") {
        "zero\nlater-borrow\ntail\n"
    } else {
        "exhausted\n"
    });
    if !empty.is_empty() {
        golden.push_str(empty);
        golden.push('\n');
    }
    golden.push_str("entry-end\n");
    if !base.owned {
        golden.push_str(&"resource-drop\n".repeat(resources + usize::from(mutable)));
    }
    Case {
        label,
        owned: base.owned,
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
    let input = super::super::identity::ast(&sources, &parsed, case.owned)?;
    let mut report = Report::default();
    let outcome = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                report.query(
                    "return original unit FE",
                    &super::fe::unit(&input, &parsed, names, typed, owned),
                )
            },
        )
    } else {
        pipeline::single_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                report.query(
                    "return original single FE",
                    &super::fe::single(&input, &parsed, names, typed, owned),
                )
            },
        )
    };
    println!(
        "RESOURCE RETURN case={} unit={unit} report={report:#?} outcome={:?}",
        case.label,
        outcome.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = outcome?;
    assert!(report.passed(), "{report:#?}");
    super::graph::check_return(&program, &input, &sources, &parsed, unit)?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA verifier", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM verifier", e))?;
    Ok(())
}
#[test]
fn resource_return_reaches_both_verified_pipelines() {
    let results = sources()
        .iter()
        .flat_map(|case| [false, true].map(|unit| (case.label, unit, observe(case, unit))))
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 30);
    assert!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
}
