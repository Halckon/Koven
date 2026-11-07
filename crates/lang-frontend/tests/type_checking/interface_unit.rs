//! 原接口边、overload 与 receiver/override 诊断的双 Unit raw type 消费者。
use super::{codes, unit_entries::checked_entries};

#[test]
fn direct_interface_edges_reject_classes_duplicates_and_cycles() {
    let (_, _, entries) = checked_entries("class Parent {}\nclass Child : Parent {}");
    for (_, non_interface) in entries {
        assert_eq!(codes(non_interface.diagnostics()), ["L0095"]);
    }
    let (_, _, entries) =
        checked_entries("interface Protocol<T> {}\nclass Both : Protocol<Int>, Protocol<Long> {}");
    for (_, duplicate) in entries {
        assert_eq!(codes(duplicate.diagnostics()), ["L0095"]);
    }
    let (_, _, entries) = checked_entries("interface Left : Right {}\ninterface Right : Left {}");
    for (_, cycle) in entries {
        assert_eq!(codes(cycle.diagnostics()), ["L0096"]);
    }
}

#[test]
fn overload_shape_is_alpha_equivalent_and_concrete_members_need_bodies() {
    let text = "fun <T> pick(input: T): Int = 1\n\
                fun <U> pick(other: U): Long = 1L\n\
                fun mode(input: Int): Unit {}\n\
                fun mode(borrow other: Int): Unit {}\n\
                fun mode(own consumed: Int): Unit {}\n\
                interface Contract { fun required(input: Int): Unit }\n\
                class Concrete { fun missing(input: Int): Unit; fun okay(): Unit {} }";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(
            codes(typed.diagnostics()),
            ["L0097", "L0097", "L0097", "L0098"]
        );
    }
}

#[test]
fn receiver_modes_are_contracts_but_not_overload_shapes() {
    let (_, _, entries) = checked_entries(
        "class Duplicate {\n\
             fun collide(input: Int): Unit {}\n\
             inout fun collide(input: Int): Unit {}\n\
         }",
    );
    for (_, overload) in entries {
        assert_eq!(codes(overload.diagnostics()), ["L0097"]);
    }
    let (_, _, entries) = checked_entries(
        "interface Base { fun act(): Unit }\n\
         interface Child : Base { inout fun act(): Unit }",
    );
    for (_, replacement) in entries {
        assert_eq!(codes(replacement.diagnostics()), ["L0099"]);
    }
    let (_, _, entries) = checked_entries(
        "interface Required { fun run(): Unit }\n\
         class Bad : Required { override inout fun run(): Unit {} }",
    );
    for (_, implementation) in entries {
        assert_eq!(codes(implementation.diagnostics()), ["L0100"]);
    }
}

#[test]
fn interface_replacements_and_overrides_require_exact_contracts() {
    let (_, _, entries) = checked_entries(
        "interface Base { fun act(input: Int): Int }\n\
         interface Child : Base { fun act(input: Int): Long }",
    );
    for (_, replacement) in entries {
        assert_eq!(codes(replacement.diagnostics()), ["L0099"]);
    }
    let text = "interface Required { fun run(input: Int): Int }\n\
                class Missing : Required {}\n\
                class NeedsOverride : Required { fun run(input: Int): Int = 1 }\n\
                class BadReturn : Required { override fun run(input: Int): Long = 1L }\n\
                class BadMode : Required { override fun run(own input: Int): Int = 1 }\n\
                class Extra { override fun lone(): Unit {} }\n\
                class Hidden : Required { private override fun run(input: Int): Int = 1 }\n\
                class Good : Required { override fun run(borrow input: Int): Int = 1 }";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(
            codes(typed.diagnostics()),
            ["L0101", "L0100", "L0100", "L0100", "L0100", "L0100"]
        );
    }
}

#[test]
fn abstract_requirements_accept_one_default_but_multiple_defaults_conflict() {
    let text = "interface Abstract { fun ping(): Int }\n\
                interface Left { fun ping(): Int = 1 }\n\
                interface Right { fun ping(): Int = 2 }\n\
                interface Child : Left { fun ping(): Int = 3 }\n\
                class Covered : Abstract, Left {}\n\
                class Shadowed : Child {}\n\
                class Conflict : Left, Right {}\n\
                class Resolved : Left, Right { override fun ping(): Int = 3 }";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0102"]);
    }
}
