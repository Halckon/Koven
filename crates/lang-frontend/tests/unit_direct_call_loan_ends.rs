//! 真实源码的直接 Borrow 调用事件切片；固定 oracle 走 Unit，其他族保留 legacy witness。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SourceUnitKey, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, LoanKind, OwnershipCheckedFile, OwnershipDeferredReason,
        UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
        check_ownership,
    },
    parser::{Expression, ParsedFile, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        UnitCallTarget, UnitExpressionId, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, standard_environments,
    },
};

struct Observed {
    environment: lang_frontend::name_resolution::NameEnvironment,
    sources: SourceMap,
    parsed: ParsedFile,
    units: Vec<CompilationUnitOwnership>,
    unit_names: Vec<ValidatedCompilationUnitNames>,
    unit_types: Vec<lang_frontend::type_checking::ValidatedCompilationUnitTypes>,
}

fn observe(text: &str) -> Observed {
    let mut sources = SourceMap::new();
    let source = sources.add_source("call_ends.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let mut units = Vec::new();
    let mut unit_names = Vec::new();
    let mut unit_types = Vec::new();
    for input in [
        SourceUnitInput::new("root", "call_ends.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        let typed = typed.validate().unwrap();
        units.push(
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap(),
        );
        unit_names.push(names);
        unit_types.push(typed);
    }
    Observed {
        environment,
        sources,
        parsed,
        units,
        unit_names,
        unit_types,
    }
}

// 只有尚未由固定完整 oracle 承接的测试族继续运行 Single 对照。
// 与 Unit 共用同一份 SourceMap / ParsedFile，不重新解析或改写原源码。
fn observe_with_legacy(text: &str) -> (Observed, OwnershipCheckedFile) {
    let observed = observe(text);
    let (environment, types) = standard_environments();
    let names = resolve_names(&observed.sources, &observed.parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&observed.sources, &observed.parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let legacy = check_ownership(&observed.sources, &observed.parsed, &names, &typed).unwrap();
    (observed, legacy)
}

fn assert_units_admitted(observed: &Observed) {
    assert_eq!(observed.units.len(), 2);
    for unit in &observed.units {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        unit.clone().validate().unwrap();
    }
}

type ByteSpan = (usize, usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpectedPoint {
    CallReturn,
    Return(ByteSpan),
    Break(ByteSpan),
    Continue(ByteSpan),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpectedTarget {
    Local(ByteSpan),
    Temporary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExpectedEnd {
    call: ByteSpan,
    argument: ByteSpan,
    parameter: ByteSpan,
    point: ExpectedPoint,
    target: ExpectedTarget,
}

// 固定期望来自逐份原源码，不从任一分析产物生成。
// 已等价的成功源码走 Unit；temporary-field、错误和 deferred 的旧 witness 保留。
fn assert_fixed_ends(observed: &Observed, expected: &[ExpectedEnd]) {
    assert_eq!(observed.units.len(), 2);
    assert_eq!(observed.unit_names.len(), 2);
    let source = observed.parsed.source_id();
    let bytes = |span: Span| {
        assert_eq!(span.source_id(), source);
        (span.start(), span.end())
    };
    for (entry, (unit, names)) in observed.units.iter().zip(&observed.unit_names).enumerate() {
        let [indexed] = names.names().index().source_units() else {
            panic!("one actual source in this entry");
        };
        assert_eq!(indexed.source_id(), source);
        match (entry, indexed.key()) {
            (0, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "call_ends.ko");
            }
            (1, SourceUnitKey::MemoryDocument) => {}
            other => panic!("unexpected source key: {other:?}"),
        }
        let [local_names] = names.names().source_units() else {
            panic!("one same-round local name product");
        };
        assert_eq!(local_names.source_unit(), indexed.id());
        let node = |expression: UnitExpressionId| {
            assert_eq!(expression.source_unit(), indexed.id());
            observed
                .parsed
                .ast()
                .expressions()
                .get(expression.expression())
                .unwrap()
        };
        let actual = unit
            .direct_call_loan_ends()
            .iter()
            .map(|event| {
                let loan = event.loan();
                assert!(unit.loans().contains(loan));
                assert_eq!(loan.kind(), LoanKind::Shared);
                assert_eq!(event.preceding_drops(), 0);
                let call = node(loan.call());
                let Expression::Call { arguments, .. } = call.payload() else {
                    panic!("actual call expression");
                };
                assert!(
                    arguments
                        .iter()
                        .any(|argument| argument.value == loan.argument().expression())
                );
                let argument = node(loan.argument());
                assert_eq!(loan.begin_span(), argument.span());
                assert_eq!(loan.end_span(), call.span());
                let point = match event.point() {
                    UnitDropPoint::CallReturn(returned) => {
                        assert_eq!(returned, loan.call());
                        ExpectedPoint::CallReturn
                    }
                    UnitDropPoint::ControlTransfer(transfer) => {
                        let transfer = node(transfer);
                        match transfer.payload() {
                            Expression::Return { .. } => {
                                ExpectedPoint::Return(bytes(transfer.span()))
                            }
                            Expression::Break { .. } => {
                                ExpectedPoint::Break(bytes(transfer.span()))
                            }
                            Expression::Continue { .. } => {
                                ExpectedPoint::Continue(bytes(transfer.span()))
                            }
                            other => panic!("actual control-transfer expression: {other:?}"),
                        }
                    }
                    other => panic!("unexpected end point: {other:?}"),
                };
                let target = match loan.target() {
                    UnitLoanTarget::Place(place) => {
                        assert!(place.is_root());
                        let root = place.root();
                        assert_eq!(root.source_unit(), indexed.id());
                        assert!(unit.formed_owned_locals().contains(&root));
                        let symbol = &local_names.resolution().symbols()[root.symbol().index()];
                        assert_eq!(symbol.id(), root.symbol());
                        assert_eq!(symbol.name(), "owner");
                        assert_eq!(observed.sources.slice(symbol.span()), Ok("owner"));
                        ExpectedTarget::Local(bytes(symbol.span()))
                    }
                    UnitLoanTarget::Temporary(owner) => {
                        assert_eq!(
                            *owner,
                            loan.argument(),
                            "the outer argument owns the temporary"
                        );
                        ExpectedTarget::Temporary
                    }
                    other => panic!("unexpected loan target: {other:?}"),
                };
                ExpectedEnd {
                    call: bytes(call.span()),
                    argument: bytes(argument.span()),
                    parameter: bytes(loan.parameter_span().expect("selected source parameter")),
                    point,
                    target,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual, expected,
            "entry {entry}: complete ordered event vector"
        );
    }
}

#[test]
fn direct_and_grouped_root_arguments_end_once_at_call_return() {
    let observed = observe(
        "fun read(first: String, second: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; read(owner, ((owner))) }",
    );
    assert_units_admitted(&observed);
    assert_fixed_ends(
        &observed,
        &[
            ExpectedEnd {
                call: (87, 109),
                argument: (99, 108),
                parameter: (24, 38),
                point: ExpectedPoint::CallReturn,
                target: ExpectedTarget::Local((71, 76)),
            },
            ExpectedEnd {
                call: (87, 109),
                argument: (92, 97),
                parameter: (9, 22),
                point: ExpectedPoint::CallReturn,
                target: ExpectedTarget::Local((71, 76)),
            },
        ],
    );
    for unit in &observed.units {
        let ends = unit.direct_call_loan_ends();
        assert_eq!(ends.len(), 2);
        assert_eq!(ends[0].preceding_call_loan_ends(), 0);
        assert_eq!(ends[1].preceding_call_loan_ends(), 1);
        assert!(
            ends.iter()
                .all(|end| end.point() == UnitDropPoint::CallReturn(end.loan().call()))
        );
        let arguments = ends
            .iter()
            .map(|end| {
                observed
                    .sources
                    .slice(
                        observed
                            .parsed
                            .ast()
                            .expressions()
                            .get(end.loan().argument().expression())
                            .unwrap()
                            .span(),
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            arguments,
            ["((owner))", "owner"],
            "actual pending stack retirement order"
        );
        let UnitLoanTarget::Place(place) = ends[0].loan().target() else {
            panic!("root place")
        };
        assert_eq!(ends[1].loan().target(), ends[0].loan().target());
        let drops = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), ends[0].point());
    }
}

#[test]
fn only_established_prefix_ends_on_return_break_and_continue() {
    for operand in ["\"prefix\"", "((\"prefix\"))"] {
        for (jump, looped) in [("return", false), ("break", true), ("continue", true)] {
            let call = format!("read({operand}, if (flag) {{ {jump} }} else {{ 0 }}, \"suffix\")");
            let body = if looped {
                format!("loop {{ {call}; break }}")
            } else {
                call
            };
            let observed = observe(&format!(
                "fun read(first: String, own n: Int, last: String): Unit {{}}\nfun run(flag: Boolean): Unit {{ {body} }}"
            ));
            assert_units_admitted(&observed);
            let (call, prefix, suffix, point) = match (operand, jump) {
                ("\"prefix\"", "return") => (
                    (90, 147),
                    (95, 103),
                    (138, 146),
                    ExpectedPoint::Return((117, 123)),
                ),
                ("\"prefix\"", "break") => (
                    (97, 153),
                    (102, 110),
                    (144, 152),
                    ExpectedPoint::Break((124, 129)),
                ),
                ("\"prefix\"", "continue") => (
                    (97, 156),
                    (102, 110),
                    (147, 155),
                    ExpectedPoint::Continue((124, 132)),
                ),
                ("((\"prefix\"))", "return") => (
                    (90, 151),
                    (95, 107),
                    (142, 150),
                    ExpectedPoint::Return((121, 127)),
                ),
                ("((\"prefix\"))", "break") => (
                    (97, 157),
                    (102, 114),
                    (148, 156),
                    ExpectedPoint::Break((128, 133)),
                ),
                ("((\"prefix\"))", "continue") => (
                    (97, 160),
                    (102, 114),
                    (151, 159),
                    ExpectedPoint::Continue((128, 136)),
                ),
                other => panic!("original prefix fixture: {other:?}"),
            };
            assert_fixed_ends(
                &observed,
                &[
                    ExpectedEnd {
                        call,
                        argument: prefix,
                        parameter: (9, 22),
                        point,
                        target: ExpectedTarget::Temporary,
                    },
                    ExpectedEnd {
                        call,
                        argument: suffix,
                        parameter: (36, 48),
                        point: ExpectedPoint::CallReturn,
                        target: ExpectedTarget::Temporary,
                    },
                    ExpectedEnd {
                        call,
                        argument: prefix,
                        parameter: (9, 22),
                        point: ExpectedPoint::CallReturn,
                        target: ExpectedTarget::Temporary,
                    },
                ],
            );
            for unit in &observed.units {
                let ends = unit.direct_call_loan_ends();
                assert_eq!(
                    ends.len(),
                    3,
                    "one transfer prefix and two normal-return operands"
                );
                let transfer = ends
                    .iter()
                    .filter(|end| matches!(end.point(), UnitDropPoint::ControlTransfer(_)))
                    .collect::<Vec<_>>();
                assert_eq!(transfer.len(), 1);
                let prefix = transfer[0];
                let argument = observed
                    .parsed
                    .ast()
                    .expressions()
                    .get(prefix.loan().argument().expression())
                    .unwrap()
                    .span();
                assert_eq!(observed.sources.slice(argument).unwrap(), operand);
                let UnitLoanTarget::Temporary(owner) = prefix.loan().target() else {
                    panic!("temporary prefix")
                };
                assert_eq!(
                    unit.drops()
                        .iter()
                        .filter(|drop| drop.point() == prefix.point()
                            && drop.target() == UnitDropTarget::Temporary(*owner))
                        .count(),
                    1
                );
                assert_eq!(
                    ends.iter()
                        .filter(|end| matches!(end.point(), UnitDropPoint::CallReturn(_)))
                        .count(),
                    2
                );
            }
        }
    }
}

#[test]
fn nested_call_return_does_not_end_the_outer_pending_loan() {
    let observed = observe(
        "fun inner(text: String): Int = 0\nfun outer(first: String, own n: Int): Unit {}\nfun run(): Unit { outer(\"outer\", inner(\"inner\")) }",
    );
    assert_units_admitted(&observed);
    assert_fixed_ends(
        &observed,
        &[
            ExpectedEnd {
                call: (112, 126),
                argument: (118, 125),
                parameter: (10, 22),
                point: ExpectedPoint::CallReturn,
                target: ExpectedTarget::Temporary,
            },
            ExpectedEnd {
                call: (97, 127),
                argument: (103, 110),
                parameter: (43, 56),
                point: ExpectedPoint::CallReturn,
                target: ExpectedTarget::Temporary,
            },
        ],
    );
    for unit in &observed.units {
        let ends = unit.direct_call_loan_ends();
        assert_eq!(ends.len(), 2);
        let operands = ends
            .iter()
            .map(|end| {
                observed
                    .sources
                    .slice(
                        observed
                            .parsed
                            .ast()
                            .expressions()
                            .get(end.loan().argument().expression())
                            .unwrap()
                            .span(),
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(operands, ["\"inner\"", "\"outer\""]);
        assert_ne!(ends[0].loan().call(), ends[1].loan().call());
        assert_ne!(ends[0].point(), ends[1].point());
    }
}

#[test]
fn inner_loop_jump_keeps_the_outer_call_prefix() {
    let observed = observe(
        "fun read(first: String, own n: Int): Unit {}\nfun run(flag: Boolean): Unit { read(\"outer\", if (flag) { loop { break }; 0 } else { 0 }) }",
    );
    assert_units_admitted(&observed);
    assert_fixed_ends(
        &observed,
        &[ExpectedEnd {
            call: (76, 133),
            argument: (81, 88),
            parameter: (9, 22),
            point: ExpectedPoint::CallReturn,
            target: ExpectedTarget::Temporary,
        }],
    );
    for unit in &observed.units {
        let [end] = unit.direct_call_loan_ends() else {
            panic!("only the completed outer call ends its prefix")
        };
        assert_eq!(end.point(), UnitDropPoint::CallReturn(end.loan().call()));
    }
}

#[test]
fn source_continuations_are_outside_the_direct_call_slice() {
    let observed = observe(
        "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)\nfun read(text: String): Unit {}\nfun run(source: String): Unit { read(view(source)) }",
    );
    assert_units_admitted(&observed);
    assert_fixed_ends(&observed, &[]);
    for unit in &observed.units {
        assert!(!unit.borrow_results().forwarded_source_loans().is_empty());
        assert!(!unit.borrow_results().call_uses().is_empty());
        assert!(
            unit.direct_call_loan_ends().is_empty(),
            "no fabricated inner or outer source end"
        );
    }
}

#[test]
fn ownership_error_clears_previously_observed_call_ends() {
    let (observed, legacy) = observe_with_legacy(
        "fun read(text: String): Unit {}\nfun consume(own text: String): Unit {}\nfun run(): Unit { val text = \"held\"; read(text); consume(text); read(text) }",
    );
    assert!(!legacy.diagnostics().is_empty());
    assert!(legacy.loan_ends().is_empty());
    for unit in &observed.units {
        assert!(!unit.diagnostics().is_empty());
        assert!(unit.direct_call_loan_ends().is_empty());
        assert!(unit.drops().is_empty());
    }
}

#[test]
fn nonreturning_suffix_and_unreachable_calls_publish_no_cleanup() {
    let observed = observe(
        "fun stop(): Nothing = stop()\nfun read(first: String, own n: Int, last: String): Unit {}\nfun run(): Unit { read(\"prefix\", stop(), \"suffix\"); read(\"unreachable\", 0, \"tail\") }",
    );
    assert_units_admitted(&observed);
    assert_fixed_ends(&observed, &[]);
    for unit in &observed.units {
        assert!(unit.direct_call_loan_ends().is_empty());
        assert!(unit.drops().is_empty());
    }
}

#[test]
fn adjacent_projection_exclusions_preserve_the_temporary_field_legacy_witness() {
    let cases = [
        ("own holder: Holder", "", "(holder.text)", None),
        (
            "",
            "",
            "Holder(\"field\").text",
            Some(OwnershipDeferredReason::MemberReceiver),
        ),
        ("own values: List<String>", "", "values[0]", None),
        ("", "", "listOf(\"element\")[0]", None),
        (
            "own source: String",
            "borrow val alias = source;",
            "((alias))",
            None,
        ),
    ];
    for (parameter, prefix, operand, legacy_deferred) in cases {
        for suffix in ["0", "if (flag) { return } else { 0 }"] {
            let parameters = if parameter.is_empty() {
                "flag: Boolean".to_owned()
            } else {
                format!("flag: Boolean, {parameter}")
            };
            let text = format!(
                "class Holder(val text: String)\nfun read(text: String, own n: Int): Unit {{}}\nfun run({parameters}): Unit {{ {prefix} read({operand}, {suffix}) }}"
            );
            let (observed, legacy) = if legacy_deferred.is_some() {
                let (observed, legacy) = observe_with_legacy(&text);
                (observed, Some(legacy))
            } else {
                (observe(&text), None)
            };
            if let Some(reason) = legacy_deferred {
                let legacy = legacy.expect("this source retains its independent legacy witness");
                // 旧 checker 对 temporary field 没有稳定 place，明确保留其准入差异。
                // 不把 legacy deferred 当作成功，也不借此放宽 Unit 的完整准入。
                assert!(
                    legacy.diagnostics().is_empty(),
                    "{:?}",
                    legacy.diagnostics()
                );
                let members = observed
                    .parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter_map(|(id, node)| {
                        (matches!(node.payload(), Expression::Member { .. })
                            && observed.sources.slice(node.span()) == Ok(operand))
                        .then_some(id)
                    })
                    .collect::<Vec<_>>();
                let [member] = members.as_slice() else {
                    panic!("one actual temporary field operand")
                };
                assert_eq!(
                    legacy
                        .deferred()
                        .iter()
                        .map(|fact| (fact.expression(), fact.reason()))
                        .collect::<Vec<_>>(),
                    [(*member, reason)],
                    "operand {operand}, suffix {suffix}"
                );
            } else {
                assert!(legacy.is_none());
            }
            assert_units_admitted(&observed);
            if legacy_deferred.is_some() {
                use temporary_field_pending::{Cleanup, assert_field_cleanup};
                let call = format!("read({operand}, {suffix})");
                let mut drops = Vec::new();
                if suffix != "0" {
                    drops.push((Cleanup::Transfer("return"), "Holder(\"field\")"));
                }
                drops.push((Cleanup::Returned(call.as_str()), "Holder(\"field\")"));
                assert_field_cleanup(&observed, &call, &[(operand, "Holder(\"field\")")], &drops);
            } else {
                assert_fixed_ends(&observed, &[]);
            }
            for unit in &observed.units {
                let actual = unit
                    .loans()
                    .iter()
                    .filter(|loan| {
                        let span = observed
                            .parsed
                            .ast()
                            .expressions()
                            .get(loan.argument().expression())
                            .unwrap()
                            .span();
                        observed.sources.slice(span) == Ok(operand)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(actual.len(), 1, "real excluded operand: {operand}");
                if !prefix.is_empty() {
                    assert_eq!(unit.borrow_results().bindings().len(), 1);
                    assert!(
                        matches!(actual[0].target(), UnitLoanTarget::Place(place) if place.is_root())
                    );
                }
                if operand.starts_with("Holder(") || operand.starts_with("listOf(") {
                    assert!(
                        matches!(actual[0].target(), UnitLoanTarget::Temporary(_)),
                        "projection backing must not be mistaken for a direct temporary"
                    );
                }
                assert!(
                    legacy_deferred.is_some() || unit.direct_call_loan_ends().is_empty(),
                    "excluded operand {operand}, suffix {suffix}"
                );
            }
        }
    }
}

#[test]
fn receiver_inout_and_provider_producers_remain_outside_the_slice() {
    let implicit_field = observe(
        "fun read(text: String): Unit {}\nclass Holder(val text: String) { fun inspect(): Unit { read(text) } }",
    );
    assert_units_admitted(&implicit_field);
    assert_fixed_ends(&implicit_field, &[]);
    for unit in &implicit_field.units {
        assert_eq!(unit.loans().len(), 1);
        assert!(
            unit.direct_call_loan_ends().is_empty(),
            "an implicit field Name is not an owned local root"
        );
    }
    let receiver = observe(
        "class Node { fun touch(text: String): Unit {} }\nfun run(own node: Node): Unit { node.touch(\"method\") }",
    );
    assert_units_admitted(&receiver);
    assert_fixed_ends(&receiver, &[]);
    for unit in &receiver.units {
        assert!(!unit.receiver_facts().is_empty());
        assert!(
            unit.loans().iter().any(|loan| {
                let span = receiver
                    .parsed
                    .ast()
                    .expressions()
                    .get(loan.argument().expression())
                    .unwrap()
                    .span();
                receiver.sources.slice(span) == Ok("\"method\"")
            }),
            "the receiver call has a real plain-string argument loan to exclude"
        );
        assert!(unit.direct_call_loan_ends().is_empty());
    }
    let inout = observe(
        "fun change(inout text: String): Unit {}\nfun read(text: String): Unit {}\nfun run(inout source: String): Unit { read(source); change(&source) }",
    );
    assert_units_admitted(&inout);
    assert_fixed_ends(&inout, &[]);
    for unit in &inout.units {
        assert_eq!(unit.loans().len(), 2);
        assert!(
            unit.loans()
                .iter()
                .any(|loan| loan.kind() == lang_frontend::ownership_checking::LoanKind::Exclusive)
        );
        assert!(unit.direct_call_loan_ends().is_empty());
    }
    let provider = observe(
        "fun read(text: String, own n: Int): Unit {}\nfun readItems(values: List<String>): Unit {}\nfun run(own values: List<String>, flag: Boolean): Unit { for (item in values) { read(item, if (flag) { return } else { 0 }); readItems(values) } }",
    );
    assert_units_admitted(&provider);
    assert_fixed_ends(&provider, &[]);
    for unit in &provider.units {
        assert_eq!(unit.iterations().len(), 1);
        assert_eq!(unit.loans().len(), 2);
        assert!(
            unit.loans().iter().all(
                |loan| matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root())
            )
        );
        assert!(unit.direct_call_loan_ends().is_empty());
    }
}

#[test]
fn later_real_deferred_work_blocks_the_event_capability() {
    let (observed, legacy) = observe_with_legacy(
        "fun read(text: String): Unit {}\nfun run(): Unit { read(\"before\"); var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
    );
    assert!(legacy.diagnostics().is_empty());
    assert!(!legacy.deferred().is_empty());
    for unit in &observed.units {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(!unit.deferred().is_empty());
        assert!(unit.direct_call_loan_ends().is_empty());
    }
}

#[test]
fn cross_source_events_preserve_caller_target_and_callee_parameter_lineage() {
    let mut sources = SourceMap::new();
    let callee_source = sources
        .add_source("read.ko", "package demo\nfun read(text: String): Unit {}")
        .unwrap();
    let caller_source = sources
        .add_source(
            "run.ko",
            "package demo\nfun run(own owner: String): Unit { read(((owner))) }",
        )
        .unwrap();
    let callee = parse_file(&sources, &lex(&sources, callee_source).unwrap()).unwrap();
    let caller = parse_file(&sources, &lex(&sources, caller_source).unwrap()).unwrap();
    assert!(callee.diagnostics().is_empty());
    assert!(caller.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let ordered = [
        SourceUnitInput::new("root", "demo/read.ko", callee_source, &callee),
        SourceUnitInput::new("root", "demo/run.ko", caller_source, &caller),
    ];
    for inputs in [ordered, [ordered[1], ordered[0]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let caller_unit = index
            .source_units()
            .iter()
            .find(|source| source.source_id() == caller_source)
            .unwrap()
            .id();
        let callee_unit = index
            .source_units()
            .iter()
            .find(|source| source.source_id() == callee_source)
            .unwrap()
            .id();
        assert_ne!(caller_unit, callee_unit);
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        let typed = raw.validate().unwrap();
        let unit =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty());
        let [end] = unit.direct_call_loan_ends() else {
            panic!("one actual caller event")
        };
        let loan = end.loan();
        let call = typed.types().call(loan.call()).unwrap();
        let UnitCallTarget::Declaration(target) = call.target() else {
            panic!("real cross-source declaration call")
        };
        let declaration = index
            .declarations()
            .iter()
            .find(|declaration| declaration.id() == target)
            .unwrap();
        assert_eq!(declaration.source_unit(), callee_unit);
        assert_eq!(declaration.name(), "read");
        let contracts = unit
            .call_argument_contracts_for(loan.call())
            .collect::<Vec<_>>();
        let [contract] = contracts.as_slice() else {
            panic!("one selected parameter")
        };
        assert_eq!(contract.argument(), loan.argument());
        assert_eq!(contract.parameter_index(), 0);
        assert_eq!(contract.parameter_span(), loan.parameter_span());
        assert_eq!(loan.call().source_unit(), caller_unit);
        assert_eq!(loan.argument().source_unit(), caller_unit);
        assert_eq!(end.point(), UnitDropPoint::CallReturn(loan.call()));
        assert_eq!(loan.begin_span().source_id(), caller_source);
        assert_eq!(loan.end_span().source_id(), caller_source);
        let parameter = loan.parameter_span().unwrap();
        assert_eq!(parameter.source_id(), callee_source);
        assert_eq!(sources.slice(parameter).unwrap(), "text: String");
        let UnitLoanTarget::Place(place) = loan.target() else {
            panic!("caller owned root")
        };
        assert!(place.is_root());
        assert_eq!(place.root().source_unit(), caller_unit);
        let binding = unit
            .bindings()
            .iter()
            .find(|binding| binding.symbol() == place.root())
            .unwrap();
        assert_eq!(binding.declaration_span().source_id(), caller_source);
        assert_eq!(
            sources.slice(binding.declaration_span()).unwrap(),
            "own owner: String"
        );
        let drops = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), end.point());
        assert_eq!(drops[0].value_origin().source_id(), caller_source);
        assert_eq!(end.preceding_drops(), 0);
        unit.validate().unwrap();
    }
}

#[path = "unit_direct_call_loan_ends/exclusive_prefix.rs"]
mod exclusive_prefix;

#[path = "unit_direct_call_loan_ends/temporary_field_pending.rs"]
mod temporary_field_pending;

#[path = "unit_direct_call_loan_ends/gate_contracts.rs"]
mod gate_contracts;

#[path = "unit_direct_call_loan_ends/borrow_handoff.rs"]
mod borrow_handoff;

#[path = "unit_direct_call_loan_ends/borrow_auxiliary.rs"]
mod borrow_auxiliary;

#[path = "unit_direct_call_loan_ends/normal_oracle.rs"]
mod normal_oracle;

#[path = "unit_direct_call_loan_ends/prefix_oracle.rs"]
mod prefix_oracle;

#[path = "unit_direct_call_loan_ends/prefix_order.rs"]
mod prefix_order;

#[path = "unit_direct_call_loan_ends/borrow_transport.rs"]
mod borrow_transport;

#[path = "unit_direct_call_loan_ends/borrow_binding_transport.rs"]
mod borrow_binding_transport;

#[path = "unit_direct_call_loan_ends/borrow_forward_transport.rs"]
mod borrow_forward_transport;

#[path = "unit_direct_call_loan_ends/temporary_field_ends.rs"]
mod temporary_field_ends;

#[path = "unit_direct_call_loan_ends/exclusive_order.rs"]
mod exclusive_order;

#[path = "unit_direct_call_loan_ends/receiver_prefix.rs"]
mod receiver_prefix;

#[path = "unit_direct_call_loan_ends/constructed_argument.rs"]
mod constructed_argument;

#[path = "unit_direct_call_loan_ends/receiver_cfg_prefix.rs"]
mod receiver_cfg_prefix;

#[path = "unit_direct_call_loan_ends/receiver_return.rs"]
mod receiver_return;

#[path = "unit_direct_call_loan_ends/borrowed_receiver_return.rs"]
mod borrowed_receiver_return;

#[path = "unit_direct_call_loan_ends/borrowed_receiver_prefix.rs"]
mod borrowed_receiver_prefix;

#[path = "unit_direct_call_loan_ends/inout_receiver_reborrow.rs"]
mod inout_receiver_reborrow;

#[path = "unit_direct_call_loan_ends/this_receiver_reborrow.rs"]
mod this_receiver_reborrow;

#[path = "unit_direct_call_loan_ends/field_receiver_reborrow.rs"]
mod field_receiver_reborrow;

#[path = "unit_direct_call_loan_ends/direct_binding_receiver.rs"]
mod direct_binding_receiver;

#[path = "unit_direct_call_loan_ends/field_binding_receiver.rs"]
mod field_binding_receiver;

#[path = "unit_direct_call_loan_ends/lifecycle_combination.rs"]
mod lifecycle_combination;

#[path = "unit_direct_call_loan_ends/constructed_receiver_combination.rs"]
mod constructed_receiver_combination;

#[path = "unit_direct_call_loan_ends/exclusive_loop_prefix.rs"]
mod exclusive_loop_prefix;

#[path = "unit_direct_call_loan_ends/borrow_publication.rs"]
mod borrow_publication;

#[path = "unit_direct_call_loan_ends/interpolated_string.rs"]
mod interpolated_string;

#[path = "unit_direct_call_loan_ends/grouped_initializer.rs"]
mod grouped_initializer;

#[path = "unit_direct_call_loan_ends/borrow_temporary_auxiliary.rs"]
mod borrow_temporary_auxiliary;

#[path = "unit_direct_call_loan_ends/loop_forward_source.rs"]
mod loop_forward_source;

#[path = "unit_direct_call_loan_ends/exclusive_auxiliary.rs"]
mod exclusive_auxiliary;

#[path = "unit_direct_call_loan_ends/action_source.rs"]
mod action_source;

#[path = "unit_direct_call_loan_ends/named_action_region.rs"]
mod named_action_region;

#[path = "unit_direct_call_loan_ends/result_action_source.rs"]
mod result_action_source;

#[path = "unit_direct_call_loan_ends/consumer_action_link.rs"]
mod consumer_action_link;
