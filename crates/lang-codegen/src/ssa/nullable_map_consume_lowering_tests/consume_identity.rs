//! Independent, read-only identity queries for the six frozen ordinary C observations.
//! No qualification/proof helper, program mutation, object emission or native execution.
use crate::test_support::diagnostics::diagnostic;
use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::ssa::model::{
    Block, BlockId, Definition, Edge, EntityId, EntityType, Function, Instruction, InstructionId,
    LoanKind, MapContainerKind, Module, Operation, Origin, Ownership, SsaTypeId, SsaTypeKind,
    TerminatorKind, ValueId,
};
use lang_frontend::{
    ast::StatementId,
    parser::{Item, NameMarker, Statement},
    type_checking::BuiltinType,
};

mod graph;
mod llvm {
    //! Query the complete, verified LLVM module for the same SSA identity. Follow
    //! actual definitions rather than assuming names for intermediate extract values.
    use super::*;

    fn parts(text: &str) -> Outcome<Vec<&str>> {
        let mut parts = Vec::new();
        let mut depth = 0usize;
        let mut start = 0;
        for (index, byte) in text.bytes().enumerate() {
            match byte {
                b'{' | b'[' | b'(' | b'<' => depth += 1,
                b'}' | b']' | b')' | b'>' => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| missing("LLVM balanced operand", text))?;
                }
                b',' if depth == 0 => {
                    parts.push(text[start..index].trim());
                    start = index + 1;
                }
                _ => {}
            }
        }
        gate(
            "consume identity: balanced complete LLVM operand",
            depth == 0,
            text,
        )?;
        parts.push(text[start..].trim());
        Ok(parts)
    }

    fn typed(text: &str) -> Outcome<(&str, &str)> {
        text.split_once(' ')
            .ok_or_else(|| missing("LLVM typed operand", text))
    }

    fn block_label(line: &str) -> Option<&str> {
        let (label, tail) = line.split_once(':')?;
        (!label.is_empty()
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            && (tail.trim().is_empty() || tail.trim().starts_with(';')))
        .then_some(label)
    }

    struct Body<'a> {
        lines: Vec<&'a str>,
        definitions: BTreeMap<&'a str, (usize, &'a str)>,
    }

    struct Header<'a> {
        result_ty: &'a str,
        source_ty: &'a str,
        source: &'a str,
        fields: &'a [&'a str],
    }

    impl<'a> Body<'a> {
        fn new(text: &'a str, function: &Function) -> Outcome<Self> {
            let symbol = format!("@f{}.{}(", function.id.index(), function.name);
            let lines: Vec<_> = text.lines().collect();
            let starts: Vec<_> = lines
                .iter()
                .enumerate()
                .filter_map(|(index, line)| {
                    (line.starts_with("define ") && line.contains(&symbol)).then_some(index)
                })
                .collect();
            let start = *one(&starts, "actual SSA callable LLVM definition")?;
            let end = lines
                .iter()
                .enumerate()
                .skip(start + 1)
                .find_map(|(index, line)| (*line == "}").then_some(index))
                .ok_or_else(|| missing("complete LLVM function body", &symbol))?;
            let lines: Vec<_> = lines[start + 1..end]
                .iter()
                .map(|line| line.trim())
                .collect();
            let mut definitions = BTreeMap::new();
            for (index, line) in lines.iter().enumerate() {
                if let Some((name, rhs)) = line.split_once(" = ") {
                    gate(
                        "consume identity: unique LLVM value definition",
                        definitions.insert(name, (index, rhs)).is_none(),
                        name,
                    )?;
                }
            }
            Ok(Self { lines, definitions })
        }

        fn phi(&self, alias: ValueId, source: ValueId, storage: &str) -> Outcome<(String, String)> {
            let name = format!("%v{}", alias.index());
            let &(position, rhs) = self
                .definitions
                .get(name.as_str())
                .ok_or_else(|| missing("LLVM exact successor phi", alias))?;
            let prefix = format!("phi {storage} ");
            let incoming = parts(
                rhs.strip_prefix(&prefix)
                    .ok_or_else(|| missing("LLVM owner phi storage", rhs))?,
            )?;
            let incoming = *one(&incoming, "complete one direct-edge LLVM phi incoming set")?;
            let pair = parts(
                incoming
                    .strip_prefix('[')
                    .and_then(|value| value.strip_suffix(']'))
                    .ok_or_else(|| missing("LLVM phi incoming pair", incoming))?
                    .trim(),
            )?;
            let [value, predecessor] = pair.as_slice() else {
                return Err(missing("LLVM phi pair arity", pair));
            };
            gate(
                "consume identity: phi carries exact original branch owner",
                *value == format!("%v{}", source.index()),
                rhs,
            )?;
            let predecessor = predecessor
                .strip_prefix('%')
                .ok_or_else(|| missing("LLVM phi predecessor label", predecessor))?;
            let block = self.lines[..=position]
                .iter()
                .rev()
                .find_map(|line| block_label(line))
                .ok_or_else(|| missing("LLVM successor block label", alias))?;
            Ok((predecessor.to_owned(), block.to_owned()))
        }

        fn branch(&self, identity: &Identity, storage: &str) -> Outcome<()> {
            let (present_predecessor, present) =
                self.phi(identity.wrapper, identity.branch_owner, storage)?;
            let (null_predecessor, absent) =
                self.phi(identity.null_owner, identity.branch_owner, storage)?;
            gate(
                "consume identity: two original owner phis share exact actual predecessor",
                present_predecessor == null_predecessor && present != absent,
                (&present_predecessor, &null_predecessor, &present, &absent),
            )?;
            let starts: Vec<_> = self
                .lines
                .iter()
                .enumerate()
                .filter_map(|(index, line)| {
                    (block_label(line) == Some(present_predecessor.as_str())).then_some(index)
                })
                .collect();
            let start = *one(&starts, "actual LLVM predecessor block")?;
            let end = self
                .lines
                .iter()
                .enumerate()
                .skip(start + 1)
                .find_map(|(index, line)| block_label(line).map(|_| index))
                .unwrap_or(self.lines.len());
            let predecessor: Vec<_> = self.lines[start + 1..end]
                .iter()
                .copied()
                .filter(|line| !line.is_empty() && !line.starts_with(';'))
                .collect();
            let branch = predecessor
                .last()
                .ok_or_else(|| missing("LLVM predecessor terminator", &present_predecessor))?;
            let operands = parts(branch)?;
            let [condition, when_null, when_non_null] = operands.as_slice() else {
                return Err(missing("LLVM actual nullable branch operands", branch));
            };
            let condition = condition
                .strip_prefix("br i1 ")
                .ok_or_else(|| missing("LLVM conditional branch", branch))?;
            gate(
                "consume identity: real predecessor targets null and nonnull phi blocks",
                *when_null == format!("label %{absent}")
                    && *when_non_null == format!("label %{present}"),
                branch,
            )?;
            let &(condition_position, rhs) = self
                .definitions
                .get(condition)
                .ok_or_else(|| missing("LLVM actual null condition", condition))?;
            let operands = parts(
                rhs.strip_prefix("xor i1 ")
                    .ok_or_else(|| missing("LLVM null polarity", rhs))?,
            )?;
            let [present_value, inverted] = operands.as_slice() else {
                return Err(missing("LLVM null inversion operands", rhs));
            };
            let &(present_position, rhs) = self
                .definitions
                .get(*present_value)
                .ok_or_else(|| missing("LLVM original presence bit", present_value))?;
            gate(
                "consume identity: null predicate uses same original owner tag in predecessor",
                *inverted == "true"
                    && rhs
                        == format!(
                            "extractvalue {storage} %v{}, 0",
                            identity.branch_owner.index()
                        )
                    && start < present_position
                    && present_position < condition_position
                    && condition_position < end,
                rhs,
            )
        }

        fn header(&self, result: ValueId, ty: &str) -> Outcome<(&str, usize)> {
            let prefix = format!("%v{}.", result.index());
            let operation = format!("insertvalue {ty} ");
            self.definitions
                .iter()
                .filter(|(name, (_, rhs))| name.starts_with(&prefix) && rhs.starts_with(&operation))
                .max_by_key(|(_, (index, _))| *index)
                .map(|(name, (index, _))| (*name, *index))
                .ok_or_else(|| missing("actual SSA result header definition", result))
        }

        fn field(
            &self,
            aggregate: &str,
            field: usize,
            header: &Header<'_>,
            depth: usize,
        ) -> Outcome<()> {
            let Header {
                result_ty,
                source_ty,
                source,
                fields,
            } = *header;
            gate(
                "consume identity: finite LLVM definition graph",
                depth <= self.definitions.len() + 6,
                aggregate,
            )?;
            let (_, rhs) = self
                .definitions
                .get(aggregate)
                .ok_or_else(|| missing("LLVM result aggregate definition", aggregate))?;
            let operands = parts(
                rhs.strip_prefix("insertvalue ")
                    .ok_or_else(|| missing("LLVM result insertion", rhs))?,
            )?;
            let [base, scalar, index] = operands.as_slice() else {
                return Err(missing("LLVM insertion operands", operands));
            };
            let (base_ty, base) = typed(base)?;
            let index: usize = index
                .parse()
                .map_err(|error| missing("LLVM field index", error))?;
            gate(
                "consume identity: result field type and index",
                base_ty == result_ty && index < fields.len(),
                rhs,
            )?;
            if index != field {
                return self.field(base, field, header, depth + 1);
            }
            let (scalar_ty, scalar) = typed(scalar)?;
            let (_, rhs) = self
                .definitions
                .get(scalar)
                .ok_or_else(|| missing("LLVM field actual scalar definition", scalar))?;
            let operands = parts(
                rhs.strip_prefix("extractvalue ")
                    .ok_or_else(|| missing("LLVM source scalar extraction", rhs))?,
            )?;
            let [base, index] = operands.as_slice() else {
                return Err(missing("LLVM extraction operands", operands));
            };
            let (base_ty, base) = typed(base)?;
            let index: usize = index
                .parse()
                .map_err(|error| missing("LLVM source field index", error))?;
            gate(
                "consume identity: exact unchanged source field, including buffer",
                scalar_ty == fields[field]
                    && base_ty == source_ty
                    && base == source
                    && index == field,
                (aggregate, field, rhs),
            )
        }
    }

    pub(super) fn check(program: &Program, identity: &Identity, text: &str) -> Outcome<()> {
        let functions: Vec<_> = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| function.id == identity.function)
            .collect();
        let function = *one(&functions, "LLVM identity full FunctionId")?;
        let body = Body::new(text, function)?;
        let nullable = format!("%koven.t{}", identity.nullable.index());
        let mutable = format!("%koven.map.t{}", identity.mutable.index());
        let readonly = format!("%koven.map.t{}", identity.readonly.index());
        body.branch(identity, &nullable)?;
        let mut headers = Vec::new();
        for ty in [&mutable, &readonly] {
            let prefix = format!("{ty} = type {{ ");
            let definitions: Vec<_> = text
                .lines()
                .filter_map(|line| {
                    line.strip_prefix(&prefix)
                        .and_then(|value| value.strip_suffix(" }"))
                })
                .collect();
            headers.push(parts(one(&definitions, "one complete LLVM map type")?)?);
        }
        gate(
            "consume identity: six precise equal header fields",
            headers[0] == headers[1]
                && headers[0].len() == 6
                && headers[0][0] == "ptr"
                && matches!(headers[0][1], "i32" | "i64")
                && headers[0][1..].iter().all(|field| *field == headers[0][1]),
            &headers,
        )?;
        let wrapper = format!("%v{}", identity.wrapper.index());
        let inner = format!("%v{}", identity.inner.index());
        let &(take_index, take) = body
            .definitions
            .get(inner.as_str())
            .ok_or_else(|| missing("LLVM exact take result", &inner))?;
        gate(
            "consume identity: take extracts same current wrapper payload",
            take == format!("extractvalue {nullable} {wrapper}, 1"),
            take,
        )?;
        let (result, result_index) = body.header(identity.result, &readonly)?;
        let header = Header {
            result_ty: &readonly,
            source_ty: &mutable,
            source: &inner,
            fields: &headers[0],
        };
        for field in 0..6 {
            body.field(result, field, &header, 0)?;
        }
        gate(
            "consume identity: take precedes completed result header",
            take_index < result_index,
            (take_index, result_index),
        )?;
        // Only inspect the conversion segment. C2's earlier puts and String cleanup
        // may legitimately allocate, free, hash and drop unrelated entries.
        for line in &body.lines[take_index + 1..=result_index] {
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            let (_, rhs) = line
                .split_once(" = ")
                .ok_or_else(|| missing("LLVM pure transfer instruction", line))?;
            gate(
                "consume identity: conversion has no allocation/hash/reinsert/drop/call/store",
                rhs.starts_with("extractvalue ") || rhs.starts_with("insertvalue "),
                line,
            )?;
        }
        let result_drop = format!(
            "call void @koven.drop.t{}({readonly} {result})",
            identity.readonly.index()
        );
        let null_drop = format!(
            "call void @koven.drop.t{}({nullable} %v{})",
            identity.nullable.index(),
            identity.null_owner.index()
        );
        let result_prefix = format!("call void @koven.drop.t{}(", identity.readonly.index());
        let null_prefix = format!("call void @koven.drop.t{}(", identity.nullable.index());
        let mutable_prefix = format!("call void @koven.drop.t{}(", identity.mutable.index());
        let result_drops: Vec<_> = body
            .lines
            .iter()
            .copied()
            .filter(|line| line.starts_with(&result_prefix))
            .collect();
        let null_drops: Vec<_> = body
            .lines
            .iter()
            .copied()
            .filter(|line| line.starts_with(&null_prefix))
            .collect();
        gate(
            "consume identity: exact result and null-wrapper LLVM destructors",
            result_drops.as_slice() == [result_drop.as_str()]
                && null_drops.as_slice() == [null_drop.as_str()]
                && !body
                    .lines
                    .iter()
                    .any(|line| line.starts_with(&mutable_prefix)),
            (result_drops, null_drops),
        )
    }
}
mod source;
mod unit;

