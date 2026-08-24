use lang_frontend::source::SourceMap;

use super::{
    model::{ModelError, Origin, Ownership, Program, SsaTypeId, SsaTypeKind},
    render::render_program,
    verify::{VerifyErrorKind, VerifyLocation, verify_program},
};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("aggregate-types.ko", "value class Pair")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 5).expect("test span must be valid"))
}

#[test]
fn named_aggregate_types_derive_ownership_and_render_deterministically() {
    let mut program = Program::default();
    let module_id = program.add_module("aggregate");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let point = module
        .add_aggregate_type("Point", vec![integer, integer])
        .expect("copyable aggregate must be valid");
    let endpoint = module
        .add_aggregate_type("Endpoint", vec![resource])
        .expect("move-only aggregate must be valid");

    assert_eq!(module.type_ownership(point), Some(Ownership::Copyable));
    assert_eq!(module.type_ownership(endpoint), Some(Ownership::MoveOnly));
    assert_eq!(
        module.add_aggregate_type("Point", vec![integer]),
        Err(ModelError::DuplicateTypeName {
            name: "Point".to_owned()
        })
    );
    verify_program(&program).expect("derived aggregate definitions must verify");

    let first = render_program(&program);
    let second = render_program(&program);
    assert_eq!(first, second);
    assert!(first.contains("aggregate \"Point\" Copyable (!t0, !t0)"));
    assert!(first.contains("aggregate \"Endpoint\" MoveOnly (!t1)"));
}

#[test]
fn heap_owner_declaration_supports_recursive_handles_and_rejects_bad_definitions() {
    let mut program = Program::default();
    let module_id = program.add_module("heap");
    let module = program.module_mut(module_id).expect("module must exist");
    let node = module
        .declare_heap_owner("Node")
        .expect("heap owner declaration must be valid");
    module
        .define_heap_owner(node, vec![node])
        .expect("self-reference is indirect through the heap-owner handle");
    assert_eq!(module.type_ownership(node), Some(Ownership::MoveOnly));
    assert_eq!(
        module.define_heap_owner(node, Vec::new()),
        Err(ModelError::TypeAlreadyDefined { ty: node })
    );

    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: false,
    });
    assert_eq!(
        module.define_heap_owner(integer, Vec::new()),
        Err(ModelError::ExpectedHeapOwner { ty: integer })
    );
    verify_program(&program).expect("recursive heap-owner handle must verify");
    assert!(render_program(&program).contains("heap_owner \"Node\" (!t0)"));
}

#[test]
fn named_type_builder_rejects_cross_module_fields_and_accepts_declared_handles() {
    let mut program = Program::default();
    let first = program.add_module("first");
    let second = program.add_module("second");
    let foreign = program
        .module_mut(second)
        .expect("second module must exist")
        .intern_type(SsaTypeKind::Boolean);
    let first_module = program.module_mut(first).expect("first module must exist");
    assert!(matches!(
        first_module.add_aggregate_type("Wrong", vec![foreign]),
        Err(ModelError::WrongTypeOwner { .. })
    ));

    let declared = first_module
        .declare_heap_owner("Declared")
        .expect("declaration must be valid");
    let inline = first_module
        .add_aggregate_type("Inline", vec![declared])
        .expect("a declared heap handle already has a fixed indirect shape");
    assert_eq!(
        first_module.type_ownership(inline),
        Some(Ownership::MoveOnly)
    );
    first_module
        .define_heap_owner(declared, Vec::new())
        .expect("the payload must still be defined before verification");
    verify_program(&program).expect("fully defined handle and aggregate must verify");
}

#[test]
fn verifier_rejects_undefined_mismatched_and_inline_cyclic_types() {
    let mut program = Program::default();
    let module_id = program.add_module("invalid");
    let module = program.module_mut(module_id).expect("module must exist");
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let mismatched = SsaTypeId {
        module: module_id,
        index: module.types.len(),
    };
    module.types.push(SsaTypeKind::Aggregate {
        name: "Mismatched".to_owned(),
        fields: vec![resource],
        ownership: Ownership::Copyable,
    });
    let cyclic = SsaTypeId {
        module: module_id,
        index: module.types.len(),
    };
    module.types.push(SsaTypeKind::Aggregate {
        name: "Cyclic".to_owned(),
        fields: vec![cyclic],
        ownership: Ownership::Copyable,
    });
    let undefined = module
        .declare_heap_owner("Undefined")
        .expect("declaration itself is valid");

    let errors = verify_program(&program).expect_err("invalid type definitions must be rejected");
    assert!(errors.errors.iter().any(|error| {
        error.location == VerifyLocation::Type(mismatched)
            && matches!(
                error.kind,
                VerifyErrorKind::InvalidTypeDefinition {
                    reason: "aggregate ownership must be derived from all fields"
                }
            )
    }));
    assert!(errors.errors.iter().any(|error| {
        error.location == VerifyLocation::Type(cyclic)
            && matches!(
                error.kind,
                VerifyErrorKind::InvalidTypeDefinition {
                    reason: "aggregate fields must not form an inline cycle"
                }
            )
    }));
    assert!(errors.errors.iter().any(|error| {
        error.location == VerifyLocation::Type(undefined)
            && matches!(
                error.kind,
                VerifyErrorKind::InvalidTypeDefinition {
                    reason: "heap owner declaration must be defined"
                }
            )
    }));
}

#[test]
fn named_types_can_be_used_in_function_signatures_after_definition() {
    let mut program = Program::default();
    let module_id = program.add_module("signature");
    let module = program.module_mut(module_id).expect("module must exist");
    let owner = module
        .declare_heap_owner("Owner")
        .expect("declaration must be valid");
    module
        .add_function("identity", vec![owner], origin())
        .expect("declarations can enter signatures before their payload is defined");
    module
        .define_heap_owner(owner, Vec::new())
        .expect("payload definition must succeed");
}
