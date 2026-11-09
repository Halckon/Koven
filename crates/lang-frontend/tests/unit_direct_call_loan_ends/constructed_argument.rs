//! Actual direct-call retirement of checked constructed temporary operands.
use super::*;

#[test]
fn checked_construction_temporaries_publish_actual_ends_but_factory_results_do_not() {
    for (operand, observed_construction) in
        [("Node()", true), ("((Node()))", true), ("factory()", false)]
    {
        let observed = observe(&format!(
            "class Node {{}}\nfun factory(): Node = Node()\nfun read(first: Node, own n: Int): Unit {{}}\nfun run(flag: Boolean): Unit = read({operand}, if (flag) {{ return }} else {{ 0 }})",
        ));
        assert_units_admitted(&observed);
        for unit in &observed.units {
            let [loan] = unit.loans() else {
                panic!("one actual Borrow operand: {operand}");
            };
            let UnitLoanTarget::Temporary(owner) = loan.target() else {
                panic!("the operand owns its temporary");
            };
            assert_eq!(*owner, loan.argument());
            let points: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Temporary(*owner))
                .map(|drop| drop.point())
                .collect();
            assert_eq!(points.len(), 2, "{operand}: {points:?}");
            assert!(matches!(points[0], UnitDropPoint::ControlTransfer(_)));
            assert_eq!(points[1], UnitDropPoint::CallReturn(loan.call()));
            let ends = unit.direct_call_loan_ends();
            if observed_construction {
                assert_eq!(
                    ends.iter().map(|end| end.point()).collect::<Vec<_>>(),
                    points
                );
                assert!(
                    ends.iter()
                        .all(|end| end.loan() == loan && end.preceding_drops() == 0)
                );
            } else {
                assert!(
                    ends.is_empty(),
                    "a factory call has no direct construction proof"
                );
            }
        }
    }
}

#[test]
fn constructed_borrow_aborts_without_unwinding_and_ends_on_the_normal_path() {
    let observed = observe(
        "class Node {}\nfun read(first: Node, own n: Int): Unit {}\nfun run(flag: Boolean): Unit = read(Node(), if (flag) { error(\"stop\") } else { 0 })",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let mut constructor_loans = unit
            .loans()
            .iter()
            .filter(|loan| observed.sources.slice(loan.begin_span()) == Ok("Node()"));
        let loan = constructor_loans.next().unwrap_or_else(|| {
            panic!(
                "construction is borrowed before the argument branches: {:?}",
                unit.loans()
            )
        });
        assert!(constructor_loans.next().is_none());
        assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
        let [end] = unit.direct_call_loan_ends() else {
            panic!("only the normal arm retires the loan");
        };
        assert_eq!(end.loan(), loan);
        let normal = UnitDropPoint::CallReturn(loan.call());
        assert_eq!(end.point(), normal);
        assert_eq!(end.preceding_drops(), 0);
        let points: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(loan.argument()))
            .map(|drop| drop.point())
            .collect();
        assert_eq!(points, [normal]);
    }
}

