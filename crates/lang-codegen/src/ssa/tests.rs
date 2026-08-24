use lang_frontend::source::SourceMap;

use super::{
    model::{
        BinaryOperator, Edge, EntityId, EntityType, LoanKind, ModelError, Operation, Origin,
        Ownership, Program, ScalarConstant, SsaTypeKind, TerminatorKind, ValueId,
    },
    render::render_program,
};

fn origins() -> (Origin, Origin) {
    let mut sources = SourceMap::default();
    let source_id = sources
        .add_source("ssa-model.ko", "fun main")
        .expect("test source must be unique");
    let source = Origin::Source(
        sources
            .span(source_id, 0, 4)
            .expect("test source span must be valid"),
    );
    let synthetic = Origin::Synthetic {
        anchor: sources
            .span(source_id, 4, 8)
            .expect("test synthetic anchor must be valid"),
        reason: "cfg split".to_owned(),
    };
    (source, synthetic)
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value entity, got {entity:?}");
    };
    value
}

#[test]
fn type_ids_are_deduplicated_and_reject_cross_program_owners() {
    let mut first = Program::default();
    let first_module = first.add_module("main");
    let module = first
        .module_mut(first_module)
        .expect("new module must be available");
    let unit = module.intern_type(SsaTypeKind::Unit);
    let repeated_unit = module.intern_type(SsaTypeKind::Unit);
    let endpoint = module.intern_type(SsaTypeKind::Opaque {
        name: "Endpoint".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    assert_eq!(unit, repeated_unit);
    assert_ne!(unit, endpoint);
    assert_eq!(module.types.len(), 2);

    let mut second = Program::default();
    let second_module = second.add_module("main");
    let foreign_unit = second
        .module_mut(second_module)
        .expect("new module must be available")
        .intern_type(SsaTypeKind::Unit);

    assert_ne!(first_module, second_module);
    assert_eq!(format!("{first_module:?}"), format!("{second_module:?}"));
    let error = first
        .module_mut(first_module)
        .expect("first module must remain available")
        .add_function("wrong_owner", vec![foreign_unit], origins().0)
        .expect_err("a type from another Program must be rejected");
    assert!(matches!(error, ModelError::WrongTypeOwner { .. }));
}

#[test]
fn builder_rejects_cross_function_entities_and_post_terminator_edits() {
    let (source, _) = origins();
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program
        .module_mut(module_id)
        .expect("new module must be available");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let first = module
        .add_function("first", Vec::new(), source.clone())
        .expect("local function signature must be valid");
    let second = module
        .add_function("second", Vec::new(), source.clone())
        .expect("local function signature must be valid");

    let first_block = module
        .function_mut(first)
        .expect("first function must be available")
        .add_block(Vec::new(), source.clone())
        .expect("entry block must be valid");
    let foreign_value = {
        let function = module
            .function_mut(second)
            .expect("second function must be available");
        let block = function
            .add_block(vec![EntityType::Value(integer)], source.clone())
            .expect("entry parameter must be valid");
        value(function.block(block).expect("block must exist").parameters[0])
    };

    let function = module
        .function_mut(first)
        .expect("first function must remain available");
    let error = function
        .append_instruction(
            first_block,
            Operation::Copy {
                source: foreign_value,
            },
            vec![EntityType::Value(integer)],
            source.clone(),
        )
        .expect_err("an entity from another function must be rejected");
    assert!(matches!(error, ModelError::WrongFunctionOwner { .. }));

    function
        .set_terminator(first_block, TerminatorKind::Abort, source.clone())
        .expect("first terminator must be accepted");
    assert!(matches!(
        function.set_terminator(first_block, TerminatorKind::Abort, source.clone()),
        Err(ModelError::TerminatorAlreadySet { .. })
    ));
    assert!(matches!(
        function.append_instruction(
            first_block,
            Operation::Constant(ScalarConstant::Integer(1)),
            vec![EntityType::Value(integer)],
            source,
        ),
        Err(ModelError::BlockAlreadyTerminated { .. })
    ));
}

#[test]
fn scalar_diamond_rendering_is_exact_and_deterministic() {
    let (source, synthetic) = origins();
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program
        .module_mut(module_id)
        .expect("new module must be available");
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let function_id = module
        .add_function("select", vec![integer], source.clone())
        .expect("function signature must be valid");
    let function = module
        .function_mut(function_id)
        .expect("new function must be available");

    let entry = function
        .add_block(
            vec![EntityType::Value(boolean), EntityType::Value(integer)],
            source.clone(),
        )
        .expect("entry block must be valid");
    let when_true = function
        .add_block(vec![EntityType::Value(integer)], source.clone())
        .expect("true block must be valid");
    let when_false = function
        .add_block(vec![EntityType::Value(integer)], synthetic.clone())
        .expect("false block must be valid");
    let join = function
        .add_block(vec![EntityType::Value(integer)], source.clone())
        .expect("join block must be valid");

    let entry_parameters = &function
        .block(entry)
        .expect("entry block must exist")
        .parameters;
    let condition = value(entry_parameters[0]);
    let input = entry_parameters[1];
    function
        .set_terminator(
            entry,
            TerminatorKind::Conditional {
                condition,
                when_true: Edge {
                    target: when_true,
                    arguments: vec![input],
                },
                when_false: Edge {
                    target: when_false,
                    arguments: vec![input],
                },
            },
            synthetic.clone(),
        )
        .expect("conditional terminator must be valid");

    let true_parameter = value(
        function
            .block(when_true)
            .expect("true block must exist")
            .parameters[0],
    );
    let (_, one) = function
        .append_instruction(
            when_true,
            Operation::Constant(ScalarConstant::Integer(1)),
            vec![EntityType::Value(integer)],
            source.clone(),
        )
        .expect("constant must be appendable");
    let (_, incremented) = function
        .append_instruction(
            when_true,
            Operation::Binary {
                operator: BinaryOperator::Add,
                left: true_parameter,
                right: value(one[0]),
            },
            vec![EntityType::Value(integer)],
            source.clone(),
        )
        .expect("add must be appendable");
    function
        .set_terminator(
            when_true,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: incremented,
            }),
            source.clone(),
        )
        .expect("true branch must be valid");

    let false_parameter = value(
        function
            .block(when_false)
            .expect("false block must exist")
            .parameters[0],
    );
    let (_, one) = function
        .append_instruction(
            when_false,
            Operation::Constant(ScalarConstant::Integer(1)),
            vec![EntityType::Value(integer)],
            source.clone(),
        )
        .expect("constant must be appendable");
    let (_, decremented) = function
        .append_instruction(
            when_false,
            Operation::Binary {
                operator: BinaryOperator::Subtract,
                left: false_parameter,
                right: value(one[0]),
            },
            vec![EntityType::Value(integer)],
            synthetic.clone(),
        )
        .expect("subtract must be appendable");
    function
        .set_terminator(
            when_false,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: decremented,
            }),
            synthetic.clone(),
        )
        .expect("false branch must be valid");

    let result = value(
        function
            .block(join)
            .expect("join block must exist")
            .parameters[0],
    );
    function
        .set_terminator(
            join,
            TerminatorKind::Return {
                values: vec![result],
            },
            source,
        )
        .expect("return terminator must be valid");

    let rendered = render_program(&program);
    assert_eq!(rendered, render_program(&program));
    assert_eq!(
        rendered,
        r#"module "main" {
  !t0 = bool
  !t1 = i64

  func "select"(%v0: value !t0, %v1: value !t1) -> (!t1) @source(SourceId(0):0..4) {
    bb0(%v0: value !t0, %v1: value !t1) @source(SourceId(0):0..4):
      cond %v0, bb1(%v1), bb2(%v1) @synthetic("cfg split", SourceId(0):4..8)
    bb1(%v2: value !t1) @source(SourceId(0):0..4):
      i0: %v5 = const 1 : (value !t1) @source(SourceId(0):0..4)
      i1: %v6 = add %v2, %v5 : (value !t1) @source(SourceId(0):0..4)
      branch bb3(%v6) @source(SourceId(0):0..4)
    bb2(%v3: value !t1) @synthetic("cfg split", SourceId(0):4..8):
      i2: %v7 = const 1 : (value !t1) @source(SourceId(0):0..4)
      i3: %v8 = sub %v3, %v7 : (value !t1) @synthetic("cfg split", SourceId(0):4..8)
      branch bb3(%v8) @synthetic("cfg split", SourceId(0):4..8)
    bb3(%v4: value !t1) @source(SourceId(0):0..4):
      return %v4 @source(SourceId(0):0..4)
  }
}
"#
    );
}

