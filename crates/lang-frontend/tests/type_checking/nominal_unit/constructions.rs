//! 原 nullable adaptation、bare case 与 expected construction 消费者。
use super::super::codes;
use super::checked_entries;
use lang_frontend::type_checking::UnitConstructionTarget;

#[test]
fn nominal_box_and_rc_values_adapt_to_their_nullable_types() {
    let text = "class Node {}\n\
                value class Token(val item: Int)\n\
                fun accepted(): Unit {\n\
                    val node: Node? = Node()\n\
                    val boxed: Box<Token>? = Box(Token(1))\n\
                    val owner: Rc<Int>? = Rc(2)\n\
                }";
    let (_, _, entries) = checked_entries(text);
    for (_, _, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.constructions().len(), 4);
    }
}

#[test]
fn bare_enum_cases_publish_zero_operand_construction_facts() {
    let text = "enum class Flag { On, Off }
                enum class Maybe<T> { Some(item: T), None }
                fun flag(): Flag = Flag.On
                fun none(): Maybe<Int> = Maybe.None";
    let (_, _, entries) = checked_entries(text);
    for (source, names, typed) in entries {
        let [local_names] = names.names().source_units() else {
            panic!("one original source");
        };
        assert_eq!(local_names.source_unit(), source);

        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.constructions().len(), 2);
        assert_eq!(typed.constructions()[0].arguments(), []);
        assert_eq!(typed.constructions()[1].arguments(), []);
        let UnitConstructionTarget::EnumCase(target) = typed.constructions()[0].target() else {
            panic!("first construction must retain the first source enum case");
        };
        assert_eq!(target.source_unit(), source);
        assert_eq!(
            target.symbol(),
            local_names.resolution().enum_cases()[0].value_symbol()
        );
        assert_eq!(
            typed.constructions()[1].instance().type_arguments().len(),
            1
        );
        assert!(typed.calls().is_empty());
    }
}

#[test]
fn generic_bare_enum_case_requires_an_independent_expected_root() {
    let text = "enum class Maybe<T> { Some(item: T), None }
                fun invalid(): Unit { val result = Maybe.None }";
    let (sources, _, entries) = checked_entries(text);
    for (_, _, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0144"]);
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .expect("constructor span"),
            "None"
        );
        assert!(typed.constructions().is_empty());
    }
}

#[test]
fn expected_result_and_intrinsic_box_complete_construction_instances() {
    let text = "class Marker<T> {}
                value class Point(val x: Int)
                fun marker(): Marker<Int> = Marker()
                fun boxed(): Box<Point> = Box(Point(1))
                fun explicit(): Box<Point> = Box<Point>(Point(2))";
    let (_, _, entries) = checked_entries(text);
    for (_, _, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.constructions().len(), 5);
        assert!(matches!(
            typed.constructions()[2].target(),
            UnitConstructionTarget::IntrinsicBox
        ));
        assert!(typed.calls().is_empty());
    }
}
