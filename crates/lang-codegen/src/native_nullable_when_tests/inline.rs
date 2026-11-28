use super::*;

#[test]
fn inline_nullable_when_evaluates_once_and_reads_only_the_non_null_payload() {
    let text = r#"fun intValue(x: Int?): Int = when ((x)) { null -> 0; else -> x }
fun boolValue(x: Boolean?): Int = when (x) {
    null -> 0
    else -> if (x) 1 else 2
}
fun produce(present: Boolean): Int? {
    println("subject")
    if (present) { return 7 }
    return null
}
fun temporary(present: Boolean): Int = when (produce(present)) { null -> 0; else -> 1 }
fun onlyLive(x: Int?): Int {
    when (x) { null -> { return 0 }; else -> {} }
    return x
}
fun writeback(input: Int?): Int {
    var x = input
    when (x) {
        null -> {}
        else -> { if (x == 7) { println("narrowed") }; x = null }
    }
    return if (x == null) 1 else 0
}
fun add(own n: Int): Int = n + 1
fun read(n: Int): Int = n + 2
fun call(x: Int?): Int = when (x) { null -> 0; else -> add(x) + read((x)) }
fun clear(inout x: Int?): Int = when ((x)) {
    null -> 0
    else -> { val current = x; x = null; current }
}
fun entry(): Unit {
    if (intValue(null) == 0 && intValue(7) == 7) { println("int") }
    if (boolValue(null) == 0 && boolValue(true) == 1 && boolValue(false) == 2) { println("bool") }
    if (temporary(true) == 1 && temporary(false) == 0) { println("temporary") }
    if (onlyLive(7) == 7 && onlyLive(null) == 0) { println("only-live") }
    if (writeback(7) == 1 && writeback(null) == 1) { println("writeback") }
    if (call(null) == 0 && call(7) == 17) { println("calls") }
    var edited: Int? = 7
    if (clear(&edited) == 7 && clear(&edited) == 0) { println("inout") }
}
"#;
    const PATH: &str = "inline-nullable-when.ko";
    let source = UnitSource::analyze(PATH, text).expect("analysis");
    source.assert_frontend_clean();
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("inline nullable when lowers through Refine");
    verify_program(&program).expect("inline nullable SSA verifies");

    for name in [
        "intValue",
        "boolValue",
        "temporary",
        "onlyLive",
        "writeback",
        "call",
        "clear",
    ] {
        let function = function(&source, &program, name);
        assert!(routed.contains(&function.name), "{name} must refine");
        assert!(function.blocks.iter().any(|block| matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::NullableBranch { .. })
        )));
    }
    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"int\nbool\nsubject\nsubject\ntemporary\nonly-live\nnarrowed\nwriteback\ncalls\ninout\n",
    );
}

#[test]
fn inline_nullable_when_restores_an_outer_view_for_the_other_arm() {
    let text = r#"class Holder(val value: Int)
fun sample(own input: Holder?, flag: Int?): Int {
    var current = input
    if (current != null) {
        return when (flag) {
            null -> { current = null; 0 }
            else -> current.value
        }
    }
    return 0
}
fun entry(): Unit {
    if (sample(Holder(7), 1) == 7 && sample(Holder(7), null) == 0) { println("outer-view") }
}
"#;
    const PATH: &str = "inline-nullable-when-outer-view.ko";
    let source = UnitSource::analyze(PATH, text).expect("analysis");
    source.assert_frontend_clean();
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("both arms lower against their own incoming views");
    assert!(routed.contains(&function(&source, &program, "sample").name));
    verify_program(&program).expect("inline nullable SSA verifies");
    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(&output, b"outer-view\n");
}