#[test]
fn place_and_loan_entities_keep_distinct_id_spaces() {
    let (source, _) = origins();
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program
        .module_mut(module_id)
        .expect("new module must be available");
    let endpoint = module.intern_type(SsaTypeKind::Opaque {
        name: "Endpoint".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let function_id = module
        .add_function("borrow", Vec::new(), source.clone())
        .expect("function signature must be valid");
    let function = module
        .function_mut(function_id)
        .expect("new function must be available");
    let entry = function
        .add_block(vec![EntityType::Value(endpoint)], source.clone())
        .expect("entry block must be valid");
    let owner = value(
        function
            .block(entry)
            .expect("entry block must exist")
            .parameters[0],
    );
    let (_, place) = function
        .append_instruction(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(endpoint)],
            source.clone(),
        )
        .expect("root place must be appendable");
    let EntityId::Place(place) = place[0] else {
        panic!("root place result must use the place ID space");
    };
    let (_, loan) = function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: endpoint,
            }],
            source.clone(),
        )
        .expect("loan must be appendable");
    let EntityId::Loan(loan) = loan[0] else {
        panic!("borrow result must use the loan ID space");
    };
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan },
            Vec::new(),
            source.clone(),
        )
        .expect("borrow end must be appendable");
    function
        .append_instruction(entry, Operation::Drop { owner }, Vec::new(), source.clone())
        .expect("drop must be appendable");
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, source)
        .expect("return must be accepted");

    let rendered = render_program(&program);
    assert!(rendered.contains("%p0 = root_place %v0 : (place !t0)"));
    assert!(rendered.contains("%l0 = borrow.shared %p0 : (loan.shared !t0)"));
    assert!(rendered.contains("end_borrow %l0"));
    assert!(rendered.contains("drop %v0"));
}
