//! Read-only identities for the exact original SPEC-0332 source, never synthetic SSA.
use lang_frontend::{
    ast::{ExpressionId, ItemId},
    name_resolution::{
        DeclarationId, NameResolution, ReferenceTarget, SymbolId, SymbolKind, UnitReferenceTarget,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropTarget, OwnershipBindingKind, OwnershipCheckedFile,
        UnitDropTarget,
    },
    parser::{Expression, Item, NameMarker, ParsedFile, Statement},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypes, Copyability, IntrinsicTypeConstructor, ParameterMode,
        TypeKind, TypedFile, UnitExpressionId, UnitTypeKind,
    },
};

use crate::ssa::model::{
    Block, BlockId, Definition, Edge, EntityId, EntityType, Function, FunctionId, Instruction,
    Operation, Origin, Ownership, Program, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
};

type Query<T> = Result<T, String>;

#[derive(Clone, Copy)]
pub(super) struct SourceIdentity {
    root: ItemId,
    function: Span,
    function_name: Span,
    control: Span,
    condition: ExpressionId,
    condition_span: Span,
    returned: Span,
    return_name: Span,
}

fn require(valid: bool, label: &str) -> Query<()> {
    if valid { Ok(()) } else { Err(label.to_owned()) }
}

fn one<T>(mut values: impl Iterator<Item = T>, label: &str) -> Query<T> {
    let value = values.next().ok_or_else(|| format!("{label}: absent"))?;
    require(values.next().is_none(), &format!("{label}: not unique"))?;
    Ok(value)
}

fn source(file: &ParsedFile) -> Query<SourceIdentity> {
    let root = one(file.roots().iter().copied(), "original source root")?;
    let function = file
        .ast()
        .items()
        .get(root)
        .map_err(|e| format!("function: {e:?}"))?;
    let Item::Function {
        name: NameMarker::Present(function_name),
        parameters,
        ..
    } = function.payload()
    else {
        return Err("original root is not a named function".into());
    };
    require(
        parameters.len() == 1,
        "original function has one source parameter",
    )?;
    let (control, condition) = one(
        file.ast()
            .statements()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Statement::While { condition, .. } => Some((node.span(), *condition)),
                _ => None,
            }),
        "original while",
    )?;
    let condition_span = file
        .ast()
        .expressions()
        .get(condition)
        .map_err(|e| format!("condition: {e:?}"))?
        .span();
    let (returned, value) = one(
        file.ast()
            .expressions()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Expression::Return {
                    value: Some(value), ..
                } => Some((node.span(), *value)),
                _ => None,
            }),
        "original return",
    )?;
    let value = file
        .ast()
        .expressions()
        .get(value)
        .map_err(|e| format!("return name: {e:?}"))?;
    require(
        matches!(value.payload(), Expression::Name),
        "original return is a direct name",
    )?;
    Ok(SourceIdentity {
        root,
        function: function.span(),
        function_name: *function_name,
        control,
        condition,
        condition_span,
        returned,
        return_name: value.span(),
    })
}

pub(super) fn single_source(
    file: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    entry: SymbolId,
) -> Query<SourceIdentity> {
    let source = source(file)?;
    let function = one(
        names.symbols().iter().filter(|symbol| symbol.id() == entry),
        "single function symbol",
    )?;
    require(
        function.kind() == SymbolKind::Function && function.span() == source.function_name,
        "single selected function matches original declaration",
    )?;
    let comparison = typed
        .null_comparison(source.condition)
        .ok_or("single comparison absent")?;
    let root = comparison.symbol();
    let signature = one(
        typed
            .callables()
            .iter()
            .filter(|signature| signature.symbol() == entry),
        "single signature",
    )?;
    let parameter = one(signature.parameters().iter(), "single parameter contract")?;
    require(
        signature.parameter_symbols() == [Some(root)]
            && parameter.mode == ParameterMode::Value
            && parameter.ty == comparison.nullable_type()
            && typed.parameter_mode(root) == Some(ParameterMode::Value)
            && typed.symbol_type(root) == Some(comparison.nullable_type())
            && comparison.expression() == source.condition
            && comparison.non_null_when_true(),
        "single comparison uses this callable's sole Value parameter",
    )?;
    let return_root = one(
        names.references().iter().filter_map(|reference| {
            if reference.span() != source.return_name {
                return None;
            }
            match reference.target() {
                ReferenceTarget::Symbol(symbol) => Some(*symbol),
                _ => None,
            }
        }),
        "single actual return reference",
    )?;
    require(
        return_root == root
            && owned.binding_kind(root) == Some(OwnershipBindingKind::Owned)
            && owned
                .drops()
                .iter()
                .all(|drop| drop.target() != DropTarget::Named(root)),
        "single return moves original owned parameter without a parameter drop",
    )?;
    let Some(TypeKind::Nullable(inner)) = typed.types().get(comparison.nullable_type()) else {
        return Err("single input is not a real nullable type".into());
    };
    let Some(TypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::Rc,
        arguments,
    }) = typed.types().get(*inner)
    else {
        return Err("single nullable input is not intrinsic Rc".into());
    };
    let argument = one(arguments.iter().copied(), "single Rc payload")?;
    require(
        typed.types().get(argument) == Some(&TypeKind::Builtin(BuiltinType::Int))
            && typed.copyability(comparison.nullable_type()) == Some(Copyability::MoveOnly),
        "single source input is MoveOnly Rc<Int>?",
    )?;
    Ok(source)
}

