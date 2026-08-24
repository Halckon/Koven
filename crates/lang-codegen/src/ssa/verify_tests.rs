use lang_frontend::source::SourceMap;

use super::{
    model::{
        BinaryOperator, BlockId, Edge, EntityId, EntityType, Function, FunctionId, ModuleId,
        Operation, Origin, Program, ScalarConstant, SsaTypeId, SsaTypeKind, TerminatorKind,
        ValueId,
    },
    verify::{VerifyError, VerifyErrorKind, VerifyLocation, verify_program},
};

struct Diamond {
    program: Program,
    module: ModuleId,
    function: FunctionId,
    entry: BlockId,
    when_true: BlockId,
    when_false: BlockId,
    join: BlockId,
    condition: ValueId,
    input: ValueId,
    true_result: ValueId,
    false_result: ValueId,
    boolean: SsaTypeId,
    origin: Origin,
}

impl Diamond {
    fn function_mut(&mut self) -> &mut Function {
        self.program
            .module_mut(self.module)
            .expect("fixture module must exist")
            .function_mut(self.function)
            .expect("fixture function must exist")
    }
}

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("ssa-verify.ko", "fun verify")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 3).expect("test span must be valid"))
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value entity, got {entity:?}");
    };
    value
}

fn diamond() -> Diamond {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program
        .module_mut(module_id)
        .expect("new module must exist");
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let function_id = module
        .add_function("diamond", vec![integer], origin.clone())
        .expect("function signature must be valid");
    let function = module
        .function_mut(function_id)
        .expect("new function must exist");

    let entry = function
        .add_block(
            vec![EntityType::Value(boolean), EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("entry must be valid");
    let when_true = function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("true block must be valid");
    let when_false = function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("false block must be valid");
    let join = function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("join must be valid");

    let entry_parameters = &function.block(entry).expect("entry must exist").parameters;
    let condition = value(entry_parameters[0]);
    let input_entity = entry_parameters[1];
    let input = value(input_entity);
    function
        .set_terminator(
            entry,
            TerminatorKind::Conditional {
                condition,
                when_true: Edge {
                    target: when_true,
                    arguments: vec![input_entity],
                },
                when_false: Edge {
                    target: when_false,
                    arguments: vec![input_entity],
                },
            },
            origin.clone(),
        )
        .expect("entry terminator must be valid");

    let true_parameter = value(
        function
            .block(when_true)
            .expect("true block must exist")
            .parameters[0],
    );
    let (_, constant) = function
        .append_instruction(
            when_true,
            Operation::Constant(ScalarConstant::Integer(1)),
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("true constant must be valid");
    let (_, result) = function
        .append_instruction(
            when_true,
            Operation::Binary {
                operator: BinaryOperator::Add,
                left: true_parameter,
                right: value(constant[0]),
            },
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("true add must be valid");
    let true_result = value(result[0]);
    function
        .set_terminator(
            when_true,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: result,
            }),
            origin.clone(),
        )
        .expect("true branch must be valid");

    let false_parameter = value(
        function
            .block(when_false)
            .expect("false block must exist")
            .parameters[0],
    );
    let (_, constant) = function
        .append_instruction(
            when_false,
            Operation::Constant(ScalarConstant::Integer(1)),
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("false constant must be valid");
    let (_, result) = function
        .append_instruction(
            when_false,
            Operation::Binary {
                operator: BinaryOperator::Subtract,
                left: false_parameter,
                right: value(constant[0]),
            },
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("false subtract must be valid");
    let false_result = value(result[0]);
    function
        .set_terminator(
            when_false,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: result,
            }),
            origin.clone(),
        )
        .expect("false branch must be valid");

    let joined = value(
        function
            .block(join)
            .expect("join block must exist")
            .parameters[0],
    );
    function
        .set_terminator(
            join,
            TerminatorKind::Return {
                values: vec![joined],
            },
            origin.clone(),
        )
        .expect("return must be valid");

    Diamond {
        program,
        module: module_id,
        function: function_id,
        entry,
        when_true,
        when_false,
        join,
        condition,
        input,
        true_result,
        false_result,
        boolean,
        origin,
    }
}

fn errors(program: &Program) -> Vec<VerifyError> {
    verify_program(program)
        .expect_err("fixture must fail verification")
        .errors
}

fn has_kind(errors: &[VerifyError], expected: impl Fn(&VerifyErrorKind) -> bool) -> bool {
    errors.iter().any(|error| expected(&error.kind))
}

#[test]
fn scalar_diamond_and_repeated_verification_are_valid() {
    let fixture = diamond();
    assert_eq!(verify_program(&fixture.program), Ok(()));
    assert_eq!(
        verify_program(&fixture.program),
        verify_program(&fixture.program)
    );
}

#[test]
fn loop_backedge_and_multiple_return_blocks_are_valid() {
    assert_eq!(verify_program(&loop_program()), Ok(()));
    assert_eq!(verify_program(&multiple_return_program()), Ok(()));
}

#[test]
fn entry_and_terminator_structure_failures_are_distinct() {
    let mut empty = Program::default();
    let module_id = empty.add_module("main");
    empty
        .module_mut(module_id)
        .expect("module must exist")
        .add_function("empty", Vec::new(), origin())
        .expect("signature must be valid");
    assert!(has_kind(&errors(&empty), |kind| matches!(
        kind,
        VerifyErrorKind::MissingEntryBlock
    )));

    let mut fixture = diamond();
    let true_block = fixture.when_true;
    fixture.function_mut().blocks[true_block.index()].terminator = None;
    let failures = errors(&fixture.program);
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::MissingTerminator
    )));
}

