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
            let class_unwinds = unit.class_result_argument_unwinds();
            if observed_construction {
                assert!(class_unwinds.is_empty());
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
                let [unwind] = class_unwinds else {
                    panic!("only the return edge abandons the class-result argument");
                };
                assert_eq!(unwind.loan(), loan);
                assert_eq!(unwind.point(), points[0]);
                assert_eq!(unwind.preceding_drops(), 0);
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

#[test]
fn singleton_prefix_publishes_only_the_completed_class_argument_unwind() {
    for (arguments, body, expected_end) in [
        ("factory(), break", "loop { CALL }", Some("break")),
        (
            "(Node()), continue",
            "while (flag) { CALL }",
            Some("continue"),
        ),
        ("factory(), return", "CALL", Some("return")),
        ("factory(), 0", "CALL", None),
        ("factory(), if (flag) { 1 } else { 0 }", "CALL", None),
        ("item = factory(), stop = return", "CALL", Some("return")),
        ("stop = return, item = factory()", "CALL", None),
    ] {
        let call = format!("Gate.read({arguments})");
        let body = body.replace("CALL", &call);
        let observed = observe(&format!(
            "class Node {{}}\nfun factory(): Node = Node()\nobject Gate {{ fun read(item: Node, stop: Int): Unit {{}} }}\nfun run(flag: Boolean): Unit {{ {body} }}"
        ));
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert!(unit.direct_call_loan_ends().is_empty());
            let ends = unit.class_result_argument_unwinds();
            let Some(keyword) = expected_end else {
                assert!(ends.is_empty(), "{arguments}: {ends:?}");
                continue;
            };
            let [end] = ends else {
                panic!("{arguments}: expected exactly one retired temporary loan");
            };
            let loan = end.loan();
            let UnitDropPoint::ControlTransfer(transfer) = end.point() else {
                panic!("the call prefix exits before dispatch");
            };
            let span = observed
                .parsed
                .ast()
                .expressions()
                .get(transfer.expression())
                .unwrap()
                .span();
            assert_eq!(observed.sources.slice(span), Ok(keyword));
            let expressions = observed.parsed.ast().expressions();
            let call_span = expressions.get(loan.call().expression()).unwrap().span();
            assert_eq!(observed.sources.slice(call_span), Ok(call.as_str()));
            let operand = if arguments.starts_with("(Node())") {
                "(Node())"
            } else {
                "factory()"
            };
            assert_eq!(observed.sources.slice(loan.begin_span()), Ok(operand));
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            assert_eq!(unit.call_may_dispatch(loan.call()), Some(false));
            assert_eq!(end.preceding_drops(), 0);
            assert_eq!(end.preceding_call_loan_ends(), 0);
            assert_eq!(
                unit.drops()
                    .iter()
                    .filter(|drop| drop.target() == UnitDropTarget::Temporary(loan.argument()))
                    .map(|drop| drop.point())
                    .collect::<Vec<_>>(),
                [end.point()],
                "{arguments}: the same exit drops the completed owner exactly once"
            );
        }
    }

    for (source, has_temporary) in [
        (
            "class Node {}\nfun factory(): Node = Node()\nobject Gate { fun read(item: Node, stop: Int): Unit {} }\nfun receiver(): Gate = Gate\nfun run(): Unit { receiver().read(factory(), return) }",
            true,
        ),
        (
            "class Node(val n: Int)\nobject Gate { fun read(item: Node, stop: Int): Unit {} }\nfun run(): Unit { Gate.read(Node(return), 0) }",
            false,
        ),
    ] {
        let observed = observe(source);
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert!(unit.direct_call_loan_ends().is_empty());
            assert!(unit.class_result_argument_unwinds().is_empty(), "{source}");
            assert_eq!(
                unit.loans().iter().any(|loan| {
                    loan.kind() == LoanKind::Shared
                        && loan.target() == &UnitLoanTarget::Temporary(loan.argument())
                }),
                has_temporary,
                "{source}"
            );
        }
    }
}

