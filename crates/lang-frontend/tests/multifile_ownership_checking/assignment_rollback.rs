use super::*;

#[test]
fn member_assignment_requires_a_var_field_and_inout_receiver() {
    for (assignment, expected_codes) in [
        ("fixed = 1", &["L0134"][..]),
        ("this.fixed = 2", &["L0134"][..]),
        ("(this).fixed = 2", &["L0134"][..]),
        ("count = 3", &[][..]),
        ("this.count = 4", &[][..]),
        ("(this).count = 4", &[][..]),
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "class Worker(val fixed: Int, var count: Int) {{\n\
                 inout fun update(): Unit {{ {assignment} }}\n\
             }}"
        );
        let (source, parsed) = parsed(&mut sources, "main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");

        assert_eq!(diagnostic_codes(&ownership), expected_codes, "{assignment}");
        if !expected_codes.is_empty() {
            assert!(ownership.loans().is_empty());
            assert!(ownership.value_deliveries().is_empty());
        }
    }
}

#[test]
fn rejected_assignment_rolls_back_rhs_move_and_executable_facts() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "main.ko",
        "class Resource {}\n\
         fun identity(own item: Resource): Resource = item\n\
         class Holder(val fixed: Resource) {\n\
             inout fun reject(own replacement: Resource): Unit {\n\
                 val ignored: Unit = fixed = identity(replacement)\n\
                 val stillOwned: Resource = replacement\n\
             }\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("recovery ownership product");

    assert_eq!(diagnostic_codes(&ownership), ["L0134"]);
    assert!(ownership.loans().is_empty());
    assert!(ownership.value_deliveries().is_empty());
    assert!(ownership.receiver_facts().is_empty());
    assert!(ownership.rc_effects().is_empty());
    assert!(ownership.construction_plans().is_empty());
}

#[test]
fn divergent_rhs_still_checks_static_field_mutability() {
    for (method, expected_codes) in [
        (
            "inout fun reject(): Unit { fixed = error(\"stop\") }",
            &["L0134"][..],
        ),
        (
            "fun reject(): Unit { count = error(\"stop\") }",
            &["L0134"][..],
        ),
        (
            "inout fun accept(): Unit { count = error(\"stop\") }",
            &[][..],
        ),
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "class Holder(val fixed: Int, var count: Int) {{\n\
                 {method}\n\
             }}"
        );
        let (source, parsed) = parsed(&mut sources, "main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let ownership =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("ownership product");

        assert_eq!(diagnostic_codes(&ownership), expected_codes, "{method}");
    }
}