pub(super) fn unit_source(
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    entry: DeclarationId,
) -> Query<SourceIdentity> {
    let source = source(file)?;
    let declaration = one(
        names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| declaration.id() == entry),
        "unit selected declaration",
    )?;
    require(
        declaration.root() == source.root
            && declaration.kind() == SymbolKind::Function
            && declaration.name_span() == source.function_name,
        "unit selected declaration is the exact parsed function root",
    )?;
    let unit = declaration.source_unit();
    let resolution = one(
        names
            .names()
            .source_units()
            .iter()
            .filter(|resolution| resolution.source_unit() == unit),
        "unit source resolution",
    )?;
    let function = one(
        resolution.resolution().symbols().iter().filter(|symbol| {
            symbol.kind() == SymbolKind::Function && symbol.span() == source.function_name
        }),
        "unit original function symbol",
    )?;
    require(
        function.span().source_id() == file.source_id(),
        "unit original source identity",
    )?;
    let condition = UnitExpressionId::new(unit, source.condition);
    let comparison = typed
        .null_comparison(condition)
        .ok_or("unit comparison absent")?;
    let root = comparison.symbol();
    let signature = typed
        .signatures()
        .declaration(entry)
        .and_then(|signature| signature.callable())
        .ok_or("unit callable absent")?;
    let parameter = one(signature.parameters().iter(), "unit parameter contract")?;
    require(
        parameter.symbol() == Some(root)
            && root.source_unit() == unit
            && parameter.mode() == ParameterMode::Value
            && parameter.ty() == comparison.nullable_type()
            && typed.symbol_type(root) == Some(comparison.nullable_type())
            && comparison.expression() == condition
            && comparison.non_null_when_true(),
        "unit comparison uses this declaration's sole Value parameter",
    )?;
    let return_root = one(
        names.names().references().iter().filter_map(|reference| {
            if reference.source_unit() != unit || reference.span() != source.return_name {
                return None;
            }
            match reference.target() {
                UnitReferenceTarget::Symbol(symbol) => Some(*symbol),
                _ => None,
            }
        }),
        "unit source-qualified return reference",
    )?;
    require(
        return_root == root
            && owned.binding_kind(root) == Some(OwnershipBindingKind::Owned)
            && owned
                .drops()
                .iter()
                .all(|drop| drop.target() != UnitDropTarget::Named(root)),
        "unit return moves original owned parameter without a parameter drop",
    )?;
    let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(comparison.nullable_type()) else {
        return Err("unit input is not a real nullable type".into());
    };
    let Some(UnitTypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::Rc,
        arguments,
    }) = typed.types().get(*inner)
    else {
        return Err("unit nullable input is not intrinsic Rc".into());
    };
    let argument = one(arguments.iter().copied(), "unit Rc payload")?;
    require(
        typed.types().get(argument) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            && typed.copyability(comparison.nullable_type()) == Copyability::MoveOnly,
        "unit source input is MoveOnly Rc<Int>?",
    )?;
    Ok(source)
}

fn term(block: &Block) -> Query<&TerminatorKind> {
    block
        .terminator
        .as_ref()
        .map(|term| &term.kind)
        .ok_or("missing terminator".into())
}

fn branch(block: &Block) -> Query<&Edge> {
    match term(block)? {
        TerminatorKind::Branch(edge) => Ok(edge),
        _ => Err("expected ordinary branch".into()),
    }
}

