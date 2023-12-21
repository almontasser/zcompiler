use crate::{
    ast::Node,
    lexer::{Literal, Token, TokenType},
    parser::Symbol,
    types::Type,
};

pub struct CodeGen {
    nodes: Vec<Node>,
    assembly: String,
    registers: [bool; 4],
    label_count: usize,
}

const REGISTER_NAMES: [&str; 4] = ["%r8", "%r9", "%r10", "%r11"];
const BYTE_REGISTER_NAMES: [&str; 4] = ["%r8b", "%r9b", "%r10b", "%r11b"];
const DWORD_REGISTER_NAMES: [&str; 4] = ["%r8d", "%r9d", "%r10d", "%r11d"];

impl CodeGen {
    pub fn new(nodes: Vec<Node>) -> Self {
        Self {
            nodes,
            assembly: String::new(),
            registers: [false; 4],
            label_count: 0,
        }
    }

    pub fn generate(&mut self) -> String {
        self.preamble();

        for node in self.nodes.clone() {
            self.generate_node(node);
        }

        self.assembly.clone()
    }

    fn generate_node(&mut self, node: Node) -> usize {
        match node {
            Node::LiteralExpr { value, ty } => match value {
                Literal::Integer(i) => self.load(i as i64, ty),
                Literal::U8(u) => self.load(u as i64, ty),
                Literal::U32(u) => self.load(u as i64, ty),
                Literal::Identifier(i) => self.load_global(i, ty),
            },
            Node::BinaryExpr {
                left,
                operator,
                right,
                ty,
            } => {
                let left = self.generate_node(*left);
                let right = self.generate_node(*right);

                match operator.token_type {
                    TokenType::Add => self.add(left, right),
                    TokenType::Sub => self.subtract(left, right),
                    TokenType::Mul => self.multiply(left, right),
                    TokenType::Div => self.divide(left, right),
                    TokenType::Equal
                    | TokenType::NotEqual
                    | TokenType::LessThan
                    | TokenType::LessThanOrEqual
                    | TokenType::GreaterThan
                    | TokenType::GreaterThanOrEqual => {
                        self.compare_and_set(operator.token_type, left, right)
                    }
                    _ => panic!("Unexpected token {:?}", operator),
                }
            }
            Node::UnaryExpr {
                operator,
                right,
                ty,
            } => {
                match operator.token_type {
                    TokenType::Sub => {
                        let right_node = self.generate_node(*right.clone());
                        self.load(0, ty);
                        self.subtract(0, right_node)
                    }
                    TokenType::Widen => {
                        let right_node = self.generate_node(*right.clone());
                        self.widen(right_node, right.ty().unwrap(), ty)
                    }
                    TokenType::Ampersand => {
                        // get identifier
                        let identifier = match &*right {
                            Node::LiteralExpr { value, .. } => match value {
                                Literal::Identifier(i) => i,
                                _ => panic!("Unexpected token {:?}", right),
                            },
                            _ => panic!("Unexpected token {:?}", right),
                        };

                        self.address_of(identifier.to_string())
                    }
                    TokenType::Mul => {
                        let right_node = self.generate_node(*right.clone());
                        self.dereference(right_node, right.ty().unwrap())
                    }
                    _ => panic!("Unexpected token {:?}", operator),
                }
            }
            Node::GlobalVar { identifier, ty } => {
                self.define_global(identifier.lexeme.unwrap(), ty);
                0
            }
            Node::AssignStmt { identifier, expr } => {
                let register = self.generate_node(*expr.clone());
                self.store(register, identifier.lexeme.unwrap(), expr.ty().unwrap());
                self.free_register(register);
                0
            }
            Node::IfStmt {
                condition,
                then_branch,
                else_branch,
            } => self.if_stmt(condition, then_branch, else_branch),
            Node::CompoundStmt { statements } => {
                for statement in statements {
                    self.generate_node(statement);
                }
                0
            }
            Node::WhileStmt { condition, body } => self.while_stmt(condition, body),
            Node::FnDecl {
                identifier,
                body,
                return_type,
            } => self.function(identifier, body),
            Node::FnCall {
                identifier,
                expr,
                ty,
            } => {
                // TODO: fix ths hack
                let r = self.function_call(identifier.clone(), expr, ty);
                if identifier.lexeme.unwrap() == "printint" {
                    self.free_register(r);
                    0
                } else {
                    r
                }
            }
            Node::ReturnStmt { expr, fn_name } => self.return_stmt(expr, fn_name),
        }
    }

