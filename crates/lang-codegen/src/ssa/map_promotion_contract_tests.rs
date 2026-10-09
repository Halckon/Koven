//! Nullable Map operand adaptation consumes owners without extending inline ABI.
use super::{
    borrow_result_contract_tests::single, borrow_storage_contract_tests::unit, model::*,
    verify::verify_program,
};

const DIRECT: &str = "class Token(val n: Int)\nfun entry() { var m = mutableMapOf<String, Token?>(); m.put(\"key\", Token(7)) }";

const WARMED_INLINE: &str = "fun warm() { var values = mutableMapOf<String, Int>(); val previous = values[\"key\"] }\nfun entry() { warm(); val flag = true; var m = mutableMapOf<String, Int?>(); m.put(\"key\", 7) }";

#[test]
fn nullable_map_cached_result_single_cannot_enable_inline_storage() {
    let error = single(WARMED_INLINE)
        .err()
        .expect("Map result identity must not enable inline nullable storage");
    assert_eq!(error.kind, super::LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}

#[test]
fn nullable_map_cached_result_unit_cannot_enable_inline_storage() {
    let error = unit(WARMED_INLINE)
        .err()
        .expect("Map result identity must not enable inline nullable storage");
    assert_eq!(error.kind, super::LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}

#[test]
fn nullable_map_cached_owned_results_cannot_enable_move_only_inline_storage() {
    for (declaration, value, operand) in [
        ("", "String", "\"value\".clone()"),
        (
            "value class Packet(val text: String)\n",
            "Packet",
            "Packet(\"value\".clone())",
        ),
    ] {
        let source = format!(
            "{declaration}fun warm() {{ var values = mutableMapOf<String, {value}>(); val previous = values.remove(\"key\") }}\nfun entry() {{ warm(); val flag = true; var m = mutableMapOf<String, {value}?>(); m.put(\"key\", {operand}) }}"
        );
        for (label, result) in [("single", single(&source)), ("unit", unit(&source))] {
            let error = result
                .err()
                .unwrap_or_else(|| panic!("{value}/{label}: cached result cannot enable storage"));
            assert_eq!(error.kind, super::LoweringErrorKind::UnsupportedNode);
            assert!(error.span.is_some());
        }
    }
}

#[test]
fn nullable_map_promotion_single_direct_constructor() {
    assert_promotion(&single(DIRECT).unwrap());
}

#[test]
fn nullable_map_promotion_unit_direct_constructor() {
    assert_promotion(&unit(DIRECT).unwrap());
}

#[test]
fn nullable_map_promotion_generic_storage_transfers_constructor_and_local_owners() {
    for case in crate::native_map_promotion_cases::cases() {
        for (label, result) in [("single", single(case.source)), ("unit", unit(case.source))] {
            let program = result.unwrap_or_else(|error| panic!("{}/{label}: {error:?}", case.name));
            assert_promotion(&program);
            let entry = program.modules[0]
                .functions
                .iter()
                .find(|f| f.name.contains("entry"))
                .unwrap();
            assert_eq!(
                entry
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::NullableWrap { .. }))
                    .count(),
                2
            );
        }
    }
}

#[test]
fn nullable_map_promotion_preserves_direct_inline_nullable_rejection() {
    for text in [
        "fun entry() { var m = mutableMapOf<String, String?>(); m.put(\"key\", \"value\".clone()) }",
        "value class Packet(val text: String)\nfun entry() { var m = mutableMapOf<String, Packet?>(); m.put(\"key\", Packet(\"value\".clone())) }",
        "fun entry() { var m = mutableMapOf<String, Int?>(); m.put(\"key\", 7) }",
    ] {
        for result in [single(text), unit(text)] {
            let error = result.err().expect("inline nullable ABI stays unsupported");
            assert_eq!(error.kind, super::LoweringErrorKind::UnsupportedNode);
            assert!(error.span.is_some());
        }
    }
}

fn assert_promotion(program: &Program) {
    verify_program(program).unwrap();
    let entry = program.modules[0]
        .functions
        .iter()
        .find(|f| f.name.contains("entry"))
        .unwrap();
    let (owner, wrapped, wrap_index) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| {
            let Operation::NullableWrap { owner, .. } = instruction.operation else {
                return None;
            };
            let [EntityId::Value(wrapped)] = instruction.results.as_slice() else {
                panic!("one nullable owner")
            };
            Some((owner, *wrapped, index))
        })
        .unwrap();
    assert_ne!(owner, wrapped);
    let put = entry
        .instructions
        .iter()
        .position(|i| matches!(i.operation, Operation::MapPut { value, .. } if value == wrapped))
        .unwrap();
    assert!(wrap_index < put);
    assert!(!entry.instructions.iter().any(|i| matches!(i.operation, Operation::Drop { owner: dropped } if dropped == owner || dropped == wrapped)));
    assert!(!entry.instructions.iter().any(|i| matches!(
        i.operation,
        Operation::Copy { .. } | Operation::SharedRetain { .. }
    )));
    crate::llvm::render_verified_program(program).unwrap();
}