#[test]
fn singleton_prefix_retires_only_the_completed_rc_argument() {
    for (family, operand, exit, expected) in [
        ("Rc<Leaf>", "make()", "break", true),
        ("Rc<Leaf>", "Rc(Leaf(8))", "continue", true),
        ("Rc<Leaf>", "owner.share()", "return", true),
        ("Rc<Leaf>?", "maybe(flag)", "break", true),
        ("Rc<Leaf>?", "make()", "return", true),
        ("Rc<Leaf>", "nested.value", "return", false),
        ("Rc<Leaf>", "Rc(Leaf(return))", "0", false),
        ("Rc<Leaf>", "make()", "if (flag) 1 else 0", false),
        ("Rc<Leaf>", "make()", "0", false),
    ] {
        let call = format!("Gate.inspect({operand}, {exit})");
        let body = match exit {
            "break" | "continue" => format!("loop {{ {call} }}"),
            _ => call.clone(),
        };
        let text = format!(
            "class Leaf(val n: Int)\nfun make(): Rc<Leaf> = Rc(Leaf(7))\nfun maybe(flag: Boolean): Rc<Leaf>? = if (flag) make() else null\nobject Gate {{ fun inspect(value: {family}, stop: Int): Unit {{}} }}\nfun run(owner: Rc<Leaf>, nested: Rc<Rc<Leaf>>, flag: Boolean): Unit {{ {body} }}"
        );
        let observed = observe(&text);
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert!(unit.direct_call_loan_ends().is_empty());
            assert!(unit.class_result_argument_unwinds().is_empty());
            let facts = unit.singleton_temporary_argument_unwinds();
            if !expected {
                assert!(facts.is_empty(), "{call}: {facts:?}");
                if operand == "nested.value" {
                    assert!(unit.loans().iter().any(|loan| {
                        observed.sources.slice(loan.begin_span()) == Ok(operand)
                            && loan.target() != &UnitLoanTarget::Temporary(loan.argument())
                    }));
                }
                continue;
            }
            let [end] = facts else {
                panic!("{call}: expected one retired temporary loan");
            };
            let loan = end.loan();
            let ast = observed.parsed.ast().expressions();
            assert_eq!(
                observed
                    .sources
                    .slice(ast.get(loan.call().expression()).unwrap().span()),
                Ok(call.as_str())
            );
            assert_eq!(observed.sources.slice(loan.begin_span()), Ok(operand));
            let UnitDropPoint::ControlTransfer(transfer) = end.point() else {
                panic!("the actual edge must exit before dispatch");
            };
            assert_eq!(
                observed
                    .sources
                    .slice(ast.get(transfer.expression()).unwrap().span()),
                Ok(exit)
            );
            assert_eq!(unit.call_may_dispatch(loan.call()), Some(false));
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            assert_eq!(end.preceding_drops(), 0);
            assert_eq!(
                unit.drops()
                    .iter()
                    .filter(|drop| drop.target() == UnitDropTarget::Temporary(loan.argument()))
                    .map(|drop| drop.point())
                    .collect::<Vec<_>>(),
                [end.point()]
            );
        }
    }
}

