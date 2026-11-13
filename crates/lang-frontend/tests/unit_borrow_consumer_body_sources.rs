//! Source-unit identity remains distinct even for structurally identical loop bodies.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        BorrowLoopExpression, capture_borrow_consumer_bodies, check_compilation_unit_ownership,
        with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
#[test]
fn ordinary_consumer_body_events_do_not_cross_source_units() {
    let mut sources = SourceMap::new();
    let a=sources.add_source("a.ko","fun observeA(item:String):Unit {}\nfun runA(source:String,flag:Boolean):Unit {while(flag){observeA(source)}}").unwrap();
    let b=sources.add_source("b.ko","fun observeB(item:String):Unit {}\nfun runB(source:String,flag:Boolean):Unit {while(flag){observeB(source)}}").unwrap();
    let pa = parse_file(&sources, &lex(&sources, a).unwrap()).unwrap();
    let pb = parse_file(&sources, &lex(&sources, b).unwrap()).unwrap();
    assert!(pa.diagnostics().is_empty());
    assert!(pb.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    // MemoryDocument intentionally supports one source only; this is a rooted multi-source proof.
    let inputs = [
        SourceUnitInput::new("root", "a.ko", a, &pa),
        SourceUnitInput::new("root", "b.ko", b, &pb),
    ];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let names = names.validate().unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let typed = typed.validate().unwrap();
    for enabled in [false, true] {
        let (checked, bodies) = capture_borrow_consumer_bodies(|| {
            with_loan_observation(enabled, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            })
        });
        let baseline = with_loan_observation(enabled, || {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
        });
        assert_eq!(checked, baseline);
        assert!(checked.diagnostics().is_empty());
        assert_eq!(bodies.len(), 2);
        assert!(bodies.iter().all(|o| o.validate()));
        assert_ne!(
            bodies[0].loop_span().source_id(),
            bodies[1].loop_span().source_id()
        );
        assert!(!bodies[0].same_loop_as(&bodies[1]));
        let events = bodies
            .iter()
            .map(|o| {
                assert_eq!(o.consumer_returns().len(), 1);
                let e = &o.consumer_returns()[0];
                assert!(o.owns_consumer_return(e));
                assert_eq!(e.acquired_count(), 1);
                assert_eq!(e.released_count(), 1);
                e
            })
            .collect::<Vec<_>>();
        assert!(!bodies[0].owns_consumer_return(events[1]));
        assert!(!bodies[1].owns_consumer_return(events[0]));
        let [
            BorrowLoopExpression::Unit(ca),
            BorrowLoopExpression::Unit(cb),
        ] = [events[0].call(), events[1].call()]
        else {
            panic!("unit-qualified calls")
        };
        assert_eq!(
            ca.expression(),
            cb.expression(),
            "same AST position deliberately tests source ownership"
        );
        assert_ne!(ca.source_unit(), cb.source_unit());
        assert_ne!(ca, cb);
        for (body, event) in bodies.iter().zip(events) {
            let BorrowLoopExpression::Unit(call) = event.call() else {
                panic!()
            };
            let [
                (
                    BorrowLoopExpression::Unit(argument),
                    lang_frontend::ownership_checking::BorrowLoopBinding::Unit(binding),
                ),
            ] = event.arguments()
            else {
                panic!("one qualified operand")
            };
            assert_eq!(call.source_unit(), argument.source_unit());
            assert_eq!(call.source_unit(), binding.source_unit());
            assert_eq!(body.body_span().source_id(), body.loop_span().source_id());
        }
    }
}
