//! SPEC-0243: source-qualified instance receiver reservation and activation.

use lang_frontend::{
    diagnostic::DiagnosticDetail,
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

const PROVIDER: &str = "class Worker(var count: Int) {\n\
    fun read(): Int = count\n\
    inout fun bump(): Int { count = count + 1\nreturn count }\n\
    inout fun set(own value: Int): Unit { count = value }\n\
    inout fun update(other: Worker): Unit {}\n\
    inout fun take(own other: Worker): Unit {}\n\
}\n\
fun exclusive(inout worker: Worker, value: Int): Unit {}\n\
fun inspect(worker: Worker): Int = worker.count\n\
fun consume(own worker: Worker): Int = 0";

fn analyze(provider: &str, consumer: &str) -> (SourceMap, CompilationUnitOwnership) {
    analyze_ordered(provider, consumer, false)
}

fn analyze_ordered(
    provider: &str,
    consumer: &str,
    reverse: bool,
) -> (SourceMap, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let provider_id = sources.add_source("provider.ko", provider).unwrap();
    let consumer_id = sources.add_source("consumer.ko", consumer).unwrap();
    let provider = parse_file(&sources, &lex(&sources, provider_id).unwrap()).unwrap();
    let consumer = parse_file(&sources, &lex(&sources, consumer_id).unwrap()).unwrap();
    assert!(
        provider.diagnostics().is_empty(),
        "{:?}",
        provider.diagnostics()
    );
    assert!(
        consumer.diagnostics().is_empty(),
        "{:?}",
        consumer.diagnostics()
    );
    let mut inputs = [
        SourceUnitInput::new("root", "provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "consumer.ko", consumer_id, &consumer),
    ];
    if reverse {
        inputs.reverse();
    }
    let (name_environment, type_environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .expect("valid names");
    let types = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .unwrap()
        .validate()
        .expect("valid types");
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &types)
            .unwrap();
    (sources, ownership)
}

fn assert_valid(ownership: &CompilationUnitOwnership) {
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert!(
        ownership.deferred().is_empty(),
        "{:?}",
        ownership.deferred()
    );
    assert!(ownership.clone().validate().is_ok());
}

#[test]
fn receiver_reservation_allows_argument_reads_and_completed_shared_calls() {
    for operand in ["worker.count", "worker.read()", "inspect(worker)"] {
        let (_, ownership) = analyze(
            PROVIDER,
            &format!("fun test(own worker: Worker): Unit {{ worker.set({operand}) }}"),
        );
        assert_valid(&ownership);
    }
}

#[test]
fn receiver_reservation_allows_implicit_and_explicit_this_reads() {
    for operand in ["count", "this.count", "read()", "this.read()"] {
        let provider = format!(
            "{PROVIDER}\nclass Counter(var count: Int) {{\n\
            fun read(): Int = count\n\
            inout fun set(own value: Int): Unit {{ count = value }}\n\
            inout fun test(): Unit {{ set({operand}) }}\n}}"
        );
        let (_, ownership) = analyze(&provider, "fun entry(): Unit {}");
        assert_valid(&ownership);
    }
}

#[test]
fn receiver_activation_keeps_borrow_arguments_live_and_reports_exact_sources() {
    let (sources, ownership) = analyze(
        PROVIDER,
        "fun test(own worker: Worker): Unit { worker.update(worker) }",
    );
    let [diagnostic] = ownership.diagnostics() else {
        panic!(
            "one activation conflict expected: {:?}",
            ownership.diagnostics()
        );
    };
    assert_eq!(diagnostic.code().to_string(), "L0135");
    let contract = ownership
        .call_receiver_contracts()
        .iter()
        .find(|contract| sources.slice(contract.call_span()).unwrap() == "worker.update(worker)")
        .unwrap();
    let argument = ownership
        .call_argument_contracts()
        .iter()
        .find(|argument| argument.call() == contract.call())
        .unwrap();
    assert_eq!(diagnostic.primary_span(), argument.argument_span());
    let label = diagnostic
        .details()
        .iter()
        .find_map(|detail| match detail {
            DiagnosticDetail::Label(label) => Some(label),
            _ => None,
        })
        .unwrap();
    assert_eq!(label.span(), contract.receiver_span());
    assert!(ownership.receiver_facts().is_empty());
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.drops().is_empty());
}

#[test]
fn reserved_receiver_still_blocks_nested_mutation_and_move() {
    for operand in ["worker.bump()", "consume(worker)"] {
        let (_, ownership) = analyze(
            PROVIDER,
            &format!("fun test(own worker: Worker): Unit {{ worker.set({operand}) }}"),
        );
        let codes: Vec<_> = ownership
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(codes, ["L0135"], "{operand}");
        assert!(ownership.receiver_facts().is_empty());
    }
}

#[test]
fn ordinary_inout_argument_is_immediately_exclusive() {
    let (_, ownership) = analyze(
        PROVIDER,
        "fun test(inout worker: Worker): Unit { exclusive(&worker, worker.count) }",
    );
    let codes: Vec<_> = ownership
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect();
    assert_eq!(codes, ["L0135"]);
    assert!(ownership.receiver_facts().is_empty());
}

#[test]
fn successful_facts_identify_reservation_and_call_entry() {
    let (sources, ownership) = analyze(
        PROVIDER,
        "fun test(own worker: Worker): Unit { worker.set(worker.read())\nworker.bump() }",
    );
    assert_valid(&ownership);
    for fact in ownership.receiver_facts() {
        let call = sources.slice(fact.end_span()).unwrap();
        if call == "worker.read()" {
            assert!(!fact.is_receiver_reservation());
            assert_eq!(fact.activation_point(), None);
        } else {
            assert!(fact.is_receiver_reservation(), "{call}");
            assert_eq!(fact.activation_point(), Some(fact.call()), "{call}");
        }
    }
    assert_eq!(ownership.receiver_facts().len(), 3);
}

#[test]
fn terminating_arguments_keep_reservation_without_activation() {
    for operand in ["return", "error(\"stop\")"] {
        let (sources, ownership) = analyze(
            PROVIDER,
            &format!("fun test(own worker: Worker): Unit {{ worker.set({operand}) }}"),
        );
        assert_valid(&ownership);
        let fact = ownership
            .receiver_facts()
            .iter()
            .find(|fact| {
                sources
                    .slice(fact.end_span())
                    .unwrap()
                    .starts_with("worker.set(")
            })
            .unwrap();
        assert!(fact.is_receiver_reservation());
        assert_eq!(fact.activation_point(), None, "{operand}");
    }
}

#[test]
fn loop_exits_cancel_reservation_and_do_not_poison_later_calls() {
    for exit in ["break", "continue"] {
        let (sources, ownership) = analyze(
            PROVIDER,
            &format!(
                "fun test(own worker: Worker, flag: Boolean): Unit {{\n\
                while (flag) {{ worker.set({exit}) }}\n\
                worker.set(worker.read())\n}}"
            ),
        );
        assert_valid(&ownership);
        for fact in ownership
            .receiver_facts()
            .iter()
            .filter(|fact| fact.is_receiver_reservation())
        {
            let call = sources.slice(fact.end_span()).unwrap();
            assert_eq!(
                fact.activation_point(),
                (call == "worker.set(worker.read())").then_some(fact.call()),
                "{call}"
            );
        }
    }
}

#[test]
fn normal_branch_activates_after_other_branch_returns() {
    let (_, ownership) = analyze(
        PROVIDER,
        "fun test(own worker: Worker, flag: Boolean): Unit {\n\
            worker.set(if (flag) { return } else { worker.read() })\n}",
    );
    assert_valid(&ownership);
    let fact = ownership
        .receiver_facts()
        .iter()
        .find(|fact| fact.is_receiver_reservation())
        .unwrap();
    assert_eq!(fact.activation_point(), Some(fact.call()));
}

#[test]
fn inner_loop_exit_preserves_outer_reservation() {
    let (_, ownership) = analyze(
        PROVIDER,
        "fun test(own worker: Worker, flag: Boolean): Unit {\n\
            worker.set(if (flag) { loop { break }\nworker.read() } else { worker.count })\n}",
    );
    assert_valid(&ownership);
    let fact = ownership
        .receiver_facts()
        .iter()
        .find(|fact| fact.is_receiver_reservation())
        .unwrap();
    assert_eq!(fact.activation_point(), Some(fact.call()));
}

#[test]
fn nothing_returning_method_still_activates_before_entering_callee() {
    let provider = "class Stopper { inout fun stop(own value: Int): Nothing = error(\"stop\") }";
    let (_, ownership) = analyze(
        provider,
        "fun test(own stopper: Stopper): Nothing = stopper.stop(1)",
    );
    assert_valid(&ownership);
    let [fact] = ownership.receiver_facts() else {
        panic!("one receiver expected")
    };
    assert!(fact.is_receiver_reservation());
    assert_eq!(fact.activation_point(), Some(fact.call()));
}

#[test]
fn overlapping_borrowed_field_conflicts_at_activation() {
    let provider = "class Counter(var count: Int) { inout fun set(value: Int): Unit {} }";
    let (sources, ownership) = analyze(
        provider,
        "fun test(own counter: Counter): Unit { counter.set(counter.count) }",
    );
    let [diagnostic] = ownership.diagnostics() else {
        panic!("one conflict expected")
    };
    assert_eq!(diagnostic.code().to_string(), "L0135");
    assert_eq!(
        sources.slice(diagnostic.primary_span()).unwrap(),
        "counter.count"
    );
    assert!(ownership.receiver_facts().is_empty());
}

#[test]
fn sibling_receiver_places_remain_disjoint() {
    let provider = format!("{PROVIDER}\nclass Pair(val left: Worker, val right: Worker) {{}}");
    let (_, ownership) = analyze(
        &provider,
        "fun test(own pair: Pair): Unit { pair.left.set(pair.right.bump()) }",
    );
    assert_valid(&ownership);
    assert_eq!(ownership.receiver_facts().len(), 2);
    assert!(
        ownership
            .receiver_facts()
            .iter()
            .all(|fact| fact.is_receiver_reservation()
                && fact.activation_point() == Some(fact.call()))
    );
}

#[test]
fn direct_mutation_and_explicit_exclusive_argument_conflict_during_reservation() {
    let provider = format!("{PROVIDER}\nfun write(inout worker: Worker): Int = 0");
    let (_, ownership) = analyze(
        &provider,
        "fun test(inout worker: Worker): Unit { worker.set(write(&worker)) }",
    );
    let codes: Vec<_> = ownership
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect();
    assert_eq!(codes, ["L0135"]);
    assert!(ownership.receiver_facts().is_empty());

    // Field mutability is available in the defining source; keep unrelated cross-source
    // field-mutability lookup outside this receiver reservation regression.
    let provider = format!(
        "{PROVIDER}\nfun test(inout worker: Worker, flag: Boolean): Unit {{\n\
        worker.set(if (flag) {{ worker.count = 1\n0 }} else {{ 0 }})\n}}"
    );
    let (_, ownership) = analyze(&provider, "fun entry(): Unit {}");
    let codes: Vec<_> = ownership
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect();
    assert_eq!(codes, ["L0135"]);
    assert!(ownership.receiver_facts().is_empty());
}

#[test]
fn implicit_receiver_activation_conflicts_with_borrowed_field() {
    let provider = "class Counter(var count: Int) {\n\
        inout fun set(value: Int): Unit {}\n\
        inout fun test(): Unit { set(count) }\n}";
    let (sources, ownership) = analyze(provider, "fun entry(): Unit {}");
    let [diagnostic] = ownership.diagnostics() else {
        panic!("one conflict expected")
    };
    assert_eq!(diagnostic.code().to_string(), "L0135");
    assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), "count");
    let label = diagnostic
        .details()
        .iter()
        .find_map(|detail| match detail {
            DiagnosticDetail::Label(label) => Some(label),
            _ => None,
        })
        .unwrap();
    assert_eq!(sources.slice(label.span()).unwrap(), "set");
    assert!(ownership.receiver_facts().is_empty());
}

