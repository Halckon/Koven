//! Independent live-owner neighbor: two Resource providers reuse the same formed root.
use super::*;

fn sources() -> Vec<Case> {
    let mut cases = super::sources()
        .into_iter()
        .filter(|case| !case.mutable && case.form == 0 && case.exit == 0)
        .map(|mut case| {
            assert_eq!(case.source.matches("};println(\"after\")").count(), 2);
            case.label.push_str("-repeat");
            case.source = case.source.replace(
                "};println(\"after\")",
                "};println(\"second\");for((nextKey,nextValue) in source){val nextLocal=Guard();read(nextKey,nextValue)};println(\"after\")",
            );
            let body = if case.nullable {
                "z\nsome\nguard-drop\nn\nnone\nguard-drop\na\nsome\nguard-drop\n"
            } else {
                "z\nvalue\nguard-drop\na\nvalue\nguard-drop\n"
            };
            case.golden = case.golden.replacen("after\n", &format!("second\n{body}after\n"), 1)
                .replace("empty\nformed\nafter\n", "empty\nformed\nsecond\nafter\n");
            case
        })
        .collect::<Vec<_>>();
    cases.sort_by_key(|case| case.nullable);
    assert_eq!(cases.len(), 2);
    cases
}

fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    println!(
        "G2 RESOURCE REPEAT INPUT case={} unit={unit} source={:?} golden={:?}",
        case.label, case.source, case.golden
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
    let result = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                report.check(
                    "Resource repeat four original qualified providers",
                    typed.map_iterations().len() == 4 && owned.map_iterations().len() == 4,
                    (typed.map_iterations(), owned.map_iterations()),
                );
                println!(
                    "G2 RESOURCE REPEAT UNIT case={} plans={:#?} drops={:#?}",
                    case.label,
                    owned.map_iterations(),
                    owned.drops()
                );
            },
        )
    } else {
        pipeline::single_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::single(&sources, &parsed, names, typed, owned, report);
                report.check(
                    "Resource repeat four original single providers",
                    typed.map_iterations().len() == 4 && owned.map_iterations().len() == 4,
                    (typed.map_iterations(), owned.map_iterations()),
                );
                println!(
                    "G2 RESOURCE REPEAT SINGLE case={} plans={:#?} drops={:#?} conditions={:#?}",
                    case.label,
                    owned.map_iterations(),
                    owned.drops(),
                    owned.cleanup_conditions()
                );
            },
        )
    };
    println!(
        "G2 RESOURCE REPEAT REPORT {} unit={unit} report={report:#?}",
        case.label
    );
    let (program, _) = result?;
    if !report.passed() {
        return Err(failure("original repeat Resource FE", report));
    }
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}

#[test]
fn repeated_resource_local_keeps_live_original_owner() {
    let mut outcomes = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            println!(
                "G2 RESOURCE REPEAT CASE {} unit={unit} result={result:?}",
                case.label
            );
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert!(
        outcomes.len() == 4 && outcomes.iter().all(|(_, _, r)| r.is_ok()),
        "{outcomes:#?}"
    );
}
