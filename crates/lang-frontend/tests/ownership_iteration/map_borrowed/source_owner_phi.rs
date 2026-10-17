//! P1 原始 owned Map 参数的独立只读身份查询；不接受任意循环 phi。
use super::{Fixture, Sites, single_reference, symbol_at};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{NameResolution, SymbolId, SymbolKind},
    ownership_checking::{
        CleanupCondition, CleanupConditions, CleanupOwnerValue, CleanupSelection,
        CleanupSelectorSource, DropPoint, DropTarget, IterationClosurePhiBinding,
        IterationPhiBoundary, IterationPhiIncomingBinding, IterationPhiIncomingKind,
        IterationPhiPresenceSource, LoanTarget, MapIterationOwnershipPlan,
        MapIterationSourceAccess, OwnershipCheckedFile,
    },
    parser::{
        Expression, FunctionBody, FunctionForm, Item, NameMarker, ParameterModeMarker, Statement,
    },
    source::Span,
};

type QueryResult<T = ()> = Result<T, &'static str>;

fn unique<T>(mut values: impl Iterator<Item = T>) -> Option<T> {
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

struct SourceSite {
    statement: StatementId,
    loop_span: Span,
    source: ExpressionId,
    symbol: SymbolId,
    origin: Span,
    fallthrough: StatementId,
}

impl SourceSite {
    fn from_ast(fixture: &Fixture, sites: &Sites, names: &NameResolution) -> QueryResult<Self> {
        let ast = fixture.file.ast();
        let node = ast
            .statements()
            .get(sites.statement)
            .map_err(|_| "missing actual for statement")?;
        let Statement::For { source, body, .. } = node.payload() else {
            return Err("actual statement is not for");
        };
        let expression = ast
            .expressions()
            .get(*source)
            .map_err(|_| "missing actual source expression")?;
        if *source != sites.source
            || expression.span() != sites.source_span
            || !matches!(expression.payload(), Expression::Name)
        {
            return Err("source expression/span is not the actual named source");
        }
        let symbol = single_reference(names, sites.source_span)
            .ok_or("source reference does not resolve uniquely")?;
        let body = ast
            .statements()
            .get(*body)
            .map_err(|_| "missing actual for body")?;
        let Statement::Block { elements } = body.payload() else {
            return Err("actual for body is not a block");
        };
        let fallthrough = *elements.last().ok_or("actual body has no last statement")?;
        ast.statements()
            .get(fallthrough)
            .map_err(|_| "missing actual last body statement")?;

        // 以函数 block 的真实子节点找声明，不靠源码名、日志数字或 span 猜测归属。
        let mut origins = Vec::new();
        for (_, item) in ast.items().iter() {
            let Item::Function {
                parameters, form, ..
            } = item.payload()
            else {
                continue;
            };
            let function_body = match form {
                FunctionForm::ImplicitUnitBlock(block)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(block),
                    ..
                } => *block,
                _ => continue,
            };
            let function_body = ast
                .statements()
                .get(function_body)
                .map_err(|_| "missing function body")?;
            let Statement::Block { elements } = function_body.payload() else {
                return Err("function block body is not a block");
            };
            if !elements.contains(&sites.statement) {
                continue;
            }
            for parameter in parameters {
                let NameMarker::Present(origin) = parameter.name else {
                    return Err("source function has a missing parameter name");
                };
                if symbol_at(names, origin).is_some_and(|candidate| {
                    candidate.id() == symbol && candidate.kind() == SymbolKind::ValueParameter
                }) {
                    if !matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_))) {
                        return Err("actual source parameter is not declared own");
                    }
                    origins.push(origin);
                }
            }
        }
        let [origin] = origins.as_slice() else {
            return Err("actual for must belong to exactly one matching own parameter declaration");
        };
        Ok(Self {
            statement: sites.statement,
            loop_span: node.span(),
            source: sites.source,
            symbol,
            origin: *origin,
            fallthrough,
        })
    }
}

struct Query<'a> {
    site: SourceSite,
    conditions: &'a CleanupConditions,
}

