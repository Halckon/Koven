use lang_frontend::{
    name_resolution::SourceUnitInput,
    parser::{BinaryOperator, Expression},
    source::SourceMap,
    type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{BlockId, EntityId, Function, FunctionId, Operation, ScalarConstant, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_cross_file_and_or_with_exact_short_edges_and_carried_owner() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun left(): Boolean = true\n\
         fun right(): Boolean = false\n\
         fun consume(own input: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Boolean {\n\
             val owner = \"kept\"\n\
             val both = p.left() && p.right()\n\
             val either = p.left() || p.right()\n\
             val same = both == either\n\
             val consumed = p.consume(owner)\n\
             return same\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "q", "entry");

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("cross-file short-circuit expressions lower to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("input permutation preserves short-circuit CFG");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let entry = module
        .functions
        .iter()
        .find(|function| function.name.contains("q.entry"))
        .expect("entry function exists");
    let left = function_id(module, "p.left");
    let right = function_id(module, "p.right");
    let consume = function_id(module, "p.consume");
    let left_calls = call_blocks(entry, left);
    let right_calls = call_blocks(entry, right);
    assert_eq!(left_calls.len(), 2);
    assert_eq!(right_calls.len(), 2);

    let and_merge = assert_short_circuit(entry, left_calls[0], right_calls[0], false, true, 1);
    let or_merge = assert_short_circuit(entry, left_calls[1], right_calls[1], true, false, 2);
    let owner_type = entry
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
        .and_then(|instruction| instruction.results.first())
        .and_then(|owner| entry.entity(*owner))
        .map(|owner| owner.ty)
        .expect("String owner type exists");
    let and_owner = parameter_with_type(entry, and_merge, owner_type);
    let second_left = entry
        .block(left_calls[1])
        .and_then(|block| block.terminator.as_ref())
        .expect("second left call has a terminator");
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &second_left.kind
    else {
        panic!("second left call forms a conditional");
    };
    assert!(when_true.arguments.contains(&and_owner));
    assert!(when_false.arguments.contains(&and_owner));
    let or_owner = parameter_with_type(entry, or_merge, owner_type);
    let consume_arguments = entry
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::DirectCall { callee, arguments } if *callee == consume => {
                Some(arguments.as_slice())
            }
            _ => None,
        })
        .expect("consume call exists after both merges");
    assert_eq!(consume_arguments, &[or_owner]);
    let compare = entry
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::Compare { left, right, .. } => Some((left, right)),
            _ => None,
        })
        .expect("both and either are compared after the second merge");
    let merge_parameters = &entry
        .block(or_merge)
        .expect("second short-circuit merge exists")
        .parameters;
    assert_eq!(
        Some(EntityId::Value(compare.1)),
        merge_parameters.first().copied()
    );
    assert!(
        merge_parameters[1..].contains(&EntityId::Value(compare.0)),
        "the first result remains carried while the second result occupies slot zero"
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        0,
        "the carried String owner transfers after both short expressions"
    );
    assert_eq!(
        module
            .functions
            .iter()
            .flat_map(|function| function.instructions.iter())
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "the callee drops the owner exactly once after both short-circuit merges"
    );
}

#[test]
fn rejects_rhs_only_move_until_frontend_publishes_short_circuit_owner_facts() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun consume(own input: String): Boolean = true",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Boolean {\n\
             val owner = \"kept\"\n\
             return true || p.consume(owner)\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let short_span = consumer
        .ast()
        .expressions()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            Expression::Binary {
                operator: BinaryOperator::LogicalOr,
                ..
            } => Some(node.span()),
            _ => None,
        })
        .expect("logical-or expression exists");

    let error = match lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    ) {
        Err(error) => error,
        Ok(_) => panic!("path-specific owner state must not be guessed"),
    };
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
    assert_eq!(error.span, Some(short_span));
}

fn function_id(module: &super::model::Module, name: &str) -> FunctionId {
    module
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .expect("reachable helper function exists")
        .id
}

fn call_blocks(function: &Function, callee: FunctionId) -> Vec<BlockId> {
    function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee: actual, .. } if actual == callee => {
                Some(instruction.block)
            }
            _ => None,
        })
        .collect()
}

fn parameter_with_type(
    function: &Function,
    block: BlockId,
    ty: super::model::EntityType,
) -> EntityId {
    function
        .block(block)
        .expect("merge block exists")
        .parameters
        .iter()
        .copied()
        .find(|parameter| {
            function
                .entity(*parameter)
                .is_some_and(|entity| entity.ty == ty)
        })
        .expect("merge carries the String owner parameter")
}

fn assert_short_circuit(
    function: &Function,
    left_block: BlockId,
    right_block: BlockId,
    short_value: bool,
    right_when_true: bool,
    carried_count: usize,
) -> BlockId {
    let left = function.block(left_block).expect("left call block exists");
    let left_call = function
        .instruction(*left.instructions.last().expect("left call exists"))
        .expect("left call instruction exists");
    let [EntityId::Value(left_result)] = left_call.results.as_slice() else {
        panic!("Boolean helper call has one value result");
    };
    let Some(terminator) = &left.terminator else {
        panic!("left call block has a terminator");
    };
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &terminator.kind
    else {
        panic!("left result must form a conditional");
    };
    assert_eq!(condition, left_result);
    let (right_edge, short_edge) = if right_when_true {
        (when_true, when_false)
    } else {
        (when_false, when_true)
    };
    assert_eq!(right_edge.target, right_block);
    assert_eq!(right_edge.arguments.len(), carried_count);
    assert_eq!(short_edge.arguments.len(), carried_count);

    let short = function
        .block(short_edge.target)
        .expect("short-value block exists");
    let short_instruction = function
        .instruction(*short.instructions.last().expect("short constant exists"))
        .expect("short constant instruction exists");
    assert!(matches!(
        short_instruction.operation,
        Operation::Constant(ScalarConstant::Boolean(actual)) if actual == short_value
    ));

    let right = function
        .block(right_block)
        .expect("right call block exists");
    let right_call = function
        .instruction(*right.instructions.last().expect("right call exists"))
        .expect("right call instruction exists");
    let [right_result] = right_call.results.as_slice() else {
        panic!("right Boolean helper has one result");
    };
    let [short_result] = short_instruction.results.as_slice() else {
        panic!("short constant has one result");
    };
    let right_merge = branch_target(right, *right_result);
    let short_merge = branch_target(short, *short_result);
    assert_eq!(right_merge, short_merge);
    assert_eq!(
        function
            .block(right_merge)
            .expect("short-circuit merge exists")
            .parameters
            .len(),
        carried_count + 1,
        "the Boolean result precedes every carried binding"
    );
    right_merge
}

fn branch_target(block: &super::model::Block, result: EntityId) -> BlockId {
    let Some(terminator) = &block.terminator else {
        panic!("short-circuit exit has a terminator");
    };
    let TerminatorKind::Branch(edge) = &terminator.kind else {
        panic!("short-circuit exit branches to one merge");
    };
    assert_eq!(edge.arguments.first(), Some(&result));
    assert_eq!(&edge.arguments[1..], block.parameters.as_slice());
    edge.target
}
