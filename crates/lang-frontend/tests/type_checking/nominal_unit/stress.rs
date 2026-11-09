//! 原 128 cases / 96 conditions 输入的 Unit 默认栈回归。
//! 此输入历史上曾 Abort，受限制的本机验证只编译；正式 CI 实跑。
use super::super::{environments, parse};
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    type_checking::check_compilation_unit_types,
};

#[test]
fn long_enum_domains_and_deep_smart_cast_conditions_stay_deterministic() {
    let mut text = String::from("enum class Large { ");
    for index in 0..128 {
        if index != 0 {
            text.push_str(", ");
        }
        text.push_str(&format!("C{index}"));
    }
    text.push_str(" }\nfun select(input: Large): Int = when (input) {\n");
    for index in 0..128 {
        text.push_str(&format!("Large.C{index} -> {index}\n"));
    }
    text.push_str("}\n");
    text.push_str("enum class Shape { Circle(radius: Int), Point }\n");
    text.push_str("fun deep(shape: Shape): Boolean = shape is Shape.Circle");
    for _ in 0..96 {
        text.push_str(" && shape.radius > 0");
    }
    text.push('\n');

    let (sources, parsed) = parse(&text);
    let (environment, types) = environments();
    for rooted in [false, true] {
        let input = if rooted {
            SourceUnitInput::new("root", "types.ko", parsed.source_id(), &parsed)
        } else {
            SourceUnitInput::memory_document(parsed.source_id(), &parsed)
        };
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("unit names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid names");
        let first = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("first");
        let second =
            check_compilation_unit_types(&sources, &inputs, &names, &types).expect("second");
        assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
        assert_eq!(first.diagnostics(), second.diagnostics());
        assert_eq!(first.expression_types(), second.expression_types());
        let cases = |typed: &lang_frontend::type_checking::CompilationUnitTypes| {
            typed
                .signatures()
                .declarations()
                .iter()
                .filter_map(|declaration| declaration.nominal())
                .filter(|nominal| !nominal.enum_cases().is_empty())
                .map(|nominal| nominal.enum_cases().to_vec())
                .collect::<Vec<_>>()
        };
        let first_cases = cases(&first);
        assert_eq!(
            first_cases.iter().map(Vec::len).collect::<Vec<_>>(),
            [128, 2]
        );
        assert_eq!(first_cases, cases(&second));
    }
}

#[test]
fn deep_generic_and_long_interface_graphs_stay_deterministic_in_both_units() {
    use lang_frontend::type_checking::{BuiltinType, UnitTypeKind};
    use std::collections::BTreeSet;

    let mut text = String::from("class Box<T> {}\ninterface I0 {}\n");
    for index in 1..64 {
        text.push_str(&format!("interface I{index} : I{} {{}}\n", index - 1));
    }
    text.push_str("class Leaf : I63 {}\nfun deep(input: ");
    for _ in 0..96 {
        text.push_str("Box<");
    }
    text.push_str("Int");
    for _ in 0..96 {
        text.push('>');
    }
    text.push_str("): Unit {}\n");

    let (_, _, entries) = super::checked_entries(&text);
    let mut interface_order = None;
    for (unit, names, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let declarations = names.names().index().declarations();
        let declaration = |name| {
            declarations
                .iter()
                .find(|item| item.source_unit() == unit && item.name() == name)
                .expect("fixture declaration")
                .id()
        };
        let leaf = typed
            .signatures()
            .declaration(declaration("Leaf"))
            .and_then(|signature| signature.nominal())
            .expect("Leaf signature");
        assert_eq!(leaf.interfaces().len(), 64);
        let interfaces = leaf
            .interfaces()
            .iter()
            .map(|ty| {
                let Some(UnitTypeKind::Nominal {
                    declaration,
                    arguments,
                }) = typed.types().get(*ty)
                else {
                    panic!("interface instance: {ty:?}");
                };
                assert!(arguments.is_empty());
                declarations[declaration.index()].name().to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            interfaces.iter().cloned().collect::<BTreeSet<_>>(),
            (0..64).map(|index| format!("I{index}")).collect()
        );
        if let Some(first) = &interface_order {
            assert_eq!(&interfaces, first);
        } else {
            interface_order = Some(interfaces);
        }

        let deep = typed
            .signatures()
            .declaration(declaration("deep"))
            .and_then(|signature| signature.callable())
            .expect("deep signature");
        let [parameter] = deep.parameters() else {
            panic!("one deeply nested parameter");
        };
        let boxed = declaration("Box");
        let mut current = parameter.ty();
        for depth in 0..96 {
            let Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) = typed.types().get(current)
            else {
                panic!("Box at depth {depth}");
            };
            assert_eq!(*declaration, boxed);
            let [argument] = arguments.as_slice() else {
                panic!("one invariant type argument at depth {depth}");
            };
            current = *argument;
        }
        assert_eq!(
            typed.types().get(current),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
    }
}
