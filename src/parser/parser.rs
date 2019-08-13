//! This module contains all functions directly responsible for parsing the tokens
//! and creating the AST from them.

use super::super::{
    ast::{
        declaration::{Declaration, FuncSignature, Function, FunctionArg, Variable},
        expression::Expression,
        literal::Literal,
        statement::Statement,
    },
    lexer::token::{Token, Type},
};
use super::Parser;

#[macro_use]
mod bin_macro {
    /// This macro is used to generate binary operator parsing functions.
    /// The parser is a recursive descent parser.
    /// name is the name of the binary operation, next is the descending function name.
    /// matching is an array literal of the tokens that should match.
    #[macro_export]
    macro_rules! binary_op {
        ($name:ident, $next:ident, $matching:expr) => {
            fn $name(&mut self) -> Option<Expression<'p>> {
                let mut left = self.$next()?;
                while let Some(operator) = self.match_tokens(&$matching) {
                    let right = self.$next()?;
                    left = Expression::Binary {
                        left: Box::new(left), operator, right: Box::new(right)
                    }
                }
                Some(left)
            }
        };
    }
}

// TODO: Implement the rest of the parser.
impl<'p> Parser<'p> {
    /// The entry point for generating a statement.
    /// The reason for returning Option is that the parser will error out and abort the current
    /// statement when illegal syntax is encountered.
    /// Note that syncronization is not done on error, and needs to done by the caller.
    pub fn declaration(&mut self) -> Option<Declaration<'p>> {
        match () {
            _ if self.match_token(Type::Class) => self.class_declaration(),
            _ if self.match_token(Type::Enum) => self.enum_declaration(),
            _ if self.match_token(Type::ExFn) => self.ex_func_declaration(),
            _ if self.match_token(Type::Func) => Some(Declaration::Function(self.function()?)),
            _ => {
                self.error_at_current("Encountered invalid top-level declaration.");
                None
        }
    }
    }

    fn ex_func_declaration(&mut self) -> Option<Declaration<'p>> {
        let name = self.consume(Type::Identifier, "Expected an external function name.")?; 
        self.consume(Type::LeftParen, "Expected '(' after function name.");

        let mut parameters: Vec<FunctionArg> = Vec::new();
        if !self.check(Type::RightParen) {
            loop {
                parameters.push(FunctionArg {
                    _type: self.consume(Type::Identifier, "Expected parameter type.")?,
                    name: self.consume(Type::Identifier, "Expected parameter name.")?,
                });
                if !self.match_token(Type::Comma) {
                    break;
                }
            }
        }
        self.consume(Type::RightParen, "Expected ')' after parameters.");

        let return_type = if self.match_token(Type::Arrow) {
            Some(self.consume(Type::Identifier, "Expected return type after '->'.")?)
        } else {
            None
        };

        Some(Declaration::ExternFunction(FuncSignature {
            name,
            return_type,
            parameters,
        }))
    }

    fn class_declaration(&mut self) -> Option<Declaration<'p>> {
        let name = self.consume(Type::Identifier, "Expected a class name.")?;
        self.consume(Type::LeftBrace, "Expected '{' before class body.");

        let mut methods: Vec<Function> = Vec::new();
        let mut variables: Vec<Variable> = Vec::new();

        while !self.check(Type::RightBrace) && !self.is_at_end() {
            match () {
                _ if self.match_token(Type::Func) => methods.push(self.function()?),
                _ if self.match_token(Type::Var) => variables.push(self.variable(false)?),
                _ if self.match_token(Type::Val) => variables.push(self.variable(true)?),
                _ => self.error_at_current("Encountered invalid declaration inside class.")?,
            }
        }

        self.consume(Type::RightBrace, "Expected '}' after class body.");
        Some(Declaration::Class {
            name,
            methods,
            variables,
        })
    }

    fn enum_declaration(&mut self) -> Option<Declaration<'p>> {
        let name = self.consume(Type::Identifier, "Expected an enum name.")?;
        self.consume(Type::LeftBrace, "Expected '{' before enum body.");

        let mut variants: Vec<Token> = Vec::new();
        while !self.check(Type::RightBrace) {
            variants.push(self.consume(Type::Identifier, "Expected enum variant.")?);
            if !self.match_token(Type::Comma) {
                break;
        }
        }
        self.consume(Type::RightBrace, "Expected '}' after enum body.");

        Some(Declaration::Enum { name, variants })
    }

    fn function(&mut self) -> Option<Function<'p>> {
        // Will generate a declaration that contains everything except a body
        let func_decl = self.ex_func_declaration()?;

        if let Declaration::ExternFunction(sig) = func_decl {
            let body = self.expression()?;
            Some(Function { sig, body })
        } else {
            panic!("External function generator generated something else!!");
        }
    }

    fn variable(&mut self, is_val: bool) -> Option<Variable<'p>> {
        let name = self.consume(Type::Identifier, "Expected variable name.")?;
        self.consume(Type::Equal, "Expected '=' after variable name.");
        let initializer = self.expression()?;
        self.consume_semi_or_nl("Expected newline or ';' after variable declaration.");

        Some(Variable {
            name,
            is_val,
            initializer,
        })
    }

    fn statement(&mut self) -> Option<Statement<'p>> {
        match () {
            _ if self.match_token(Type::Error) => self.error_statement(),
            _ if self.match_token(Type::For) => self.for_statement(),
            _ if self.match_token(Type::Var) => Some(Statement::Variable(self.variable(false)?)),
            _ if self.match_token(Type::Val) => Some(Statement::Variable(self.variable(true)?)),
            _ => self.expression_statement(),
        }
    }

    fn error_statement(&mut self) -> Option<Statement<'p>> {
        let mut value = None;
        if !self.check_semi_or_nl() {
            value = Some(self.expression()?);
        }
        self.consume_semi_or_nl("Expected newline or ';' after 'error'.");
        Some(Statement::Error(value))
    }

    fn for_statement(&mut self) -> Option<Statement<'p>> {
        self.consume(Type::LeftParen, "Expected '(' after 'for'.");
        
        Some(//if self.check_next(Type::In) { // for (x in y)
            // TODO: Implement "for each in" loops
        /*} else*/ { // for (condition)
            let condition = self.expression()?;
            self.consume(Type::RightParen, "Expected ')' after for condition.");
            let body = self.expression()?;

            Statement::For { condition, body }
            },
        )
    }

    fn expression_statement(&mut self) -> Option<Statement<'p>> {
        let requires_semicolon =
            ![Type::If, Type::LeftBrace, Type::When].contains(&self.current.t_type);
        let statement = Statement::Expression(self.expression()?);
        if requires_semicolon {
            self.consume_semi_or_nl("Expected newline or ';' after expression.");
        }
        Some(statement)
    }

    fn expression(&mut self) -> Option<Expression<'p>> {
        match () {
            _ if self.match_token(Type::LeftBrace) => self.block(),
            _ if self.match_token(Type::If) => self.if_expression(),
            _ if self.match_token(Type::Return) => self.return_expression(),
            _ if self.match_token(Type::Take) => self.take_expression(),
            _ if self.match_token(Type::When) => self.when_expression(),
            _ => self.assignment(),
        }
    }

    fn block(&mut self) -> Option<Expression<'p>> {
        let mut statements: Vec<Statement> = Vec::new();
        while !self.check(Type::RightBrace) && !self.is_at_end() {
            statements.push(self.statement()?);
        }

        self.consume(Type::RightBrace, "Expected '}' after block.");
        Some(Expression::Block(statements))
    }

    fn take_expression(&mut self) -> Option<Expression<'p>> {
        let value = Box::new(self.expression()?);
        let mut else_branch = None;
        if self.match_token(Type::Else) {
            else_branch = Some(Box::new(self.expression()?));
        }
        Some(Expression::Take { value, else_branch })
    }

    fn if_expression(&mut self) -> Option<Expression<'p>> {
        self.consume(Type::LeftParen, "Expected '(' after 'if'.");
        let condition = Box::new(self.expression()?);
        self.consume(Type::RightParen, "Expected ')' after if condition.");
        let then_branch = Box::new(self.expression()?);

        let mut else_branch = None;
        if self.match_token(Type::Else) {
            else_branch = Some(Box::new(self.expression()?));
        }

        Some(Expression::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    fn return_expression(&mut self) -> Option<Expression<'p>> {
        let mut value = None;
        if !self.check_semi_or_nl() {
            value = Some(Box::new(self.expression()?));
        }
        self.consume_semi_or_nl("Expected newline or ';' after 'return'.");
        Some(Expression::Return(value))
    }

    fn when_expression(&mut self) -> Option<Expression<'p>> {
        self.consume(Type::LeftParen, "Expected '(' after 'when'.");
        let value = Box::new(self.expression()?);
        self.consume(Type::RightParen, "Expected ')' after when value.");
        self.consume(Type::LeftBrace, "Expected '{' after when value.");

        let mut branches: Vec<(Expression<'p>, Expression<'p>)> = Vec::new();
        let mut else_branch = None;
        while !self.match_token(Type::RightBrace) {
            if self.match_token(Type::Else) {
                self.consume(Type::Arrow, "Expected '->' after when condition.");
                else_branch = Some(self.expression()?);
            } else {
                let condition = self.expression()?;
                self.consume(Type::Arrow, "Expected '->' after when condition.");
                let expression = self.expression()?;
                branches.push((condition, expression));
            }
        }
        if else_branch.is_none() {
            self.error_at_current("'when' expression is missing 'else' branch.");
        }

        Some(Expression::When {
            value,
            branches,
            else_branch: Box::new(else_branch?),
        })
    }

    fn assignment(&mut self) -> Option<Expression<'p>> {
        let expression = self.logic_or()?;

        if self.match_token(Type::Equal) {
            let value = Box::new(self.expression()?);
            match expression {
                Expression::Variable(name) => Some(Expression::Assignment { name, value }),
                Expression::Get { object, name } => Some(Expression::Set {
                    object: object,
                    name: name,
                    value,
                }),
                _ => {
                    self.error_at_current("Invalid assignment target.");
                    None
                }
            }
        } else {
            Some(expression)
        }
    }

    /// See the macro at the top of the file for info on how this works.
    binary_op!(logic_or, logic_and, [Type::Or]);
    binary_op!(logic_and, equality, [Type::And]);
    binary_op!(equality, comparison, [Type::BangEqual, Type::EqualEqual]);
    binary_op!(comparison, addition, [Type::Less, Type::LessEqual, Type::Greater, Type::GreaterEqual]);
    binary_op!(addition, multiplication, [Type::Plus, Type::Minus]);
    binary_op!(multiplication, unary, [Type::Star, Type::Slash]);

    fn unary(&mut self) -> Option<Expression<'p>> {
        Some(
            if let Some(operator) = self.match_tokens(&[Type::Bang, Type::Minus]) {
            let right = Box::new(self.unary()?);
                Expression::Unary { operator, right }
        } else {
            self.call()?
            },
        )
    }

    fn call(&mut self) -> Option<Expression<'p>> {
        let mut expression = self.primary()?;
        loop {
            match () {
                _ if self.match_token(Type::LeftParen) => {
                    let mut arguments: Vec<Expression> = Vec::new();
                    if !self.check(Type::RightParen) {
                        loop {
                            arguments.push(self.expression()?);
                            if !self.match_token(Type::Comma) {
                                break;
                            }
                        }
                    }

                    let paren =
                        self.consume(Type::RightParen, "Expected ')' after call arguments.")?;
                    expression = Expression::Call {
                        callee: Box::new(expression),
                        token: paren,
                        arguments,
                    }
                    }

                _ if self.match_token(Type::Dot) => {
                    expression = Expression::Get {
                        object: Box::new(expression),
                        name: self.consume(Type::Identifier, "Expected property name after '.'.")?,
                    }
                    }

                _ => break,
            }
        }
        Some(expression)
    }

    // TODO: Support for array literals
    fn primary(&mut self) -> Option<Expression<'p>> {
        Some(match () {
            _ if self.match_token(Type::None) => Expression::Literal(Literal::None),
            _ if self.match_token(Type::False) => Expression::Literal(Literal::Bool(false)),
            _ if self.match_token(Type::True) => Expression::Literal(Literal::Bool(true)),
            _ if self.match_token(Type::LeftParen) => self.grouping()?,
            _ if self.check(Type::Identifier) => Expression::Variable(self.advance()),
            _ if self.check(Type::This) => Expression::This(self.advance()),
            _ if self.check(Type::Int) => self.integer()?,
            _ if self.check(Type::Float) => self.float()?,
            _ if self.check(Type::String) => self.string(),
            _ => {
                self.error_at_current("Expected expression.");
                None?
            }
        })
    }

    fn grouping(&mut self) -> Option<Expression<'p>> {
        let expression = self.expression()?;
        self.consume(Type::RightParen, "Expected ')' after expression.");
        Some(Expression::Grouping(Box::new(expression)))
    }

    fn integer(&mut self) -> Option<Expression<'p>> {
        let token = self.advance();
        Some(Expression::Literal(Literal::Int(
            token.lexeme.parse().ok()?,
        )))
    }

    // TODO: Support for single-prec float
    fn float(&mut self) -> Option<Expression<'p>> {
        let token = self.advance();
        Some(Expression::Literal(Literal::Double(
            token.lexeme.parse().ok()?,
        )))
    }

    fn string(&mut self) -> Expression<'p> {
        let token = self.advance();
        Expression::Literal(Literal::String(token.lexeme.to_string()))
    }
}
