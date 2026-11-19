//! Prove a missing normal successor without type facts or constant evaluation.

use std::collections::BTreeMap;

use crate::{
    ast::{AstError, ExpressionId, StatementId},
    parser::{Expression, Item, Statement, StringPart, SyntaxAst},
};

#[derive(Clone, Copy)]
enum Node {
    Expression(ExpressionId),
    Statement(StatementId),
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum NodeKey {
    Expression(usize),
    Statement(usize),
}

impl Node {
    fn key(self) -> NodeKey {
        match self {
            Self::Expression(id) => NodeKey::Expression(id.index()),
            Self::Statement(id) => NodeKey::Statement(id.index()),
        }
    }
}

enum Rule {
    Known(bool),
    Sequence(Vec<Node>),
    If {
        condition: Node,
        then_branch: Node,
        else_branch: Option<Node>,
    },
}

enum Work {
    Visit(Node),
    Finish(Node, Rule),
}

pub(super) struct SyntacticJumps<'a> {
    ast: &'a SyntaxAst,
    stops: BTreeMap<NodeKey, bool>,
}

impl<'a> SyntacticJumps<'a> {
    pub(super) fn new(ast: &'a SyntaxAst) -> Self {
        Self {
            ast,
            stops: BTreeMap::new(),
        }
    }

    pub(super) fn expression_stops(&mut self, id: ExpressionId) -> Result<bool, AstError> {
        self.prove(Node::Expression(id))
    }

    pub(super) fn statement_stops(&mut self, id: StatementId) -> Result<bool, AstError> {
        self.prove(Node::Statement(id))
    }

    fn prove(&mut self, root: Node) -> Result<bool, AstError> {
        if let Some(&stops) = self.stops.get(&root.key()) {
            return Ok(stops);
        }
        let mut pending = vec![Work::Visit(root)];
        while let Some(work) = pending.pop() {
            match work {
                Work::Visit(node) if !self.stops.contains_key(&node.key()) => {
                    let rule = self.rule(node)?;
                    let children = pending.len();
                    match &rule {
                        Rule::Known(value) => {
                            self.stops.insert(node.key(), *value);
                            continue;
                        }
                        Rule::Sequence(nodes) => {
                            pending.extend(nodes.iter().rev().copied().map(Work::Visit));
                        }
                        Rule::If {
                            condition,
                            then_branch,
                            else_branch,
                        } => {
                            pending.extend(else_branch.iter().copied().map(Work::Visit));
                            pending.push(Work::Visit(*then_branch));
                            pending.push(Work::Visit(*condition));
                        }
                    }
                    // The stack visits children before publishing the parent proof.
                    pending.insert(children, Work::Finish(node, rule));
                }
                Work::Visit(_) => {}
                Work::Finish(node, rule) => {
                    let stops = |child: Node| self.stops[&child.key()];
                    let result = match rule {
                        Rule::Known(value) => value,
                        Rule::Sequence(nodes) => nodes.into_iter().any(stops),
                        Rule::If {
                            condition,
                            then_branch,
                            else_branch,
                        } => {
                            stops(condition)
                                || (stops(then_branch) && else_branch.is_some_and(stops))
                        }
                    };
                    self.stops.insert(node.key(), result);
                }
            }
        }
        Ok(self.stops[&root.key()])
    }

    fn rule(&self, node: Node) -> Result<Rule, AstError> {
        Ok(match node {
            Node::Expression(id) => match self.ast.expressions().get(id)?.payload() {
                Expression::Return { .. }
                | Expression::Break { .. }
                | Expression::Continue { .. } => Rule::Known(true),
                Expression::Group { expression } => {
                    Rule::Sequence(vec![Node::Expression(*expression)])
                }
                Expression::String { parts } => Rule::Sequence(
                    parts
                        .iter()
                        .filter_map(|part| match part {
                            StringPart::Interpolation { expression, .. } => {
                                Some(Node::Expression(*expression))
                            }
                            _ => None,
                        })
                        .collect(),
                ),
                Expression::Call {
                    callee, arguments, ..
                } if matches!(
                    self.ast.expressions().get(*callee)?.payload(),
                    Expression::Name
                ) =>
                {
                    Rule::Sequence(
                        arguments
                            .iter()
                            .map(|arg| Node::Expression(arg.value))
                            .collect(),
                    )
                }
                Expression::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => Rule::If {
                    condition: Node::Expression(*condition),
                    then_branch: Node::Statement(*then_branch),
                    else_branch: else_branch.map(Node::Statement),
                },
                _ => Rule::Known(false),
            },
            Node::Statement(id) => match self.ast.statements().get(id)?.payload() {
                Statement::Block { elements } | Statement::ControlBody { elements } => {
                    Rule::Sequence(elements.iter().copied().map(Node::Statement).collect())
                }
                Statement::Expression { expression } => {
                    Rule::Sequence(vec![Node::Expression(*expression)])
                }
                Statement::LocalDestructuring { initializer, .. } => {
                    Rule::Sequence(vec![Node::Expression(*initializer)])
                }
                Statement::LocalVariable { declaration } => {
                    match self.ast.items().get(*declaration)?.payload() {
                        Item::Variable { initializer, .. } => {
                            Rule::Sequence(vec![Node::Expression(*initializer)])
                        }
                        _ => Rule::Known(false),
                    }
                }
                // Loop headers are evaluated before entering the nested loop body.
                Statement::While { condition, .. } => {
                    Rule::Sequence(vec![Node::Expression(*condition)])
                }
                Statement::For { source, .. } => Rule::Sequence(vec![Node::Expression(*source)]),
                // A jump within a nested loop or callable does not prove that this node stops.
                _ => Rule::Known(false),
            },
        })
    }
}
