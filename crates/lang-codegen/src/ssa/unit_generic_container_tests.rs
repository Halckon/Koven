//! SPEC-0275: direct generic container signatures consume existing concrete identities.

use super::{
    model::{
        Definition, EntityId, EntityType, Function, LoanKind, Operation, Program,
        SequentialContainerKind, SsaTypeKind, TerminatorKind,
    },
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use lang_frontend::{
    name_resolution::SourceUnitInput,
    source::SourceMap,
    type_checking::{BuiltinType, IntrinsicTypeConstructor, UnitTypeKind, standard_environments},
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
    let forward = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        entry,
    );
    assert_eq!(typed.types().types().len(), arena_len);
    let (program, _) = forward.expect("direct generic signatures use existing concrete types");
    let backward = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &environment,
        &typed,
        &owned,
        entry,
    );
    assert_eq!(typed.types().types().len(), arena_len);
    let (permuted, _) = backward.expect("source identity survives input permutation");
    assert_eq!(render_program(&program), render_program(&permuted));
    assert_eq!(
        crate::llvm::render_verified_program(&program).unwrap(),
        crate::llvm::render_verified_program(&permuted).unwrap()
    );
    program
}
