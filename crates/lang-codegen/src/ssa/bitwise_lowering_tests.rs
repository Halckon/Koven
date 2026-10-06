//! Integer bitwise lowering must retain typed, eager operations in both entry paths.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, resolve_names},
    ownership_checking::check_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

use super::{
    lower_frontend::orchestrate::lower_scalar_file,
    model::Program,
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use crate::llvm::render_verified_program;

const TYPES: &[&str] = &[
    "Byte", "Short", "Int", "Long", "UByte", "UShort", "UInt", "ULong",
];
const OPERATORS: &[&str] = &["and", "or", "xor", "shl", "shr", "ushr"];

fn lower_file(text: &str) -> Program {
    let mut sources = SourceMap::new();
    let source = sources.add_source("bitwise.ko", text).expect("source");
    let lexed = lex(&sources, source).expect("lex");
    let file = parse_file(&sources, &lexed).expect("parse");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &file, &names).expect("names");
    let typed = check_types(&sources, &file, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &file, &names, &typed).expect("ownership");
    lower_scalar_file(&sources, &file, &names, &typed, &owned).expect("bitwise file lowers")
}

fn lower_unit(text: &str) -> Program {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "p/bitwise.ko", &format!("package p\n{text}"));
    let inputs = [SourceUnitInput::new("root", "p/bitwise.ko", source, &file)];
    let (names, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &names, &types);
    lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("bitwise unit lowers")
    .0
}

fn assert_binary_matrix(lower: fn(&str) -> Program) {
    for ty in TYPES {
        for operator in OPERATORS {
            let program = lower(&format!(
                "fun entry(left: {ty}, right: {ty}): {ty} = left {operator} right"
            ));
            let ssa = render_program(&program);
            assert!(
                ssa.contains(&format!("bitwise.{operator}")),
                "{ty} {operator}: {ssa}"
            );
            assert!(!ssa.contains("checked."), "{ssa}");
            let llvm = render_verified_program(&program).expect("bitwise LLVM verifies");
            assert!(!llvm.contains("poison"), "{llvm}");
            assert!(!llvm.contains(" nsw ") && !llvm.contains(" nuw "), "{llvm}");
            assert!(!llvm.contains(" exact "), "{llvm}");
            let bits = match *ty {
                "Byte" | "UByte" => 8,
                "Short" | "UShort" => 16,
                "Int" | "UInt" => 32,
                _ => 64,
            };
            let instruction = match *operator {
                "shr" if !ty.starts_with('U') => "ashr",
                "shr" | "ushr" => "lshr",
                other => other,
            };
            assert!(llvm.contains(&format!("{instruction} i{bits}")), "{llvm}");
            if matches!(*operator, "shl" | "shr" | "ushr") {
                assert!(
                    llvm.contains(&format!("and i{bits} %v1, {}", bits - 1)),
                    "{llvm}"
                );
            }
        }
    }
}

#[test]
fn single_file_bitwise_lowering_all_integer_types() {
    assert_binary_matrix(lower_file);
}

#[test]
fn unit_bitwise_lowering_all_integer_types() {
    assert_binary_matrix(lower_unit);
}

fn assert_inv_matrix(lower: fn(&str) -> Program) {
    for ty in TYPES {
        let program = lower(&format!("fun entry(value: {ty}): {ty} = value.inv()"));
        let rendered = render_program(&program);
        assert!(rendered.contains("bitwise.inv"), "{rendered}");
        assert!(!rendered.contains("checked."), "{rendered}");
        let llvm = render_verified_program(&program).expect("integer inv LLVM verifies");
        assert!(llvm.contains("xor i"), "{llvm}");
    }
}

#[test]
fn single_file_bitwise_inv_lowering_all_integer_types() {
    assert_inv_matrix(lower_file);
}

#[test]
fn unit_bitwise_inv_lowering_all_integer_types() {
    assert_inv_matrix(lower_unit);
}

#[test]
fn bitwise_lowering_preserves_adjacent_checked_arithmetic() {
    for lower in [lower_file as fn(&str) -> Program, lower_unit] {
        let program = lower("fun entry(left: Int, right: Int): Int = (left shl right) + 1");
        let rendered = render_program(&program);
        assert_eq!(rendered.matches("bitwise.shl").count(), 1, "{rendered}");
        assert_eq!(rendered.matches("checked.add").count(), 1, "{rendered}");
        let llvm = render_verified_program(&program).expect("checked neighbor LLVM");
        assert!(llvm.contains("llvm.sadd.with.overflow.i32"), "{llvm}");
    }
}

#[test]
fn single_file_integer_index_reads_then_inverts_without_consuming_container() {
    use super::model::{EntityId, Operation, PlaceAccess};
    for (expression, inverted) in [("values[0]", false), ("values[0].inv()", true)] {
        let program = lower_file(&format!("fun entry(values: List<Int>): Int = {expression}"));
        let function = &program.modules[0].functions[0];
        let element = function
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::ContainerElementPlace { .. }
                )
            })
            .expect("index uses the descriptor's element place");
        assert!(matches!(
            element.operation,
            Operation::ContainerElementPlace {
                owner: EntityId::Loan(_),
                ..
            }
        ));
        let [EntityId::Place(place)] = element.results.as_slice() else {
            panic!("index must produce a place")
        };
        let read = function
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::Read { source: PlaceAccess::Place(source) } if source == *place
                )
            })
            .expect("Copyable Int is read from the borrowed element");
        let inversions = function
            .instructions
            .iter()
            .filter_map(|instruction| {
                if let Operation::IntegerNot { operand } = instruction.operation {
                    Some(EntityId::Value(operand))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            inversions,
            if inverted {
                read.results.clone()
            } else {
                Vec::new()
            }
        );
        assert!(
            function
                .instructions
                .iter()
                .all(|instruction| !matches!(instruction.operation, Operation::Drop { .. }))
        );
        render_verified_program(&program)
            .expect("descriptor index and integer inversion verify through LLVM");
    }
}