#[test]
fn known_container_element_receiver_places_keep_existing_overlap_rules() {
    for (operand, expected) in [
        ("workers[0].read()", vec![]),
        ("workers[1].bump()", vec![]),
        ("workers[0].bump()", vec!["L0135"]),
    ] {
        let (_, ownership) = analyze(
            PROVIDER,
            &format!("fun test(own workers: Array<Worker>): Unit {{ workers[0].set({operand}) }}"),
        );
        let codes: Vec<_> = ownership
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(codes, expected, "{operand}");
        if expected.is_empty() {
            assert_valid(&ownership);
        }
    }
}

fn stable_snapshot(sources: &SourceMap, ownership: &CompilationUnitOwnership) -> Vec<String> {
    let span = |span: lang_frontend::source::Span| {
        format!(
            "{}:{}..{}:{}",
            sources.source_name(span.source_id()).unwrap(),
            span.start(),
            span.end(),
            sources.slice(span).unwrap()
        )
    };
    let mut result = Vec::new();
    for fact in ownership.receiver_facts() {
        result.push(format!(
            "receiver {:?} {:?} {:?} {:?} {} {:?} {} {} {:?}",
            fact.call(),
            fact.source(),
            fact.target(),
            fact.kind(),
            fact.is_receiver_reservation(),
            fact.activation_point(),
            fact.receiver_type().index(),
            span(fact.begin_span()),
            (span(fact.end_span()), fact.declaration_span().map(span))
        ));
    }
    for contract in ownership.call_receiver_contracts() {
        result.push(format!(
            "contract {:?} {:?} {:?} {} {:?} {:?} {} {} {:?}",
            contract.call(),
            contract.target(),
            contract.source(),
            contract.receiver_type().index(),
            contract.category(),
            contract.kind(),
            span(contract.receiver_span()),
            span(contract.call_span()),
            contract.declaration_span().map(span)
        ));
    }
    for loan in ownership.loans() {
        result.push(format!(
            "loan {:?} {:?} {:?} {:?} {} {} {:?}",
            loan.call(),
            loan.argument(),
            loan.target(),
            loan.kind(),
            span(loan.begin_span()),
            span(loan.end_span()),
            loan.parameter_span().map(span)
        ));
    }
    for diagnostic in ownership.diagnostics() {
        result.push(format!(
            "diagnostic {} {} {} {:?}",
            diagnostic.code(),
            diagnostic.message(),
            span(diagnostic.primary_span()),
            diagnostic.details()
        ));
    }
    result
}