#[test]
fn nullable_when_unwraps_map_results_only_on_the_present_edge() {
    let text = r#"fun lookup(m: MutableMap<Int, Int>, key: Int): Int {
    val found = m[key]
    return when (found) { null -> 0; else -> found }
}
fun entry(): Unit {
    var m = mutableMapOf<Int, Int>()
    m.put(7, 9)
    if (lookup(m, 7) == 9 && lookup(m, 8) == 0) { println("map") }
}
"#;
    const PATH: &str = "map-result-nullable-when.ko";
    let source = UnitSource::analyze(PATH, text).expect("analysis");
    source.assert_frontend_clean();
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("nullable Map result lowers through Refine");
    verify_program(&program).expect("nullable Map result SSA verifies");
    let lookup = function(&source, &program, "lookup");
    assert!(routed.contains(&lookup.name));
    let (present, missing) = lookup
        .blocks
        .iter()
        .find_map(|block| {
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = &block.terminator.as_ref()?.kind
            else {
                return None;
            };
            block
                .instructions
                .iter()
                .map(|&id| lookup.instruction(id).unwrap())
                .find_map(|inst| match inst.operation {
                    Operation::AggregateProject { field: 0, .. }
                        if inst.results == [EntityId::Value(*condition)] =>
                    {
                        Some((when_true.target, when_false.target))
                    }
                    _ => None,
                })
        })
        .expect("Map result presence drives the dispatch");
    // SSA emission can insert a forwarding block on each successor edge.
    let unwraps = |mut target| {
        for _ in &lookup.blocks {
            let block = lookup.block(target).unwrap();
            if block
                .instructions
                .iter()
                .map(|&id| lookup.instruction(id).unwrap())
                .any(|inst| matches!(inst.operation, Operation::MapResultUnwrap { .. }))
            {
                return true;
            }
            let Some(TerminatorKind::Branch(next)) =
                block.terminator.as_ref().map(|term| &term.kind)
            else {
                return false;
            };
            target = next.target;
        }
        panic!("acyclic lookup fixture");
    };
    assert!(unwraps(present));
    assert!(!unwraps(missing));
    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(&output, b"map\n");
}

#[test]
fn nullable_boolean_when_covers_all_three_values_and_grouped_selectors() {
    let text = r#"fun bit(x: Boolean): Int = if (x) 1 else 2
fun ordered(x: Boolean?): Int = when (x) {
    true -> 1
    null -> 0
    false -> 2
}
fun grouped(x: Boolean?): Int = when ((x)) {
    (true), false -> bit((x))
    null -> 0
}
fun mixed(x: Boolean?): Int = when (x) {
    null, true -> if (x == null) 1 else 2
    false -> if (x) 2 else 3
}
fun remaining(x: Boolean?): Int {
    when (x) { null -> { return 0 }; true, false -> {} }
    return bit(x)
}
fun produce(x: Boolean?): Boolean? { println("subject"); return x }
fun temporary(x: Boolean?): Int = when (produce(x)) {
    false -> 2
    true -> 1
    null -> 0
}
fun entry(): Unit {
    if (ordered(null) == 0 && ordered(true) == 1 && ordered(false) == 2) { println("ordered") }
    if (grouped(null) == 0 && grouped(true) == 1 && grouped(false) == 2) { println("grouped") }
    if (mixed(null) == 1 && mixed(true) == 2 && mixed(false) == 3) { println("mixed") }
    if (remaining(null) == 0 && remaining(true) == 1 && remaining(false) == 2) { println("remaining") }
    if (temporary(null) == 0 && temporary(true) == 1 && temporary(false) == 2) { println("temporary") }
}
"#;
    const PATH: &str = "nullable-boolean-when.ko";
    let source = UnitSource::analyze(PATH, text).expect("analysis");
    source.assert_frontend_clean();
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("exhaustive Boolean? when lowers through Refine");
    for name in ["ordered", "grouped", "mixed", "remaining", "temporary"] {
        assert!(routed.contains(&function(&source, &program, name).name));
    }
    verify_program(&program).expect("Boolean? when SSA verifies");
    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"ordered\ngrouped\nmixed\nremaining\nsubject\nsubject\nsubject\ntemporary\n",
    );
}

#[test]
fn nullable_when_rechecks_the_subject_after_loop_writeback() {
    let text = r#"fun entry(): Unit {
    var x: Boolean? = true
    if (x == null) { return }
    var i = 0
    while (i < 2) {
        when (x) {
            null -> println("null")
            else -> println("present")
        }
        x = null
        i = i + 1
    }
}
"#;
    let output =
        crate::native_tests::emit_link_and_run("nullable-when-loop-writeback.ko", text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(&output, b"present\nnull\n");
}