    fn preamble(&mut self) {
        self.free_all_registers();
        self.assembly.push_str("\t.text\n");
        self.assembly.push_str(".LC0:\n");
        self.assembly.push_str(".string\t\"%d\\n\"\n");
        self.assembly.push_str("printint:\n");
        self.assembly.push_str("\tpushq\t%rbp\n");
        self.assembly.push_str("\tmovq\t%rsp, %rbp\n");
        self.assembly.push_str("\tsubq\t$16, %rsp\n");
        self.assembly.push_str("\tmovl\t%edi, -4(%rbp)\n");
        self.assembly.push_str("\tmovl\t-4(%rbp), %eax\n");
        self.assembly.push_str("\tmovl\t%eax, %esi\n");
        self.assembly.push_str("\tleaq\t.LC0(%rip), %rdi\n");
        self.assembly.push_str("\tmovl\t$0, %eax\n");
        self.assembly.push_str("\tcall\tprintf@PLT\n");
        self.assembly.push_str("\tnop\n");
        self.assembly.push_str("\tleave\n");
        self.assembly.push_str("\tret\n\n");
    }

    fn postamble(&mut self) {
        self.assembly.push_str("\tmovl\t$0, %eax\n");
        self.assembly.push_str("\tpopq\t%rbp\n");
        self.assembly.push_str("\tret\n");
    }

    fn load(&mut self, value: i64, _ty: Type) -> usize {
        let r = self.allocate_register();
        self.assembly
            .push_str(&format!("\tmovq\t${}, {}\n", value, REGISTER_NAMES[r]));
        r
    }

    fn load_global(&mut self, identifier: String, ty: Type) -> usize {
        let r = self.allocate_register();
        if ty == Type::Int
            || ty == Type::PInt
            || ty == Type::PU8
            || ty == Type::PU32
            || ty == Type::U32
        {
            self.assembly
                .push_str(&format!("\tmovq\t{}, {}\n", identifier, REGISTER_NAMES[r]));
        } else if ty == Type::U8 {
            self.assembly.push_str(&format!(
                "\tmovzbq\t{}, {}\n",
                identifier, REGISTER_NAMES[r]
            ));
        } else if ty == Type::U32 {
            self.assembly.push_str(&format!(
                "\tmovzbl\t{}, {}\n",
                identifier, REGISTER_NAMES[r]
            ));
        } else {
            panic!("Unexpected type {:?}", ty);
        }

        r
    }

    fn store(&mut self, register: usize, identifier: String, ty: Type) {
        if ty == Type::Int || ty == Type::PInt || ty == Type::PU8 || ty == Type::PU32 {
            self.assembly.push_str(&format!(
                "\tmovq\t{}, {}\n",
                REGISTER_NAMES[register], identifier
            ));
        } else if ty == Type::U8 {
            self.assembly.push_str(&format!(
                "\tmovb\t{}, {}\n",
                BYTE_REGISTER_NAMES[register], identifier
            ));
        } else if ty == Type::U32 {
            self.assembly.push_str(&format!(
                "\tmovl\t{}, {}\n",
                DWORD_REGISTER_NAMES[register], identifier
            ));
        } else {
            panic!("Unexpected type {:?}", ty);
        }
    }

    fn widen(&mut self, register: usize, old_ty: Type, new_ty: Type) -> usize {
        register
    }

    fn define_global(&mut self, identifier: String, ty: Type) {
        let size = ty.size();
        self.assembly
            .push_str(&format!("\t.comm\t{}, {}, {}\n", identifier, size, size));
    }

    fn add(&mut self, left: usize, right: usize) -> usize {
        self.assembly.push_str(&format!(
            "\taddq\t{}, {}\n",
            REGISTER_NAMES[left], REGISTER_NAMES[right]
        ));
        self.free_register(left);
        right
    }

    fn subtract(&mut self, left: usize, right: usize) -> usize {
        self.assembly.push_str(&format!(
            "\tsubq\t{}, {}\n",
            REGISTER_NAMES[right], REGISTER_NAMES[left]
        ));
        self.free_register(right);
        left
    }