fn block(function: &Function, id: BlockId) -> Query<&Block> {
    function
        .block(id)
        .ok_or("edge target does not belong to original function".into())
}

fn parameter(function: &Function, block: &Block, ty: SsaTypeId) -> Query<ValueId> {
    let entity = one(
        block.parameters.iter().copied(),
        "unique owner block parameter",
    )?;
    let EntityId::Value(value) = entity else {
        return Err("owner block parameter is not Value".into());
    };
    let data = function
        .entity(entity)
        .ok_or("owner parameter entity absent")?;
    require(
        data.ty == EntityType::Value(ty)
            && data.definition
                == Definition::BlockParameter {
                    block: block.id,
                    index: 0,
                },
        "owner parameter has exact block/column definition and nullable type",
    )?;
    Ok(value)
}

fn result(function: &Function, instruction: &Instruction, ty: SsaTypeId) -> Query<ValueId> {
    let entity = one(
        instruction.results.iter().copied(),
        "unique Boolean instruction result",
    )?;
    let EntityId::Value(value) = entity else {
        return Err("Boolean result is not Value".into());
    };
    let data = function
        .entity(entity)
        .ok_or("Boolean result entity absent")?;
    require(
        data.ty == EntityType::Value(ty)
            && data.definition
                == Definition::InstructionResult {
                    instruction: instruction.id,
                    index: 0,
                },
        "Boolean result has exact instruction/column definition and type",
    )?;
    Ok(value)
}

fn incoming(function: &Function, target: BlockId) -> Query<Vec<(BlockId, &Edge)>> {
    let mut result = Vec::new();
    for block in &function.blocks {
        let edges: Vec<&Edge> = match term(block)? {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            TerminatorKind::NullableBranch {
                when_null,
                when_non_null,
                ..
            } => vec![when_null, when_non_null],
            _ => Vec::new(),
        };
        result.extend(
            edges
                .into_iter()
                .filter(|edge| edge.target == target)
                .map(|edge| (block.id, edge)),
        );
    }
    Ok(result)
}

fn exact_incoming(
    function: &Function,
    target: BlockId,
    expected: &[(BlockId, ValueId)],
) -> Query<()> {
    let actual = incoming(function, target)?;
    require(
        actual.len() == expected.len(),
        "all owner parameter incoming edges are accounted for",
    )?;
    for (predecessor, owner) in expected {
        let (_, edge) = one(
            actual.iter().filter(|(id, _)| id == predecessor),
            "unique expected predecessor edge",
        )?;
        require(
            edge.arguments == [EntityId::Value(*owner)],
            "incoming edge transports exact predecessor owner",
        )?;
    }
    Ok(())
}

