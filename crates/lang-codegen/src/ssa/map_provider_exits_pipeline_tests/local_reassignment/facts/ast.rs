//! Original assignments, epoch births and loop identities shared by independent FE/SSA queries.
use super::*;
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{
        AssignmentOperator, Expression, FunctionBody, FunctionForm, Item, NameMarker, Statement,
        VariableKind,
    },
    source::Span,
};
#[derive(Debug)]
pub(in super::super) struct Loop {
    pub span: Span,
    pub statement: StatementId,
    pub source: ExpressionId,
    pub body: StatementId,
    pub epoch: usize,
    pub marker: Span,
}
#[derive(Debug)]
pub(in super::super) struct Site {
    pub scan: Span,
    pub scope: StatementId,
    pub declaration: StatementId,
    pub binding: Span,
    pub birth: ExpressionId,
    pub assignment: ExpressionId,
    pub target: ExpressionId,
    pub rhs: ExpressionId,
    pub alias: Option<(Span, ExpressionId)>,
    pub factories: [Span; 2],
    pub loops: Vec<Loop>,
}
pub(in super::super) fn inside(span: Span, outer: Span) -> bool {
    span.source_id() == outer.source_id()
        && span.start() > outer.start()
        && span.end() < outer.end()
}
pub(in super::super) fn expression(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}
pub(in super::super) fn declaration(
    sources: &SourceMap,
    parsed: &ParsedFile,
    wanted: &str,
) -> Result<Span, Failure> {
    one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok(wanted)).then_some(n.span())}),"actual original factory/scanner declaration")
}
pub(in super::super) fn sites(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
) -> Result<Vec<Site>, Failure> {
    [false,true].into_iter().map(|empty|{
        let scan=declaration(sources,parsed,if empty{"scanEmpty"}else{"scan"})?;
        let node=one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok()).filter(|n|n.span()==scan),"actual scanner item")?;
        let Item::Function{parameters,form,..}=node.payload() else{unreachable!()};
        need(parameters.is_empty(),"actual zero parameter scanner",node)?;
        let scope=match form{FunctionForm::ImplicitUnitBlock(b)|FunctionForm::Explicit{body:FunctionBody::Block(b),..}=>*b,_=>return Err(failure("actual scanner scope",form))};
        let (local,decl)=one(parsed.ast().statements().iter().filter_map(|(id,n)|{let Statement::LocalVariable{declaration}=n.payload() else{return None;};let item=parsed.ast().items().get(*declaration).ok()?;matches!(item.payload(),Item::Variable{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("source") && inside(n.span(),scan)).then_some((item,id))}),"actual source declaration statement")?;
        let Item::Variable{name:NameMarker::Present(binding),initializer:birth,kind:VariableKind::Var,..}=local.payload() else{return Err(failure("original var source initializer",local))};
        let (assignment,target,rhs)=one(parsed.ast().expressions().iter().filter_map(|(id,n)|{let Expression::Assignment{operator:AssignmentOperator::Assign,target,value}=n.payload() else{return None;};(inside(n.span(),scan) && sources.slice(expression(parsed,*target))==Ok("source")).then_some((id,*target,*value))}),"actual replacement RHS expression")?;
        let alias=parsed.ast().items().iter().filter_map(|(_,n)|{let Item::Variable{name:NameMarker::Present(name),initializer,kind:VariableKind::Val,..}=n.payload() else{return None;};(inside(n.span(),scan) && sources.slice(*name)==Ok("alias")).then_some((*name,*initializer))}).collect::<Vec<_>>();
        need(alias.len()==usize::from(case.history==2),"exact moved alias history",&alias)?;
        if let Some((_,init))=alias.first(){need(sources.slice(expression(parsed,*init))==Ok("source") && expression(parsed,*init).start()>expression(parsed,assignment).end(),"alias moves replacement epoch only",init)?;}
        let factories=[declaration(sources,parsed,if empty{"makeEmpty"}else{"make"})?,declaration(sources,parsed,if empty{"otherEmpty"}else{"other"})?];
        for (id,wanted) in [(*birth,if empty{"makeEmpty"}else{"make"}),(rhs,if empty{"otherEmpty"}else{"other"})]{let n=parsed.ast().expressions().get(id).unwrap();let Expression::Call{callee,arguments,..}=n.payload() else{return Err(failure("original complete factory Call",n))};need(arguments.is_empty() && sources.slice(expression(parsed,*callee))==Ok(wanted),"actual distinct epoch producer AST",n)?;}
        let mut loops=parsed.ast().statements().iter().filter_map(|(id,n)|{let Statement::For{source,body,..}=n.payload() else{return None;};inside(n.span(),scan).then_some((id,n.span(),*source,*body))}).collect::<Vec<_>>();loops.sort_by_key(|(_,s,_,_)|s.start());
        need(loops.len()==if case.history==1{2}else{1},"original epoch provider count",&loops)?;
        let loops=loops.into_iter().map(|(statement,span,source,body)|{
            let epoch=usize::from(span.start()>expression(parsed,assignment).end());let marker=if epoch==1{alias.first().map_or(*binding,|(name,_)|*name)}else{*binding};
            need(sources.slice(expression(parsed,source))==Ok(if epoch==1 && case.history==2{"alias"}else{"source"}),"actual epoch source reference",source)?;
            Ok(Loop{statement,span,source,body,epoch,marker})
        }).collect::<Result<Vec<_>,Failure>>()?;
        need(loops.iter().map(|l|l.epoch).collect::<Vec<_>>()==if case.history==1{vec![0,1]}else{vec![1]},"old/new loop chronology",&loops)?;
        Ok(Site{scan,scope,declaration:decl,binding:*binding,birth:*birth,assignment,target,rhs,alias:alias.first().copied(),factories,loops})
    }).collect()
}