pub(super) use unit::facts as unit_facts;

#[derive(Debug)]
pub(super) struct Facts {
    callable: Span,
    root: Span,
    declaration: Span,
    receiver: Span,
    consume: Span,
    control: Span,
    scalar: BuiltinType,
    source: Source,
    cleanup: Cleanup,
}

#[derive(Debug)]
enum Source {
    Local {
        construction: Span,
        puts: Vec<Span>,
    },
    Parameter {
        parameter: Span,
        index: usize,
        calls: Vec<CallSite>,
    },
}

#[derive(Debug)]
struct Cleanup {
    origin: Span,
    read: Option<Span>,
}

#[derive(Debug)]
struct CallSite {
    callable: Span,
    call: Span,
    root: Span,
    declaration: Span,
    initializer: CallerInit,
}

#[derive(Debug)]
enum CallerInit {
    Present { construction: Span },
    Absent { null: Span },
}

#[derive(Debug)]
struct Identity {
    function: FunctionId,
    branch_owner: ValueId,
    wrapper: ValueId,
    inner: ValueId,
    result: ValueId,
    null_owner: ValueId,
    nullable: SsaTypeId,
    mutable: SsaTypeId,
    readonly: SsaTypeId,
}

fn missing(label: &str, detail: impl std::fmt::Debug) -> Failure {
    Failure {
        stage: format!("consume identity: {label}"),
        detail: format!("{detail:#?}"),
    }
}