#[test]
fn dangling_and_cross_owner_ids_are_rejected_before_later_phases() {
    let mut dangling = diamond();
    let function_id = dangling.function;
    let invalid_target = BlockId {
        function: function_id,
        index: 99,
    };
    let entry = dangling.entry;
    let terminator = dangling.function_mut().blocks[entry.index()]
        .terminator
        .as_mut()
        .expect("entry terminator must exist");
    let TerminatorKind::Conditional { when_true, .. } = &mut terminator.kind else {
        panic!("fixture entry must use a conditional terminator");
    };
    when_true.target = invalid_target;
    assert!(has_kind(&errors(&dangling.program), |kind| matches!(
        kind,
        VerifyErrorKind::UnknownBlock
    )));

    let mut cross_owner = diamond();
    let module_id = cross_owner.module;
    let origin = cross_owner.origin.clone();
    let foreign_block = {
        let module = cross_owner
            .program
            .module_mut(module_id)
            .expect("module must exist");
        let foreign = module
            .add_function("foreign", Vec::new(), origin.clone())
            .expect("foreign signature must be valid");
        let function = module
            .function_mut(foreign)
            .expect("foreign function must exist");
        let block = function
            .add_block(Vec::new(), origin.clone())
            .expect("foreign block must be valid");
        function
            .set_terminator(block, TerminatorKind::Abort, origin)
            .expect("foreign terminator must be valid");
        block
    };
    let entry = cross_owner.entry;
    let terminator = cross_owner.function_mut().blocks[entry.index()]
        .terminator
        .as_mut()
        .expect("entry terminator must exist");
    let TerminatorKind::Conditional { when_true, .. } = &mut terminator.kind else {
        panic!("fixture entry must use a conditional terminator");
    };
    when_true.target = foreign_block;
    assert!(has_kind(&errors(&cross_owner.program), |kind| matches!(
        kind,
        VerifyErrorKind::WrongOwner
    )));
}