#[test]
fn singleton_prefix_retires_owned_container_operands_from_the_actual_exit() {
    for (family, operand, exit, expected) in [
        ("Box<Packet>", "makeBox()", "break", true),
        ("Box<Packet>?", "makeOptional(flag)", "return", true),
        ("Box<Packet>?", "makeBox()", "return", true),
        ("Array<Leaf>", "arrayOf(Leaf(1))", "continue", true),
        ("List<Leaf>", "listOf(Leaf(2))", "return", true),
        ("MutableList<Leaf>", "mutableListOf(Leaf(3))", "break", true),
        ("Map<Int, Leaf>", "mapOf<Int, Leaf>()", "return", true),
        ("Map<Int, Leaf>", "makeImmutable()", "return", true),
        ("MutableMap<Int, Leaf>", "makeMap()", "break", true),
        ("Map<Int, Leaf>?", "maybeMap(flag)", "return", true),
        (
            "MutableMap<Int, Leaf>?",
            "maybeMutable(flag)",
            "return",
            true,
        ),
        ("Array<Leaf>", "borrowed", "return", false),
        (
            "Array<Leaf>",
            "Holder(arrayOf(Leaf(5))).values",
            "return",
            false,
        ),
        ("List<Leaf>", "listOf(Leaf(6))", "0", false),
        ("Array<Leaf>", "arrayOf(Leaf(return))", "0", false),
        ("List<Leaf>", "listOf(Leaf(4))", "if (flag) 1 else 0", false),
    ] {
        let call = format!("Gate.inspect({operand}, {exit})");
        let body = match exit {
            "break" | "continue" => format!("loop {{ {call} }}"),
            _ => call.clone(),
        };
        let observed = observe(&format!(
            r#"
class Leaf(val number: Int)
class Holder(val values: Array<Leaf>)
enum class Packet {{ Full(value: Leaf), Empty }}
fun makeBox(): Box<Packet> = Box(Packet.Full(Leaf(7)))
fun makeOptional(flag: Boolean): Box<Packet>? = if (flag) makeBox() else null
fun makeMap(): MutableMap<Int, Leaf> {{
    var values = mutableMapOf<Int, Leaf>()
    values.put(1, Leaf(8))
    return values
}}
fun makeImmutable(): Map<Int, Leaf> {{
    var values = makeMap()
    return values.consume()
}}
fun maybeMap(flag: Boolean): Map<Int, Leaf>? = if (flag) makeImmutable() else null
fun maybeMutable(flag: Boolean): MutableMap<Int, Leaf>? = if (flag) makeMap() else null
object Gate {{ fun inspect(value: {family}, stop: Int): Unit {{}} }}
fun run(borrowed: Array<Leaf>, flag: Boolean): Unit {{ {body} }}
"#
        ));
        assert_units_admitted(&observed);
        for unit in &observed.units {
            let facts = unit.singleton_temporary_argument_unwinds();
            assert!(unit.direct_call_loan_ends().is_empty());
            assert!(unit.class_result_argument_unwinds().is_empty());
            if !expected {
                assert!(facts.is_empty(), "{call}: {facts:?}");
                if operand.ends_with(".values") {
                    assert!(unit.loans().iter().any(|loan| {
                        observed.sources.slice(loan.begin_span()) == Ok(operand)
                            && loan.target() != &UnitLoanTarget::Temporary(loan.argument())
                    }));
                }
                continue;
            }
            let [end] = facts else {
                panic!("{call}: expected one retired temporary loan");
            };
            let loan = end.loan();
            let expressions = observed.parsed.ast().expressions();
            let UnitDropPoint::ControlTransfer(transfer) = end.point() else {
                panic!("the actual edge must exit before dispatch");
            };
            for (id, spelling) in [
                (loan.call(), call.as_str()),
                (loan.argument(), operand),
                (transfer, exit),
            ] {
                assert_eq!(
                    observed
                        .sources
                        .slice(expressions.get(id.expression()).unwrap().span()),
                    Ok(spelling)
                );
            }
            assert_eq!(unit.call_may_dispatch(loan.call()), Some(false));
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(end.preceding_drops(), 0);
            assert_eq!(
                unit.drops()
                    .iter()
                    .filter(|drop| drop.target() == UnitDropTarget::Temporary(loan.argument()))
                    .map(|drop| drop.point())
                    .collect::<Vec<_>>(),
                [end.point()]
            );
        }
    }
}

#[test]
fn singleton_mixed_dispatch_publishes_the_actual_exit_and_preserves_call_return_drop() {
    for (family, expression, class_result) in [
        ("Leaf", "Leaf(1)", true),
        ("String", "\"source\".clone()", false),
        ("Rc<Leaf>", "Rc(Leaf(2))", false),
        ("Box<Packet>", "Box(Packet.Full(Leaf(3)))", false),
        ("Array<Leaf>", "arrayOf(Leaf(4))", false),
        ("Map<Int, Leaf>?", "mapOf<Int, Leaf>()", false),
    ] {
        let call = "Gate.inspect(make(), if (flag) return else 0)";
        let observed = observe(&format!(
            r#"
class Leaf(val value: Int)
enum class Packet {{ Full(value: Leaf), Empty }}
fun make(): {family} = {expression}
object Gate {{ fun inspect(value: {family}, stop: Int): Unit {{}} }}
fun run(flag: Boolean): Unit {{ {call} }}
"#
        ));
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert!(unit.direct_call_loan_ends().is_empty());
            let class = unit.class_result_argument_unwinds();
            let other = unit.singleton_temporary_argument_unwinds();
            let (ends, unrelated) = if class_result {
                (class, other)
            } else {
                (other, class)
            };
            assert!(unrelated.is_empty());
            let [end] = ends else {
                panic!("{family}: only the real early exit retires this prefix loan");
            };
            let loan = end.loan();
            let UnitDropPoint::ControlTransfer(transfer) = end.point() else {
                panic!("register the early edge independently of normal dispatch");
            };
            let expressions = observed.parsed.ast().expressions();
            for (id, spelling) in [
                (loan.call(), call),
                (loan.argument(), "make()"),
                (transfer, "return"),
            ] {
                assert_eq!(
                    observed
                        .sources
                        .slice(expressions.get(id.expression()).unwrap().span()),
                    Ok(spelling)
                );
            }
            assert_eq!(unit.call_may_dispatch(loan.call()), Some(true));
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(end.preceding_drops(), 0);
            let drops = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Temporary(loan.argument()))
                .map(|drop| drop.point())
                .collect::<Vec<_>>();
            assert_eq!(drops, [end.point(), UnitDropPoint::CallReturn(loan.call())]);
        }
    }
}
