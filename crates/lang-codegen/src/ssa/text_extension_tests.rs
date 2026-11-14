//! Receiver ABI fixtures, not evidence for the public byte-count or overflow algorithms.
use super::{model::*, unit_lower_test_support as support};
use crate::test_support::diagnostics::assert_diagnostic;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

#[test]
fn unit_text_receiver_is_a_shared_first_argument_with_an_owned_result() {
    let mut sources = SourceMap::new();
    let (text_id, text) = support::parsed(
        &mut sources,
        "text.ko",
        "package koven.text\nborrow fun String.byteLength(): Int = 7\nborrow fun String.checkedPlus(other: String): String? = this + other",
    );
    let (app_id, app) = support::parsed(
        &mut sources,
        "app.ko",
        "package app\nimport koven.text.byteLength\nimport koven.text.checkedPlus\nfun main(): Unit { val source = \"a\".clone(); if (source.byteLength() != 7) { error(\"length\") }; println(\"7\"); val joined = source.checkedPlus(\"b\"); if (joined != null) { println(joined) }; println(source) }",
    );
    let inputs = [
        SourceUnitInput::new("std", "koven/text/text.ko", text_id, &text),
        SourceUnitInput::new("app", "app/Main.ko", app_id, &app),
    ];
    let (environment, mut types) = standard_environments();
    types.authorize_text_source(&sources, text_id).unwrap();
    let (names, typed, owned) = support::analyze(&sources, &inputs, &environment, &types);
    let (program, entry) = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        support::declaration(&names, "app", "main"),
    )
    .unwrap();
    super::verify::verify_program(&program).unwrap();
    let module = &program.modules[0];
    for spelling in ["byteLength", "checkedPlus"] {
        let function = module
            .functions
            .iter()
            .find(|f| f.name.contains(spelling))
            .unwrap();
        assert!(function.receiver.is_none());
        assert!(function.carrier_return.is_none());
        assert!(function.borrow_return.is_none());
        let entry = function.block(function.entry_block().unwrap()).unwrap();
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = function.entity(entry.parameters[0]).unwrap().ty
        else {
            panic!("text receiver must be a shared entry loan");
        };
        assert_eq!(module.types[target.index()], SsaTypeKind::StringOwner);
        let calls = module.functions.iter().flat_map(|f| &f.instructions).filter(|i| matches!(&i.operation, Operation::DirectCall { callee, receiver: None, .. } if *callee == function.id)).count();
        assert_eq!(
            calls, 1,
            "{spelling} uses a static call with an explicit borrowed first slot"
        );
    }
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run =
        crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[1, 0]);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"7\nab\na\n");
}

/// Byte offsets must count UTF-8 bytes and retain embedded NUL, without consuming the owner.
#[test]
fn trusted_text_storage_reads_preserve_utf8_bytes_and_the_source_owner() {
    let mut sources = SourceMap::new();
    let (text_id, text) = support::parsed(
        &mut sources,
        "text.ko",
        r#"package koven.text
borrow fun String.byteLength(): Int = textByteLength(this)
fun inspect(text: String): Unit {
    if (textByteLength(text) != 4) { error("length") }
    val ascii: UByte = 65u
    if (textByteAt(text, 0) != ascii) { error("ascii") }
    val nul: UByte = 0u
    if (textByteAt(text, 1) != nul) { error("nul") }
    val lead: UByte = 195u
    if (textByteAt(text, 2) != lead) { error("lead") }
    val continuation: UByte = 169u
    if (textByteAt(text, 3) != continuation) { error("continuation") }
}
"#,
    );
    let (app_id, app) = support::parsed(
        &mut sources,
        "app.ko",
        r#"package app
import koven.text.inspect
import koven.text.byteLength
fun main(): Unit {
    val text = "A\0é".clone()
    inspect(text)
    if (text.byteLength() != 4) { error("length") }
    println("4")
    println(text)
    if ("".byteLength() != 0) { error("empty") }
    println("0")
}
"#,
    );
    let inputs = [
        SourceUnitInput::new("std", "koven/text/text.ko", text_id, &text),
        SourceUnitInput::new("app", "app/Main.ko", app_id, &app),
    ];
    let (environment, mut types) = standard_environments();
    types.authorize_text_source(&sources, text_id).unwrap();
    let (names, typed, owned) = support::analyze(&sources, &inputs, &environment, &types);
    let (program, entry) = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        support::declaration(&names, "app", "main"),
    )
    .unwrap();
    super::verify::verify_program(&program).unwrap();
    let operations: Vec<_> = program.modules[0]
        .functions
        .iter()
        .flat_map(|f| &f.instructions)
        .map(|i| &i.operation)
        .collect();
    assert_eq!(
        operations
            .iter()
            .filter(|op| matches!(op, Operation::StringByteLength { .. }))
            .count(),
        2
    );
    assert_eq!(
        operations
            .iter()
            .filter(|op| matches!(op, Operation::StringByteAt { .. }))
            .count(),
        4
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0]);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"4\nA\0\xc3\xa9\n0\n");
}

