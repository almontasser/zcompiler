use crate::{
    lexer::{Literal, Token},
    parser::Symbol,
    types::Type,
};

#[derive(Debug, Clone)]
pub enum Node {
    BinaryExpr {
        left: Box<Node>,
        operator: Token,
        right: Box<Node>,
        ty: Type,
    },
    UnaryExpr {
        operator: Token,
        right: Box<Node>,
        ty: Type,
    },
    LiteralExpr {
        value: Literal,
        ty: Type,
    },
    GlobalVar {
        identifier: Token,
        ty: Type,
    },
    AssignStmt {
        identifier: Token,
        expr: Box<Node>,
    },
    CompoundStmt {
        statements: Vec<Node>,
    },
    IfStmt {
        condition: Box<Node>,
        then_branch: Box<Node>,
        else_branch: Option<Box<Node>>,
    },
    WhileStmt {
        condition: Box<Node>,
        body: Box<Node>,
    },
    FnDecl {
        identifier: Token,
        body: Box<Node>,
        return_type: Option<Type>,
    },
    FnCall {
        identifier: Token,
        expr: Box<Node>,
        ty: Type,
    },
    ReturnStmt {
        expr: Box<Node>,
        fn_name: Symbol,
    },
}

impl Node {
    pub fn ty(&self) -> Option<Type> {
        match self {
            Node::BinaryExpr { ty, .. } => Some(ty.clone()),
            Node::UnaryExpr { ty, .. } => Some(ty.clone()),
            Node::LiteralExpr { ty, .. } => Some(ty.clone()),
            Node::GlobalVar { ty, .. } => Some(ty.clone()),
            Node::AssignStmt { .. } => None,
            Node::CompoundStmt { .. } => None,
            Node::IfStmt { .. } => None,
            Node::WhileStmt { .. } => None,
            Node::FnDecl { .. } => None,
            Node::FnCall { ty, .. } => Some(ty.clone()),
            Node::ReturnStmt { .. } => None,
        }
    }
}