#[test]
fn receiver_facts_and_activation_diagnostics_are_input_order_stable() {
    for body in [
        "fun test(own worker: Worker): Unit { worker.set(worker.read()) }",
        "fun test(own worker: Worker): Unit { worker.update(worker) }",
    ] {
        let (forward_sources, forward) = analyze_ordered(PROVIDER, body, false);
        let (reverse_sources, reverse) = analyze_ordered(PROVIDER, body, true);
        assert_eq!(
            stable_snapshot(&forward_sources, &forward),
            stable_snapshot(&reverse_sources, &reverse)
        );
    }
}

#[test]
fn named_arguments_follow_source_order_before_activation() {
    let provider = format!(
        "{PROVIDER}\nclass Receiver {{\n\
        fun read(): Int = 0\n\
        inout fun mix(other: Receiver, own value: Int): Unit {{}}\n}}"
    );
    let (sources, ownership) = analyze(
        &provider,
        "fun test(own receiver: Receiver): Unit { receiver.mix(value = receiver.read(), other = receiver) }",
    );
    let [diagnostic] = ownership.diagnostics() else {
        panic!("one conflict expected")
    };
    assert_eq!(diagnostic.code().to_string(), "L0135");
    let argument = ownership
        .call_argument_contracts()
        .iter()
        .find(|argument| sources.slice(argument.argument_span()).unwrap() == "receiver")
        .unwrap();
    assert_eq!(diagnostic.primary_span(), argument.argument_span());
    assert!(ownership.receiver_facts().is_empty());
}