#[test]
fn nullable_boolean_when_ends_argument_loan_before_early_exit() {
    let text = r#"fun bit(x: Boolean, z: Int): Int = if (x) z else 0
class Resource(val value: Int) { deinit() { println("drop") } }
fun route(x: Boolean?, stop: Boolean): Int {
    val r = Resource(1)
    val selected = when ((x)) {
        true, (false) -> bit((x), if (stop) { return 9 } else { 7 })
        null -> 2
    }
    if (r.value == 1) { println("live") }
    return selected
}
fun entry(): Unit {
    if (route(true, true) == 9
        && route(true, false) == 7
        && route(false, false) == 0
        && route(null, true) == 2) { println("done") }
}
"#;
    let output = crate::native_tests::emit_link_and_run(
        "nullable-boolean-when-borrow-exit.ko",
        text,
        "entry",
    );
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"drop\nlive\ndrop\nlive\ndrop\nlive\ndrop\ndone\n",
    );
}

#[test]
fn borrowed_nullable_when_observes_the_original_storage() {
    let text = r#"class Leaf(val name: String) { deinit() { println("drop") } }
fun readRc(borrow owner: Rc<Leaf>): Unit { println(owner.value.name) }
fun readText(borrow text: String): Unit { if (text == "") println("empty") else println(text) }
fun inspectRc(borrow owner: Rc<Leaf>?, early: Boolean): Unit {
    when ((owner)) {
        null -> { println("null rc"); return }
        else -> { readRc((owner)); if (early) { return }; readRc(owner) }
    }
    println("after when")
}
fun inspectText(borrow text: String?): Unit {
    when (text) { null -> { println("null text") }; else -> { readText(text); readText((text)) } }
}
fun entry(): Unit {
    val present: Rc<Leaf>? = Rc(Leaf("leaf".clone()))
    inspectRc(null, false)
    inspectRc(present, false)
    inspectRc(present, true)
    inspectText(null)
    inspectText("")
    inspectText("text")
    println("done")
}
"#;
    const PATH: &str = "borrowed-nullable-when.ko";
    let source = UnitSource::analyze(PATH, text).expect("analysis");
    source.assert_frontend_clean();
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("borrowed nullable when lowers through Refine");
    verify_program(&program).expect("borrowed nullable SSA verifies");
    for name in ["inspectRc", "inspectText"] {
        let function = function(&source, &program, name);
        assert!(routed.contains(&function.name), "{name} must refine");
        let entry = function
            .block(function.entry_block().unwrap())
            .expect("entry block");
        let EntityId::Loan(parent) = entry.parameters[0] else {
            panic!("{name} must receive the original loan");
        };
        assert!(function.blocks.iter().any(|block| matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::NullableLoanBranch { source, .. }) if *source == parent
        )));
        assert!(function.instructions.iter().all(|inst| !matches!(
            inst.operation,
            Operation::NullableTake { .. }
                | Operation::SharedRetain { .. }
                | Operation::StringClone { .. }
        )));
    }
    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"null rc\nleaf\nleaf\nafter when\nleaf\nnull text\nempty\nempty\ntext\ntext\ndone\ndrop\n",
    );
}

#[test]
fn local_borrow_nullable_when_ends_its_proof_before_the_binding() {
    let text = r#"class Leaf(val text: String) { deinit() { println("drop") } }
fun view(borrow text: String?): borrow String? from text = text
fun show(borrow text: String): Unit { if (text == "") println("empty") else println(text) }
fun showRc(borrow owner: Rc<Leaf>): Unit { println(owner.value.text) }
fun run(borrow text: String?, early: Boolean): Unit {
    block {
        borrow val parent = view(text)
        borrow val local = parent
        when ((local)) {
            null -> { println("null text") }
            else -> { show(local); if (early) { return }; show((local)) }
        }
    }
    println("after text")
}
fun runRc(borrow owner: Rc<Leaf>?, early: Boolean): Unit {
    block {
        borrow val local = owner
        when (local) {
            null -> { println("null rc"); return }
            else -> { showRc(local); if (early) { return }; showRc(local) }
        }
    }
    println("after rc")
}
fun entry(): Unit {
    val owner: Rc<Leaf>? = Rc(Leaf("leaf".clone()))
    run(null, false)
    run("", false)
    run("text", true)
    runRc(null, false)
    runRc(owner, false)
    runRc(owner, true)
    println("done")
}
"#;
    const PATH: &str = "local-borrow-nullable-when.ko";
    let source = UnitSource::analyze(PATH, text).expect("analysis");
    source.assert_frontend_clean();
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("lexical borrowed nullable when lowers through Refine");
    verify_program(&program).expect("each proof ends before its lexical parent");
    for name in ["run", "runRc"] {
        let function = function(&source, &program, name);
        assert!(routed.contains(&function.name), "{name} must refine");
        let entry = function.block(function.entry_block().unwrap()).unwrap();
        assert!(function.blocks.iter().any(|block| matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::NullableLoanBranch { source, .. })
                if !entry.parameters.contains(&EntityId::Loan(*source))
        )));
    }
    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"null text\nafter text\nempty\nempty\nafter text\ntext\nnull rc\nleaf\nleaf\nafter rc\nleaf\ndone\ndrop\n",
    );
}

