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
            scalar_ty == fields[field] && base_ty == source_ty && base == source && index == field,
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
        headers.push(parts(*one(&definitions, "one complete LLVM map type")?)?);
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