#[test]
fn edge_arity_type_condition_and_return_contracts_are_independent() {
    let mut arity = diamond();
    let true_block = arity.when_true;
    let terminator = arity.function_mut().blocks[true_block.index()]
        .terminator
        .as_mut()
        .expect("true terminator must exist");
    let TerminatorKind::Branch(edge) = &mut terminator.kind else {
        panic!("true terminator must branch");
    };
    edge.arguments.clear();
    assert!(has_kind(&errors(&arity.program), |kind| matches!(
        kind,
        VerifyErrorKind::EdgeArity { .. }
    )));

    let mut edge_type = diamond();
    let condition = edge_type.condition;
    let true_block = edge_type.when_true;
    let terminator = edge_type.function_mut().blocks[true_block.index()]
        .terminator
        .as_mut()
        .expect("true terminator must exist");
    let TerminatorKind::Branch(edge) = &mut terminator.kind else {
        panic!("true terminator must branch");
    };
    edge.arguments[0] = EntityId::Value(condition);
    assert!(has_kind(&errors(&edge_type.program), |kind| matches!(
        kind,
        VerifyErrorKind::EdgeType { .. }
    )));

    let mut condition = diamond();
    let integer = condition.input;
    let entry = condition.entry;
    let terminator = condition.function_mut().blocks[entry.index()]
        .terminator
        .as_mut()
        .expect("entry terminator must exist");
    let TerminatorKind::Conditional {
        condition: operand, ..
    } = &mut terminator.kind
    else {
        panic!("entry terminator must be conditional");
    };
    *operand = integer;
    assert!(has_kind(&errors(&condition.program), |kind| matches!(
        kind,
        VerifyErrorKind::ConditionType
    )));

    let mut returned = diamond();
    let join = returned.join;
    let terminator = returned.function_mut().blocks[join.index()]
        .terminator
        .as_mut()
        .expect("join terminator must exist");
    let TerminatorKind::Return { values } = &mut terminator.kind else {
        panic!("join terminator must return");
    };
    values.clear();
    assert!(has_kind(&errors(&returned.program), |kind| matches!(
        kind,
        VerifyErrorKind::ReturnArity { .. }
    )));

    let mut return_type = diamond();
    let join = return_type.join;
    let condition = return_type.condition;
    let terminator = return_type.function_mut().blocks[join.index()]
        .terminator
        .as_mut()
        .expect("join terminator must exist");
    let TerminatorKind::Return { values } = &mut terminator.kind else {
        panic!("join terminator must return");
    };
    values[0] = condition;
    assert!(has_kind(&errors(&return_type.program), |kind| matches!(
        kind,
        VerifyErrorKind::ReturnType { .. }
    )));
}

#[test]
fn entry_predecessor_and_dangling_entity_are_rejected() {
    let mut entry_predecessor = diamond();
    let true_block = entry_predecessor.when_true;
    let entry = entry_predecessor.entry;
    let condition = entry_predecessor.condition;
    let input = entry_predecessor.input;
    let terminator = entry_predecessor.function_mut().blocks[true_block.index()]
        .terminator
        .as_mut()
        .expect("true terminator must exist");
    let TerminatorKind::Branch(edge) = &mut terminator.kind else {
        panic!("true terminator must branch");
    };
    edge.target = entry;
    edge.arguments = vec![EntityId::Value(condition), EntityId::Value(input)];
    assert!(has_kind(
        &errors(&entry_predecessor.program),
        |kind| matches!(kind, VerifyErrorKind::EntryHasPredecessor)
    ));

    let mut dangling = diamond();
    let function = dangling.function;
    let true_block = dangling.when_true;
    let invalid = ValueId {
        function,
        index: 999,
    };
    let instruction = dangling.function_mut().blocks[true_block.index()].instructions[1];
    let Operation::Binary { right, .. } =
        &mut dangling.function_mut().instructions[instruction.index()].operation
    else {
        panic!("true instruction must be binary");
    };
    *right = invalid;
    assert!(has_kind(&errors(&dangling.program), |kind| matches!(
        kind,
        VerifyErrorKind::UnknownEntity
    )));
}