#[test]
fn narrowed_when_child_borrow_keeps_its_proof_alive_until_exit() {
    let text = r#"class Leaf(val name: String) { deinit() { println("drop") } }
fun view(borrow text: String): borrow String from text = text
fun viewRc(borrow owner: Rc<Leaf>): borrow Rc<Leaf> from owner = owner
fun show(borrow text: String): Unit { if (text == "") println("empty") else println(text) }
fun showRc(borrow owner: Rc<Leaf>): Unit { println(owner.value.name) }
fun run(borrow text: String?, call: Boolean, stop: Boolean): Unit {
    borrow val local = text
    when ((local)) {
        null -> { println("null text") }
        else -> {
            if (call) {
                borrow val child = view((local))
                show(child)
                if (stop) { return }
                show(child)
            } else {
                borrow val child = (local)
                show(child)
                if (stop) { return }
                show(child)
            }
        }
    }
    println("after text")
}
fun runRc(borrow owner: Rc<Leaf>?, call: Boolean, stop: Boolean): Unit {
    when (owner) {
        null -> { println("null rc"); return }
        else -> {
            if (call) {
                borrow val child = viewRc(owner)
                showRc(child)
                if (stop) { return }
            } else {
                borrow val child = owner
                showRc(child)
                if (stop) { return }
            }
        }
    }
    println("after rc")
}
fun entry(): Unit {
    val owner: Rc<Leaf>? = Rc(Leaf("leaf".clone()))
    run(null, false, false)
    run("", false, false)
    run("text", true, true)
    runRc(null, false, false)
    runRc(owner, false, false)
    runRc(owner, true, true)
    println("done")
}
"#;
    const PATH: &str = "narrowed-when-child-borrow.ko";
    let source = UnitSource::analyze(PATH, text).expect("analysis");
    source.assert_frontend_clean();
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("named child uses the active When proof");
    verify_program(&program).expect("named child ends before proof and nullable source");
    for name in ["run", "runRc"] {
        let function = function(&source, &program, name);
        assert!(routed.contains(&function.name), "{name} must refine");
    }
    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"null text\nafter text\nempty\nempty\nafter text\ntext\nnull rc\nleaf\nafter rc\nleaf\ndone\ndrop\n",
    );
}

