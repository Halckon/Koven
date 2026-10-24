//! 同次公开 FE facts 的独立桥；仅在完整验证后交付源码身份，不制造 SSA ID。
#[path = "query_inputs_unit.rs"]
mod unit_bridge;

use super::{
    super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    },
    ast::{Anchors, Function, Input, Read, Site},
};
use lang_frontend::{
    ast::ExpressionId, name_resolution::ValidatedCompilationUnitNames,
    ownership_checking::CompilationUnitOwnership, parser::Expression, source::Span,
    type_checking::CompilationUnitTypes,
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
}

#[derive(Debug)]
pub(super) struct Inputs {
    pub entry: Span,
    pub scan: Span,
    pub read: Span,
    pub factory: Span,
    pub parameter: Span,
    pub sites: [SiteInput; 2],
    pub reads: [ReadInput; 4],
    pub callers: [CallerInput; 2],
    pub sizes: [Span; 2],
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
        "nested FE unique actual function AST",
    )
}

fn callee_span(f: &Input, expression: ExpressionId) -> Result<Span, Failure> {
    let (value, _) = f.expression(expression)?;
    let Expression::Call { callee, .. } = value else {
        return Err(failure("nested FE actual Call expression", value));
    };
    Ok(f.expression(*callee)?.1)
}

fn site_span(f: &Input, site: &Site) -> Result<SiteInput, Failure> {
    let body = f
        .parsed
        .ast()
        .statements()
        .get(site.body)
        .map_err(|e| failure("nested FE actual body span", e))?
        .span();
    Ok(SiteInput {
        span: site.span,
        source: site.source_span,
        body,
    })
}

fn read_span(f: &Input, read: &Read) -> Result<ReadInput, Failure> {
    let [key, value] = read.call.arguments.as_slice() else {
        return Err(failure("nested FE actual read argument pair", read));
    };
    need(
        read.binding_site < 2,
        "nested FE actual read provider index",
        read,
    )?;
    Ok(ReadInput {
        span: read.call.span,
        arguments: [f.expression(*key)?.1, f.expression(*value)?.1],
        site: read.binding_site,
    })
}

fn caller_span(f: &Input, a: &Anchors, index: usize, drop: Span) -> Result<CallerInput, Failure> {
    let call = &a.scans[index];
    let [argument] = call.arguments.as_slice() else {
        return Err(failure("nested FE actual scan argument", call));
    };
    let (local, initializer) = a.locals[index];
    let size = one(
        a.sizes.iter().filter(|s| s.root_marker == local),
        "nested FE actual caller size anchor",
    )?;
    need(
        drop == local,
        "nested FE caller Drop origin is actual local marker",
        (local, drop),
    )?;
    Ok(CallerInput {
        initializer: f.expression(initializer)?.1,
        call: call.span,
        argument: f.expression(*argument)?.1,
        local,
        size: size.span,
        drop,
    })
}

fn spans(f: &Input, a: &Anchors, drops: [Span; 2]) -> Result<Inputs, Failure> {
    let parameter = a
        .parameter()
        .ok_or_else(|| failure("nested FE actual sole scan parameter", a))?;
    let mut sizes = a
        .sizes
        .iter()
        .filter(|s| s.root_marker == parameter)
        .collect::<Vec<_>>();
    sizes.sort_by_key(|s| s.span.start());
    let [first, second] = sizes.as_slice() else {
        return Err(failure("nested FE actual two scan sizes", sizes));
    };
    need(
        first.span != second.span && first.span.end() <= second.span.start(),
        "nested FE scan sizes in actual source order",
        &sizes,
    )?;
    Ok(Inputs {
        entry: anchor(a, "entry")?.span,
        scan: anchor(a, "scan")?.span,
        read: anchor(a, "read")?.span,
        factory: anchor(a, a.factory.target)?.span,
        parameter,
        sites: [site_span(f, &a.sites[0])?, site_span(f, &a.sites[1])?],
        reads: [
            read_span(f, &a.reads[0])?,
            read_span(f, &a.reads[1])?,
            read_span(f, &a.reads[2])?,
            read_span(f, &a.reads[3])?,
        ],
        callers: [
            caller_span(f, a, 0, drops[0])?,
            caller_span(f, a, 1, drops[1])?,
        ],
        sizes: [first.span, second.span],
    })
}