fn one<'a, T: std::fmt::Debug>(values: &'a [T], label: &str) -> Outcome<&'a T> {
    match values {
        [value] => Ok(value),
        _ => Err(missing(label, values)),
    }
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn span(parsed: &ParsedFile, expression: ExpressionId) -> Outcome<Span> {
    parsed
        .ast()
        .expressions()
        .get(expression)
        .map(|node| node.span())
        .map_err(|error| missing("expression", error))
}

fn transparent(parsed: &ParsedFile, mut expression: ExpressionId) -> Outcome<ExpressionId> {
    let mut seen = Vec::new();
    loop {
        gate(
            "consume identity: finite transparent source",
            !seen.contains(&expression),
            expression,
        )?;
        seen.push(expression);
        let node = parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|error| missing("group", error))?;
        match node.payload() {
            Expression::Group { expression: inner } => expression = *inner,
            _ => return Ok(expression),
        }
    }
}

fn declaration(parsed: &ParsedFile, root: Span) -> Outcome<(StatementId, Span, ExpressionId)> {
    let matches: Vec<_> = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            let Statement::LocalVariable { declaration } = node.payload() else {
                return None;
            };
            let item = parsed.ast().items().get(*declaration).ok()?;
            match item.payload() {
                Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if *name == root => Some((id, node.span(), *initializer)),
                _ => None,
            }
        })
        .collect();
    one(&matches, "real local declaration").copied()
}