#[test]
fn nullable_when_borrow_source_prefix_ends_before_an_argument_returns() {
    let text = r#"class Leaf(val name: String) { deinit() { println("deinit"); println(name) } }
fun view(borrow text: String, flag: Boolean): borrow String from text = text
fun viewRc(borrow owner: Rc<Leaf>, flag: Boolean): borrow Rc<Leaf> from owner = owner
fun show(borrow text: String): Unit { if (text == "") println("empty") else println(text) }
fun viewCase(borrow text: String?, stop: Boolean): Unit {
    borrow val local = text
    when ((local)) {
        null -> { println("null text") }
        else -> {
            borrow val child = view((local), if (stop) { return } else { true })
            show(child)
        }
    }
    println("after text")
}
fun ownedCase(own owner: Rc<Leaf>?, stop: Boolean): Unit {
    when (owner) {
        null -> { println("null rc"); return }
        else -> {
            borrow val child = viewRc(owner, if (stop) { return } else { true })
            println(child.value.name)
        }
    }
    println("after rc")
}
fun entry(): Unit {
    viewCase(null, false)
    viewCase("", false)
    viewCase("not read", true)
    println("after text calls")
    ownedCase(null, false)
    ownedCase(Rc(Leaf("early drop".clone())), true)
    println("after early")
    ownedCase(Rc(Leaf("normal drop".clone())), false)
    println("after normal")
}
"#;
    const PATH: &str = "nullable-when-borrow-source-prefix.ko";
    let source = UnitSource::analyze(PATH, text).expect("analysis");
    source.assert_frontend_clean();
    let ends = source.ownership().borrow_call_prefix_ends();
    assert_eq!(ends.len(), 2, "only the two evaluated source prefixes");
    assert!(ends.iter().all(|end| {
        matches!(end.point(), UnitDropPoint::ControlTransfer(_))
            && end.preceding_drops() == 0
            && end.preceding_call_loan_ends() == 0
    }));
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("later argument returns using its exact source loan end");
    verify_program(&program).expect("abandoned source loan ends before the nullable proof");
    for name in ["viewCase", "ownedCase"] {
        assert!(routed.contains(&function(&source, &program, name).name));
    }
    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"null text\nafter text\nempty\nafter text\nafter text calls\nnull rc\ndeinit\nearly drop\nafter early\nnormal drop\nafter rc\ndeinit\nnormal drop\nafter normal\n",
    );
}

#[test]
fn nullable_when_borrow_prefix_cleans_an_earlier_temporary_rc() {
    let text = r#"class Leaf(val name: String) { deinit() { println("drop"); println(name) } }
fun view(borrow extra: Rc<Leaf>, borrow owner: Rc<Leaf>, flag: Boolean): borrow Rc<Leaf> from owner = owner
fun run(own owner: Rc<Leaf>?, stop: Boolean): Unit {
    when (owner) {
        null -> { println("null"); return }
        else -> {
            borrow val child = view(Rc(Leaf("extra".clone())), owner, if (stop) { return } else { true })
            println("read")
            println(child.value.name)
        }
    }
    println("end run")
}
fun entry(): Unit {
    run(null, false)
    run(Rc(Leaf("early".clone())), true)
    println("after early")
    run(Rc(Leaf("normal".clone())), false)
    println("after normal")
}
"#;
    const PATH: &str = "nullable-when-borrow-prefix-resource.ko";
    let source = UnitSource::analyze(PATH, text).expect("analysis");
    source.assert_frontend_clean();
    let owned = source.ownership();
    let [prefix] = owned.borrow_call_prefix_ends() else {
        panic!("only the evaluated source has a prefix-end receipt");
    };
    let drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|drop| drop.point() == prefix.point())
        .map(|drop| drop.target())
        .collect();
    assert!(matches!(
        drops.as_slice(),
        [UnitDropTarget::Temporary(_), UnitDropTarget::Named(_)]
    ));
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("earlier resource argument and source borrow prefix lower");
    verify_program(&program).expect("prefix loans end before the temporary and original owner");
    assert!(routed.contains(&function(&source, &program, "run").name));
    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"null\ndrop\nextra\ndrop\nearly\nafter early\ndrop\nextra\nread\nnormal\nend run\ndrop\nnormal\nafter normal\n",
    );
}

