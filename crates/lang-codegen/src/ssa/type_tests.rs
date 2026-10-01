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
    let payload = module
        .add_aggregate_type("Node.payload", vec![node])
        .expect("heap handles break inline payload recursion");
    module
        .define_heap_owner(node, payload)
        .expect("self-reference is indirect through the heap-owner handle");
    assert_eq!(module.type_ownership(node), Some(Ownership::MoveOnly));
    assert_eq!(
        module.define_heap_owner(node, payload),
        Err(ModelError::TypeAlreadyDefined { ty: node })
    );

    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: false,
    });
    let undeclared = module
        .declare_heap_owner("UndeclaredPayload")
        .expect("second heap declaration must be valid");
    assert_eq!(
        module.define_heap_owner(undeclared, integer),
        Err(ModelError::ExpectedAggregate { ty: integer })
    );
    let empty_payload = module
        .add_aggregate_type("UndeclaredPayload.payload", Vec::new())
        .expect("empty payload aggregate must be valid");
    module
        .define_heap_owner(undeclared, empty_payload)
        .expect("failed definition must not poison the declaration");
    assert_eq!(
        module.define_heap_owner(integer, payload),
        Err(ModelError::ExpectedHeapOwner { ty: integer })
    );
    verify_program(&program).expect("recursive heap-owner handle must verify");
    assert!(render_program(&program).contains("heap_owner \"Node\" payload !t1"));
}

#[test]
fn boxed_enum_payload_definitions_verify_recursive_indirection() {
    let mut program = Program::default();
    let module_id = program.add_module("boxed-enum");
    let module = program.module_mut(module_id).unwrap();
    let boxed = module.declare_heap_owner("Box<Expr>").unwrap();
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let leaf = module
        .add_aggregate_type("Expr.Num", vec![integer])
        .unwrap();
    let branch = module
        .add_aggregate_type("Expr.Add", vec![boxed, boxed])
        .unwrap();
    let enumeration = module
        .add_tagged_union_type("Expr", vec![leaf, branch])
        .unwrap();
    module
        .define_heap_owner(boxed, enumeration)
        .expect("a tagged enum is a valid Box payload");
    assert_eq!(module.heap_payload(boxed), Some(enumeration));
    assert_eq!(module.type_ownership(boxed), Some(Ownership::MoveOnly));
    assert_eq!(
        module.type_ownership(enumeration),
        Some(Ownership::MoveOnly)
    );
    verify_program(&program).expect("Box handles break the enum's inline cycle");
    assert_eq!(render_program(&program), render_program(&program));

    let module = program.module_mut(module_id).unwrap();
    let SsaTypeKind::HeapOwner { payload, .. } = &mut module.types[boxed.index()] else {
        panic!("heap owner");
    };
    *payload = Some(integer);
    let errors =
        verify_program(&program).expect_err("forged scalar Box payload must remain invalid");
    assert!(
        errors
            .errors
            .iter()
            .any(|error| error.location == VerifyLocation::Type(boxed))
    );
}

#[test]
fn shared_owner_is_move_only_accepts_any_defined_payload_and_renders_stably() {
    let mut program = Program::default();
    let module_id = program.add_module("shared");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let owner = module
        .declare_shared_owner("Rc<Int>")
        .expect("shared owner declaration must be valid");
    module
        .define_shared_owner(owner, integer)
        .expect("a defined scalar payload must be valid");

    assert_eq!(module.type_ownership(owner), Some(Ownership::MoveOnly));
    assert_eq!(module.shared_payload(owner), Some(integer));
    assert_eq!(
        module.define_shared_owner(owner, integer),
        Err(ModelError::TypeAlreadyDefined { ty: owner })
    );
    assert_eq!(
        module.define_shared_owner(integer, integer),
        Err(ModelError::ExpectedSharedOwner { ty: integer })
    );
    verify_program(&program).expect("defined shared owner must verify");
    assert!(render_program(&program).contains("shared_owner \"Rc<Int>\" payload !t0"));
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
        .define_heap_owner(declared, inline)
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
        error.location == VerifyLocation::Type(mismatched)
            && matches!(
                error.kind,
                VerifyErrorKind::InvalidTypeDefinition {
                    reason: "named type identity must match the module name index"
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
        .add_aggregate_type("Owner.payload", Vec::new())
        .and_then(|payload| module.define_heap_owner(owner, payload))
        .expect("payload definition must succeed");
}