#[test]
fn result_contract_use_before_definition_and_non_dominating_use_are_rejected() {
    let mut result_type = diamond();
    let result = result_type.true_result;
    let boolean = result_type.boolean;
    result_type.function_mut().values[result.index].ty = EntityType::Value(boolean);
    assert!(has_kind(&errors(&result_type.program), |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let mut use_before_definition = diamond();
    let true_block = use_before_definition.when_true;
    let true_result = use_before_definition.true_result;
    let input = use_before_definition.input;
    let first_instruction =
        use_before_definition.function_mut().blocks[true_block.index()].instructions[0];
    use_before_definition.function_mut().instructions[first_instruction.index()].operation =
        Operation::Binary {
            operator: BinaryOperator::Add,
            left: input,
            right: true_result,
        };
    assert!(has_kind(
        &errors(&use_before_definition.program),
        |kind| matches!(kind, VerifyErrorKind::UseBeforeDefinition { .. })
    ));

    let mut non_dominating = diamond();
    let true_result = non_dominating.true_result;
    let false_block = non_dominating.when_false;
    let false_binary = non_dominating.function_mut().blocks[false_block.index()].instructions[1];
    let Operation::Binary { left, .. } =
        &mut non_dominating.function_mut().instructions[false_binary.index()].operation
    else {
        panic!("false instruction must be binary");
    };
    *left = true_result;
    assert!(has_kind(&errors(&non_dominating.program), |kind| matches!(
        kind,
        VerifyErrorKind::NonDominatingUse { .. }
    )));
}

#[test]
fn verifier_errors_keep_stable_location_origin_and_order() {
    let mut fixture = diamond();
    let join = fixture.join;
    fixture.function_mut().blocks[join.index()].terminator = None;
    let first = errors(&fixture.program);
    let second = errors(&fixture.program);
    assert_eq!(first, second);
    assert!(matches!(
        first.as_slice(),
        [VerifyError {
            kind: VerifyErrorKind::MissingTerminator,
            location: VerifyLocation::Block(block),
            origin: Some(_),
        }] if *block == join
    ));
}

fn loop_program() -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program.module_mut(module_id).expect("module must exist");
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let function_id = module
        .add_function("loop", vec![integer], origin.clone())
        .expect("signature must be valid");
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let entry = function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("entry must be valid");
    let header = function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("header must be valid");
    let body = function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("body must be valid");
    let exit = function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("exit must be valid");
    let input = function.block(entry).expect("entry must exist").parameters[0];
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: header,
                arguments: vec![input],
            }),
            origin.clone(),
        )
        .expect("entry branch must be valid");
    let current = function
        .block(header)
        .expect("header must exist")
        .parameters[0];
    let (_, condition) = function
        .append_instruction(
            header,
            Operation::Constant(ScalarConstant::Boolean(true)),
            vec![EntityType::Value(boolean)],
            origin.clone(),
        )
        .expect("condition must be valid");
    function
        .set_terminator(
            header,
            TerminatorKind::Conditional {
                condition: value(condition[0]),
                when_true: Edge {
                    target: body,
                    arguments: vec![current],
                },
                when_false: Edge {
                    target: exit,
                    arguments: vec![current],
                },
            },
            origin.clone(),
        )
        .expect("loop condition must be valid");
    let body_value = value(function.block(body).expect("body must exist").parameters[0]);
    let (_, one) = function
        .append_instruction(
            body,
            Operation::Constant(ScalarConstant::Integer(1)),
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("one must be valid");
    let (_, next) = function
        .append_instruction(
            body,
            Operation::Binary {
                operator: BinaryOperator::Add,
                left: body_value,
                right: value(one[0]),
            },
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("increment must be valid");
    function
        .set_terminator(
            body,
            TerminatorKind::Branch(Edge {
                target: header,
                arguments: next,
            }),
            origin.clone(),
        )
        .expect("backedge must be valid");
    let result = value(function.block(exit).expect("exit must exist").parameters[0]);
    function
        .set_terminator(
            exit,
            TerminatorKind::Return {
                values: vec![result],
            },
            origin,
        )
        .expect("return must be valid");
    program
}

fn multiple_return_program() -> Program {
    let mut fixture = diamond();
    let true_block = fixture.when_true;
    let false_block = fixture.when_false;
    let true_result = fixture.true_result;
    let false_result = fixture.false_result;
    fixture.function_mut().blocks[true_block.index()].terminator = Some(super::model::Terminator {
        kind: TerminatorKind::Return {
            values: vec![true_result],
        },
        origin: origin(),
    });
    fixture.function_mut().blocks[false_block.index()].terminator =
        Some(super::model::Terminator {
            kind: TerminatorKind::Return {
                values: vec![false_result],
            },
            origin: origin(),
        });
    fixture.program
}
