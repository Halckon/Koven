use lang_frontend::source::SourceMap;

use super::{
    model::{
        EntityId, EntityType, FunctionId, LoanKind, Operation, Origin, Program, SsaTypeId,
        SsaTypeKind, TerminatorKind,
    },
    verify::verify_program,
};

fn origin() -> Origin {
    let mut sources = SourceMap::new();
    let source = sources.add_source("deinit.ko", "class Guard").unwrap();
    Origin::Source(sources.span(source, 0, 5).unwrap())
}

fn fixture() -> (Program, SsaTypeId, FunctionId) {
    let mut program = Program::default();
    let module_id = program.add_module("resource");
    let module = program.module_mut(module_id).unwrap();
    let owner = module.declare_heap_owner("Guard").unwrap();
    let child = module.declare_heap_owner("Child").unwrap();
    let empty = module.add_aggregate_type("Child.fields", vec![]).unwrap();
    module.define_heap_owner(child, empty).unwrap();
    let payload = module
        .add_aggregate_type("Guard.fields", vec![child, child])
        .unwrap();
    module.define_heap_owner(owner, payload).unwrap();
    let receiver = EntityType::Loan {
        kind: LoanKind::Shared,
        target: owner,
    };
    let deinit = module
        .add_instance_function("Guard.deinit", receiver, vec![], origin())
        .unwrap();
    let body = module.function_mut(deinit).unwrap();
    let entry = body.add_block(vec![receiver], origin()).unwrap();
    body.set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin())
        .unwrap();
    (program, owner, deinit)
}

#[test]
fn deinit_registration_preserves_exact_readonly_receiver_and_rejects_duplicates() {
    let (mut program, owner, deinit) = fixture();
    let module = program.module_mut(owner.module()).unwrap();
    module.set_deinit(owner, deinit).unwrap();
    assert_eq!(module.deinit(owner), Some(deinit));
    assert!(module.set_deinit(owner, deinit).is_err());
    verify_program(&program).unwrap();
    let rendered = super::render_program(&program);
    assert!(rendered.contains("deinit !t0 = @f0"), "{rendered}");
}

#[test]
fn deinit_rejects_non_owner_wrong_receiver_result_and_foreign_function() {
    let (mut program, owner, deinit) = fixture();
    let foreign_module = program.add_module("foreign");
    let foreign = program
        .module_mut(foreign_module)
        .unwrap()
        .add_function("bad", vec![], origin())
        .unwrap();
    let module = program.module_mut(owner.module()).unwrap();
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    assert!(module.set_deinit(integer, deinit).is_err());
    assert!(module.set_deinit(owner, foreign).is_err());
    for receiver in [
        EntityType::Value(owner),
        EntityType::Loan {
            kind: LoanKind::Exclusive,
            target: owner,
        },
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: integer,
        },
    ] {
        let bad = module
            .add_instance_function("bad", receiver, vec![], origin())
            .unwrap();
        assert!(module.set_deinit(owner, bad).is_err());
    }
    let bad_result = module
        .add_instance_function(
            "bad_result",
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner,
            },
            vec![integer],
            origin(),
        )
        .unwrap();
    assert!(module.set_deinit(owner, bad_result).is_err());
}

#[test]
fn deinit_verifier_rechecks_mutated_signature_and_extra_parameters() {
    let (mut program, owner, deinit) = fixture();
    let module = program.module_mut(owner.module()).unwrap();
    module.set_deinit(owner, deinit).unwrap();
    module.function_mut(deinit).unwrap().receiver = None;
    assert!(verify_program(&program).is_err());
    let (mut program, owner, deinit) = fixture();
    let module = program.module_mut(owner.module()).unwrap();
    module.set_deinit(owner, deinit).unwrap();
    let body = module.function_mut(deinit).unwrap();
    let receiver = body.blocks[0].parameters[0];
    body.blocks[0].parameters.push(receiver);
    assert!(verify_program(&program).is_err());
}

#[test]
fn llvm_deinit_calls_body_before_reverse_field_cleanup_and_free() {
    let (mut program, owner, deinit) = fixture();
    let module = program.module_mut(owner.module()).unwrap();
    module.set_deinit(owner, deinit).unwrap();
    let caller = module.add_function("release", vec![], origin()).unwrap();
    let body = module.function_mut(caller).unwrap();
    let entry = body
        .add_block(vec![EntityType::Value(owner)], origin())
        .unwrap();
    let EntityId::Value(input) = body.blocks[0].parameters[0] else {
        panic!("owner")
    };
    body.append_instruction(entry, Operation::Drop { owner: input }, vec![], origin())
        .unwrap();
    body.set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin())
        .unwrap();
    let ir = crate::llvm::render_verified_program(&program).unwrap();
    let glue = ir
        .split("define internal void @koven.drop.t0(")
        .nth(1)
        .expect("owner glue")
        .split("\n}")
        .next()
        .unwrap();
    let user = glue.find("call void @f0.Guard.deinit").expect("body call");
    let fields = glue
        .find("call void @koven.drop.t3")
        .expect("payload cleanup");
    let free = glue.find("call void @free").unwrap();
    assert!(user < fields && fields < free, "{glue}");
    assert!(glue.contains("alloca ptr"), "{glue}");
    let payload = ir
        .split("define internal void @koven.drop.t3(")
        .nth(1)
        .unwrap()
        .split("\n}")
        .next()
        .unwrap();
    assert!(
        payload.find("%field1 =").unwrap() < payload.find("%field0 =").unwrap(),
        "{payload}"
    );
}

#[test]
fn deinit_verifier_rejects_foreign_metadata_and_owned_receiver_after_registration() {
    let (mut program, owner, deinit) = fixture();
    let module = program.module_mut(owner.module()).unwrap();
    module.set_deinit(owner, deinit).unwrap();
    module.function_mut(deinit).unwrap().receiver = Some(EntityType::Value(owner));
    assert!(verify_program(&program).is_err());
    let (mut program, owner, deinit) = fixture();
    let foreign = program.add_module("foreign");
    let invalid_owner = program
        .module_mut(foreign)
        .unwrap()
        .declare_heap_owner("Foreign")
        .unwrap();
    program
        .module_mut(owner.module())
        .unwrap()
        .deinits
        .insert(invalid_owner, deinit);
    assert!(verify_program(&program).is_err());
}