#[test]
fn trusted_utf8_scan_reports_sequence_start_and_preserves_byte_array() {
    let mut sources = SourceMap::new();
    let (text_id, text) = support::parsed(
        &mut sources,
        "text.ko",
        r#"package koven.text
fun checkBytes(): Unit {
    val bytes = Array<UByte>(3, { index -> if (index == 0) 65u else if (index == 1) 226u else 130u })
    if (textUtf8ErrorOffset(bytes) != 1) { error("truncated sequence start") }
    if (bytes.size != 3) { error("source retained") }
    if (textUtf8ErrorOffset(Array<UByte>(0, { index -> 0u })) != -1) { error("empty") }
    if (textUtf8ErrorOffset(Array<UByte>(1, { index -> 0u })) != -1) { error("nul") }
    println("utf8")
}
"#,
    );
    let (app_id, app) = support::parsed(
        &mut sources,
        "app.ko",
        "package app\nimport koven.text.checkBytes\nfun main(): Unit { checkBytes() }",
    );
    let inputs = [
        SourceUnitInput::new("std", "koven/text/text.ko", text_id, &text),
        SourceUnitInput::new("app", "app/Main.ko", app_id, &app),
    ];
    let (environment, mut types) = standard_environments();
    types.authorize_text_source(&sources, text_id).unwrap();
    let (names, typed, owned) = support::analyze(&sources, &inputs, &environment, &types);
    let (program, entry) = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        support::declaration(&names, "app", "main"),
    )
    .unwrap();
    super::verify::verify_program(&program).unwrap();
    assert_eq!(
        program.modules[0]
            .functions
            .iter()
            .flat_map(|f| &f.instructions)
            .filter(|i| matches!(i.operation, Operation::Utf8ErrorOffset { .. }))
            .count(),
        3
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert_eq!(
        llvm.matches("define internal i64 @koven.utf8.error_offset(")
            .count(),
        1
    );
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 2);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"utf8\n");
}

#[test]
fn nullable_string_if_borrows_retest_the_current_owner_and_distinguish_empty_from_null() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;
    let source = UnitSource::analyze(
        "nullable-string.ko",
        r#"fun make(flag: Boolean): String? {
    if (flag) { return ("value" + "") }
    return null
}
fun show(flag: Boolean): Unit {
    val text = make(flag)
    if (text != null) { println(text); println(text) } else { println("null") }
    if (text == null) { println("absent") } else { println((text)) }
}
fun main(): Unit {
    show(true)
    show(false)
    val empty: String? = ""
    if (empty != null) { println(empty) }
}"#,
    )
    .expect("actual nullable String source passes admission");
    let (program, entry) = source
        .lower_entry(source.declaration("main", SymbolKind::Function))
        .expect("each non-null branch borrows its current String owner");
    super::verify::verify_program(&program).unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 1);
    crate::native_tests::boxed_enum_tests::assert_success(
        &run,
        b"value\nvalue\nvalue\nnull\nabsent\n\n",
    );
}

#[test]
fn per_family_nullable_string_if_keeps_nested_and_prefix_carriers_rejected() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;
    for (name, text) in [
        (
            "nested",
            r#"fun entry(own text: String?, own other: String?): Unit {
    if (text != null) {
        println(text)
        if (other != null) { println(other) }
        println(text)
    }
}"#,
        ),
        (
            "prefix",
            r#"fun consume(marker: Int, value: Int): Unit {}
fun entry(own text: String?): Unit {
    consume(1, if (true) { if (text != null) { println(text) }; 2 } else { 3 })
}"#,
        ),
    ] {
        let source =
            UnitSource::analyze(name, text).expect("rejection begins after actual admission");
        let result = super::refine::with_per_family_lowering(|| {
            source.lower_entry(source.declaration("entry", SymbolKind::Function))
        });
        let error = match result {
            Ok(_) => panic!("{name} must not bypass the existing proof transfer boundary"),
            Err(error) => error,
        };
        assert_diagnostic!(
            matches!(
                error.kind,
                super::LoweringErrorKind::MissingFact | super::LoweringErrorKind::UnsupportedNode
            ),
            "{name}: {error:?}"
        );
    }
}