#[test]
fn failed_assignment_rolls_back_inner_activation_and_retains_outer_reservation() {
    let (_, ownership) = analyze(
        PROVIDER,
        "fun test(own worker: Worker, own other: Worker, flag: Boolean): Unit {\n\
            val fixed = 0\n\
            worker.set(if (flag) { fixed = other.bump()\nworker.read() } else { 0 })\n}",
    );
    let codes: Vec<_> = ownership
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect();
    assert_eq!(
        codes,
        ["L0134"],
        "a rejected assignment must not activate the outer receiver early"
    );
    assert!(ownership.receiver_facts().is_empty());
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.drops().is_empty());
    assert!(ownership.validate().is_err());
}

#[test]
fn borrowed_this_argument_conflicts_with_receiver_activation() {
    for call in [
        "update(this)",
        "this.update(this)",
        "update((this))",
        "(this).update((this))",
    ] {
        let provider = format!(
            "class Counter {{\n\
            inout fun update(other: Counter): Unit {{}}\n\
            inout fun test(): Unit {{ {call} }}\n}}"
        );
        let (sources, ownership) = analyze(&provider, "fun entry(): Unit {}");
        let [diagnostic] = ownership.diagnostics() else {
            panic!("one conflict expected for {call}")
        };
        assert_eq!(diagnostic.code().to_string(), "L0135", "{call}");
        let contract = ownership
            .call_receiver_contracts()
            .iter()
            .find(|contract| sources.slice(contract.call_span()).unwrap() == call)
            .unwrap();
        let argument = ownership
            .call_argument_contracts()
            .iter()
            .find(|argument| argument.call() == contract.call())
            .unwrap();
        assert_eq!(
            diagnostic.primary_span(),
            argument.argument_span(),
            "{call}"
        );
        let label = diagnostic
            .details()
            .iter()
            .find_map(|detail| match detail {
                DiagnosticDetail::Label(label) => Some(label),
                _ => None,
            })
            .unwrap();
        assert_eq!(label.span(), contract.receiver_span(), "{call}");
        assert!(ownership.receiver_facts().is_empty());
        assert!(ownership.loans().is_empty());
        assert!(ownership.validate().is_err());
    }
}

