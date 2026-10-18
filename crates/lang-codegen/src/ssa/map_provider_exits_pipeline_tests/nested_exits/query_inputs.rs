//! 同次分析的公开 FE→SSA 身份桥；普通 query，不调用生产资格证明。
#[path = "query_cleanup.rs"]
mod query_cleanup;
#[path = "query_guards.rs"]
mod query_guards;
#[path = "query_inputs_single.rs"]
mod single_bridge;
#[path = "query_inputs_unit.rs"]
mod unit_bridge;

use super::{
    super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    },
    ast::{Anchors, Exit, Function, Input, Read, Site},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, ValidatedCompilationUnitNames},
    ownership_checking::{CompilationUnitOwnership, OwnershipCheckedFile},
    parser::Expression,
    source::Span,
    type_checking::{BuiltinType, CompilationUnitTypes, TypedFile},
};

#[derive(Clone, Copy, Debug)]
pub(super) struct SiteInput {
    pub span: Span,
    pub source: Span,
    pub body: Span,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ReadInput {
    pub span: Span,
    pub arguments: [Span; 2],
    pub site: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CallerInput {
    pub initializer: Span,
    pub call: Span,
    pub argument: Span,
    pub local: Span,
    pub size: Span,
    pub drop: Span,
    pub result_reference: Option<Span>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct GuardInput {
    pub marker: Span,
    pub construction: Span,
    pub deinit: Span,
    pub class_marker: Span,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct OperandInput {
    pub function: Span,
    pub call: Span,
    pub arguments: [Span; 4],
}

#[derive(Debug)]
pub(super) struct Inputs {
    pub unit: bool,
    pub entry: Span,
    pub scan: Span,
    pub read: Span,
    pub factory: Span,
    pub parameter: Span,
    pub sites: [SiteInput; 2],
    pub reads: Vec<ReadInput>,
    pub callers: [CallerInput; 2],
    pub sizes: Vec<Span>,
    pub guards: [GuardInput; 2],
    pub transfer: Span,
    pub operand: Option<OperandInput>,
}

pub(super) fn single(
    f: &Input,
    a: &Anchors,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Inputs, Failure> {
    single_bridge::collect(f, a, names, typed, owned)
}

pub(super) fn unit(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    unit_bridge::collect(f, a, names, typed, owned)
}

fn anchor<'a>(a: &'a Anchors, name: &str) -> Result<&'a Function, Failure> {
    one(
        a.functions.iter().filter(|v| v.name == name),
        "nested exits FE unique actual function AST",
    )
}

fn expected_result(f: &Input, name: &str) -> BuiltinType {
    if name == "operand" || (name == "scan" && f.exit == Exit::Return) {
        BuiltinType::Int
    } else {
        BuiltinType::Unit
    }
}

fn callee_span(f: &Input, expression: ExpressionId) -> Result<Span, Failure> {
    let (value, _) = f.expression(expression)?;
    let Expression::Call { callee, .. } = value else {
        return Err(failure("nested exits FE actual Call expression", value));
    };
    let (callee, span) = f.expression(*callee)?;
    need(
        matches!(callee, Expression::Name),
        "nested exits FE actual direct Name callee",
        callee,
    )?;
    Ok(span)
}

fn site_span(f: &Input, site: &Site) -> Result<SiteInput, Failure> {
    let body = f
        .parsed
        .ast()
        .statements()
        .get(site.body)
        .map_err(|e| failure("nested exits FE actual body span", e))?
        .span();
    Ok(SiteInput {
        span: site.span,
        source: site.source_span,
        body,
    })
}

fn read_span(f: &Input, a: &Anchors, read: &Read, index: usize) -> Result<ReadInput, Failure> {
    let [key, value] = read.call.arguments.as_slice() else {
        return Err(failure("nested exits FE actual read argument pair", read));
    };
    let sites = if f.exit == Exit::Return {
        &[0, 0, 1][..]
    } else {
        &[0, 0, 1, 0][..]
    };
    let site = *sites
        .get(index)
        .ok_or_else(|| failure("nested exits FE read count", index))?;
    let expected = if f.exit == Exit::Return && index > 0 {
        let parameters = &anchor(a, "operand")?.parameters;
        parameters
            .get(site * 2..site * 2 + 2)
            .ok_or_else(|| failure("nested exits FE operand parameter pair", parameters))?
    } else {
        &a.sites[site].bindings
    };
    need(
        read.binding_markers == expected,
        "nested exits FE read keeps real site or operand parameter marker pair",
        read,
    )?;
    Ok(ReadInput {
        span: read.call.span,
        arguments: [f.expression(*key)?.1, f.expression(*value)?.1],
        site,
    })
}

fn caller_span(f: &Input, a: &Anchors, index: usize, drop: Span) -> Result<CallerInput, Failure> {
    let call = &a.scans[index];
    let [argument] = call.arguments.as_slice() else {
        return Err(failure("nested exits FE actual scan argument", call));
    };
    let (local, initializer) = a.locals[index];
    let size = one(
        a.sizes.iter().filter(|s| s.root_marker == local),
        "nested exits FE actual caller size anchor",
    )?;
    need(
        drop == local,
        "nested exits FE caller Drop is actual local marker",
        (local, drop),
    )?;
    let result_reference = a
        .return_checks
        .get(index)
        .map(|check| f.expression(check.reference).map(|(_, span)| span))
        .transpose()?;
    Ok(CallerInput {
        initializer: f.expression(initializer)?.1,
        call: call.span,
        argument: f.expression(*argument)?.1,
        local,
        size: size.span,
        drop,
        result_reference,
    })
}

fn spans(f: &Input, a: &Anchors, drops: [Span; 2], unit: bool) -> Result<Inputs, Failure> {
    let parameter = a
        .parameter()
        .ok_or_else(|| failure("nested exits FE sole scan parameter", a))?;
    let mut sizes = a
        .sizes
        .iter()
        .filter(|s| s.root_marker == parameter)
        .map(|s| s.span)
        .collect::<Vec<_>>();
    sizes.sort_by_key(|s| s.start());
    need(
        sizes.len() == if f.exit == Exit::Return { 0 } else { 2 }
            && sizes
                .windows(2)
                .all(|pair| pair[0].end() <= pair[1].start())
            && a.reads.len() == if f.exit == Exit::Return { 3 } else { 4 }
            && a.return_checks.len() == if f.exit == Exit::Return { 2 } else { 0 },
        "nested exits FE exact scan size/read/result-reference coverage",
        (&sizes, a),
    )?;
    let operand = match &a.operand {
        Some(call) => {
            let [ok, ov, ik, iv] = call.arguments.as_slice() else {
                return Err(failure(
                    "nested exits FE exact four operand arguments",
                    call,
                ));
            };
            need(
                f.exit == Exit::Return,
                "nested exits FE operand only for Return",
                call,
            )?;
            Some(OperandInput {
                function: anchor(a, "operand")?.span,
                call: call.span,
                arguments: [
                    f.expression(*ok)?.1,
                    f.expression(*ov)?.1,
                    f.expression(*ik)?.1,
                    f.expression(*iv)?.1,
                ],
            })
        }
        None => {
            need(
                f.exit != Exit::Return,
                "nested exits FE Return requires operand",
                f.exit,
            )?;
            None
        }
    };
    Ok(Inputs {
        unit,
        entry: anchor(a, "entry")?.span,
        scan: anchor(a, "scan")?.span,
        read: anchor(a, "read")?.span,
        factory: anchor(a, a.factory.target)?.span,
        parameter,
        sites: [site_span(f, &a.sites[0])?, site_span(f, &a.sites[1])?],
        reads: a
            .reads
            .iter()
            .enumerate()
            .map(|(i, read)| read_span(f, a, read, i))
            .collect::<Result<Vec<_>, _>>()?,
        callers: [
            caller_span(f, a, 0, drops[0])?,
            caller_span(f, a, 1, drops[1])?,
        ],
        sizes,
        guards: a.guards.each_ref().map(|g| GuardInput {
            marker: g.marker,
            construction: g.span,
            deinit: g.deinit_span,
            class_marker: g.class_marker,
        }),
        transfer: f.expression(a.transfer)?.1,
        operand,
    })
}
