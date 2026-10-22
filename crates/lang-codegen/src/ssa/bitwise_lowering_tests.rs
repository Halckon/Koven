//! Integer bitwise lowering retains typed, eager operations for both source shapes.
//! Existing test names distinguish memory/package inputs, not separate lowering engines.
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::owned_compilation_unit_view,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use super::{
    model::Program, render::render_program, unit_lower::lower_owned_unit_with_entry,
    unit_lower_test_support::parsed,
};
use crate::llvm::render_verified_program;

const TYPES: &[&str] = &[
    "Byte", "Short", "Int", "Long", "UByte", "UShort", "UInt", "ULong",
];
const OPERATORS: &[&str] = &["and", "or", "xor", "shl", "shr", "ushr"];

fn lower_memory_unit(text: &str) -> Program {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "bitwise.ko", text);
    let inputs = [SourceUnitInput::memory_document(source, &file)];
    lower_unit(&sources, &inputs)
}

fn lower_package_unit(text: &str) -> Program {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "p/bitwise.ko", &format!("package p\n{text}"));
    let inputs = [SourceUnitInput::new("root", "p/bitwise.ko", source, &file)];
    lower_unit(&sources, &inputs)
}

fn lower_unit(sources: &SourceMap, inputs: &[SourceUnitInput<'_>]) -> Program {
    let (name_environment, type_environment) = standard_environments();
    let index = index_compilation_unit(sources, inputs).expect("valid bitwise unit input");
    let names = resolve_compilation_unit_names(sources, inputs, &index, &name_environment)
        .expect("bitwise unit names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().expect("valid bitwise unit names");
    let raw = check_compilation_unit_types(sources, inputs, &names, &type_environment)
        .expect("bitwise unit types");
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    complete_compilation_unit_types(sources, inputs, &names, &type_environment, &raw)
        .expect("complete bitwise unit types");
    let (typed, owned) =
        analyze_basic_unit_ownership(sources, inputs, &names, &type_environment, raw)
            .expect("bitwise unit ownership")
            .into_result()
            .expect("bitwise unit has basic type capability");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let owned = owned.validate().expect("valid bitwise unit ownership");
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| {
            declaration.name() == "entry" && declaration.kind() == SymbolKind::Function
        })
        .expect("bitwise entry exists")
        .id();
    let unit =
        owned_compilation_unit_view(sources, inputs, &names, &type_environment, &typed, &owned)
            .expect("matching bitwise unit analysis");
    lower_owned_unit_with_entry(&unit, entry)
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
    assert_binary_matrix(lower_memory_unit);
}

#[test]
fn unit_bitwise_lowering_all_integer_types() {
    assert_binary_matrix(lower_package_unit);
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
    assert_inv_matrix(lower_memory_unit);
}

#[test]
fn unit_bitwise_inv_lowering_all_integer_types() {
    assert_inv_matrix(lower_package_unit);
}

#[test]
fn bitwise_lowering_preserves_adjacent_checked_arithmetic() {
    for lower in [lower_memory_unit as fn(&str) -> Program, lower_package_unit] {
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
        let program =
            lower_memory_unit(&format!("fun entry(values: List<Int>): Int = {expression}"));
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
