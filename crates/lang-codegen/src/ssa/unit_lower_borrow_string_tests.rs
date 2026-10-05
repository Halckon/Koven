//! SPEC-0274: String binary views preserve the caller's shared owner.

use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{
        Definition, EntityId, EntityType, Function, LoanKind, Operation, Program, SsaTypeKind,
    },
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn unit_borrow_string_binary_preserves_parameter_loans_and_owned_operands() {
    let program = lower_fixture(
        r#"package p
        fun same(a: String, b: String): Boolean = ((a)) == (b)
        fun different(a: String, b: String): Boolean = (a) != ((b))
        fun decorated(text: String): String = (text) + "!"
        fun prefix(text: String): String = ">" + (text)
        fun mixed(text: String, own suffix: String): String = text + suffix
        "#,
        r#"package q
        fun entry(): Unit {
            val source = "界\0" + "é"
            val same = p.same(source, "界\0é")
            val different = p.different("other", source)
            println(p.decorated(source))
            println(p.prefix(source))
            println(p.mixed(source, "?"))
            println(source)
        }"#,
    );
    for name in ["p.same", "p.different"] {
        let function = function(&program, name);
        let parameters = &function.blocks[0].parameters;
        assert!(matches!(
            parameters.as_slice(),
            [EntityId::Loan(_), EntityId::Loan(_)]
        ));
        assert!(function.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::StringEqual { left, right } if [left, right] == parameters.as_slice()
        )));
        assert!(
            function.instructions.iter().all(|instruction| !matches!(
                instruction.operation,
                Operation::BorrowBegin { .. }
                    | Operation::BorrowEnd { .. }
                    | Operation::Drop { .. }
            )),
            "shared parameters belong to the caller, not the callee"
        );
    }
    let different = function(&program, "p.different");
    assert!(
        different
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::BooleanNot { .. }))
    );

    for name in ["p.decorated", "p.mixed"] {
        let function = function(&program, name);
        let borrowed = function.blocks[0].parameters[0];
        assert!(matches!(borrowed, EntityId::Loan(_)));
        assert!(function.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::StringConcat { left, right: EntityId::Value(_) } if left == borrowed
        )));
    }
    let mixed = function(&program, "p.mixed");
    let prefix = function(&program, "p.prefix");
    assert!(
        prefix.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::StringConcat { left: EntityId::Value(_), right }
                if right == prefix.blocks[0].parameters[0]
        )),
        "Borrow String also remains a loan when it is the right operand"
    );
    let EntityId::Value(suffix) = mixed.blocks[0].parameters[1] else {
        panic!("explicit own suffix remains an owned operand");
    };
    assert_eq!(
        mixed
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation, Operation::Drop { owner } if owner == suffix
            ))
            .count(),
        1,
        "only the consumed suffix owner is cleaned up by mixed"
    );
    assert!(
        !program.modules[0]
            .functions
            .iter()
            .flat_map(|function| &function.instructions)
            .any(|instruction| matches!(
                instruction.operation,
                Operation::StringClone { .. } | Operation::SharedRetain { .. }
            )),
        "binary shared reads must not allocate replacement owners"
    );
}

#[test]
fn unit_borrow_string_binary_uses_rebound_loan_after_rhs_control_flow() {
    let program = lower_fixture(
        r#"package p
        fun choose(text: String, own flag: Boolean): String {
            val joined = (text) + (if (flag) "界" else "")
            println(text)
            return joined
        }"#,
        r#"package q
        fun entry(): Unit {
            val source = "dynamic" + "é"
            println(p.choose(source, true))
            println(p.choose(source, false))
            println(source)
        }"#,
    );
    let function = function(&program, "p.choose");
    let initial = function.blocks[0].parameters[0];
    let view = function
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::StringConcat { left, .. } => Some(left),
            _ => None,
        })
        .expect("concat has a String view");
    assert_ne!(
        view, initial,
        "RHS CFG replaces the entry loan with an edge parameter"
    );
    let EntityId::Loan(loan) = view else {
        panic!("concat uses a shared loan")
    };
    let entity = function.entity(view).unwrap();
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = entity.ty
    else {
        panic!("shared String loan remains typed after CFG");
    };
    assert_eq!(
        program.modules[0].types[target.index()],
        SsaTypeKind::StringOwner
    );
    assert!(
        matches!(entity.definition, Definition::BlockParameter { block, .. } if block != function.blocks[0].id)
    );
    assert!(
        function.instructions.iter().any(|instruction| matches!(
            instruction.operation, Operation::PrintString { value } if value == loan
        )),
        "pending operand and later source use share the same rebound loan"
    );
    assert!(
        !function.instructions.iter().any(|instruction| matches!(
            instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan
        )),
        "the callee does not end its incoming shared loan"
    );
}