pub(super) fn inspect(program: &Program, entry: FunctionId, source: SourceIdentity) -> Query<()> {
    let module = program
        .module(entry.module())
        .ok_or("original module absent")?;
    let function = module
        .function(entry)
        .ok_or("lowered original FunctionId absent")?;
    require(
        function.origin == Origin::Source(source.function)
            && function.receiver().is_none()
            && function.borrow_return.is_none()
            && function.carrier_return.is_none(),
        "original function identity and owned-return ABI",
    )?;
    let ty = one(
        function.return_types.iter().copied(),
        "original nullable return type",
    )?;
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(ty) else {
        return Err("original return is not actual NullableHandle".into());
    };
    let Some(SsaTypeKind::SharedOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(*inner)
    else {
        return Err("original nullable inner is not actual SharedOwner".into());
    };
    require(
        module.type_kind(*payload)
            == Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
            && module.type_ownership(ty) == Some(Ownership::MoveOnly)
            && module.type_ownership(*inner) == Some(Ownership::MoveOnly),
        "original SSA nullable SharedOwner has Int payload and MoveOnly obligations",
    )?;
    let entry_block = block(
        function,
        function.entry_block().ok_or("entry block absent")?,
    )?;
    let input = parameter(function, entry_block, ty)?;
    let test = one(
        function.instructions.iter().filter(|instruction| {
            instruction.origin == Origin::Source(source.condition_span)
                && matches!(instruction.operation, Operation::NullableIsNull { .. })
        }),
        "source-anchored nullable test",
    )?;
    let header = block(function, test.block)?;
    let current = parameter(function, header, ty)?;
    require(
        matches!(test.operation, Operation::NullableIsNull { owner } if owner == current),
        "nullable test reads the exact transported owner",
    )?;
    let invert = one(
        function.instructions.iter().filter(|instruction| {
            instruction.block == header.id
                && instruction.origin == Origin::Source(source.condition_span)
                && matches!(instruction.operation, Operation::BooleanNot { .. })
        }),
        "source-anchored Boolean inversion",
    )?;
    let test_result = one(test.results.iter().copied(), "nullable test result")?;
    let boolean = function
        .entity(test_result)
        .ok_or("nullable test result absent")?
        .ty
        .semantic_type();
    require(
        module.type_kind(boolean) == Some(&SsaTypeKind::Boolean),
        "nullable test is Boolean",
    )?;
    let tested = result(function, test, boolean)?;
    let inverted = result(function, invert, boolean)?;
    require(
        matches!(invert.operation, Operation::BooleanNot { operand } if operand == tested),
        "!= null branches on inversion of the actual nullable test",
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = term(header)?
    else {
        return Err("original header is not an ordinary conditional".into());
    };
    require(
        *condition == inverted
            && header.terminator.as_ref().map(|term| &term.origin)
                == Some(&Origin::Source(source.control)),
        "while conditional uses the actual non-null condition",
    )?;
    require(
        branch(entry_block)?.target == header.id,
        "original parameter enters actual test header",
    )?;
    exact_incoming(function, entry_block.id, &[])?;
    exact_incoming(function, header.id, &[(entry_block.id, input)])?;
    let non_null = block(function, when_true.target)?;
    let null = block(function, when_false.target)?;
    let non_null_owner = parameter(function, non_null, ty)?;
    let null_owner = parameter(function, null, ty)?;
    exact_incoming(function, non_null.id, &[(header.id, current)])?;
    exact_incoming(function, null.id, &[(header.id, current)])?;
    let joined = block(function, branch(non_null)?.target)?;
    require(
        branch(null)?.target == joined.id,
        "break and null edges join at the same actual block",
    )?;
    let output = parameter(function, joined, ty)?;
    exact_incoming(
        function,
        joined.id,
        &[(non_null.id, non_null_owner), (null.id, null_owner)],
    )?;
    require(
        matches!(term(joined)?, TerminatorKind::Return { values } if values == &[output])
            && joined.terminator.as_ref().map(|term| &term.origin)
                == Some(&Origin::Source(source.returned)),
        "original result returns exactly the owner transported by both normal paths",
    )?;
    let expected = [entry_block.id, header.id, non_null.id, null.id, joined.id];
    require(
        expected
            .iter()
            .enumerate()
            .all(|(index, id)| !expected[..index].contains(id))
            && function.blocks.len() == expected.len()
            && function
                .blocks
                .iter()
                .all(|block| expected.contains(&block.id)),
        "exact original five-block closure, no backedge or extra normal exit",
    )?;
    require(
        function.instructions.len() == 2
            && test.id != invert.id
            && header.instructions == [test.id, invert.id]
            && [entry_block, non_null, null, joined]
                .iter()
                .all(|block| block.instructions.is_empty())
            && function.places.is_empty()
            && function.loans.is_empty(),
        "only is-null/not exist: original owner is neither created, retained, extracted nor dropped",
    )?;
    Ok(())
}

#[test]
fn original_rc_nullable_while_preserves_return_owner_identity() {
    // Preserve both actual frontend/lowering/SSA/LLVM outcomes before assertions.
    let observations = [
        super::observe_with_inspector(false, true),
        super::observe_with_inspector(true, true),
    ];
    for (observation, identity) in &observations {
        println!(
            "SPEC0332 IDENTITY unit={} query={identity:?} complete={} ownership={} frontend_anchor={} lower_attempted={} pipeline={}",
            observation.unit,
            observation.complete_ok,
            observation.ownership_ok,
            observation.frontend_anchor_ok,
            observation.lower_attempted,
            if observation.outcome.is_ok() {
                "ok"
            } else {
                "err"
            }
        );
        if let Err(error) = &observation.outcome {
            println!(
                "SPEC0332 IDENTITY PIPELINE ERROR unit={} stage={} detail={} lower={:?} span_text={:?}",
                observation.unit, error.stage, error.detail, error.lower, error.span_text
            );
        }
    }
    assert!(
        observations.iter().all(|(observation, identity)| {
            observation.complete_ok
                && observation.ownership_ok
                && observation.frontend_anchor_ok
                && observation.lower_attempted
                && observation.outcome.is_ok()
                && identity.is_ok()
        }),
        "both original-source paths must independently preserve source/SSA/returned owner identity and verified LLVM"
    );
}