#[test]
fn per_family_nullable_string_if_returns_the_owned_parameter_after_borrow() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;
    let source = UnitSource::analyze(
        "nullable-owned-return.ko",
        r#"fun entry(own text: String?): String {
    if (text != null) { println(text); return text }
    return "fallback"
}
fun make(flag: Boolean): String? {
    if (flag) { return ("value" + "") }
    return null
}
fun main(): Unit {
    val present = make(true)
    println(entry(present))
    val absent = make(false)
    println(entry(absent))
}"#,
    )
    .unwrap();
    let (program, entry) = source
        .lower_entry(source.declaration("main", SymbolKind::Function))
        .unwrap();
    super::verify::verify_program(&program).unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 1);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"value\nvalue\nfallback\n");
    let err = match super::refine::with_per_family_lowering(|| {
        source.lower_entry(source.declaration("entry", SymbolKind::Function))
    }) {
        Err(err) => err,
        Ok(_) => panic!("the retired nullable String if path must reject per-family lowering"),
    };
    assert_eq!(
        err.kind,
        super::LoweringErrorKind::UnsupportedNode,
        "{err:?}"
    );
}

pub(in crate::ssa) mod borrow_call_lowering_tests {
    //! 普通源码覆盖透明分组及字段来源调用，不改写前端事实或 SSA。
    use super::super::{
        borrow_memory_test_support::single, borrow_storage_contract_tests::unit, model::*,
        verify::verify_program,
    };