#[test]
fn unit_borrow_string_comparisons_use_rebound_loans_after_rhs_control_flow() {
    let program = lower_fixture(
        r#"package p
        fun sameAfter(text: String, own flag: Boolean): Boolean {
            val result = (text) == (if (flag) "界\0é" else "界\0ê")
            println(text)
            return result
        }
        fun differentAfter(text: String, own flag: Boolean): Boolean {
            val result = (text) != (if (flag) "界\0é" else "界\0ê")
            println(text)
            return result
        }"#,
        r#"package q
        fun entry(): Unit {
            val source = "界\0" + "é"
            val equal = p.sameAfter(source, true)
            val unequal = p.sameAfter(source, false)
            val notDifferent = p.differentAfter(source, true)
            val different = p.differentAfter(source, false)
            println(source)
        }"#,
    );
    for name in ["p.sameAfter", "p.differentAfter"] {
        let function = function(&program, name);
        let (view, result) = function
            .instructions
            .iter()
            .find_map(|instruction| match instruction.operation {
                Operation::StringEqual { left, .. } => Some((left, instruction.results[0])),
                _ => None,
            })
            .expect("comparison reads both String views");
        assert_ne!(
            view, function.blocks[0].parameters[0],
            "comparison must use the loan transported through the RHS branch"
        );
        let EntityId::Loan(loan) = view else {
            panic!("comparison preserves the Borrow loan")
        };
        let entity = function.entity(view).unwrap();
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = entity.ty
        else {
            panic!("rebound comparison view is a shared loan");
        };
        assert_eq!(
            program.modules[0].types[target.index()],
            SsaTypeKind::StringOwner
        );
        assert!(
            matches!(entity.definition, Definition::BlockParameter { block, .. }
            if block != function.blocks[0].id)
        );
        assert!(
            function.instructions.iter().any(|instruction| matches!(
                instruction.operation, Operation::PrintString { value } if value == loan
            )),
            "the source remains readable with the same loan after comparison"
        );
        assert!(
            !function.instructions.iter().any(|instruction| matches!(
                instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan
            )),
            "comparison must not end its incoming loan"
        );
        let negations = function
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::BooleanNot { operand } => Some(EntityId::Value(operand)),
                _ => None,
            })
            .collect::<Vec<_>>();
        if name == "p.differentAfter" {
            assert_eq!(
                negations,
                [result],
                "!= negates the exact StringEqual result"
            );
        } else {
            assert!(negations.is_empty(), "== preserves the StringEqual result");
        }
    }
}

fn function<'a>(program: &'a Program, name: &str) -> &'a Function {
    program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .expect("reachable helper exists")
}

fn lower_fixture(provider: &str, consumer: &str) -> Program {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", provider);
    let (consumer_source, consumer) = parsed(&mut sources, "q/consumer.ko", consumer);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let lower = |inputs: &[_]| {
        lower_scalar_unit_with_entry(
            &sources,
            inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "q", "entry"),
        )
        .expect("Borrow String operations must lower through the unit entry")
    };
    let (program, entry) = lower(&inputs);
    let (reversed, reverse_entry) = lower(&[inputs[1], inputs[0]]);
    assert_eq!(render_program(&program), render_program(&reversed));
    assert_eq!(
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap(),
        crate::llvm::render_verified_program_with_entry(&reversed, reverse_entry).unwrap(),
    );
    program
}
