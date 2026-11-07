//! 原构造推导、trial 回滚、源码遮蔽与错误诊断的双入口消费者。
use super::{checked_entries, codes};

#[test]
fn invalid_and_underconstrained_constructions_fail_without_typed_facts() {
    let text = "interface Contract
                enum class Choice { One }
                class Marker<T> {}
                fun invalidTargets(): Unit {
                    val contract = Contract()
                    val choice = Choice()
                }
                fun missing(): Unit { val result = Marker() }";
    let (sources, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0143", "L0143", "L0144"]);
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .expect("span"),
            "Contract"
        );
        assert!(typed.constructions().is_empty());
    }
}

#[test]
fn uninstantiated_outer_type_parameters_do_not_complete_constructor_inference() {
    let text = "class Marker<T> {}
                fun <T> invalid(): Marker<T> = Marker()";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0144"]);
        assert!(typed.constructions().is_empty());
    }
}

#[test]
fn constructor_overload_trials_commit_only_the_unique_nested_fact() {
    let text = "class Marker<T> {}
                fun choose(action: () -> Marker<Int>): Int = 1
                fun choose(action: () -> Marker<Long>): Long = 1L
                fun selected(): Int = choose({ Marker<Int>() })";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.constructions().len(), 1);
        assert_eq!(
            typed.constructions()[0].instance().type_arguments().len(),
            1
        );
    }
}

#[test]
fn candidate_local_expected_does_not_infer_nested_constructor_arguments() {
    let text = "class Marker<T> {}
                fun choose(action: () -> Marker<Int>): Int = 1
                fun choose(action: () -> Marker<Long>): Long = 1L
                fun rejected(): Unit { val result = choose({ Marker() }) }";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0123"]);
        assert!(typed.constructions().is_empty());
    }
}

#[test]
fn source_box_name_keeps_nominal_constructor_identity() {
    let text = "class Box<T>(val item: T)
                fun source(): Box<Int> = Box(1)";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(matches!(
            typed.constructions()[0].target(),
            lang_frontend::type_checking::UnitConstructionTarget::Nominal(_)
        ));
    }
}

#[test]
fn source_rc_name_never_gains_intrinsic_construction_or_share_facts() {
    let text = "class Rc<T>(val item: T) {
                    fun share(): Rc<T> = Rc(item)
                }
                fun source(): Rc<Int> {
                    val owner = Rc(1)
                    return owner.share()
                }";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(typed.constructions().iter().all(|construction| matches!(
            construction.target(),
            lang_frontend::type_checking::UnitConstructionTarget::Nominal(_)
        )));
        assert!(typed.rc_operations().is_empty());
        assert!(typed.calls().iter().any(|call| matches!(
            call.instance().target(),
            lang_frontend::type_checking::UnitCallTarget::Symbol(_)
        )));
    }
}

#[test]
fn construction_reuses_named_arity_mode_and_type_diagnostics() {
    let text = "class Pair(val first: Int, val second: Int)
                fun named(): Unit { val result = Pair(missing = 1, second = 2) }
                fun arity(): Unit { val result = Pair(1) }
                fun mode(): Unit { val result = Pair(&1, 2) }
                fun typed(): Unit { val result = Pair(true, 2) }";
    let (sources, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(
            codes(typed.diagnostics()),
            ["L0120", "L0121", "L0122", "L0084"]
        );
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .expect("named span"),
            "missing"
        );
        assert!(typed.constructions().is_empty());
    }
}

#[test]
fn construction_reuses_capability_bound_diagnostics() {
    let text = "class Resource {}
                class NeedsCopy<T: Copyable> {}
                fun invalid(): Unit { val result = NeedsCopy<Resource>() }";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0115"]);
        assert!(typed.constructions().is_empty());
    }
}