#[test]
fn readonly_outer_receiver_cannot_reserve_mutable_field_receiver() {
    for expression in ["child", "this.child", "branch.child", "this.branch.child"] {
        let provider = format!(
            "{PROVIDER}\nclass Branch(val child: Worker) {{}}\n\
            class Host(val child: Worker, val branch: Branch) {{\n\
                fun bad(): Unit {{ {expression}.set({expression}.read()) }}\n}}"
        );
        let (_, ownership) = analyze(&provider, "fun entry(): Unit {}");
        let codes: Vec<_> = ownership
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(codes, ["L0134"], "{expression}");
        assert!(ownership.receiver_facts().is_empty());
        let provider = provider.replace("fun bad()", "inout fun good()");
        let (_, ownership) = analyze(&provider, "fun entry(): Unit {}");
        assert_valid(&ownership);
    }
}

#[test]
fn moving_this_during_reservation_reports_conflict_before_nonowning_move() {
    for expression in ["this", "(this)"] {
        let provider = format!(
            "class Counter {{\n\
            inout fun set(own value: Int): Unit {{}}\n\
            inout fun test(): Unit {{ set(consume({expression})) }}\n}}\n\
            fun consume(own counter: Counter): Int = 0"
        );
        let (_, ownership) = analyze(&provider, "fun entry(): Unit {}");
        let codes: Vec<_> = ownership
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(codes, ["L0135"], "{expression}");
        assert!(ownership.receiver_facts().is_empty());
    }
}