#[test]
fn inout_nullable_rc_proofs_end_before_writeback() {
    let text = r#"class Leaf(val name: String) { deinit() { println("drop"); println(name) } }
fun read(borrow owner: Rc<Leaf>): Unit { println(owner.value.name) }
fun clear(inout owner: Rc<Leaf>?): Unit { owner = null }
fun guard(inout owner: Rc<Leaf>?, forward: Boolean): Unit {
    if (owner == null) { println("empty"); return }
    read(owner)
    read((owner))
    if (forward) { clear(&owner) } else { owner = null }
    if (owner == null) { println("cleared") }
}
fun select(inout owner: Rc<Leaf>?): Unit {
    when ((owner)) {
        null -> { println("empty when"); return }
        else -> { read(owner); owner = null }
    }
    if (owner == null) { println("cleared when") }
}
fun entry(): Unit {
    var owner: Rc<Leaf>? = null
    guard(&owner, false)
    select(&owner)
    owner = Rc(Leaf("direct".clone()))
    guard(&owner, false)
    owner = Rc(Leaf("forwarded".clone()))
    guard(&owner, true)
    owner = Rc(Leaf("when".clone()))
    select(&owner)
    println("done")
}
"#;
    const PATH: &str = "inout-nullable-rc-proof.ko";
    let source = UnitSource::analyze(PATH, text).expect("analysis");
    source.assert_frontend_clean();
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("inout Rc? tests and writeback lower through Refine");
    verify_program(&program).expect("exclusive source stays live and has no child at mutation");
    for name in ["guard", "select"] {
        let function = function(&source, &program, name);
        assert!(routed.contains(&function.name));
        assert!(function.blocks.iter().any(|block| matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::NullableLoanBranch { .. })
        )));
    }
    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"empty\nempty when\ndirect\ndirect\ndrop\ndirect\ncleared\nforwarded\nforwarded\ndrop\nforwarded\ncleared\nwhen\ndrop\nwhen\ncleared when\ndone\n",
    );
}

#[test]
fn inout_nullable_rc_short_circuit_retires_the_skipped_proof() {
    let text = r#"class Leaf(val name: String) { deinit() { println("drop") } }
fun read(borrow owner: Rc<Leaf>): Unit { println(owner.value.name) }
fun clear(inout owner: Rc<Leaf>?): Boolean { owner = null; return true }
fun guard(inout owner: Rc<Leaf>?, forward: Boolean): Unit {
    if (owner == null) { return }
    read(owner)
    if (forward && clear(&owner)) { println("forward") }
    if (owner == null) { println("cleared") }
}
fun entry(): Unit {
    var owner: Rc<Leaf>? = Rc(Leaf("leaf".clone()))
    guard(&owner, false)
    guard(&owner, true)
}
"#;
    for (path, clause) in [
        (
            "inout-nullable-rc-short-circuit.ko",
            "if (forward && clear(&owner))",
        ),
        (
            "inout-nullable-rc-short-circuit-value.ko",
            "val selected = forward && (clear(&owner) || true); if (selected)",
        ),
    ] {
        let text = text.replace("if (forward && clear(&owner))", clause);
        let output = crate::native_tests::emit_link_and_run(path, &text, "entry");
        crate::native_tests::boxed_enum_tests::assert_success(
            &output,
            b"leaf\nleaf\ndrop\nforward\ncleared\n",
        );
    }
}

#[test]
fn inout_nullable_rc_share_and_projection_use_the_current_owner() {
    let text = r#"class Leaf(val name: String) { deinit() { println("drop"); println(name) } }
fun inspect(inout owner: Rc<Leaf>?): Unit {
    if (owner != null) {
        println(owner.value.name)
        owner = owner.share()
    }
    when (owner) {
        null -> { println("null") }
        else -> {
            val saved = owner.share()
            println(owner.value.name)
            owner = null
            println(saved.value.name)
        }
    }
}
fun entry(): Unit {
    var owner: Rc<Leaf>? = null
    inspect(&owner)
    owner = Rc(Leaf("leaf".clone()))
    inspect(&owner)
    if (owner == null) { println("cleared") }
}
"#;
    let output =
        crate::native_tests::emit_link_and_run("inout-nullable-rc-operations.ko", text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"null\nleaf\nleaf\nleaf\ndrop\nleaf\ncleared\n",
    );
}

#[test]
fn inout_nullable_rc_borrow_result_prefix_preserves_the_slot_on_return() {
    let text = r#"class Leaf(val name: String) { deinit() { println("drop") } }
fun read(borrow owner: Rc<Leaf>): Unit { println(owner.value.name) }
fun view(borrow owner: Rc<Leaf>, flag: Boolean): borrow Rc<Leaf> from owner = owner
fun inspect(inout owner: Rc<Leaf>?, stop: Boolean): Unit {
    when (owner) {
        null -> {}
        else -> {
            block {
                borrow val child = view(owner, if (stop) { return } else { true })
                read(child)
            }
            owner = null
        }
    }
}
fun entry(): Unit {
    var owner: Rc<Leaf>? = Rc(Leaf("leaf".clone()))
    inspect(&owner, true)
    println("after early")
    inspect(&owner, false)
    if (owner == null) { println("cleared") }
}
"#;
    let output =
        crate::native_tests::emit_link_and_run("inout-nullable-rc-borrow-prefix.ko", text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"after early\nleaf\ndrop\ncleared\n",
    );
}

