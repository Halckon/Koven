//! 同次公开 FE facts 的独立身份桥；不调用生产 qualification/proof。
#[path = "query_cleanup.rs"]
mod query_cleanup;
mod query_guards {
    //! 两个独立 Resource local 共用真实 Guard 类型与析构；形成不冒充当前 owner。
    use super::super::ast::{Anchors, Input};
    use super::{Failure, failure, need, one, unit_bridge};
    use lang_frontend::{
        name_resolution::{
            Namespace, SourceUnitId, SymbolKind, UnitSymbolId, ValidatedCompilationUnitNames,
        },
        ownership_checking::CompilationUnitOwnership,
        type_checking::{
            CompilationUnitTypes, ExpressionCategory, NominalKind, ParameterMode,
            UnitConstructionTarget, UnitExpressionId, UnitItemId, UnitStatementId, UnitTypeKind,
        },
    };

    pub(super) fn unit(
        f: &Input,
        a: &Anchors,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        unit: SourceUnitId,
    ) -> Result<[UnitSymbolId; 2], Failure> {
        let mut locals = Vec::new();
        let mut classes = Vec::new();
        let mut types = Vec::new();
        for guard in &a.guards {
            // 普通 Guard 无 Name 读取；只从真实 formed 表取 qualified ID，再回核 names marker。
            let local = one(
                owned.formed_owned_locals().iter().copied().filter(|id| {
                    id.source_unit() == unit
                        && unit_bridge::binding_at(names, *id, guard.marker, SymbolKind::Variable)
                            .is_ok()
                }),
                "temporary exits FE unique actual qualified Guard formation",
            )?;
            unit_bridge::binding_at(names, local, guard.marker, SymbolKind::Variable)?;
            let class = one(
                names.names().index().declarations().iter().filter(|d| {
                    d.source_unit() == unit
                        && d.root() == guard.class_item
                        && d.name_span() == guard.class_marker
                        && d.kind() == SymbolKind::Classifier
                        && d.namespace() == Namespace::Type
                }),
                "temporary exits FE unique actual qualified Guard declaration",
            )?
            .id();
            let nominal = one(
                typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter(|d| d.declaration() == class)
                    .filter_map(|d| d.nominal()),
                "temporary exits FE unique qualified Guard nominal",
            )?;
            let expression = UnitExpressionId::new(unit, guard.initializer);
            let construction = typed.construction(expression).ok_or_else(|| {
                failure(
                    "temporary exits FE actual qualified Guard construction",
                    guard,
                )
            })?;
            let deinit = nominal
                .deinit()
                .ok_or_else(|| failure("temporary exits FE actual qualified Guard deinit", nominal))?;
            need(
                nominal.declaration() == class
                    && nominal.kind() == NominalKind::Class
                    && nominal.fields().is_empty()
                    && nominal.type_parameters().is_empty()
                    && deinit.owner() == class
                    && deinit.item() == UnitItemId::new(unit, guard.deinit_item)
                    && deinit.body() == UnitStatementId::new(unit, guard.deinit_body)
                    && deinit.receiver_mode() == ParameterMode::Borrow
                    && deinit.receiver_type() == construction.result_type()
                    && construction.expression() == expression
                    && construction.arguments().is_empty()
                    && construction.instance().type_arguments().is_empty()
                    && construction.target() == UnitConstructionTarget::Nominal(class)
                    && matches!(typed.types().get(construction.result_type()),
                    Some(UnitTypeKind::Nominal { declaration, arguments }) if *declaration == class && arguments.is_empty())
                    && typed.expression_type(expression) == Some(construction.result_type())
                    && typed.expression_category(expression) == Some(ExpressionCategory::Temporary)
                    && typed.symbol_type(local) == Some(construction.result_type())
                    && typed.is_resource_type(construction.result_type()) == Some(true),
                "temporary exits FE qualified Guard Resource formation, nominal and real deinit",
                (guard, local, nominal, construction, deinit),
            )?;
            unit_bridge::callee(f, names, guard.initializer, class, Namespace::Type)?;
            locals.push(local);
            classes.push(class);
            types.push(construction.result_type());
        }
        need(
            locals.len() == 2
                && locals[0] != locals[1]
                && classes[0] == classes[1]
                && types[0] == types[1],
            "temporary exits FE qualified Guard locals distinct and same class/type",
            (&locals, classes, types),
        )?;
        locals
            .try_into()
            .map_err(|v| failure("temporary exits FE two actual qualified Guard locals", v))
    }
}
#[path = "query_inputs_unit.rs"]
mod unit_bridge;