#[test]
fn completed_nested_borrow_of_this_ends_before_receiver_activation() {
    let provider = "class Counter {\n\
        inout fun set(own value: Int): Unit {}\n\
        inout fun test(): Unit { set(inspect((this))) }\n}\n\
        fun inspect(counter: Counter): Int = 0";
    let (_, ownership) = analyze(provider, "fun entry(): Unit {}");
    assert_valid(&ownership);
    assert!(ownership.loans().iter().any(|loan| matches!(
        loan.target(),
        lang_frontend::ownership_checking::UnitLoanTarget::This(_)
    )));
    let fact = ownership
        .receiver_facts()
        .iter()
        .find(|fact| fact.is_receiver_reservation())
        .unwrap();
    assert_eq!(fact.activation_point(), Some(fact.call()));
}

#[test]
fn branch_selected_capture_cannot_hide_shared_loan_from_activation() {
    for operand in [
        "if (flag) ({ val n = cell.read() }) else ({})",
        "when { flag -> ({ val n = cell.read() })\nelse -> ({}) }",
    ] {
        let provider = "class Cell(var n: Int) {\n\
            fun read(): Int = n\n\
            inout fun set(action: () -> Unit): Unit {}\n}";
        let (_, ownership) = analyze(
            provider,
            &format!("fun test(own cell: Cell, flag: Boolean): Unit {{ cell.set({operand}) }}"),
        );
        let codes: Vec<_> = ownership
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(codes, ["L0135"], "{operand}");
        assert!(ownership.receiver_facts().is_empty());
        assert!(ownership.loans().is_empty());
        assert!(ownership.validate().is_err());
    }
}

#[test]
fn moving_inout_parameter_during_reservation_reports_conflict() {
    let (_, ownership) = analyze(
        PROVIDER,
        "fun test(inout worker: Worker): Unit { worker.set(consume(worker)) }",
    );
    let codes: Vec<_> = ownership
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect();
    assert_eq!(codes, ["L0135"]);
    assert!(ownership.receiver_facts().is_empty());
}

#[test]
fn reservation_conflict_precedes_consuming_this_receiver_capability_error() {
    for call in ["finish()", "this.finish()"] {
        let provider = format!(
            "class Counter {{\n\
            inout fun set(own value: Int): Unit {{}}\n\
            own fun finish(): Int = 0\n\
            inout fun test(): Unit {{ set({call}) }}\n}}"
        );
        let (_, ownership) = analyze(&provider, "fun entry(): Unit {}");
        let codes: Vec<_> = ownership
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(codes, ["L0135"], "{call}");
        assert!(ownership.receiver_facts().is_empty());
    }
    let provider = "class Counter { inout fun test(): Int = consume(this) }\n\
        fun consume(own counter: Counter): Int = 0";
    let (_, ownership) = analyze(provider, "fun entry(): Unit {}");
    let codes: Vec<_> = ownership
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect();
    assert_eq!(
        codes,
        ["L0133"],
        "without a reservation, retain non-owning move diagnostics"
    );
}

#[test]
fn completed_nested_call_ends_branch_temporary_captures_before_activation() {
    for operand in [
        "if (flag) ({ val n = cell.read() }) else ({})",
        "when { flag -> ({ val n = cell.read() })\nelse -> ({}) }",
    ] {
        let provider = "class Cell(var n: Int) {\n\
            fun read(): Int = n\n\
            inout fun set(own value: Int): Unit {}\n}\n\
            fun inspect(action: () -> Unit): Int = 0";
        let (_, ownership) = analyze(
            provider,
            &format!(
                "fun test(own cell: Cell, flag: Boolean): Unit {{ cell.set(inspect({operand})) }}"
            ),
        );
        assert_valid(&ownership);
        let fact = ownership
            .receiver_facts()
            .iter()
            .find(|fact| fact.is_receiver_reservation())
            .unwrap();
        assert_eq!(fact.activation_point(), Some(fact.call()));
    }
}