#[test]
fn inout_nullable_string_reads_and_replaces_the_current_slot() {
    let text = r#"fun read(borrow text: String): Unit {
    if (text == "") { println("empty") } else { println(text) }
}
fun clear(inout owner: String?): Unit { owner = null }
fun inspect(inout owner: String?): Unit {
    if (owner != null) {
        read(owner)
        owner = owner.clone()
    }
    when (owner) {
        null -> { println("null") }
        else -> {
            read(owner)
            read(owner)
            clear(&owner)
        }
    }
    if (owner == null) { println("cleared") }
}
fun entry(): Unit {
    var owner: String? = null
    inspect(&owner)
    owner = "".clone()
    inspect(&owner)
    owner = "text".clone()
    inspect(&owner)
}
"#;
    let output = crate::native_tests::emit_link_and_run("inout-nullable-string.ko", text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"null\ncleared\nempty\nempty\nempty\ncleared\ntext\ntext\ntext\ncleared\n",
    );
}

#[test]
fn borrowed_nullable_when_distinguishes_empty_and_reuses_the_string_proof() {
    let text = r#"fun read(borrow owner: String?): Unit {
    when (owner) {
        null -> { println("null") }
        else -> {
            if ((owner) == "") { println("empty") }
            if ("text" != (owner)) { println("different") }
            println(((owner)))
            val saved = (owner).clone()
            println(saved)
        }
    }
}
fun replace(inout owner: String?): Unit {
    when ((owner)) {
        null -> {}
        else -> {
            owner = (owner).clone()
            if (owner != null) { println(owner) }
        }
    }
}
fun entry(): Unit {
    var owner: String? = null
    read(owner)
    owner = "".clone()
    read(owner)
    owner = "text".clone()
    read(owner)
    replace(&owner)
    read(owner)
}
"#;
    let output =
        crate::native_tests::emit_link_and_run("nullable-when-string-builtins.ko", text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"null\nempty\ndifferent\n\n\ntext\ntext\ntext\ntext\ntext\n",
    );
}

#[test]
fn nullable_string_equality_reads_both_operands_without_moving_them() {
    let text = r#"fun compare(borrow a: String?, borrow b: String?): Unit {
    if (a == b) { println("same") } else { println("different") }
    if (a == "") { println("left empty") }
    if ("" == b) { println("right empty") }
    if (a != "x") { println("left not x") }
}
fun clear(inout slot: String?): String { slot = null; return "x".clone() }
fun inspect(inout slot: String?): Unit {
    when (slot) {
        null -> {}
        else -> {
            if (clear(&slot) == slot) { println("wrong") } else { println("cleared") }
        }
    }
}
fun make(text: String): String? = text.clone()
fun early(): Boolean = make("released") == (return true)
fun entry(): Unit {
    var a: String? = null
    var b: String? = null
    compare(a, b)
    a = "".clone()
    compare(a, b)
    b = "".clone()
    compare(a, b)
    a = "x".clone()
    b = "x".clone()
    compare(a, b)
    if ((a) == (b)) { println("owned same") }
    if (make("N\0é") == make("N\0é")) { println("temporary same") }
    if (make("abc") != make("abd")) { println("different bytes") }
    inspect(&a)
    if (a == null) { println("null after") }
    if (early()) { println("early") }
}
"#;
    let expected = b"same\nleft not x\ndifferent\nleft empty\nleft not x\nsame\nleft empty\nright empty\nleft not x\nsame\nowned same\ntemporary same\ndifferent bytes\ncleared\nnull after\nearly\n";
    let output =
        crate::native_tests::emit_link_and_run("nullable-string-equality.ko", text, "entry");
    let test_support = crate::native_tests::boxed_enum_tests::assert_success;
    test_support(&output, expected);
    let llvm =
        crate::native_tests::boxed_enum_tests::lower_to_llvm("nullable-string-equality.ko", text);
    let counted = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 8);
    test_support(&counted, expected);
}