    const GROUPED_BINDING: &str = "fun view(source: String): borrow String from source = source\nfun consume(own source: String) { println(source) }\nfun entry() { val source = \"kept\".clone(); borrow val item = ((view(source))); println(item); consume(source) }";
    const GROUPED_RETURN: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = ((view(source)))\nfun entry() { val source = \"kept\".clone(); borrow val item = wrap(source); println(item) }";
    const FIELD_RETURN: &str = "class Packet(val text: String)\nfun view(source: String): borrow String from source = source\nfun wrap(source: Packet): borrow String from source = view(source.text)\nfun consume(own source: Packet) {}\nfun entry() { val source = Packet(\"kept\".clone()); borrow val item = wrap(source); println(item); consume(source) }";

    fn assert_borrow_call(program: &Program) {
        verify_program(program).unwrap();
        let entry = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.contains("entry"))
            .unwrap();
        let call = entry
            .instructions
            .iter()
            .find(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
            .unwrap();
        let Operation::BorrowCall { source, .. } = call.operation else {
            unreachable!()
        };
        let [EntityId::Loan(result)] = call.results.as_slice() else {
            panic!("borrow call must produce a loan");
        };
        let print = entry.instructions.iter().position(|instruction| {
        matches!(instruction.operation, Operation::PrintString { value } if value == *result)
    }).unwrap();
        let end = |loan| {
            entry.instructions.iter().position(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan)
    }).unwrap()
        };
        assert!(print < end(*result) && end(*result) < end(source));
        crate::llvm::render_verified_program(program).unwrap();
    }

    #[test]
    fn abandoned_call_prefix_demands_only_completed_nullable_parameter_storage() {
        let text = "fun unreachableCallee(number: Int?, own stop: Int, own suffix: Boolean?): Unit {}\nfun entry() { unreachableCallee(7, return, false) }";
        let program = unit(text).unwrap();
        verify_program(&program).unwrap();
        let module = &program.modules[0];
        assert!(
            !module
                .functions
                .iter()
                .any(|function| { function.name.contains("unreachableCallee") })
        );
        let wrappers: Vec<_> = module
            .types
            .iter()
            .filter_map(|ty| {
                if let SsaTypeKind::InlineNullable { inner, .. } = ty {
                    Some(&module.types[inner.index()])
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(wrappers.len(), 1);
        assert_ne!(wrappers[0], &SsaTypeKind::Boolean);
        let entry = module
            .functions
            .iter()
            .find(|function| function.name.contains("entry"))
            .unwrap();
        assert!(entry.instructions.iter().any(|instruction| {
            matches!(instruction.operation, Operation::NullableWrap { .. })
        }));
    }

    #[test]
    fn temporary_field_borrow_uses_the_field_loan_or_copies_before_wrapping() {
        for parameter in ["Int", "Int?"] {
            let program = unit(&format!(
            "value class Leaf(val number: Int)\nclass Holder(val leaf: Leaf)\nfun read(number: {parameter}): Unit {{}}\nfun entry() {{ read((Holder(Leaf(7))).leaf.number) }}"
        ))
        .unwrap();
            verify_program(&program).unwrap();
            let entry = program.modules[0]
                .functions
                .iter()
                .find(|function| function.name.contains("entry"))
                .unwrap();
            let instructions = &entry.instructions;
            let field = instructions
                .iter()
                .find(|instruction| {
                    matches!(instruction.operation, Operation::SharedFieldLoan { .. })
                })
                .unwrap();
            let Operation::SharedFieldLoan { base: heap, .. } = field.operation else {
                unreachable!()
            };
            let [EntityId::Loan(field)] = field.results.as_slice() else {
                panic!("field projection must produce a loan");
            };
            let producer = |entity| {
                instructions
                    .iter()
                    .find(|instruction| instruction.results.contains(&entity))
                    .unwrap()
            };
            let Operation::SharedHeapFieldLoan { base: root, .. } =
                producer(EntityId::Loan(heap)).operation
            else {
                panic!("inline field must descend from the heap field");
            };
            let (call, delivered) = instructions
                .iter()
                .enumerate()
                .find_map(|(index, instruction)| {
                    let Operation::DirectCall { arguments, .. } = &instruction.operation else {
                        return None;
                    };
                    let [EntityId::Loan(loan)] = arguments.as_slice() else {
                        return None;
                    };
                    Some((index, *loan))
                })
                .unwrap();
            let end = |loan| {
                instructions
                    .iter()
                    .position(|instruction| instruction.operation == Operation::BorrowEnd { loan })
                    .unwrap()
            };
            assert!(end(*field) < end(heap) && end(heap) < end(root));
            if parameter == "Int" {
                assert_eq!(delivered, *field);
                assert!(call < end(*field));
            } else {
                let read = instructions.iter().position(|instruction| {
                matches!(instruction.operation, Operation::Read { source: PlaceAccess::Loan(source) } if source == *field)
            }).unwrap();
                let wrap = instructions
                    .iter()
                    .position(|instruction| {
                        matches!(instruction.operation, Operation::NullableWrap { .. })
                    })
                    .unwrap();
                let Operation::BorrowBegin { place, .. } =
                    producer(EntityId::Loan(delivered)).operation
                else {
                    panic!("the nullable argument must borrow its own slot");
                };
                let [EntityId::Value(wrapper)] = instructions[wrap].results.as_slice() else {
                    panic!("expected the wrapped nullable value");
                };
                assert_eq!(
                    producer(EntityId::Place(place)).operation,
                    Operation::RootPlace { owner: *wrapper }
                );
                assert!(read < end(*field) && end(root) < wrap && wrap < call);
                assert!(call < end(delivered));
            }
        }
    }

    #[test]
    fn grouped_binding_single_preserves_returned_loan() {
        assert_borrow_call(&single(GROUPED_BINDING).unwrap());
    }

    #[test]
    fn grouped_binding_unit_preserves_returned_loan() {
        assert_borrow_call(&unit(GROUPED_BINDING).unwrap());
    }

    #[test]
    fn grouped_return_single_preserves_forwarded_loan() {
        assert_borrow_call(&single(GROUPED_RETURN).unwrap());
    }

    #[test]
    fn grouped_return_unit_preserves_forwarded_loan() {
        assert_borrow_call(&unit(GROUPED_RETURN).unwrap());
    }

    #[test]
    fn field_return_unit_preserves_projected_source_loan() {
        let program = unit(FIELD_RETURN).unwrap();
        assert_borrow_call(&program);
        let entry = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.contains("entry"))
            .unwrap();
        let source = entry
            .instructions
            .iter()
            .find_map(|instruction| {
                if let Operation::BorrowCall { source, .. } = instruction.operation {
                    Some(source)
                } else {
                    None
                }
            })
            .unwrap();
        let source_end = entry.instructions.iter().position(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == source)
    }).unwrap();
        let consume = entry
            .instructions
            .iter()
            .position(|instruction| {
                matches!(&instruction.operation, Operation::DirectCall { arguments, .. }
            if matches!(arguments.as_slice(), [EntityId::Value(_)]))
            })
            .unwrap();
        assert!(
            source_end < consume,
            "last use restores the owner's move permission"
        );
        let wrapper = program.modules[0]
            .functions
            .iter()
            .find(|f| f.name.contains("wrap"))
            .unwrap();
        let field = wrapper
            .instructions
            .iter()
            .find(|instruction| {
                matches!(instruction.operation, Operation::SharedHeapFieldLoan { .. })
            })
            .unwrap();
        let [EntityId::Loan(field_loan)] = field.results.as_slice() else {
            panic!("field projection must produce a loan");
        };
        assert!(wrapper.instructions.iter().any(|instruction| {
        matches!(instruction.operation, Operation::BorrowCall { source, .. } if source == *field_loan)
    }));
        assert!(
        !wrapper.instructions.iter().any(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == *field_loan)
        }),
        "forwarded source must remain live through BorrowReturn"
    );
    }

    #[test]
    fn nested_nullable_copy_keeps_unrelated_drop_before_the_read() {
        let text = "class Boxed(val value: Int)\nclass Aux() { deinit() { println(\"aux\") } }\nfun view(source: Boxed, aux: Aux): borrow Int from source = source.value\nfun observe(value: Int?): Unit {}\nfun entry() { val boxed = Boxed(7); observe(view(boxed, Aux())) }";
        let program = unit(text).unwrap();
        verify_program(&program).unwrap();
        let entry = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.contains("entry"))
            .unwrap();
        let (call, result, source) = entry
            .instructions
            .iter()
            .enumerate()
            .find_map(|(index, instruction)| {
                match (&instruction.operation, instruction.results.as_slice()) {
                    (Operation::BorrowCall { source, .. }, [EntityId::Loan(result)]) => {
                        Some((index, *result, *source))
                    }
                    _ => None,
                }
            })
            .unwrap();
        let position = |operation| {
            entry
                .instructions
                .iter()
                .position(|i| i.operation == operation)
                .unwrap()
        };
        let read = position(Operation::Read {
            source: PlaceAccess::Loan(result),
        });
        let end_result = position(Operation::BorrowEnd { loan: result });
        let end_source = position(Operation::BorrowEnd { loan: source });
        let drops = entry
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| {
                matches!(instruction.operation, Operation::Drop { .. }).then_some(index)
            })
            .collect::<Vec<_>>();
        let [drop_aux, drop_source] = drops.as_slice() else {
            panic!("expected one auxiliary and one source drop, got {drops:?}");
        };
        assert!(call < *drop_aux && *drop_aux < read);
        assert!(read < end_result && end_result < end_source && end_source < *drop_source);
    }
}

