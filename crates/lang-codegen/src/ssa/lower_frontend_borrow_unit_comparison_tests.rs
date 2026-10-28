//! Stage A: unchanged original sources through strict MemoryDocument Unit admission.
//! Legacy tests remain intact. Selected-root programs are not whole-file equivalence;
//! LLVM verification below is not native execution or allocation/free evidence.
use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

use super::{
    model::{EntityId, EntityType, LoanKind, Operation, Origin},
    render::render_program,
    verify::verify_program,
};
use crate::{
    llvm::render_verified_program, native_tests::boxed_enum_tests::unit_source::UnitSource,
};

fn analyze(text: &str) -> UnitSource {
    let unit = UnitSource::analyze("lowering.ko", text).expect("strict original Unit admission");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument, no wrapper or source split");
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
fn repeated_class_and_box_root_borrows_unit_comparison() {
    let text = SOURCE_CLASS;
    let unit = analyze(text);
    let declaration = unit.declaration("exercise", SymbolKind::Function);
    let span = unit.declaration_span(declaration);
    assert_eq!(
        unit.sources().slice(span),
        Ok(&text[text.find("fun exercise").unwrap()..])
    );
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("original exercise Unit root");
    let module = program.module(root.module()).expect("root module");
    let function = module.function(root).expect("root function");
    assert_eq!(function.origin, Origin::Source(span));
    assert!(!function.blocks.is_empty());
    let mut targets = Vec::new();
    for (name, source) in [
        ("inspect", "fun inspect(resource: Resource): Unit {}"),
        (
            "inspectBox",
            "fun inspectBox(resource: Box<Token>): Unit {}",
        ),
    ] {
        let declaration = unit.declaration(name, SymbolKind::Function);
        let span = unit.declaration_span(declaration);
        assert_eq!(unit.sources().slice(span), Ok(source));
        let callees = module
            .functions
            .iter()
            .filter(|callee| callee.origin == Origin::Source(span))
            .collect::<Vec<_>>();
        let [callee] = callees.as_slice() else {
            panic!("one exact source callee");
        };
        let [parameter @ EntityId::Loan(_)] = callee.blocks[0].parameters.as_slice() else {
            panic!("one original Borrow parameter");
        };
        assert!(matches!(
            callee.entity(*parameter).expect("parameter").ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ));
        targets.push(callee.id);
    }
    assert_ne!(targets[0], targets[1]);
    let calls = function
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } = &instruction.operation
            {
                assert!(receiver.is_none());
                assert!(matches!(arguments.as_slice(), [EntityId::Loan(_)]));
                Some(*callee)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(calls, [targets[0], targets[0], targets[1], targets[1]]);
    verify_program(&program).expect("complete original Unit SSA verifies");
    let ssa = render_program(&program);
    for operation in ["root_place", "borrow.shared", "end_borrow"] {
        assert_eq!(ssa.matches(operation).count(), 4, "{operation}: {ssa}");
    }
    // Preserve the two per-callee call-count oracles without legacy @f0/@f1 assumptions.
    for target in targets {
        assert_eq!(
            ssa.matches(&format!("call @f{}(", target.index())).count(),
            2,
            "{ssa}"
        );
    }
    assert!(!ssa.contains("shared.payload_place"), "{ssa}");
    render_verified_program(&program).expect("original Unit LLVM verifies");
}

#[test]
fn string_binary_views_value_calls_and_returns_unit_comparison() {
    let text = SOURCE_STRING;
    let unit = analyze(text);
    let declaration = unit.declaration("combine", SymbolKind::Function);
    let span = unit.declaration_span(declaration);
    assert_eq!(
        unit.sources().slice(span),
        Ok(&text[text.find("fun combine").unwrap()..])
    );
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("original combine Unit root");
    let module = program.module(root.module()).expect("root module");
    let combine = module.function(root).expect("exact combine function");
    assert_eq!(combine.origin, Origin::Source(span));
    let [left @ EntityId::Loan(_), EntityId::Value(_)] = combine.blocks[0].parameters.as_slice()
    else {
        panic!("original left Borrow and own right Value parameters");
    };
    let identity = unit.declaration("identity", SymbolKind::Function);
    let identity_span = unit.declaration_span(identity);
    assert_eq!(
        unit.sources().slice(identity_span),
        Ok("fun identity(own input: String): String = input")
    );
    let callees = module
        .functions
        .iter()
        .filter(|callee| callee.origin == Origin::Source(identity_span))
        .collect::<Vec<_>>();
    let [callee] = callees.as_slice() else {
        panic!("one exact identity declaration");
    };
    assert!(matches!(
        callee.blocks[0].parameters.as_slice(),
        [EntityId::Value(_)]
    ));
    let calls = combine
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
        panic!("one actual identity call");
    };
    assert!(receiver.is_none());
    assert!(matches!(arguments.as_slice(), [EntityId::Value(_)]));
    let concat = combine.instructions.iter().position(|instruction| {
        matches!(instruction.operation, Operation::StringConcat { left: operand, .. } if operand == *left)
    }).expect("concat reads actual left entry loan");
    let first_drop = combine
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .expect("operand drop");
    assert!(concat < first_drop, "{:?}", combine.instructions);
    verify_program(&program).expect("complete original Unit SSA verifies");
    let ssa = render_program(&program);
    // The legacy %l0 oracle is retained above by actual entry-loan identity instead.
    for operation in ["string.concat", "string.equal", "not ", "call @"] {
        assert!(ssa.contains(operation), "{operation}: {ssa}");
    }
    render_verified_program(&program).expect("original Unit LLVM verifies");
}

const SOURCE_CLASS: &str = "class Resource {}\n\
         value class Token(val item: Int)\n\
         fun inspect(resource: Resource): Unit {}\n\
         fun inspectBox(resource: Box<Token>): Unit {}\n\
         fun exercise(): Unit {\n\
             val resource = Resource()\n\
             val first = inspect(resource)\n\
             val second = inspect(resource)\n\
             val boxed = Box(Token(1))\n\
             val third = inspectBox(boxed)\n\
             val fourth = inspectBox(boxed)\n\
         }";

const SOURCE_STRING: &str = "fun identity(own input: String): String = input\n\
         fun combine(left: String, own right: String): String {\n\
             val joined = left + \"!\"\n\
             val differs = joined != right\n\
             return identity(right)\n\
         }";
