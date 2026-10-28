//! Stage A same-source observations: the legacy tests remain unchanged.
use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

use super::{
    model::{EntityId, EntityType, LoanKind, Operation, Origin, SsaTypeKind},
    render::render_program,
    verify::verify_program,
};
use crate::{
    llvm::render_verified_program, native_tests::boxed_enum_tests::unit_source::UnitSource,
};

fn analyze(text: &str) -> UnitSource {
    let unit = UnitSource::analyze("lowering.ko", text).expect("strict Unit admission");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
    unit
}

#[test]
fn intrinsic_rc_construction_share_and_copyable_payload_read_unit_comparison() {
    let text = "fun shared(): Int {\n\
             val first = Rc(40)\n\
             val second = first.share()\n\
             val copied = second.value\n\
             return copied + first.value\n\
         }";
    let unit = analyze(text);
    let declaration = unit.declaration("shared", SymbolKind::Function);
    let span = unit.declaration_span(declaration);
    assert_eq!(unit.sources().slice(span), Ok(text));
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("original shared Unit root");
    let module = program.module(root.module()).expect("root module");
    let function = module.function(root).expect("root function");
    assert_eq!(function.origin, Origin::Source(span));
    assert!(!function.blocks.is_empty());
    let owners = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::SharedAllocate { owner, .. } => Some(owner),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [owner] = owners.as_slice() else {
        panic!("one actual Rc allocation");
    };
    let SsaTypeKind::SharedOwner {
        payload: Some(payload),
        ..
    } = &module.types[owner.index()]
    else {
        panic!("actual allocation has SharedOwner storage");
    };
    assert_eq!(
        module.types[payload.index()],
        SsaTypeKind::Integer {
            bits: 32,
            signed: true
        }
    );
    verify_program(&program).expect("complete Unit SSA verification");
    let ssa = render_program(&program);
    assert_eq!(ssa.matches("shared.allocate").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("shared.retain").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("shared.payload_place").count(), 2, "{ssa}");
    assert_eq!(ssa.matches("drop ").count(), 2, "{ssa}");
    let llvm = render_verified_program(&program).expect("complete Unit LLVM verification");
    assert_eq!(llvm.matches("call ptr @malloc").count(), 1, "{llvm}");
    assert_eq!(llvm.matches("call void @free").count(), 1, "{llvm}");
    assert!(llvm.contains(".overflow = icmp eq"), "{llvm}");
    assert_eq!(llvm.matches("strong.next = sub").count(), 1, "{llvm}");
    assert!(!llvm.contains("atomicrmw"), "{llvm}");
    assert!(!llvm.contains("cmpxchg"), "{llvm}");
}

#[test]
fn move_only_rc_payload_borrow_call_unit_comparison() {
    let text = "class Resource {}\n\
         fun inspect(resource: Resource): Unit {}\n\
         fun exercise(): Unit {\n\
             val owner = Rc(Resource())\n\
             val inspected = inspect(owner.value)\n\
             val retained = owner.share()\n\
         }";
    let unit = analyze(text);
    let declaration = unit.declaration("exercise", SymbolKind::Function);
    let span = unit.declaration_span(declaration);
    let original_root = &text[text
        .find("fun exercise")
        .expect("original exercise declaration")..];
    assert_eq!(unit.sources().slice(span), Ok(original_root));
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("original exercise Unit root");
    let module = program.module(root.module()).expect("root module");
    let function = module.function(root).expect("root function");
    assert_eq!(function.origin, Origin::Source(span));
    let inspect = unit.declaration("inspect", SymbolKind::Function);
    let inspect_span = unit.declaration_span(inspect);
    assert_eq!(
        unit.sources().slice(inspect_span),
        Ok("fun inspect(resource: Resource): Unit {}")
    );
    let callees = module
        .functions
        .iter()
        .filter(|function| function.origin == Origin::Source(inspect_span))
        .collect::<Vec<_>>();
    let [callee] = callees.as_slice() else {
        panic!("one exact inspect declaration");
    };
    let [parameter] = callee.blocks[0].parameters.as_slice() else {
        panic!("one Borrow parameter");
    };
    assert!(matches!(
        callee.entity(*parameter).map(|entity| entity.ty),
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        })
    ));
    let calls = function
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                callee: target,
                receiver,
                arguments,
            } if *target == callee.id => Some((receiver, arguments)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(receiver, arguments)] = calls.as_slice() else {
        panic!("one call to exact inspect declaration");
    };
    assert!(receiver.is_none());
    assert!(matches!(arguments.as_slice(), [EntityId::Loan(_)]));
    verify_program(&program).expect("complete Unit SSA verification");
    let ssa = render_program(&program);
    for operation in [
        "shared.payload_place",
        "borrow.shared",
        "end_borrow",
        "shared.retain",
    ] {
        assert_eq!(ssa.matches(operation).count(), 1, "{operation}: {ssa}");
    }
    let llvm = render_verified_program(&program).expect("complete Unit LLVM verification");
    let symbol = format!("f{}.{}", callee.id.index(), callee.name);
    assert!(
        llvm.contains(&format!("define internal void @{symbol}(ptr ")),
        "{llvm}"
    );
    assert!(
        llvm.contains(&format!("call void @{symbol}(ptr ")),
        "{llvm}"
    );
    assert_eq!(llvm.matches("call ptr @malloc").count(), 2, "{llvm}");
    assert_eq!(llvm.matches("call void @free").count(), 2, "{llvm}");
}
