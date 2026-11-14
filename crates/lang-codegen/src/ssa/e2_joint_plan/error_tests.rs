//! Real-source stage diagnostics; no emission or fault injection.
use super::{JointPlan, PlanRejection};
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::two_source_experiment::analyze_two_source_experiment,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
#[test]
fn checked_plan_preserves_selection_order_and_preflight_error() {
    let base = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);consume(b)};remaining=remaining-1}}\nfun main():Unit {val rounds=3;val choose=true;run(\"left\",\"right\",rounds,choose)}";
    for rejected in [false, true] {
        let text = if rejected {
            format!(
                "{base}\nfun ordinary(source:String):Unit {{borrow val b=view(source);val number=1+1;consume(b)}}"
            )
        } else {
            base.to_owned()
        };
        let mut sources = SourceMap::new();
        let id = sources.add_source("plan.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "plan.ko", id, &parsed)];
        let (env, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let foreign_typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let experiment = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .unwrap();
        let foreign =
            analyze_two_source_experiment(&sources, &inputs, &names, &types, &foreign_typed)
                .unwrap()
                .unwrap();
        let handoff = experiment
            .handoff(&sources, &inputs, &names, &types, &typed)
            .unwrap();
        let facts: Vec<_> = experiment.facts().iter().collect();
        let entries: Vec<_> = experiment.entries(&typed).unwrap().iter().collect();
        assert_eq!((facts.len(), entries.len()), (1, 1));
        let classify = |facts: &[&_], entries: &[&_]| {
            let checked = JointPlan::build_checked(&handoff, &experiment, facts, entries);
            assert_eq!(
                checked.is_ok(),
                JointPlan::build(&handoff, &experiment, facts, entries).is_some()
            );
            checked.err()
        };
        assert_eq!(
            classify(&facts, &[]),
            Some(PlanRejection::SelectionRejected)
        );
        assert_eq!(
            classify(&[facts[0], facts[0]], &[entries[0], entries[0]]),
            Some(PlanRejection::SelectionRejected)
        );
        assert_eq!(
            JointPlan::build_checked(&handoff, &foreign, &facts, &[]).err(),
            Some(PlanRejection::AnalysisIdentityMismatch)
        );
        assert!(JointPlan::build(&handoff, &foreign, &facts, &[]).is_none());
        if rejected {
            let selected = handoff.for_facts(&facts).unwrap();
            let error = crate::ssa::unit_lower::preflight::validate(
                &[&parsed],
                &names,
                typed.types(),
                &selected,
            )
            .unwrap_err();
            assert_eq!(
                error.kind,
                crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode
            );
            assert!(error.span.is_some());
            assert_eq!(
                classify(&facts, &entries),
                Some(PlanRejection::UnitPreflightRejected(error))
            );
        } else {
            assert_eq!(classify(&facts, &entries), None);
            let foreign_entries: Vec<_> = foreign.entries(&foreign_typed).unwrap().iter().collect();
            assert_eq!(
                classify(&facts, &foreign_entries),
                Some(PlanRejection::ClosedPlanRejected)
            );
        }
    }
}