fn callable(parsed: &ParsedFile, within: Span) -> Outcome<(Span, Span)> {
    let matches: Vec<_> = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Function {
                name: NameMarker::Present(name),
                ..
            } if contains(node.span(), within) => Some((node.span(), *name)),
            _ => None,
        })
        .collect();
    one(&matches, "real containing callable").copied()
}

fn call_argument(
    parsed: &ParsedFile,
    expression: ExpressionId,
    index: usize,
) -> Outcome<ExpressionId> {
    let node = parsed
        .ast()
        .expressions()
        .get(expression)
        .map_err(|error| missing("call", error))?;
    let Expression::Call { arguments, .. } = node.payload() else {
        return Err(missing("real AST call", node));
    };
    arguments
        .get(index)
        .map(|argument| argument.value)
        .ok_or_else(|| missing("actual call argument", (node, index)))
}

fn control(
    parsed: &ParsedFile,
    comparison: ExpressionId,
    consume: Span,
    non_null_when_true: bool,
) -> Outcome<(ExpressionId, Span)> {
    let mut matches = Vec::new();
    for (id, node) in parsed.ast().expressions().iter() {
        let Expression::If {
            condition,
            then_branch,
            else_branch,
            ..
        } = node.payload()
        else {
            continue;
        };
        if transparent(parsed, *condition)? != comparison {
            continue;
        }
        let body = if non_null_when_true {
            Some(*then_branch)
        } else {
            *else_branch
        };
        if body
            .and_then(|id| parsed.ast().statements().get(id).ok())
            .is_some_and(|node| contains(node.span(), consume))
        {
            matches.push((id, node.span()));
        }
    }
    one(&matches, "descriptor-polarity If containing consume").copied()
}

