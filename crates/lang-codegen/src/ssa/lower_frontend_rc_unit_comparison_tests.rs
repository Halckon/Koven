//! Stage A same-source observations: the legacy tests remain unchanged.
use lang_frontend::{
    name_resolution::{SourceUnitKey, SymbolKind},
    type_checking::UnitConstructionTarget,
};

use super::{
    model::{EntityId, EntityType, Function, LoanKind, Module, Operation, Origin, SsaTypeKind},
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

// Bind the rendered owner to the admitted source construction, not a legacy TypeId.
fn assert_rc_identity(
    unit: &UnitSource,
    module: &Module,
    function: &Function,
    source: &str,
) -> String {
    let constructions = unit
        .types()
        .constructions()
        .iter()
        .filter(|construction| construction.target() == UnitConstructionTarget::IntrinsicRc)
        .collect::<Vec<_>>();
    let [construction] = constructions.as_slice() else {
        panic!("one source Rc construction")
    };
    let [document] = unit.names().names().index().source_units() else {
        panic!("one source")
    };
    assert_eq!(construction.expression().source_unit(), document.id());
    let expression = unit
        .parsed()
        .ast()
        .expressions()
        .get(construction.expression().expression())
        .expect("original Rc expression");
    assert_eq!(unit.sources().slice(expression.span()), Ok(source));
    let allocations = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::SharedAllocate { owner, .. } => Some((owner, &instruction.origin)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(owner, origin)] = allocations.as_slice() else {
        panic!("one root Rc allocation")
    };
    assert_eq!(**origin, Origin::Source(expression.span()));
    let SsaTypeKind::SharedOwner {
        name,
        payload: Some(_),
    } = &module.types[owner.index()]
    else {
        panic!("defined Rc owner")
    };
    let expected = format!("Rc#u{}", construction.result_type().index());
    assert_eq!(name, &expected);
    expected
}

fn ssa_body<'a>(ssa: &'a str, function: &Function) -> &'a str {
    let marker = format!("  func {:?}(", function.name);
    let (_, body) = ssa
        .split_once(&marker)
        .expect("source-selected SSA function");
    body.split_once("\n  }").expect("SSA function end").0
}

fn llvm_body<'a>(llvm: &'a str, function: &Function) -> &'a str {
    let marker = format!("@f{}.{}(", function.id.index(), function.name);
    let definition = llvm
        .lines()
        .find(|line| line.starts_with("define ") && line.contains(&marker))
        .expect("source-selected LLVM definition");
    let (_, body) = llvm.split_once(definition).expect("LLVM definition");
    body.split_once("\n}").expect("LLVM function end").0
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
    let rc_name = assert_rc_identity(&unit, module, function, "Rc(40)");
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
    assert!(
        ssa.contains(&format!("shared_owner {rc_name:?} payload ")),
        "{ssa}"
    );
    assert_eq!(ssa.matches("shared.allocate").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("shared.retain").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("shared.payload_place").count(), 2, "{ssa}");
    assert_eq!(ssa.matches("drop ").count(), 2, "{ssa}");
    let llvm = render_verified_program(&program).expect("complete Unit LLVM verification");
    let root_ssa = ssa_body(&ssa, function);
    for (operation, count) in [
        ("shared.allocate", 1),
        ("shared.retain", 1),
        ("shared.payload_place", 2),
        ("drop ", 2),
    ] {
        assert_eq!(root_ssa.matches(operation).count(), count, "{root_ssa}");
    }
    let root_llvm = llvm_body(&llvm, function);
    assert_eq!(
        root_llvm.matches("call ptr @malloc").count(),
        1,
        "{root_llvm}"
    );
    assert!(root_llvm.contains(".overflow = icmp eq"), "{root_llvm}");
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
    let rc_name = assert_rc_identity(&unit, module, function, "Rc(Resource())");
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
    let [EntityId::Loan(loan)] = arguments.as_slice() else {
        panic!("exact caller loan")
    };
    let payloads = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::SharedPayloadPlace { .. }))
        .collect::<Vec<_>>();
    let [payload] = payloads.as_slice() else {
        panic!("one payload projection")
    };
    let [EntityId::Place(place)] = payload.results.as_slice() else {
        panic!("payload place result")
    };
    // Check execution order in the containing block, not instruction arena order.
    let block = &function.blocks[payload.block.index()];
    let operations = block
        .instructions
        .iter()
        .map(|id| function.instruction(*id).unwrap())
        .collect::<Vec<_>>();
    let projection = operations
        .iter()
        .position(|instruction| instruction.id == payload.id)
        .unwrap();
    let borrow = operations
        .iter()
        .position(|instruction| {
            instruction.operation
                == Operation::BorrowBegin {
                    place: *place,
                    kind: LoanKind::Shared,
                }
                && instruction.results == [EntityId::Loan(*loan)]
        })
        .expect("payload place produces the exact call loan");
    let call = operations
        .iter()
        .position(|instruction| {
            matches!(&instruction.operation,
                Operation::DirectCall { callee: target, receiver: None, arguments }
                    if *target == callee.id && arguments == &[EntityId::Loan(*loan)]
            )
        })
        .expect("same-block call consumes exact loan");
    let end = operations
        .iter()
        .position(|instruction| instruction.operation == Operation::BorrowEnd { loan: *loan })
        .expect("same-block end of exact loan");
    assert!(projection < borrow && borrow < call && call < end);
    let EntityId::Loan(parameter) = parameter else {
        panic!("callee Borrow parameter")
    };
    verify_program(&program).expect("complete Unit SSA verification");
    let ssa = render_program(&program);
    assert!(
        ssa.contains(&format!("shared_owner {rc_name:?} payload ")),
        "{ssa}"
    );
    let root_ssa = ssa_body(&ssa, function);
    assert!(
        root_ssa.contains(&format!("call @f{}(%l{})", callee.id.index(), loan.index())),
        "{root_ssa}"
    );
    for operation in [
        "shared.payload_place",
        "borrow.shared",
        "end_borrow",
        "shared.retain",
    ] {
        assert_eq!(ssa.matches(operation).count(), 1, "{operation}: {ssa}");
        assert_eq!(root_ssa.matches(operation).count(), 1, "{root_ssa}");
    }
    let llvm = render_verified_program(&program).expect("complete Unit LLVM verification");
    let symbol = format!("f{}.{}", callee.id.index(), callee.name);
    let root_llvm = llvm_body(&llvm, function);
    let pointer_definition = format!("%p{} = getelementptr", place.index());
    let pointer_position = root_llvm
        .find(&pointer_definition)
        .expect("payload GEP defines caller pointer");
    let call_text = format!("call void @{symbol}(ptr %p{})", place.index());
    let call_position = root_llvm
        .find(&call_text)
        .expect("exact payload pointer passed to inspect");
    assert!(pointer_position < call_position);
    assert!(
        llvm.contains(&format!(
            "define internal void @{symbol}(ptr %l{})",
            parameter.index()
        )),
        "{llvm}"
    );
    assert_eq!(llvm.matches("call ptr @malloc").count(), 2, "{llvm}");
    assert_eq!(llvm.matches("call void @free").count(), 2, "{llvm}");
}