impl Query<'_> {
    fn phi<'a>(
        &self,
        plan: &'a MapIterationOwnershipPlan,
        boundary: IterationPhiBoundary,
    ) -> QueryResult<&'a IterationClosurePhiBinding> {
        let phi = unique(
            plan.closure_phis()
                .iter()
                .filter(|phi| phi.symbol() == self.site.symbol && phi.boundary() == boundary),
        )
        .ok_or("source boundary does not have exactly one phi")?;
        if !matches!(self.conditions.owner_value(phi.owner()),
            Some(CleanupOwnerValue::IterationPhi { statement, boundary: actual, symbol, origin })
            if *statement == self.site.statement && *actual == boundary
                && *symbol == self.site.symbol && *origin == self.site.loop_span)
        {
            return Err("phi owner is not this source at this actual loop boundary");
        }
        let selector = self
            .conditions
            .selector(phi.availability_selector())
            .ok_or("missing phi availability selector")?;
        if !matches!(selector.source(), CleanupSelectorSource::IterationPhi {
            statement, boundary: actual, ..
        } if statement == self.site.statement && actual == boundary)
            || selector.selection() != CleanupSelection::IterationPresence
            || selector.origin() != self.site.loop_span
            || selector.branch_count() != 2
        {
            return Err("availability selector is not this loop boundary's presence bit");
        }
        let Some(CleanupCondition::Choice { selector, branches }) =
            self.conditions.get(phi.availability_condition())
        else {
            return Err("phi availability is not a presence choice");
        };
        let [absent, present] = branches.as_slice() else {
            return Err("phi presence choice must have exactly two branches");
        };
        if *selector != phi.availability_selector()
            || self.conditions.get(*absent) != Some(&CleanupCondition::Never)
            || self.conditions.get(*present) != Some(&CleanupCondition::Always)
            || !phi.root_nodes().is_empty()
            || !phi.capture_layout().is_empty()
            || !phi.origins().is_empty()
        {
            return Err("plain Map owner presence/layout differs from its declared phi");
        }
        Ok(phi)
    }

    fn incoming<'a>(
        &self,
        plan: &'a MapIterationOwnershipPlan,
        phi: &IterationClosurePhiBinding,
        kind: IterationPhiIncomingKind,
        point: DropPoint,
    ) -> QueryResult<&'a IterationPhiIncomingBinding> {
        let incoming = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|incoming| incoming.kind() == kind),
        )
        .ok_or("missing or duplicate actual incoming kind")?;
        if incoming.point() != point
            || incoming.boundary() != phi.boundary()
            || self.conditions.get(incoming.condition()) != Some(&CleanupCondition::Always)
        {
            return Err("incoming does not match the actual AST point/boundary/path condition");
        }
        let [binding] = incoming.bindings() else {
            return Err("actual incoming must have exactly one source binding");
        };
        if binding.target() != phi.owner()
            || binding.availability_selector() != phi.availability_selector()
            || binding.presence_source() != IterationPhiPresenceSource::StaticConditions
            || !binding.capture_slots_to_clear().is_empty()
            || !binding.root_sources().is_empty()
            || !binding.selector_writes().is_empty()
            || !binding.origins().is_empty()
        {
            return Err("incoming binding does not write the exact plain source phi");
        }
        Ok(binding)
    }
}

pub(super) fn query(
    fixture: &Fixture,
    sites: &Sites,
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
) -> QueryResult {
    let query = Query {
        site: SourceSite::from_ast(fixture, sites, names)?,
        conditions: owned.cleanup_conditions(),
    };
    let site = &query.site;
    let plan = unique(
        owned
            .map_iterations()
            .iter()
            .filter(|plan| plan.descriptor().statement() == site.statement),
    )
    .ok_or("missing or duplicate actual Map plan")?;
    if plan.descriptor().source() != site.source
        || plan.source_access() != MapIterationSourceAccess::Owned
        || !matches!(plan.source(), Some(LoanTarget::Place(place))
            if place.is_root() && place.root() == site.symbol)
        || plan.closure_phis().len() != 2
        || plan.closure_phi_incomings().len() != 3
    {
        return Err("Map plan has wrong source identity or extra/missing phi topology");
    }
    let header = query.phi(plan, IterationPhiBoundary::Header)?;
    let exit = query.phi(plan, IterationPhiBoundary::Exit)?;
    if header.owner() == exit.owner()
        || header.availability_selector() == exit.availability_selector()
        || header.availability_condition() == exit.availability_condition()
    {
        return Err("Header and Exit must have independent owner/presence identities");
    }

    // 唯一入口必须落到真实参数根；仅存在自环绝不能证明 owner 有来源。
    let entry = query.incoming(
        plan,
        header,
        IterationPhiIncomingKind::Entry,
        DropPoint::AfterExpression(site.source),
    )?;
    let [root] = entry.values() else {
        return Err("Entry must carry exactly one parameter root");
    };
    if query.conditions.get(entry.available_when()) != Some(&CleanupCondition::Always)
        || root.condition() != entry.available_when()
        || !matches!(query.conditions.owner_value(root.source()),
            Some(CleanupOwnerValue::Parameter { symbol, origin })
            if *symbol == site.symbol && *origin == site.origin)
    {
        return Err("Entry root/condition does not match the actual own parameter");
    }
    let backedge = query.incoming(
        plan,
        header,
        IterationPhiIncomingKind::Fallthrough,
        DropPoint::AfterStatement(site.fallthrough),
    )?;
    let [carried] = backedge.values() else {
        return Err("actual body backedge must carry exactly one Header owner");
    };
    if carried.source() != header.owner()
        || carried.condition() != header.availability_condition()
        || backedge.available_when() != header.availability_condition()
    {
        return Err("body backedge does not preserve the same Header owner/presence");
    }
    let exhaustion = query.incoming(
        plan,
        exit,
        IterationPhiIncomingKind::Exhaustion,
        DropPoint::LoopExit(site.statement),
    )?;
    if query.conditions.get(exhaustion.available_when()) != Some(&CleanupCondition::Never)
        || !exhaustion.values().is_empty()
    {
        return Err("Exit must explicitly receive absent owner after exhaustion cleanup");
    }
    let drop = unique(owned.drops().iter().filter(|fact| {
        fact.target() == DropTarget::Named(site.symbol)
            || fact.owner().is_some_and(|owner| {
                owner == root.source() || owner == header.owner() || owner == exit.owner()
            })
    }))
    .ok_or("source/root/Header/Exit must have exactly one cleanup fact")?;
    if drop.target() != DropTarget::Named(site.symbol)
        || drop.point() != DropPoint::LoopExit(site.statement)
        || drop.value_origin() != site.origin
        || drop.owner() != Some(header.owner())
        || drop.condition() != Some(header.availability_condition())
        || drop.capture_slot().is_some()
        || drop.instance_address().is_some()
    {
        return Err("source Drop does not match the parameter origin and live Header owner");
    }
    Ok(())
}