fn value(function: &Function, instruction: &Instruction) -> Outcome<ValueId> {
    match instruction.results.as_slice() {
        [EntityId::Value(value)] if value.function() == function.id => Ok(*value),
        _ => Err(missing("one source-qualified Value result", instruction)),
    }
}

fn ty(function: &Function, value: ValueId) -> Outcome<SsaTypeId> {
    match function.entity(EntityId::Value(value)).map(|data| data.ty) {
        Some(EntityType::Value(ty)) => Ok(ty),
        other => Err(missing("owner storage", (function.id, value, other))),
    }
}

fn at(
    function: &Function,
    origin: Span,
    matches: impl Fn(&Operation) -> bool,
) -> Outcome<&Instruction> {
    let found: Vec<_> = function
        .instructions
        .iter()
        .filter(|i| i.origin == Origin::Source(origin) && matches(&i.operation))
        .collect();
    one(&found, "source-qualified instruction").copied()
}

fn position(function: &Function, instruction: &Instruction) -> Outcome<usize> {
    function
        .block(instruction.block)
        .and_then(|block| {
            block
                .instructions
                .iter()
                .position(|id| *id == instruction.id)
        })
        .ok_or_else(|| missing("instruction block position", instruction))
}

fn check(
    program: &Program,
    entry: FunctionId,
    facts: &Facts,
    trace: &mut Trace,
) -> Outcome<Identity> {
    let matches: Vec<_> = program
        .modules
        .iter()
        .flat_map(|module| {
            module
                .functions
                .iter()
                .filter(|function| {
                    function.origin == Origin::Source(facts.callable)
                        && function.instructions.iter().any(|i| {
                            i.origin == Origin::Source(facts.consume)
                                && matches!(i.operation, Operation::MapConsume { .. })
                        })
                })
                .map(move |function| (module, function))
        })
        .collect();
    let &(module, function) = one(&matches, "actual callable and consume origin")?;
    let consume = at(function, facts.consume, |op| {
        matches!(op, Operation::MapConsume { .. })
    })?;
    gate(
        "consume identity: unique consume in selected callable",
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::MapConsume { .. }))
            .count()
            == 1,
        function.id,
    )?;
    let Operation::MapConsume {
        owner: inner,
        map_type: readonly,
    } = consume.operation
    else {
        return Err(missing("MapConsume", consume));
    };
    let result = value(function, consume)?;
    let branches: Vec<_> = function
        .blocks
        .iter()
        .filter_map(|block| {
            let term = block.terminator.as_ref()?;
            (term.origin == Origin::Source(facts.control)
                && matches!(term.kind, TerminatorKind::NullableBranch { .. }))
            .then_some((block, term))
        })
        .collect();
    let &(header, branch) = one(&branches, "real source If NullableBranch")?;
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = &branch.kind
    else {
        return Err(missing("NullableBranch", branch));
    };
    let present = graph::target(function, when_non_null)?;
    let absent = graph::target(function, when_null)?;
    let columns: Vec<_> = when_non_null
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(column, arg)| (*arg == EntityId::Value(*owner)).then_some(column))
        .collect();
    let column = *one(&columns, "actual owner column")?;
    let (Some(EntityId::Value(wrapper)), Some(EntityId::Value(null_owner))) = (
        present.parameters.get(column).copied(),
        absent.parameters.get(column).copied(),
    ) else {
        return Err(missing("actual successor owner columns", (present, absent)));
    };
    gate(
        "consume identity: exact original two edges and direct shared proof parameter",
        present.id != absent.id
            && when_null.arguments.get(column) == Some(&EntityId::Value(*owner))
            && present.parameters.len() == when_non_null.arguments.len() + 1
            && present.parameters.last() == Some(&EntityId::Loan(*view))
            && absent.parameters.len() == when_null.arguments.len(),
        (branch, present, absent),
    )?;
    graph::only_incoming(function, present.id, header.id, when_non_null)?;
    graph::only_incoming(function, absent.id, header.id, when_null)?;
    let take = at(function, facts.receiver, |op| {
        matches!(op, Operation::NullableTake { .. })
    })?;
    gate(
        "consume identity: current wrapper and original view take into same consume",
        take.operation
            == Operation::NullableTake {
                owner: wrapper,
                proof: *view,
            }
            && value(function, take)? == inner
            && take.block == present.id
            && consume.block == present.id
            && position(function, take)? < position(function, consume)?,
        (take, consume, branch),
    )?;
    let nullable = ty(function, *owner)?;
    let mutable = ty(function, inner)?;
    gate(
        "consume identity: canonical N/M/result and shared inner view",
        module.nullable_maps.get(&nullable) == Some(&mutable)
            && ty(function, wrapper)? == nullable
            && ty(function, null_owner)? == nullable
            && ty(function, result)? == readonly
            && mutable != readonly
            && inner != result
            && function.entity(EntityId::Loan(*view)).is_some_and(|data| {
                data.ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: mutable,
                    }
                    && data.definition
                        == Definition::BlockParameter {
                            block: present.id,
                            index: present.parameters.len() - 1,
                        }
            }),
        (nullable, mutable, readonly, view),
    )?;
    let pair = source::map_types(module, mutable, readonly, facts.scalar)?;
    for storage in [nullable, mutable, readonly] {
        gate(
            "consume identity: each obligation MoveOnly",
            module.type_ownership(storage) == Some(Ownership::MoveOnly),
            storage,
        )?;
    }
    source::check(
        source::SourceQuery {
            program,
            entry,
            module,
            consumer: function,
            header,
            owner: *owner,
        },
        facts,
        trace,
    )?;
    graph::only_instruction_use(function, EntityId::Value(wrapper), take.id)?;
    graph::only_instruction_use(function, EntityId::Loan(*view), take.id)?;
    graph::only_instruction_use(function, EntityId::Value(inner), consume.id)?;
    let result_aliases = graph::aliases(function, result)?;
    let null_aliases = graph::aliases(function, null_owner)?;
    graph::cleanup(
        function,
        consume.block,
        position(function, consume)? + 1,
        result,
        &result_aliases,
        &facts.cleanup,
        consume.id,
    )?;
    graph::cleanup(
        function,
        absent.id,
        0,
        null_owner,
        &null_aliases,
        &Cleanup {
            origin: facts.root,
            read: None,
        },
        consume.id,
    )?;
    trace.stages.push(format!("consume identity: source={:?}; function={:?}; branch={:?}; wrapper={wrapper:?}; original view={view:?}; take={:?}; inner={inner:?}; consume={:?}; result={result:?}; nullable/mutable/readonly={nullable:?}/{mutable:?}/{readonly:?}; K/V={pair:?}; result aliases={result_aliases:?}; null aliases={null_aliases:?}", facts.consume, function.id, header.id, take.id, consume.id));
    Ok(Identity {
        function: function.id,
        branch_owner: *owner,
        wrapper,
        inner,
        result,
        null_owner,
        nullable,
        mutable,
        readonly,
    })
}