    fn multiply(&mut self, left: usize, right: usize) -> usize {
        self.assembly.push_str(&format!(
            "\timulq\t{}, {}\n",
            REGISTER_NAMES[left], REGISTER_NAMES[right]
        ));
        self.free_register(left);
        right
    }

    fn divide(&mut self, left: usize, right: usize) -> usize {
        self.assembly
            .push_str(&format!("\tmovq\t{}, %rax\n", REGISTER_NAMES[left]));
        self.assembly.push_str("\tcqo\n");
        self.assembly
            .push_str(&format!("\tidivq\t{}\n", REGISTER_NAMES[right]));
        self.assembly
            .push_str(&format!("\tmovq\t%rax, {}\n", REGISTER_NAMES[left]));
        self.free_register(right);
        left
    }

    fn printint(&mut self, register: usize) {
        self.assembly
            .push_str(&format!("\tmovq\t{}, %rdi\n", REGISTER_NAMES[register]));
        self.assembly.push_str("\tcall\tprintint\n");
        self.free_register(register);
    }

    fn allocate_register(&mut self) -> usize {
        for (i, available) in self.registers.iter_mut().enumerate() {
            if !*available {
                *available = true;
                return i;
            }
        }

        panic!("No available register");
    }

    fn free_register(&mut self, register: usize) {
        self.registers[register] = false;
    }

    fn free_all_registers(&mut self) {
        for i in 0..self.registers.len() {
            self.free_register(i);
        }
    }

    fn compare_and_jump(&mut self, operation: TokenType, left: usize, right: usize, label: usize) {
        // get inverted jump instructions
        let jump_instruction = match operation {
            TokenType::Equal => "jne",
            TokenType::NotEqual => "je",
            TokenType::LessThan => "jge",
            TokenType::LessThanOrEqual => "jg",
            TokenType::GreaterThan => "jle",
            TokenType::GreaterThanOrEqual => "jl",
            _ => panic!("Unexpected token {:?}", operation),
        };

        self.assembly.push_str(&format!(
            "\tcmpq\t{}, {}\n",
            REGISTER_NAMES[right], REGISTER_NAMES[left]
        ));
        self.assembly
            .push_str(&format!("\t{} L{}\n", jump_instruction, label));
        self.free_all_registers();
    }

    fn compare_and_set(&mut self, operation: TokenType, left: usize, right: usize) -> usize {
        // get set instructions
        let set_instruction = match operation {
            TokenType::Equal => "sete",
            TokenType::NotEqual => "setne",
            TokenType::LessThan => "setl",
            TokenType::LessThanOrEqual => "setle",
            TokenType::GreaterThan => "setg",
            TokenType::GreaterThanOrEqual => "setge",
            _ => panic!("Unexpected token {:?}", operation),
        };

        self.assembly.push_str(&format!(
            "\tcmpq\t{}, {}\n",
            REGISTER_NAMES[right], REGISTER_NAMES[left]
        ));
        self.assembly.push_str(&format!(
            "\t{} {}\n",
            set_instruction, BYTE_REGISTER_NAMES[right]
        ));
        self.assembly.push_str(&format!(
            "\tmovzbq\t{}, {}\n",
            BYTE_REGISTER_NAMES[right], REGISTER_NAMES[right]
        ));
        self.free_register(left);
        right
    }

    fn label(&mut self) -> usize {
        self.label_count += 1;
        self.label_count
    }

    fn generate_label(&mut self, label: usize) {
        self.assembly.push_str(&format!("L{}:\n", label));
    }

    fn jump(&mut self, label: usize) {
        self.assembly.push_str(&format!("\tjmp\tL{}\n", label));
    }

    fn if_stmt(
        &mut self,
        condition: Box<Node>,
        then_branch: Box<Node>,
        else_branch: Option<Box<Node>>,
    ) -> usize {
        let false_label = self.label();
        let end_label = self.label();

        let (left_reg, right_reg, operation) = match *condition {
            Node::BinaryExpr {
                left,
                operator,
                right,
                ty,
            } => {
                let left_reg = self.generate_node(*left);
                let right_reg = self.generate_node(*right);

                (left_reg, right_reg, operator.token_type)
            }
            _ => panic!("Unexpected token {:?}", condition),
        };

        // zero jump to the false label
        self.compare_and_jump(operation, left_reg, right_reg, false_label);
        self.free_all_registers();

        // generate the then branch code
        self.generate_node(*then_branch);
        self.free_all_registers();
        // unconditional jump to the end label
        self.jump(end_label);

        // generate the false label
        self.generate_label(false_label);

        // generate the else branch code
        if let Some(else_branch) = else_branch {
            self.generate_node(*else_branch);
            self.free_all_registers();
        }

        // generate the end label
        self.generate_label(end_label);
        0
    }