pub(in crate::ssa) mod unit_generic_container_tests {
    //! SPEC-0275: direct generic container signatures consume existing concrete identities.

    use super::super::{
        model::{
            Definition, EntityId, EntityType, Function, LoanKind, Operation, Program,
            SequentialContainerKind, SsaTypeKind, TerminatorKind,
        },
        render::render_program,
        unit_lower::lower_owned_unit_with_entry,
        unit_lower_test_support::{analyze, declaration, parsed},
    };
    use crate::test_support::diagnostics::assert_text_eq;
    use lang_frontend::{
        name_resolution::SourceUnitInput,
        ownership_checking::owned_compilation_unit_view,
        source::SourceMap,
        type_checking::{
            BuiltinType, IntrinsicTypeConstructor, UnitTypeKind, standard_environments,
        },
    };

    #[test]
    fn unit_generic_container_array_signatures_preserve_concrete_ownership() {
        signature_matrix(
            "Array",
            "arrayOf",
            IntrinsicTypeConstructor::Array,
            SequentialContainerKind::Array,
        );
    }

    #[test]
    fn unit_generic_container_list_signatures_preserve_concrete_ownership() {
        signature_matrix(
            "List",
            "listOf",
            IntrinsicTypeConstructor::List,
            SequentialContainerKind::List,
        );
    }