/// Keep identity, verifier and complete LLVM outcomes separate. A failed query never
/// suppresses a reachable verifier/render; a failed verifier never enters the adapter.
pub(super) fn finish(
    program: &Program,
    entry: FunctionId,
    facts: Outcome<Facts>,
    trace: &mut Trace,
) -> Outcome<()> {
    let identity = facts.and_then(|facts| {
        trace
            .stages
            .push(format!("consume identity real FE facts={facts:#?}"));
        check(program, entry, &facts, trace)
    });
    trace
        .stages
        .push(format!("consume identity outcome={identity:#?}"));
    trace.stages.push(format!(
        "actual selected SSA entry={entry:?}; actual modules={:#?}",
        program.modules
    ));
    let verified = super::step(
        "consume identity pipeline verify",
        crate::ssa::verify::verify_program(program),
        trace,
    );
    let rendered = match verified {
        Ok(()) => {
            trace.stages.push(format!(
                "verified SSA:\n{}",
                crate::ssa::render_program(program)
            ));
            super::step(
                "consume identity pipeline complete LLVM",
                crate::llvm::render_verified_program_with_entry(program, entry),
                trace,
            )
        }
        Err(error) => {
            trace
                .stages
                .push("consume identity pipeline LLVM: not reached (verify failed)".to_owned());
            Err(error)
        }
    };
    let llvm_identity = match (&rendered, &identity) {
        (Ok(text), Ok(identity)) => {
            trace.stages.push(format!(
                "actual complete LLVM bytes={}; LLVM:\n{text}",
                text.len()
            ));
            Some(llvm::check(program, identity, text))
        }
        (Ok(text), Err(_)) => {
            trace.stages.push(format!("actual complete LLVM bytes={}; LLVM:\n{text}\nLLVM identity: not reached (SSA identity failed)", text.len()));
            None
        }
        (Err(_), _) => {
            trace
                .stages
                .push("LLVM identity: not reached (LLVM unavailable)".to_owned());
            None
        }
    };
    trace.stages.push(format!(
        "consume LLVM identity outcome={llvm_identity:#?}; pipeline outcome={:?}",
        rendered.as_ref().map(|_| ())
    ));
    identity?;
    rendered?;
    match llvm_identity {
        Some(outcome) => outcome,
        None => Err(missing("LLVM identity not reached", "preceding failure")),
    }
}

#[test]
fn nullable_map_consumes_keep_source_take_result_cleanup_identity() {
    let mut observations = Vec::new();
    for case in &CASES[5..] {
        for unit in [false, true] {
            let mut trace = Trace::default();
            let outcome = if unit {
                super::unit::observe_consume_identity(case, &mut trace)
            } else {
                super::single::observe_consume_identity(case, &mut trace)
            };
            observations.push(Observation {
                label: format!(
                    "{}/{}:{}",
                    case.id,
                    if unit { "unit" } else { "single" },
                    case.label
                ),
                trace,
                outcome,
            });
        }
    }
    for observation in &observations {
        diagnostic!(
            "NULLABLE CONSUME IDENTITY {}\n{}\ninput failures={:?}\nfinal outcome={:#?}",
            observation.label,
            observation.trace.stages.join("\n"),
            observation.trace.input_failures,
            observation.outcome
        );
    }
    assert_eq!(observations.len(), 6);
    assert!(
        observations
            .iter()
            .all(|value| value.trace.input_failures.is_empty() && value.outcome.is_ok()),
        "all six original C observations must independently pass input, identity, verify and full LLVM; see all outcomes above"
    );
}
