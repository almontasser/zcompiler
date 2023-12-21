use core::panic;

use crate::{
    ast::Node,
    lexer::{Literal, Token, TokenType},
    types::Type,
};

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolType {
    Function,
    Variable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub identifier: Token,
    pub structure: SymbolType,
    pub ty: Option<Type>,
    pub end_label: Option<String>,
}

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
    nodes: Vec<Node>,
    symbols: Vec<Symbol>,
    current_fn: Option<Symbol>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            current: 0,
            nodes: Vec::new(),
            symbols: vec![
                // builtin functions
                // add print function
                Symbol {
                    identifier: Token {
                        token_type: TokenType::Identifier,
                        lexeme: Some(String::from("printint")),
                        line: 0,
                        column: 0,
                        value: None,
                    },
                    structure: SymbolType::Function,
                    ty: Some(Type::Int),
                    end_label: None,
                },
            ],
            current_fn: None,
        }
    }

    pub fn parse(&mut self) -> &Vec<Node> {
        while !self.is_at_end() {
            let node = self.fn_decl();
            self.nodes.push(node);
        }

        &self.nodes
    }

    fn is_at_end(&self) -> bool {
        self.peek().token_type == TokenType::EOF
    }

    fn compound_statement(&mut self) -> Node {
        let mut nodes = Vec::new();

        self.expect(vec![TokenType::LeftBrace]);

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            let node = self.single_statement();
            match node {
                Node::AssignStmt { .. }
                | Node::GlobalVar { .. }
                | Node::FnCall { .. }
                | Node::ReturnStmt { .. } => {
                    self.expect(vec![TokenType::SemiColon]);
                }
                _ => {}
            }
            nodes.push(node);
        }

        self.expect(vec![TokenType::RightBrace]);

        Node::CompoundStmt { statements: nodes }
    }

    fn single_statement(&mut self) -> Node {
        if self.match_token(vec![TokenType::Let]) {
            return self.var_decl();
        } else if self.match_token(vec![TokenType::Identifier]) {
            return self.assignment();
        } else if self.match_token(vec![TokenType::If]) {
            return self.if_statement();
        } else if self.match_token(vec![TokenType::While]) {
            return self.while_statement();
        } else if self.match_token(vec![TokenType::For]) {
            return self.for_statement();
        } else if self.match_token(vec![TokenType::Fn]) {
            return self.fn_decl();
        } else if self.match_token(vec![TokenType::Return]) {
            return self.return_statement();
        } else {
            panic!(
                "Expected print at line {} column {} got {:?}",
                self.peek().line,
                self.peek().column,
                self.peek().token_type
            );
        }
    }

    fn parse_type(&mut self) -> Type {
        // a type of a variable is like these examples:
        // let x: int;
        // let y: u8;
        // let z: *u32; // pointer to u32
        // let a: **int; // pointer to pointer to int

        let mut pointers_counter: u8 = 0;
        while self.match_token(vec![TokenType::Mul]) {
            pointers_counter += 1
        }

        let ty_token = self.expect(vec![TokenType::Int, TokenType::U8, TokenType::U32]);

        let mut ty = match ty_token.token_type {
            TokenType::Int => Type::Int,
            TokenType::U8 => Type::U8,
            TokenType::U32 => Type::U32,
            _ => panic!("Expected type"),
        };

        for _ in 0..pointers_counter {
            ty = ty.pointer_to();
        }

        ty
    }

    fn var_decl(&mut self) -> Node {
        // let ty = self.parse_type(self.previous(1));
        let identifier = self.expect(vec![TokenType::Identifier]);
        self.expect(vec![TokenType::Colon]);
        let ty = self.parse_type();
        self.add_symbol(identifier.clone(), SymbolType::Variable, ty.clone(), None);
        Node::GlobalVar { identifier, ty }
    }

    fn assignment(&mut self) -> Node {
        let identifier = self.previous(1);
        // make sure the identifier is declared
        let symbol = self.find_symbol(identifier.clone());

        if symbol.is_none() {
            panic!(
                "Identifier {} not declared at line {} column {}",
                identifier.lexeme.clone().unwrap(),
                identifier.line,
                identifier.column
            );
        }
        let symbol = symbol.unwrap();

        if self.match_token(vec![TokenType::LeftParen]) {
            if symbol.structure != SymbolType::Function {
                panic!(
                    "Expected function at line {} column {}",
                    identifier.line, identifier.column
                );
            }

            return self.function_call();
        }

        if symbol.structure != SymbolType::Variable {
            panic!(
                "Expected variable at line {} column {}",
                identifier.line, identifier.column
            );
        }

        self.expect(vec![TokenType::Assign]);
        let expr = self.expression();

        // Check if the type is compatible
        let mut widen_left = false;
        let mut widen_right = false;
        if !self.type_compatible(
            expr.ty().unwrap(),
            symbol.ty.unwrap(),
            true,
            &mut widen_left,
            &mut widen_right,
        ) {
            panic!(
                "Incompatible types at line {} column {}",
                self.previous(1).line,
                self.previous(1).column
            );
        }

        if widen_left {
            return Node::AssignStmt {
                identifier,
                expr: Box::new(Node::UnaryExpr {
                    operator: Token {
                        token_type: TokenType::Widen,
                        lexeme: None,
                        line: self.previous(1).line,
                        column: self.previous(1).column,
                        value: None,
                    },
                    right: Box::new(expr.clone()),
                    ty: expr.ty().unwrap(),
                }),
            };
        } else {
            Node::AssignStmt {
                identifier,
                expr: Box::new(expr),
            }
        }
    }

    fn if_statement(&mut self) -> Node {
        self.expect(vec![TokenType::LeftParen]);
        let expr = self.expression();
        match &expr {
            Node::BinaryExpr { operator, .. } => {
                if operator.token_type != TokenType::Equal
                    && operator.token_type != TokenType::NotEqual
                    && operator.token_type != TokenType::LessThan
                    && operator.token_type != TokenType::LessThanOrEqual
                    && operator.token_type != TokenType::GreaterThan
                    && operator.token_type != TokenType::GreaterThanOrEqual
                {
                    panic!(
                        "Expected comparison operator at line {} column {}",
                        operator.line, operator.column
                    );
                }
            }
            _ => panic!("Expected comparison operator"),
        }
        self.expect(vec![TokenType::RightParen]);
        let then_branch = self.compound_statement();
        let else_branch = if self.match_token(vec![TokenType::Else]) {
            Some(Box::new(self.compound_statement()))
        } else {
            None
        };

        Node::IfStmt {
            condition: Box::new(expr),
            then_branch: Box::new(then_branch),
            else_branch,
        }
    }

    fn expression(&mut self) -> Node {
        let node = self.equality();
        node
    }

    fn equality(&mut self) -> Node {
        let mut node = self.comparison();

        while self.match_token(vec![TokenType::Equal, TokenType::NotEqual]) {
            let operator = self.previous(1);
            let right = self.comparison();
            node = Node::BinaryExpr {
                left: Box::new(node),
                operator,
                right: Box::new(right),
                ty: Type::U8,
            };
        }

        node
    }

    fn comparison(&mut self) -> Node {
        let mut node = self.term();

        while self.match_token(vec![
            TokenType::LessThan,
            TokenType::LessThanOrEqual,
            TokenType::GreaterThan,
            TokenType::GreaterThanOrEqual,
        ]) {
            let operator = self.previous(1);
            let right = self.term();
            node = Node::BinaryExpr {
                left: Box::new(node),
                operator,
                right: Box::new(right),
                ty: Type::U8,
            };
        }

        node
    }

    fn term(&mut self) -> Node {
        let mut node = self.factor();

        while self.match_token(vec![TokenType::Add, TokenType::Sub]) {
            let operator = self.previous(1);
            let mut right = self.factor();
            // check if the types are compatible
            let mut widen_left = false;
            let mut widen_right = false;
            if !self.type_compatible(
                node.ty().unwrap(),
                right.ty().unwrap(),
                operator.token_type == TokenType::Sub,
                &mut widen_left,
                &mut widen_right,
            ) {
                panic!(
                    "Incompatible types at line {} column {}",
                    operator.line, operator.column
                );
            }

            if widen_left {
                node = Node::UnaryExpr {
                    operator: Token {
                        token_type: TokenType::Widen,
                        lexeme: None,
                        line: operator.line,
                        column: operator.column,
                        value: None,
                    },
                    right: Box::new(node.clone()),
                    ty: node.ty().unwrap(),
                };
            }

            if widen_right {
                right = Node::UnaryExpr {
                    operator: Token {
                        token_type: TokenType::Widen,
                        lexeme: None,
                        line: operator.line,
                        column: operator.column,
                        value: None,
                    },
                    right: Box::new(right.clone()),
                    ty: right.ty().unwrap(),
                };
            }

            node = Node::BinaryExpr {
                left: Box::new(node.clone()),
                operator,
                right: Box::new(right),
                ty: node.ty().unwrap(),
            };
        }

        node
    }

    fn factor(&mut self) -> Node {
        let mut node = self.unary();

        while self.match_token(vec![TokenType::Mul, TokenType::Div]) {
            let operator = self.previous(1);
            let mut right = self.unary();

            // check if the types are compatible
            let mut widen_left = false;
            let mut widen_right = false;
            if !self.type_compatible(
                node.ty().unwrap(),
                right.ty().unwrap(),
                operator.token_type == TokenType::Sub,
                &mut widen_left,
                &mut widen_right,
            ) {
                panic!(
                    "Incompatible types at line {} column {}",
                    operator.line, operator.column
                );
            }

            if widen_left {
                node = Node::UnaryExpr {
                    operator: Token {
                        token_type: TokenType::Widen,
                        lexeme: None,
                        line: operator.line,
                        column: operator.column,
                        value: None,
                    },
                    right: Box::new(node.clone()),
                    ty: node.ty().unwrap(),
                };
            }

            if widen_right {
                right = Node::UnaryExpr {
                    operator: Token {
                        token_type: TokenType::Widen,
                        lexeme: None,
                        line: operator.line,
                        column: operator.column,
                        value: None,
                    },
                    right: Box::new(right.clone()),
                    ty: right.ty().unwrap(),
                };
            }

            node = Node::BinaryExpr {
                left: Box::new(node.clone()),
                operator,
                right: Box::new(right),
                ty: node.ty().unwrap(),
            };
        }

        node
    }

    fn unary(&mut self) -> Node {
        if self.match_token(vec![TokenType::Sub]) {
            let operator = self.previous(1);
            let right = self.unary();
            return Node::UnaryExpr {
                operator,
                right: Box::new(right.clone()),
                ty: right.ty().unwrap(),
            };
        }

        self.prefix()
    }

    fn prefix(&mut self) -> Node {
        let mut node: Node;
        if self.match_token(vec![TokenType::Ampersand]) {
            node = self.prefix();

            // ensure that the node is an identifier
            match &node {
                Node::LiteralExpr { value, .. } => match value {
                    Literal::Identifier(_) => {}
                    _ => panic!("Expected identifier"),
                },
                _ => panic!("Expected identifier"),
            }

            node = Node::UnaryExpr {
                operator: Token {
                    token_type: TokenType::Ampersand,
                    lexeme: None,
                    line: self.previous(1).line,
                    column: self.previous(1).column,
                    value: None,
                },
                right: Box::new(node.clone()),
                ty: node.ty().unwrap().pointer_to(),
            };
        } else if self.match_token(vec![TokenType::Mul]) {
            node = self.prefix();

            // ensure that the node is an identifier or a dereference
            match &node {
                Node::LiteralExpr { value, .. } => match value {
                    Literal::Identifier(_) => {}
                    _ => panic!("Expected identifier"),
                },
                Node::UnaryExpr { operator, .. } => {
                    if operator.token_type != TokenType::Ampersand {
                        panic!("Expected identifier");
                    }
                }
                _ => panic!("Expected identifier"),
            }

            node = Node::UnaryExpr {
                operator: Token {
                    token_type: TokenType::Mul,
                    lexeme: None,
                    line: self.previous(1).line,
                    column: self.previous(1).column,
                    value: None,
                },
                right: Box::new(node.clone()),
                ty: node.ty().unwrap().value_at(),
            };
        } else {
            node = self.primary();
        }

        node
    }

    fn primary(&mut self) -> Node {
        if self.match_token(vec![TokenType::Integer]) {
            let val: u64 = match self.previous(1).value {
                Some(Literal::Integer(val)) => val,
                _ => panic!("Expected integer"),
            };
            return Node::LiteralExpr {
                value: if val <= u8::MAX as u64 {
                    Literal::U8(val as u8)
                } else {
                    Literal::Integer(val)
                },
                ty: if val <= u8::MAX as u64 {
                    Type::U8
                } else {
                    Type::Int
                },
            };
        } else if self.match_token(vec![TokenType::Identifier]) {
            let identifier = self.previous(1);
            match self.find_symbol(identifier.clone()) {
                Some(symbol) => {
                    // TODO: This is hacky, fix it
                    if self.match_token(vec![TokenType::LeftParen]) {
                        if symbol.structure != SymbolType::Function {
                            panic!(
                                "Expected function at line {} column {}",
                                identifier.line, identifier.column
                            );
                        }
                        return self.function_call();
                    } else {
                        if symbol.structure != SymbolType::Variable {
                            panic!(
                                "Expected variable at line {} column {}",
                                identifier.line, identifier.column
                            );
                        }
                    }
                    return Node::LiteralExpr {
                        value: Literal::Identifier(identifier.lexeme.clone().unwrap()),
                        ty: symbol.ty.unwrap(),
                    };
                }
                None => panic!(
                    "Variable {} not declared at line {} column {}",
                    identifier.lexeme.clone().unwrap(),
                    identifier.line,
                    identifier.column
                ),
            }
        }

        let token = self.peek();
        panic!(
            "Unexpected token {:?} at line {} column {}",
            token.token_type, token.line, token.column
        );
    }

    fn match_token(&mut self, vec: Vec<TokenType>) -> bool {
        for token_type in vec {
            if self.check(token_type) {
                self.advance();
                return true;
            }
        }

        false
    }

    fn expect(&mut self, tokens: Vec<TokenType>) -> Token {
        for token in &tokens {
            if self.check(*token) {
                return self.advance();
            }
        }

        panic!(
            "Expected {:?} at line {} column {}, got {:?}",
            tokens,
            self.peek().line,
            self.peek().column,
            self.peek().token_type
        );
    }

    fn check(&self, token_type: TokenType) -> bool {
        if self.is_at_end() {
            return false;
        }

        self.peek().token_type == token_type
    }

    fn peek(&self) -> Token {
        self.tokens[self.current].clone()
    }

    fn advance(&mut self) -> Token {
        if !self.is_at_end() {
            self.current += 1;
        }

        self.previous(1)
    }

    fn previous(&self, i: usize) -> Token {
        self.tokens[self.current - i].clone()
    }

    fn add_symbol(
        &mut self,
        identifier: Token,
        structure: SymbolType,
        ty: Type,
        end_label: Option<String>,
    ) -> Symbol {
        let symbol = self.find_symbol(identifier.clone());
        if symbol.is_some() {
            panic!(
                "Variable {} already declared at line {} column {}",
                identifier.lexeme.clone().unwrap(),
                identifier.line,
                identifier.column
            );
        }

        let symbol = Symbol {
            identifier,
            structure,
            ty: Some(ty),
            end_label,
        };

        self.symbols.push(symbol.clone());

        symbol
    }

    fn find_symbol(&self, identifier: Token) -> Option<Symbol> {
        for symbol in &self.symbols {
            if symbol.identifier.lexeme.clone().unwrap() == identifier.lexeme.clone().unwrap() {
                return Some(symbol.clone());
            }
        }

        None
    }

    fn while_statement(&mut self) -> Node {
        self.expect(vec![TokenType::LeftParen]);
        let expr = self.expression();
        match &expr {
            Node::BinaryExpr { operator, .. } => {
                if operator.token_type != TokenType::Equal
                    && operator.token_type != TokenType::NotEqual
                    && operator.token_type != TokenType::LessThan
                    && operator.token_type != TokenType::LessThanOrEqual
                    && operator.token_type != TokenType::GreaterThan
                    && operator.token_type != TokenType::GreaterThanOrEqual
                {
                    panic!(
                        "Expected comparison operator at line {} column {}",
                        operator.line, operator.column
                    );
                }
            }
            _ => panic!("Expected comparison operator"),
        }
        self.expect(vec![TokenType::RightParen]);
        let body = self.compound_statement();

        Node::WhileStmt {
            condition: Box::new(expr),
            body: Box::new(body),
        }
    }

    fn for_statement(&mut self) -> Node {
        self.expect(vec![TokenType::LeftParen]);
        let initializer = if self.match_token(vec![TokenType::SemiColon]) {
            None
        // } else if self.match_token(vec![TokenType::Let]) {
        //     Some(self.var_decl())
        } else if self.match_token(vec![TokenType::Identifier]) {
            let node = self.assignment();
            self.expect(vec![TokenType::SemiColon]);
            Some(node)
        } else {
            panic!("Expected identifier");
        };

        let condition = if self.check(TokenType::SemiColon) {
            Node::LiteralExpr {
                value: Literal::Integer(1),
                ty: Type::U8,
            }
        } else {
            self.expression()
        };
        self.expect(vec![TokenType::SemiColon]);

        let increment = if self.check(TokenType::RightParen) {
            None
        } else {
            Some(self.single_statement())
        };
        self.expect(vec![TokenType::RightParen]);

        let mut body = self.compound_statement();

        if let Some(increment) = increment {
            body = Node::CompoundStmt {
                statements: vec![body, increment],
            };
        }

        body = Node::WhileStmt {
            condition: Box::new(condition),
            body: Box::new(body),
        };

        if let Some(initializer) = initializer {
            body = Node::CompoundStmt {
                statements: vec![initializer, body],
            };
        }

        body
    }

    fn fn_decl(&mut self) -> Node {
        self.expect(vec![TokenType::Fn]);
        let identifier = self.expect(vec![TokenType::Identifier]);
        let end_label = Some(format!("{}{}", identifier.lexeme.clone().unwrap(), "_end"));
        let symbol = self.add_symbol(
            identifier.clone(),
            SymbolType::Function,
            Type::Int,
            end_label,
        );
        self.expect(vec![TokenType::LeftParen]);
        // TODO: parse parameters
        self.expect(vec![TokenType::RightParen]);

        let mut ty: Option<Type> = None;
        if self.match_token(vec![TokenType::Colon]) {
            ty = Some(self.parse_type());
        }
        self.current_fn = Some(symbol.clone());
        let body = self.compound_statement();
        // ensure that the function returns a value if it has a return type in the last statement
        if ty.is_some() {
            match &body {
                Node::CompoundStmt { statements } => {
                    if statements.len() == 0 {
                        panic!(
                            "Function {} does not return a value at line {} column {}",
                            identifier.lexeme.clone().unwrap(),
                            identifier.line,
                            identifier.column
                        );
                    }

                    let last = statements.last().unwrap();
                    match last {
                        Node::ReturnStmt { .. } => {}
                        _ => panic!(
                            "Function {} does not return a value at line {} column {}",
                            identifier.lexeme.clone().unwrap(),
                            identifier.line,
                            identifier.column
                        ),
                    }
                }
                _ => panic!(
                    "Function {} does not return a value at line {} column {}",
                    identifier.lexeme.clone().unwrap(),
                    identifier.line,
                    identifier.column
                ),
            }
        }

        self.current_fn = None;

        Node::FnDecl {
            identifier,
            body: Box::new(body),
            return_type: ty,
        }
    }

    fn type_compatible(
        &self,
        left: Type,
        right: Type,
        right_only: bool,
        widen_left: &mut bool,
        widen_right: &mut bool,
    ) -> bool {
        if left == right {
            *widen_left = false;
            *widen_right = false;
            return true;
        }

        let left_size = left.size();
        let right_size = right.size();

        if left_size < right_size {
            *widen_left = true;
            *widen_right = false;
            return true;
        }

        if left_size > right_size {
            if right_only {
                return false;
            }

            *widen_left = false;
            *widen_right = true;
            return true;
        }

        *widen_left = false;
        *widen_right = false;
        true
    }

    fn function_call(&mut self) -> Node {
        let identifier = self.previous(2);
        let symbol = self.find_symbol(identifier.clone());

        if symbol.is_none() {
            panic!(
                "Function {} not declared at line {} column {}",
                identifier.lexeme.clone().unwrap(),
                identifier.line,
                identifier.column
            );
        }

        let symbol = symbol.unwrap();
        if symbol.structure != SymbolType::Function {
            panic!(
                "Expected function at line {} column {}",
                identifier.line, identifier.column
            );
        }

        let expr = self.expression();

        self.expect(vec![TokenType::RightParen]);

        Node::FnCall {
            identifier,
            expr: Box::new(expr),
            ty: symbol.ty.unwrap(),
        }
    }

    fn return_statement(&mut self) -> Node {
        if self.current_fn.is_none() {
            panic!("Return statement outside of function");
        }

        let fn_sym = self.current_fn.clone().unwrap();

        if !fn_sym.ty.is_some() {
            panic!(
                "Function {} has no return type",
                fn_sym.identifier.lexeme.clone().unwrap()
            );
        }

        let mut expr = self.expression();

        // Check if the type is compatible
        let mut widen_left = false;
        let mut widen_right = false;
        if !self.type_compatible(
            expr.ty().unwrap(),
            fn_sym.clone().ty.unwrap(),
            true,
            &mut widen_left,
            &mut widen_right,
        ) {
            panic!(
                "Incompatible types at line {} column {}",
                self.previous(1).line,
                self.previous(1).column
            );
        }

        if widen_left {
            expr = Node::UnaryExpr {
                operator: Token {
                    token_type: TokenType::Widen,
                    lexeme: None,
                    line: self.previous(1).line,
                    column: self.previous(1).column,
                    value: None,
                },
                right: Box::new(expr.clone()),
                ty: expr.ty().unwrap(),
            };
        }

        Node::ReturnStmt {
            expr: Box::new(expr),
            fn_name: fn_sym,
        }
    }
}
