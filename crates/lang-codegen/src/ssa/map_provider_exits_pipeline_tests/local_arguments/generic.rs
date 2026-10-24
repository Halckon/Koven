//! Concrete Map return with an unused generic parameter exercises the ordinary generic ABI.
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
use super::temporary_arguments::{ast, facts};
#[path = "generic_graph.rs"]
mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms, temporary_arguments,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
use temporary_arguments::Case;
#[path = "wrapper.rs"]
pub(super) mod wrapper;
fn prerequisite_cases() -> Vec<Case> {
    temporary_arguments::sources()
        .into_iter()
        .map(|mut c| {
            let change = |text: String| {
                let declaration = "fun produce(input:Int)";
                assert_eq!(text.matches(declaration).count(), 1);
                let text = text.replace(declaration, "fun <T> produce(input:Int)");
                assert_eq!(text.matches("produce(argument())").count(), 1);
                text.replace("produce(argument())", "produce<Int>(argument())")
            };
            c.label = c
                .label
                .replacen("temporary-argument", "generic-local-argument", 1);
            c.source = change(c.source);
            c.control = change(c.control);
            c
        })
        .collect()
}
// The original direct-body inputs remain a hard-success prerequisite for 0291.
// Actual stable-v2 observations: 9/12 success; all three Single Map shapes stop
// inside generic `result.consume()`, including no-For. No rejection oracle or
// per-kind/per-entry filter registers them as this bounded positive slice.
pub(crate) fn prerequisite_sources() -> Vec<(String, String, String)> {
    tuples(prerequisite_cases())
}
fn cases() -> Vec<Case> {
    temporary_arguments::sources().into_iter().map(|mut c| {
        let kind=if c.mutable {"MutableMap"} else {"Map"};
        let change=|text:String| {
            let declaration="fun produce(input:Int)";
            assert_eq!(text.matches(declaration).count(),1);
            let renamed=text.replace(declaration,"fun nonGenericImpl(input:Int)");
            let wrapper=format!("fun <T> produce(input:Int):{kind}<String,String>{{return nonGenericImpl(input)}}\n");
            wrapper+&renamed.replace("produce(argument())","produce<Int>(argument())")
        };
        c.label=c.label.replacen("temporary-argument","generic-wrapper-argument",1);
        c.source=change(c.source);c.control=change(c.control);c
    }).collect()
}
pub(crate) fn native_sources() -> Vec<(String, String, String)> {
    tuples(cases())
}
fn tuples(cases: Vec<Case>) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for c in cases {
        out.push((format!("{}-no-for", c.label), c.control, c.control_golden));
        let old = "for((key,value) in produce<Int>(argument()))";
        assert_eq!(c.source.matches(old).count(), 1);
        out.push((
            format!("{}-named", c.label),
            c.source.replace(
                old,
                "val source=produce<Int>(argument());for((key,value) in source)",
            ),
            c.golden.clone(),
        ));
        out.push((format!("{}-direct", c.label), c.source, c.golden));
    }
    assert_eq!(out.len(), 6);
    out
}
fn observe(c: &Case, unit: bool, control: bool) -> Result<(), Failure> {
    let text = if control { &c.control } else { &c.source };
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |n, t, o, r| {
                source_forms::facts::unit(&sources, &parsed, n, t, o, r);
                r.query(
                    "generic wrapper original qualified forwarding Call",
                    &wrapper::unit(&sources, &parsed, n, t, o),
                );
                r.query(
                    "generic original ordinary argument and actual overload proof",
                    &facts::unit(c, &sources, &parsed, n, t, o, control),
                );
                let input = ast::input(&sources, &parsed, control).unwrap();
                let unit = t
                    .calls()
                    .iter()
                    .find(|call| call.expression().expression() == input.produce)
                    .unwrap()
                    .expression()
                    .source_unit();
                let call = t
                    .call(lang_frontend::type_checking::UnitExpressionId::new(
                        unit,
                        input.produce,
                    ))
                    .unwrap();
                r.check("generic actual concrete Int type argument",matches!(call.instance().type_arguments(),[ty]if t.types().get(*ty)==Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(lang_frontend::type_checking::BuiltinType::Int))),call);
            },
        )
    } else {
        pipeline::memory_document_observed(&sources, &parsed, &mut report, |n, t, o, r| {
            source_forms::facts::unit(&sources, &parsed, n, t, o, r);
            r.query(
                "generic wrapper original qualified forwarding Call",
                &wrapper::unit(&sources, &parsed, n, t, o),
            );
            r.query(
                "generic original ordinary argument and actual overload proof",
                &facts::unit(c, &sources, &parsed, n, t, o, control),
            );
            let input = ast::input(&sources, &parsed, control).unwrap();
            let unit = t
                .calls()
                .iter()
                .find(|call| call.expression().expression() == input.produce)
                .unwrap()
                .expression()
                .source_unit();
            let call = t
                .call(lang_frontend::type_checking::UnitExpressionId::new(
                    unit,
                    input.produce,
                ))
                .unwrap();
            r.check("generic actual concrete Int type argument",matches!(call.instance().type_arguments(),[ty]if t.types().get(*ty)==Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(lang_frontend::type_checking::BuiltinType::Int))),call);
        })
    };
    println!(
        "LOCAL GENERIC LOWER label={} unit={unit} control={control} source={text:?} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    report.query(
        "Unit complete temporary source acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    report.query(
        "generic same existing exact argument/current owner SSA proof",
        &graph::check(c, &sources, &parsed, &program, control),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if report.passed() {
        Ok(())
    } else {
        Err(failure("generic exact FE/SSA identities", report))
    }
}
#[test]
fn concrete_return_generic_wrapper_keeps_all_three_shapes() {
    // Preserve the entire original direct-body corpus for the separate 0291
    // hard-success target, including its nine successful v2 observations.
    // This exports identity only; it neither executes nor accepts the three U's.
    let prerequisites =
        crate::ssa::map_provider_exits_pipeline_tests::local_generic_argument_prerequisite_sources(
        );
    assert_eq!(prerequisites.len(), 6);
    let labels = prerequisites
        .iter()
        .map(|(label, _, _)| label)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(labels.len(), 6);
    let originals = [
        (
            "generic-local-argument-Map-borrow-int-overload-no-for",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-no-for.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-no-for.stdout"
            ),
        ),
        (
            "generic-local-argument-Map-borrow-int-overload-named",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-named.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-named.stdout"
            ),
        ),
        (
            "generic-local-argument-Map-borrow-int-overload-direct",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-direct.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-direct.stdout"
            ),
        ),
        (
            "generic-local-argument-MutableMap-borrow-int-overload-no-for",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-no-for.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-no-for.stdout"
            ),
        ),
        (
            "generic-local-argument-MutableMap-borrow-int-overload-named",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-named.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-named.stdout"
            ),
        ),
        (
            "generic-local-argument-MutableMap-borrow-int-overload-direct",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-direct.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-direct.stdout"
            ),
        ),
    ];
    for ((label, source, golden), (original_label, original_source, original_golden)) in
        prerequisites.into_iter().zip(originals)
    {
        assert_eq!(label, original_label);
        assert_eq!(
            source, original_source,
            "{label}: preserve original prerequisite source"
        );
        assert_eq!(
            golden, original_golden,
            "{label}: preserve original prerequisite golden"
        );
        assert!(
            source.contains("fun <T> produce(input:Int)")
                && source.contains("produce<Int>(argument())")
        );
        assert!(!golden.is_empty());
        println!("LOCAL GENERIC PREREQUISITE label={label} source={source:?} golden={golden:?}");
    }
    let mut outcomes = Vec::new();
    for c in cases() {
        for unit in [false, true] {
            for control in [true, false] {
                let r = observe(&c, unit, control);
                println!(
                    "LOCAL GENERIC CASE label={} unit={unit} control={control} result={r:?}",
                    c.label
                );
                outcomes.push(r);
            }
            let old = "for((key,value) in produce<Int>(argument()))";
            let local = super::Case {
                label: format!("{}-named", c.label),
                source: c.source.replace(
                    old,
                    "val source=produce<Int>(argument());for((key,value) in source)",
                ),
                golden: c.golden.clone(),
                mutable: c.mutable,
                resource: false,
                unit_only: false,
                direct: false,
                route: false,
                delegated: false,
            };
            let r = super::observe(&local, unit);
            println!(
                "LOCAL GENERIC CASE label={} unit={unit} result={r:?}",
                local.label
            );
            outcomes.push(r);
        }
    }
    assert_eq!(outcomes.len(), 12);
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
}