    #[test]
    fn unit_generic_container_mutable_list_signatures_preserve_concrete_ownership() {
        signature_matrix(
            "MutableList",
            "mutableListOf",
            IntrinsicTypeConstructor::MutableList,
            SequentialContainerKind::MutableList,
        );
    }

    fn signature_matrix(
        container: &str,
        constructor: &str,
        intrinsic: IntrinsicTypeConstructor,
        kind: SequentialContainerKind,
    ) {
        let provider = format!(
            "package p\n\
         fun <T> sizeOf(items: {container}<T>): Int {{\n\
             val first = items.size\n\
             return first + ((items)).size\n\
         }}\n\
         fun <T> pass(own items: {container}<T>): {container}<T> = items"
        );
        let consumer = format!(
            r#"package q
        fun entry(): Int {{
            val i0: {container}<Int> = {constructor}()
            val i2 = {constructor}(1, 2)
            val s0: {container}<String> = {constructor}()
            val s2 = {constructor}("one", "two")
            val before = p.sizeOf<Int>(i0) + p.sizeOf(i0) + p.sizeOf(i2) + p.sizeOf<Int>(i2) + p.sizeOf<String>(s0) + p.sizeOf(s0) + p.sizeOf(s2) + p.sizeOf<String>(s2)
            val after = i0.size + i2.size + s0.size + s2.size
            val r0 = p.pass<Int>(i0)
            val r2 = p.pass(i2)
            val t0 = p.pass<String>(s0)
            val t2 = p.pass(s2)
            return before + after + r0.size + r2.size + t0.size + t2.size
        }}"#
        );
        let program = fixture(&provider, &consumer, intrinsic);
        let module = &program.modules[0];
        let sizes = functions(&program, "p.sizeOf");
        let passes = functions(&program, "p.pass");
        assert_eq!(sizes.len(), 2, "repeated calls share Int/String instances");
        assert_eq!(passes.len(), 2, "own returns share Int/String instances");
        let mut elements = Vec::new();
        for size in sizes {
            let parameter = size.blocks[0].parameters[0];
            let EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            } = size.entity(parameter).unwrap().ty
            else {
                panic!("generic Borrow parameter keeps its shared loan")
            };
            let SsaTypeKind::SequentialContainer {
                kind: actual,
                element,
            } = module.type_kind(target).unwrap()
            else {
                panic!("generic parameter is a concrete sequential container")
            };
            assert_eq!(*actual, kind);
            elements.push(module.type_kind(*element).unwrap().clone());
            let lengths = size
                .instructions
                .iter()
                .filter_map(|instruction| match instruction.operation {
                    Operation::ContainerLength { owner } => Some(owner),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(lengths, [parameter, parameter]);
            assert_no_callee_cleanup(size);
            assert!(matches!(
                module.type_kind(size.return_types[0]),
                Some(SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                })
            ));
            let pass = passes
                .iter()
                .find(|pass| pass.return_types == [target])
                .expect("own return uses exactly the same concrete container identity");
            let input = pass.blocks[0].parameters[0];
            assert_eq!(pass.entity(input).unwrap().ty, EntityType::Value(target));
            assert_no_callee_cleanup(pass);
            let EntityId::Value(owner) = input else {
                panic!("own parameter transports an owner")
            };
            assert!(matches!(
                &pass.blocks[0].terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { values } if values == &[owner]
            ));
        }
        assert!(elements.contains(&SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        }));
        assert!(elements.contains(&SsaTypeKind::StringOwner));
        let entry = functions(&program, "q.entry")[0];
        assert_eq!(
            entry
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
                .count(),
            4,
            "only the four returned owners retain caller cleanup obligations"
        );
    }

    #[test]
    fn unit_generic_container_borrow_size_uses_current_loan_after_cfg() {
        for (container, constructor, intrinsic) in [
            ("Array", "arrayOf", IntrinsicTypeConstructor::Array),
            ("List", "listOf", IntrinsicTypeConstructor::List),
            (
                "MutableList",
                "mutableListOf",
                IntrinsicTypeConstructor::MutableList,
            ),
        ] {
            let provider = format!(
                r#"package p
            fun <T> sizeOf(items: {container}<T>, own flag: Boolean): Int {{
                if (flag) {{ println("true") }} else {{ println("false") }}
                return ((items)).size
            }}"#
            );
            let consumer = format!(
                r#"package q
            fun entry(): Int {{
                val ints = {constructor}(1, 2)
                val strings = {constructor}("text")
                val first = p.sizeOf(ints, true) + p.sizeOf<Int>(ints, false)
                val second = p.sizeOf<String>(strings, false) + p.sizeOf(strings, true)
                return first + second + ints.size + strings.size
            }}"#
            );
            let program = fixture(&provider, &consumer, intrinsic);
            let sizes = functions(&program, "p.sizeOf");
            assert_eq!(sizes.len(), 2);
            for size in sizes {
                let parameter = size.blocks[0].parameters[0];
                let owner = size
                    .instructions
                    .iter()
                    .find_map(|instruction| match instruction.operation {
                        Operation::ContainerLength { owner } => Some(owner),
                        _ => None,
                    })
                    .expect("generic size emits a header read");
                assert_ne!(owner, parameter, "CFG transports the active loan");
                let entity = size.entity(owner).unwrap();
                assert_eq!(entity.ty, size.entity(parameter).unwrap().ty);
                let EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                } = entity.ty
                else {
                    panic!("size reads the current shared container loan")
                };
                assert!(matches!(
                    entity.definition,
                    Definition::BlockParameter { block, .. } if block != size.blocks[0].id
                ));
                // println has independent String temporaries; protect every container loan alias.
                for instruction in &size.instructions {
                    match instruction.operation {
                        Operation::BorrowEnd { loan } => assert_ne!(
                            size.entity(EntityId::Loan(loan)).unwrap().ty,
                            entity.ty,
                            "callee cannot end the incoming container loan or its CFG rebinding"
                        ),
                        Operation::Drop { owner } | Operation::Consume { owner } => assert_ne!(
                            size.entity(EntityId::Value(owner)).unwrap().ty,
                            EntityType::Value(target),
                            "Borrow size cannot drop or consume a container owner"
                        ),
                        _ => {}
                    }
                }
            }
        }
    }

    fn assert_no_callee_cleanup(function: &Function) {
        assert!(
            !function.instructions.iter().any(|instruction| matches!(
                instruction.operation,
                Operation::Drop { .. } | Operation::BorrowEnd { .. }
            )),
            "Borrow reads preserve the incoming loan; own returns transfer their owner"
        );
    }

    fn functions<'a>(program: &'a Program, name: &str) -> Vec<&'a Function> {
        program.modules[0]
            .functions
            .iter()
            .filter(|function| function.name.contains(name))
            .collect()
    }

    fn fixture(provider: &str, consumer: &str, intrinsic: IntrinsicTypeConstructor) -> Program {
        let mut sources = SourceMap::new();
        let (p_source, p) = parsed(&mut sources, "p/provider.ko", provider);
        let (q_source, q) = parsed(&mut sources, "q/consumer.ko", consumer);
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", p_source, &p),
            SourceUnitInput::new("root", "q/consumer.ko", q_source, &q),
        ];
        let reversed = [inputs[1], inputs[0]];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        for builtin in [BuiltinType::Int, BuiltinType::String] {
            let element = typed.types().types().builtin(builtin).unwrap();
            assert!(
                typed
                    .types()
                    .types()
                    .find(&UnitTypeKind::Intrinsic {
                        constructor: intrinsic,
                        arguments: vec![element],
                    })
                    .is_some(),
                "real calls and signatures publish the concrete container before backend consumption"
            );
        }
        let arena_len = typed.types().types().len();
        let entry = declaration(&names, "q", "entry");
        let unit =
            owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
                .expect("sealed Unit view");
        let forward = lower_owned_unit_with_entry(&unit, entry);
        assert_eq!(typed.types().types().len(), arena_len);
        let (program, _) = forward.expect("direct generic signatures use existing concrete types");
        let unit =
            owned_compilation_unit_view(&sources, &reversed, &names, &environment, &typed, &owned)
                .expect("sealed Unit view");
        let backward = lower_owned_unit_with_entry(&unit, entry);
        assert_eq!(typed.types().types().len(), arena_len);
        let (permuted, _) = backward.expect("source identity survives input permutation");
        assert_text_eq!(render_program(&program), render_program(&permuted));
        assert_text_eq!(
            crate::llvm::render_verified_program(&program).unwrap(),
            crate::llvm::render_verified_program(&permuted).unwrap()
        );
        program
    }
}