#[test]
fn cross_source_construction_prefix_follows_named_operand_order() {
    let mut sources = SourceMap::new();
    let callee_source = sources
        .add_source(
            "read.ko",
            "package lib\nclass Node(val value: String)\nfun read(own n: Int, item: Node): Unit {}",
        )
        .unwrap();
    let caller_source = sources
        .add_source(
            "run.ko",
            r#"package app
import lib.Node
import lib.read
fun formed(flag: Boolean): Unit = read(item = ((Node("kept"))), n = if (flag) { return } else { 0 })
fun unformed(flag: Boolean): Unit = read(n = if (flag) { return } else { 0 }, item = ((Node("kept"))))"#,
        )
        .unwrap();
    let callee = parse_file(&sources, &lex(&sources, callee_source).unwrap()).unwrap();
    let caller = parse_file(&sources, &lex(&sources, caller_source).unwrap()).unwrap();
    assert!(callee.diagnostics().is_empty());
    assert!(caller.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let ordered = [
        SourceUnitInput::new("root", "lib/read.ko", callee_source, &callee),
        SourceUnitInput::new("root", "app/run.ko", caller_source, &caller),
    ];
    for inputs in [ordered, [ordered[1], ordered[0]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let source_unit = |source_id| {
            index
                .source_units()
                .iter()
                .find(|unit| unit.source_id() == source_id)
                .unwrap()
                .id()
        };
        let caller_unit = source_unit(caller_source);
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        let typed = raw.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        let owned = owned.ownership();
        let caller_span = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), caller_unit);
            caller
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span()
        };
        let mut loans: Vec<_> = owned
            .loans()
            .iter()
            .filter(|loan| sources.slice(loan.begin_span()) == Ok("((Node(\"kept\")))"))
            .collect();
        loans.sort_by_key(|loan| loan.begin_span().start());
        let [formed, unformed] = loans.as_slice() else {
            panic!("one actual temporary loan per call");
        };
        assert_ne!(formed.argument(), unformed.argument());
        let mut points_by_call = [Vec::new(), Vec::new()];
        for end in owned.direct_call_loan_ends() {
            let loan = end.loan();
            let call_span = caller_span(loan.call());
            let call_text = sources.slice(call_span).unwrap();
            let argument_span = caller_span(loan.argument());
            assert_eq!(sources.slice(argument_span), Ok("((Node(\"kept\")))"));
            assert!(
                call_span.start() < argument_span.start() && argument_span.end() < call_span.end()
            );
            assert_eq!(loan.begin_span(), argument_span);
            assert_eq!(loan.end_span(), call_span);
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(end.preceding_drops(), 0);
            assert_eq!(end.preceding_call_loan_ends(), 0);
            let parameter = loan.parameter_span().unwrap();
            assert_eq!(parameter.source_id(), callee_source);
            assert_eq!(sources.slice(parameter), Ok("item: Node"));
            let UnitCallTarget::Declaration(target) =
                typed.types().call(loan.call()).unwrap().target()
            else {
                panic!("selected cross-source callee")
            };
            let declaration = index
                .declarations()
                .iter()
                .find(|declaration| declaration.id() == target)
                .expect("selected declaration");
            assert_eq!(declaration.source_unit(), source_unit(callee_source));
            assert_eq!(declaration.name(), "read");
            let contract = owned
                .call_argument_contracts_for(loan.call())
                .find(|contract| contract.argument() == loan.argument())
                .expect("selected Borrow operand contract");
            assert_eq!(contract.parameter_index(), 1);
            assert_eq!(contract.parameter_span(), Some(parameter));
            let is_formed = call_text.starts_with("read(item");
            assert!(is_formed || call_text.starts_with("read(n"));
            let call_index = usize::from(!is_formed);
            assert_eq!(loan, loans[call_index]);
            let call = caller
                .ast()
                .expressions()
                .get(loan.call().expression())
                .unwrap();
            let Expression::Call { arguments, .. } = call.payload() else {
                panic!("original call expression");
            };
            assert_eq!(arguments[call_index].value, loan.argument().expression());
            match end.point() {
                UnitDropPoint::ControlTransfer(exit) => {
                    assert!(is_formed, "Return precedes construction on the other call");
                    let span = caller_span(exit);
                    assert_eq!(sources.slice(span), Ok("return"));
                    assert!(call_span.start() < span.start() && span.end() < call_span.end());
                }
                point => assert_eq!(point, UnitDropPoint::CallReturn(loan.call())),
            }
            points_by_call[call_index].push(end.point());
        }
        assert!(matches!(
            points_by_call[0].as_slice(),
            [
                UnitDropPoint::ControlTransfer(_),
                UnitDropPoint::CallReturn(_)
            ]
        ));
        assert!(matches!(
            points_by_call[1].as_slice(),
            [UnitDropPoint::CallReturn(_)]
        ));
        for (loan, expected) in loans.into_iter().zip(&points_by_call) {
            let actual: Vec<_> = owned
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Temporary(loan.argument()))
                .map(|drop| {
                    assert_eq!(drop.value_origin(), caller_span(loan.argument()));
                    drop.point()
                })
                .collect();
            assert_eq!(&actual, expected);
        }
        assert_eq!(
            owned
                .drops()
                .iter()
                .filter(|drop| { sources.slice(drop.value_origin()) == Ok("((Node(\"kept\")))") })
                .count(),
            3,
            "no cleanup for the construction skipped by Return"
        );
    }
}