use super::{
    super::{
        fixture::{Failure, contains, failure, need},
        query_inputs::one,
    },
    ast::{Anchors, Exit, Function, Input},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{SourceUnitId, ValidatedCompilationUnitNames},
    ownership_checking::CompilationUnitOwnership,
    parser::{BinaryOperator, Expression},
    source::Span,
    type_checking::{BuiltinType, CompilationUnitTypes},
};

#[derive(Clone, Copy, Debug)]
pub(super) struct GuardInput {
    pub marker: Span,
    pub construction: Span,
    pub deinit: Span,
    pub class_marker: Span,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct SiteInput {
    pub function: Span,
    pub span: Span,
    pub source: Span,
    pub factory: Span,
    pub body: Span,
    pub consumer: Span,
    pub read_call: Span,
    pub arguments: [Span; 2],
    pub transfer: Span,
    pub drop: Span,
    pub guard: GuardInput,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct OperandInput {
    pub function: Span,
    pub call: Span,
    pub arguments: [Span; 2],
}
#[derive(Clone, Copy, Debug)]
pub(super) struct CallerInput {
    pub function: Span,
    pub call: Span,
    pub returned: Span,
    pub expected: Span,
}
#[derive(Debug)]
pub(super) struct Inputs {
    pub unit: SourceUnitId,
    pub entry: Span,
    pub read: Span,
    pub sites: [SiteInput; 2],
    pub operand: Option<OperandInput>,
    pub callers: Vec<CallerInput>,
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
        "temporary exits FE unique actual function AST",
    )
}
fn expected_result(name: &str) -> BuiltinType {
    if matches!(name, "operand" | "scanNonempty" | "scanEmpty") {
        BuiltinType::Int
    } else {
        BuiltinType::Unit
    }
}
fn callee_span(f: &Input, expression: ExpressionId) -> Result<Span, Failure> {
    let (value, _) = f.expression(expression)?;
    let Expression::Call { callee, .. } = value else {
        return Err(failure("temporary exits FE actual Call", value));
    };
    let (callee, span) = f.expression(*callee)?;
    need(
        matches!(callee, Expression::Name),
        "temporary exits FE actual direct Name callee",
        callee,
    )?;
    Ok(span)
}

fn spans(f: &Input, a: &Anchors, drops: [Span; 2], unit: SourceUnitId) -> Result<Inputs, Failure> {
    let entry = anchor(a, "entry")?;
    let read = anchor(a, "read")?;
    let returning = f.exit == Exit::Return;
    need(
        a.scans.len() == if returning { 2 } else { 0 }
            && a.return_checks.len() == a.scans.len()
            && a.operand.is_some() == returning
            && a.sites[0].statement != a.sites[1].statement
            && a.sites[0].source != a.sites[1].source
            && a.sites[0].transfer != a.sites[1].transfer
            && a.sites[0].span.end() <= a.sites[1].span.start(),
        "temporary exits FE independent two sites and exact Return-only caller coverage",
        a,
    )?;
    let mut sites = Vec::new();
    for (index, site) in a.sites.iter().enumerate() {
        let function = anchor(
            a,
            if returning {
                if index == 0 {
                    "scanNonempty"
                } else {
                    "scanEmpty"
                }
            } else {
                "entry"
            },
        )?;
        let guard = &a.guards[index];
        let body = f
            .parsed
            .ast()
            .statements()
            .get(site.body)
            .map_err(|e| failure("temporary exits FE actual body span", e))?
            .span();
        let transfer = f.expression(site.transfer)?.1;
        let arguments = [
            f.expression(site.arguments[0])?.1,
            f.expression(site.arguments[1])?.1,
        ];
        need(
            contains(function.span, site.span)
                && contains(site.span, body)
                && contains(body, site.read_span)
                && contains(body, transfer)
                && contains(body, guard.span)
                && guard.marker.end() <= guard.span.start()
                && guard.span.end() <= site.read_span.start()
                && site.consumer == if returning { "operand" } else { "read" }
                && drops[index] == site.source_span
                && site.span.source_id() == f.parsed.source_id()
                && f.sources.source_name(site.span.source_id()) == Ok("provider.ko")
                && function.parameters.is_empty(),
            "temporary exits FE real enclosing function, construction, consumer and transfer spans",
            (site, guard, function),
        )?;
        sites.push(SiteInput {
            function: function.span,
            span: site.span,
            source: site.source_span,
            factory: anchor(a, site.factory)?.span,
            body,
            consumer: anchor(a, site.consumer)?.span,
            read_call: site.read_span,
            arguments,
            transfer,
            drop: drops[index],
            guard: GuardInput {
                marker: guard.marker,
                construction: guard.span,
                deinit: guard.deinit_span,
                class_marker: guard.class_marker,
            },
        });
    }
    let operand = if let Some(value) = &a.operand {
        let [key, value_arg] = value.call.arguments.as_slice() else {
            return Err(failure(
                "temporary exits FE actual operand read argument pair",
                value,
            ));
        };
        let function = anchor(a, "operand")?;
        need(
            value.binding_markers.as_slice() == function.parameters.as_slice()
                && value.call.target == "read"
                && contains(function.span, value.call.span),
            "temporary exits FE operand read keeps actual parameter roots",
            value,
        )?;
        Some(OperandInput {
            function: function.span,
            call: value.call.span,
            arguments: [f.expression(*key)?.1, f.expression(*value_arg)?.1],
        })
    } else {
        None
    };
    let mut callers = Vec::new();
    for (index, (scan, check)) in a.scans.iter().zip(&a.return_checks).enumerate() {
        let expected_text = if index == 0 { "7" } else { "0" };
        let expected = one(
            f.parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(_, node)| match node.payload() {
                    Expression::Binary {
                        left,
                        operator: BinaryOperator::Equal,
                        right,
                        ..
                    } if *left == check.reference => Some(*right),
                    _ => None,
                }),
            "temporary exits FE unique actual caller equality RHS",
        )?;
        let (literal, expected) = f.expression(expected)?;
        let returned = f.expression(check.reference)?.1;
        need(
            matches!(literal, Expression::Literal(_))
                && f.sources.slice(expected) == Ok(expected_text)
                && check.expected == expected_text
                && check.initializer == scan.expression
                && scan.arguments.is_empty()
                && scan.target
                    == if index == 0 {
                        "scanNonempty"
                    } else {
                        "scanEmpty"
                    }
                && contains(entry.span, scan.span)
                && contains(entry.span, returned)
                && scan.span.end() <= returned.start()
                && returned.end() <= expected.start(),
            "temporary exits FE actual zeroarg scanner and exact caller 7/0 comparison",
            (scan, check, expected),
        )?;
        callers.push(CallerInput {
            function: anchor(a, scan.target)?.span,
            call: scan.span,
            returned,
            expected,
        });
    }
    Ok(Inputs {
        unit,
        entry: entry.span,
        read: read.span,
        sites: sites
            .try_into()
            .map_err(|v| failure("temporary exits FE two source sites", v))?,
        operand,
        callers,
    })
}