    fn while_stmt(&mut self, condition: Box<Node>, body: Box<Node>) -> usize {
        let start_label = self.label();
        let end_label = self.label();

        self.generate_label(start_label);

        let (left_reg, right_reg, operation) = match *condition {
            Node::BinaryExpr {
                left,
                operator,
                right,
                ty,
            } => {
                let left_reg = self.generate_node(*left);
                let right_reg = self.generate_node(*right);

                (left_reg, right_reg, operator.token_type)
            }
            _ => panic!("Unexpected token {:?}", condition),
        };

        // zero jump to the end label
        self.compare_and_jump(operation, left_reg, right_reg, end_label);
        self.free_all_registers();

        // generate the body code
        self.generate_node(*body);
        self.free_all_registers();

        // unconditional jump to the start label
        self.jump(start_label);

        // generate the end label
        self.generate_label(end_label);
        0
    }

    fn function(&mut self, identifier: Token, body: Box<Node>) -> usize {
        let fn_name = identifier.lexeme.unwrap();
        self.function_preamble(fn_name.clone());
        self.generate_node(*body);
        self.function_postamble(fn_name.clone());
        0
    }

    fn function_preamble(&mut self, name: String) {
        self.assembly.push_str("\t.global main\n");
        self.assembly
            .push_str(&format!("\t.type\t{}, @function\n", name));
        self.assembly.push_str(&format!("{}:\n", name));
        self.assembly.push_str("\tpushq\t%rbp\n");
        self.assembly.push_str("\tmovq\t%rsp, %rbp\n");
    }

    fn function_postamble(&mut self, fn_name: String) {
        // self.assembly.push_str(&format!("\tmovl\t$0, %eax\n"));
        // self.assembly.push_str(&format!("\tpopq\t%rbp\n"));
        // self.assembly.push_str(&format!("\tret\n"));
        self.assembly
            .push_str(format!("{}_end:\n", fn_name).as_str());
        self.assembly.push_str(&format!("\tpopq\t%rbp\n"));
        self.assembly.push_str(&format!("\tret\n"));
    }

    fn function_call(
        &mut self,
        identifier: crate::lexer::Token,
        expr: Box<Node>,
        ty: Type,
    ) -> usize {
        let register = self.generate_node(*expr);
        let out_register = self.allocate_register();
        self.assembly
            .push_str(&format!("\tmovq\t{}, %rdi\n", REGISTER_NAMES[register]));
        self.assembly
            .push_str(&format!("\tcall\t{}\n", identifier.lexeme.unwrap()));
        self.assembly
            .push_str(&format!("\tmovq\t%rax, {}\n", REGISTER_NAMES[out_register]));
        self.free_register(register);
        out_register
    }

    fn return_stmt(&mut self, expr: Box<Node>, fn_name: Symbol) -> usize {
        let register = self.generate_node(*expr);
        self.assembly
            .push_str(&format!("\tmovq\t{}, %rax\n", REGISTER_NAMES[register]));
        self.free_register(register);
        0
    }

    fn address_of(&mut self, ident: String) -> usize {
        let r = self.allocate_register();

        self.assembly
            .push_str(&format!("\tleaq\t{}(%rip), {}\n", ident, REGISTER_NAMES[r]));

        r
    }

    fn dereference(&mut self, register: usize, ty: Type) -> usize {
        match ty {
            Type::PInt | Type::PU32 => self.assembly.push_str(&format!(
                "\tmovq\t({}), {}\n",
                REGISTER_NAMES[register], REGISTER_NAMES[register]
            )),
            Type::PU8 => self.assembly.push_str(&format!(
                "\tmovzbq\t({}), {}\n",
                REGISTER_NAMES[register], REGISTER_NAMES[register]
            )),
            _ => panic!("Unexpected type {:?}", ty),
        }

        register
    }
}