#[test]
fn nullable_string_equality_uses_map_presence_without_consuming_the_payload() {
    let text = r#"fun entry(): Unit {
    var map = mutableMapOf<Int, String>()
    map.put(1, "value".clone())
    map.put(2, "".clone())
    map.put(3, "other".clone())
    if (map.remove(1) == "value") { println("hit") }
    if ("" == map.remove(2)) { println("empty") }
    if (map.remove(4) != "") { println("absent") }
    if (map.remove(5) == map.remove(6)) { println("both absent") }
    val saved = map.remove(3)
    if (saved == "other" && "other" == saved) { println("kept") }
}
"#;
    let output = crate::native_tests::emit_link_and_run("map-string-equality.ko", text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"hit\nempty\nabsent\nboth absent\nkept\n",
    );
}

#[test]
fn nullable_string_field_presence_reads_the_original_slot() {
    let text = r#"class Leaf(val label: String?) {
    deinit() { println("drop leaf") }
}
class Parent(val leaf: Leaf) {
    deinit() { println("drop parent") }
}
fun inspect(borrow holder: Parent): Unit {
    if (null == ((holder.leaf).label)) { println("null") }
    if ((holder.leaf.label) != null) { println("present") }
}
fun entry(): Unit {
    inspect(Parent(Leaf(null)))
    inspect(Parent(Leaf("".clone())))
    inspect(Parent(Leaf("text".clone())))
    val local = Leaf(null)
    if (local.label == null) { println("local") }
}
"#;
    use crate::native_tests::boxed_enum_tests::{
        assert_success, lower_to_llvm, run_counted_allocations,
    };
    let expected = b"null\ndrop parent\ndrop leaf\npresent\ndrop parent\ndrop leaf\npresent\ndrop parent\ndrop leaf\nlocal\ndrop leaf\n";
    let name = "nullable-string-field-presence.ko";
    let output = crate::native_tests::emit_link_and_run(name, text, "entry");
    assert_success(&output, expected);
    let counted = run_counted_allocations(&lower_to_llvm(name, text), 8);
    assert_success(&counted, expected);
}

#[test]
fn nullable_string_field_presence_releases_the_inout_receiver_before_writeback() {
    let text = r#"class Cell(var label: String?)
fun inspect(inout c: Cell): Unit {
    if (c.label != null) { println("present") }
    c.label = null
    if (null == c.label) { println("cleared") }
    c.label = "new".clone()
    if (c.label != null) { println("rewritten") }
}
fun entry(): Unit {
    var c = Cell("".clone())
    inspect(&c)
}
"#;
    let output =
        crate::native_tests::emit_link_and_run("nullable-string-field-writeback.ko", text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(
        &output,
        b"present\ncleared\nrewritten\n",
    );
}

#[test]
fn nullable_string_field_presence_drops_a_temporary_parent_before_the_branch() {
    let text = r#"class Leaf(val label: String?) {
    deinit() { println("drop leaf") }
}
class Parent(val leaf: Leaf) {
    deinit() { println("drop parent") }
}
fun make(): Parent {
    println("make")
    return Parent(Leaf("".clone()))
}
fun inspect(own p: Parent): Boolean = p.leaf.label != null
class OptionalField<T>(val label: T?) {
    fun empty(): Boolean = OptionalField<T>(null).label == null
}
fun entry(): Unit {
    if (inspect(Parent(Leaf("".clone())))) { println("owned") }
    if ((make()).leaf.label != null) { println("temporary") }
    val generic = OptionalField<String>("".clone())
    if (generic.empty()) { println("generic temporary null") }
    if (generic.label != null) { println("generic local present") }
}
"#;
    use crate::native_tests::boxed_enum_tests::{
        assert_success, lower_to_llvm, run_counted_allocations,
    };
    let output = run_counted_allocations(&lower_to_llvm("nullable-string-field-temp.ko", text), 6);
    assert_success(
        &output,
        b"drop parent\ndrop leaf\nowned\nmake\ndrop parent\ndrop leaf\ntemporary\ngeneric temporary null\ngeneric local present\n",
    );
}
