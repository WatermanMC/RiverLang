// last updated: june 2, 2026
// Parser: converts a stream of tokens into an AST

use crate::ast::*;
use crate::lexer::{Token, TokenWithPos};

pub struct Parser {
    tokens: Vec<TokenWithPos>,
    pos: usize,
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub _col: usize,
}

impl ParseError {
    fn new(msg: &str, line: usize, _col: usize) -> Self {
        Self {
            message: msg.to_string(),
            line,
            _col,
        }
    }
}

type ParseResult<T> = Result<T, ParseError>;

impl Parser {
    pub fn new(tokens: Vec<TokenWithPos>) -> Self {
        Self { tokens, pos: 0 }
    }

    // Peek at the current token without consuming it
    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    // Peek ahead by 'offset' tokens
    fn peek_at(&self, offset: usize) -> &Token {
        let idx = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[idx].token
    }

    fn current_line(&self) -> usize {
        self.tokens[self.pos].line
    }

    fn current_col(&self) -> usize {
        self.tokens[self.pos].col
    }

    // Advance to the next token and return the previous one
    fn advance(&mut self) -> &TokenWithPos {
        let t = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    // Skip over newline and semicolon tokens (they act as statement separators)
    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline | Token::Semicolon) {
            self.advance();
        }
    }

    // Expect a specific token; consume it if present, otherwise error
    fn expect(&mut self, expected: &Token) -> ParseResult<()> {
        self.skip_newlines();
        if self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            Err(ParseError::new(
                &format!("expected {:?} but got {:?}", expected, self.peek()),
                self.current_line(),
                self.current_col(),
            ))
        }
    }

    // If the current token matches 'tok', consume it and return true
    fn eat_if(&mut self, tok: &Token) -> bool {
        if self.peek() == tok {
            self.advance();
            true
        } else {
            false
        }
    }

    // Parse a whole program (list of top‑level items)
    pub fn parse_program(&mut self) -> ParseResult<Program> {
        let mut items = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek(), Token::Eof) {
            match self.parse_top_level() {
                Ok(item) => items.push(item),
                Err(e) => return Err(e),
            }
            self.skip_newlines();
        }
        Ok(Program { items })
    }

    // Parse a top‑level item: import, include, class, or raw C++
    fn parse_top_level(&mut self) -> ParseResult<TopLevel> {
        self.skip_newlines();
        match self.peek().clone() {
            Token::Use => {
                self.advance();
                let mut path = String::new();
                while !matches!(self.peek(), Token::Newline | Token::Semicolon | Token::Eof) {
                    match self.peek().clone() {
                        Token::Ident(s) => {
                            path.push_str(&s);
                            self.advance();
                        }
                        Token::Colon => {
                            path.push(':');
                            self.advance();
                        }
                        Token::DoubleColon => {
                            path.push_str("::");
                            self.advance();
                        }
                        Token::Slash => {
                            path.push('/');
                            self.advance();
                        }
                        Token::Dot => {
                            path.push('.');
                            self.advance();
                        }
                        _ => break,
                    }
                }
                Ok(TopLevel::Import(path))
            }
            Token::Include | Token::IncludeNext => {
                let is_next = matches!(self.peek(), Token::IncludeNext);
                self.advance();
                let rest = self.collect_rest_of_line();
                let directive = if is_next {
                    format!("#include_next {}", rest)
                } else {
                    format!("#include {}", rest)
                };
                Ok(TopLevel::Include(directive))
            }
            Token::Define | Token::Ifdef | Token::Ifndef | Token::Endif | Token::Pragma => {
                let kw = match self.peek().clone() {
                    Token::Define => "#define",
                    Token::Ifdef => "#ifdef",
                    Token::Ifndef => "#ifndef",
                    Token::Endif => "#endif",
                    Token::Pragma => "#pragma",
                    _ => unreachable!(),
                };
                self.advance();
                let rest = self.collect_rest_of_line();
                Ok(TopLevel::RawCpp(format!("{} {}", kw, rest)))
            }
            Token::Public | Token::Private => {
                let access = if matches!(self.peek(), Token::Public) {
                    AccessModifier::Public
                } else {
                    AccessModifier::Private
                };
                self.advance();
                if matches!(self.peek(), Token::Class | Token::Throwable) {
                    self.parse_class_or_throwable(access)
                } else {
                    let line = self.collect_rest_of_line();
                    Ok(TopLevel::RawCpp(line))
                }
            }
            Token::Class | Token::Throwable => {
                self.parse_class_or_throwable(AccessModifier::Private)
            }
            _ => {
                let line = self.collect_rest_of_line();
                Ok(TopLevel::RawCpp(line))
            }
        }
    }

    // Parse a class or throwable declaration
    fn parse_class_or_throwable(&mut self, access: AccessModifier) -> ParseResult<TopLevel> {
        let is_throwable = matches!(self.peek(), Token::Throwable);
        if !is_throwable && !matches!(self.peek(), Token::Class) {
            return Err(ParseError::new(
                "expected 'class' or 'throwable' after access modifier",
                self.current_line(),
                self.current_col(),
            ));
        }
        self.advance(); // consume 'class' or 'throwable'

        let name = self.expect_ident()?;
        let parent = if matches!(self.peek(), Token::Uses) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        self.expect(&Token::LBrace)?;
        self.skip_newlines();

        let mut members = Vec::new();
        let mut current_access = if is_throwable {
            AccessModifier::Public
        } else {
            AccessModifier::Private
        };

        while !matches!(self.peek(), Token::RBrace | Token::Eof) {
            self.skip_newlines();
            if matches!(self.peek(), Token::RBrace | Token::Eof) {
                break;
            }
            match self.parse_member(&mut current_access) {
                Ok(member) => members.push(member),
                Err(e) => return Err(e),
            }
            self.skip_newlines();
        }
        self.expect(&Token::RBrace)?;

        Ok(TopLevel::Class(ClassDecl {
            access,
            is_throwable,
            name,
            parent,
            members,
        }))
    }

    // Parse a class member (field, method, or constructor)
    fn parse_member(&mut self, current_access: &mut AccessModifier) -> ParseResult<ClassMember> {
        self.skip_newlines();

        // Optional explicit access modifier overrides current default
        let member_access = if self.eat_if(&Token::Public) {
            AccessModifier::Public
        } else if self.eat_if(&Token::Private) {
            AccessModifier::Private
        } else {
            current_access.clone()
        };

        let is_static = self.eat_if(&Token::Static);
        self.skip_newlines();

        match self.peek().clone() {
            Token::Constructor => {
                let ctor = self.parse_constructor()?;
                Ok(ClassMember::Constructor(ctor))
            }
            Token::Method => {
                let method = self.parse_method(member_access, is_static)?;
                Ok(ClassMember::Method(method))
            }
            Token::Ident(ref name) if name == "func" => {
                // Accept "func" as alternative to the Method token.
                let method = self.parse_method(member_access, is_static)?;
                Ok(ClassMember::Method(method))
            }
            Token::Mut
            | Token::Int
            | Token::Float
            | Token::Boolean
            | Token::StringType
            | Token::Ident(_) => {
                Ok(ClassMember::Field(self.parse_field(member_access, is_static)?))
            }
            _ => {
                // Unrecognised member: treat as raw C++ line.
                let line = self.collect_rest_of_line();
                Ok(ClassMember::Method(MethodDecl {
                    access: member_access,
                    is_static,
                    name: String::new(),
                    params: Vec::new(),
                    return_type: H20Type::Void,
                    body: vec![Stmt::RawCpp(line)],
                }))
            }
        }
    }

    // Parse a field declaration.
    fn parse_field(&mut self, access: AccessModifier, is_static: bool) -> ParseResult<FieldDecl> {
        let is_mut = self.eat_if(&Token::Mut);
        let typ = self.parse_type()?;
        let name = self.expect_ident()?;

        let value = if self.eat_if(&Token::Equals) {
            Some(self.parse_expr(0)?)
        } else {
            if !is_mut {
                return Err(ParseError::new(
                    &format!("immutable field '{}' must have an initial value", name),
                    self.current_line(),
                    self.current_col(),
                ));
            }
            None
        };

        self.skip_newlines();
        Ok(FieldDecl {
            access,
            is_static,
            is_mut,
            typ,
            name,
            value,
        })
    }

    // Parse a method declaration
    fn parse_method(&mut self, access: AccessModifier, is_static: bool) -> ParseResult<MethodDecl> {
        // Consume the 'func' keyword (either Token::Method or Identifier "func")
        match self.peek() {
            Token::Method => self.advance(),
            Token::Ident(name) if name == "func" => self.advance(),
            _ => {
                return Err(ParseError::new(
                    "expected 'func' keyword",
                    self.current_line(),
                    self.current_col(),
                ));
            }
        };
        let name = self.expect_ident()?;
        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen)?;

        let return_type = if self.eat_if(&Token::Returns) {
            self.parse_type()?
        } else {
            H20Type::Void
        };

        let body = self.parse_block()?;
        Ok(MethodDecl {
            access,
            is_static,
            name,
            params,
            return_type,
            body,
        })
    }

    // Parse a constructor declaration
    fn parse_constructor(&mut self) -> ParseResult<ConstructorDecl> {
        self.expect(&Token::Constructor)?;
        // Optional class name after 'constructor' (legacy syntax)
        if matches!(self.peek(), Token::Ident(_)) {
            self.advance();
        }
        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen)?;

        let super_args = if self.eat_if(&Token::Uses) {
            self.expect_keyword_super()?;
            self.expect(&Token::LParen)?;
            let args = self.parse_arg_list()?;
            self.expect(&Token::RParen)?;
            Some(args)
        } else {
            None
        };

        let body = self.parse_block()?;
        Ok(ConstructorDecl {
            params,
            super_args,
            body,
        })
    }

    // Parse a parameter list (inside parentheses)
    fn parse_params(&mut self) -> ParseResult<Vec<Param>> {
        let mut params = Vec::new();
        while !matches!(self.peek(), Token::RParen | Token::Eof) {
            // c‑style variadic "..."
            if self.eat_if(&Token::Ellipsis) {
                params.push(Param {
                    is_mut: false,
                    typ: H20Type::Custom("...".to_string()),
                    name: String::new(),
                });
                break;
            }
            let is_mut = self.eat_if(&Token::Mut);
            let typ = self.parse_type()?;
            let name = self.expect_ident()?;
            params.push(Param { is_mut, typ, name });
            if !self.eat_if(&Token::Comma) {
                break;
            }
        }
        Ok(params)
    }

    // Parse a RiverLang type (with optional trailing '*')
    fn parse_type(&mut self) -> ParseResult<H20Type> {
        let base = self.parse_type_atom()?;
        while self.eat_if(&Token::Star) {} // ignore pointer markers
        Ok(base)
    }

    // Parse a block of statements enclosed in braces
    fn parse_block(&mut self) -> ParseResult<Vec<Stmt>> {
        self.skip_newlines();
        self.expect(&Token::LBrace)?;
        self.skip_newlines();
        let mut stmts = Vec::new();
        while !matches!(self.peek(), Token::RBrace | Token::Eof) {
            let stmt = self.parse_stmt()?;
            stmts.push(stmt);
            self.skip_newlines();
        }
        self.expect(&Token::RBrace)?;
        Ok(stmts)
    }

    // Parse a single statement
    fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        self.skip_newlines();
        match self.peek().clone() {
            Token::Mut => {
                self.advance();
                let typ = self.parse_type()?;
                let name = self.expect_ident()?;
                let value = if self.eat_if(&Token::Equals) {
                    Some(self.parse_expr(0)?)
                } else {
                    None
                };
                self.skip_newlines();
                Ok(Stmt::VarDecl {
                    is_mut: true,
                    is_static: false,
                    typ,
                    name,
                    value,
                })
            }
            Token::Int | Token::Float | Token::Boolean | Token::StringType => {
                let typ = self.parse_type()?;
                let name = self.expect_ident()?;
                let value = if self.eat_if(&Token::Equals) {
                    Some(self.parse_expr(0)?)
                } else {
                    return Err(ParseError::new(
                        &format!("immutable variable '{}' must be initialized", name),
                        self.current_line(),
                        self.current_col(),
                    ));
                };
                self.skip_newlines();
                Ok(Stmt::VarDecl {
                    is_mut: false,
                    is_static: false,
                    typ,
                    name,
                    value,
                })
            }
            Token::Return => {
                self.advance();
                if matches!(self.peek(), Token::Newline | Token::Semicolon | Token::RBrace) {
                    self.skip_newlines();
                    Ok(Stmt::Return(None))
                } else {
                    let expr = self.parse_expr(0)?;
                    self.skip_newlines();
                    Ok(Stmt::Return(Some(expr)))
                }
            }
            Token::Throw => {
                self.advance();
                let expr = self.parse_expr(0)?;
                self.skip_newlines();
                Ok(Stmt::Throw(expr))
            }
            Token::If => {
                self.advance();
                self.expect(&Token::LParen)?;
                let condition = self.parse_expr(0)?;
                self.expect(&Token::RParen)?;
                let then_block = self.parse_block()?;
                let mut elif_blocks = Vec::new();
                let mut else_block = None;

                loop {
                    self.skip_newlines();
                    if matches!(self.peek(), Token::Else) {
                        self.advance();
                        self.skip_newlines();
                        if matches!(self.peek(), Token::If) {
                            self.advance();
                            self.expect(&Token::LParen)?;
                            let cond = self.parse_expr(0)?;
                            self.expect(&Token::RParen)?;
                            let body = self.parse_block()?;
                            elif_blocks.push((cond, body));
                        } else {
                            else_block = Some(self.parse_block()?);
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Ok(Stmt::If {
                    condition,
                    then_block,
                    elif_blocks,
                    else_block,
                })
            }
            Token::While => {
                self.advance();
                self.expect(&Token::LParen)?;
                let condition = self.parse_expr(0)?;
                self.expect(&Token::RParen)?;
                let body = self.parse_block()?;
                Ok(Stmt::While { condition, body })
            }
            Token::For => {
                self.advance();
                self.expect(&Token::LParen)?;
                let init = if !matches!(self.peek(), Token::Semicolon) {
                    let s = self.parse_stmt()?;
                    Some(Box::new(s))
                } else {
                    None
                };
                self.eat_if(&Token::Semicolon);
                let condition = if !matches!(self.peek(), Token::Semicolon) {
                    Some(self.parse_expr(0)?)
                } else {
                    None
                };
                self.eat_if(&Token::Semicolon);
                let update = if !matches!(self.peek(), Token::RParen) {
                    Some(self.parse_expr(0)?)
                } else {
                    None
                };
                self.expect(&Token::RParen)?;
                let body = self.parse_block()?;
                Ok(Stmt::For {
                    init,
                    condition,
                    update,
                    body,
                })
            }
            Token::Break => {
                self.advance();
                self.skip_newlines();
                Ok(Stmt::Break)
            }
            Token::Continue => {
                self.advance();
                self.skip_newlines();
                Ok(Stmt::Continue)
            }
            Token::Release => {
                self.advance();
                let name = self.expect_ident()?;
                self.skip_newlines();
                Ok(Stmt::Release(name))
            }
            Token::Zombify => {
                self.advance();
                let name = self.expect_ident()?;
                self.skip_newlines();
                Ok(Stmt::Zombify(name))
            }
            Token::Unsafe => {
                self.advance();
                let body = self.parse_block()?;
                Ok(Stmt::Unsafe(body))
            }
            _ => {
                let line = self.collect_rest_of_line();
                Ok(Stmt::RawCpp(line))
            }
        }
    }

    // Parse an expression with operator precedence (min_prec is the minimal precedence to allow)
    fn parse_expr(&mut self, min_prec: u8) -> ParseResult<Expr> {
        let mut left = self.parse_unary()?;

        loop {
            let prec = self.infix_precedence();
            if prec < min_prec {
                break;
            }
            match self.peek().clone() {
                Token::Equals => {
                    self.advance();
                    let right = self.parse_expr(prec)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::Assign, Box::new(right));
                }
                Token::Plus => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::Add, Box::new(right));
                }
                Token::Minus => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::Sub, Box::new(right));
                }
                Token::Star => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::Mul, Box::new(right));
                }
                Token::Slash => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::Div, Box::new(right));
                }
                Token::Percent => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::Mod, Box::new(right));
                }
                Token::EqEq => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::Eq, Box::new(right));
                }
                Token::NotEq => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::NotEq, Box::new(right));
                }
                Token::LAngle => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::Lt, Box::new(right));
                }
                Token::RAngle => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::Gt, Box::new(right));
                }
                Token::LtEq => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::LtEq, Box::new(right));
                }
                Token::GtEq => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::GtEq, Box::new(right));
                }
                Token::AmpAmp => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::And, Box::new(right));
                }
                Token::PipePipe => {
                    self.advance();
                    let right = self.parse_expr(prec + 1)?;
                    left = Expr::BinaryOp(Box::new(left), BinOp::Or, Box::new(right));
                }
                Token::Dot => {
                    self.advance();
                    let name = self.expect_ident()?;
                    if self.eat_if(&Token::LParen) {
                        let args = self.parse_arg_list()?;
                        self.expect(&Token::RParen)?;
                        left = Expr::MethodCall(Box::new(left), name, args);
                    } else {
                        left = Expr::FieldAccess(Box::new(left), name);
                    }
                }
                _ => break,
            }
        }
        Ok(left)
    }

    // Precedence of infix operators (higher number = binds tighter)
    fn infix_precedence(&self) -> u8 {
        match self.peek() {
            Token::Equals => 1,
            Token::AmpAmp | Token::PipePipe => 2,
            Token::EqEq | Token::NotEq => 3,
            Token::LAngle | Token::RAngle | Token::LtEq | Token::GtEq => 4,
            Token::Plus | Token::Minus => 5,
            Token::Star | Token::Slash | Token::Percent => 6,
            Token::Dot => 8,
            _ => 0,
        }
    }

    // Parse a unary expression
    fn parse_unary(&mut self) -> ParseResult<Expr> {
        match self.peek().clone() {
            Token::Bang => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(expr)))
            }
            Token::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(expr)))
            }
            Token::Star => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::UnaryOp(UnaryOp::Deref, Box::new(expr)))
            }
            _ => self.parse_primary(),
        }
    }

    // Parse a type atom (without trailing '*')
    fn parse_type_atom(&mut self) -> ParseResult<H20Type> {
        match self.peek().clone() {
            Token::Int => {
                self.advance();
                Ok(H20Type::Int)
            }
            Token::Float => {
                self.advance();
                Ok(H20Type::Float)
            }
            Token::Boolean => {
                self.advance();
                Ok(H20Type::Boolean)
            }
            Token::StringType => {
                self.advance();
                Ok(H20Type::StringType)
            }
            Token::Void => {
                self.advance();
                Ok(H20Type::Void)
            }
            Token::Ident(name) if name == "List" => {
                self.advance();
                self.expect(&Token::LAngle)?;
                let inner = self.parse_type()?;
                self.expect(&Token::RAngle)?;
                Ok(H20Type::List(Box::new(inner)))
            }
            Token::Ident(name) => {
                self.advance();
                Ok(H20Type::Custom(name))
            }
            _ => Err(ParseError::new(
                &format!("expected type but got {:?}", self.peek()),
                self.current_line(),
                self.current_col(),
            )),
        }
    }

    // Parse a primary expression: literal, identifier, parenthesised, etc
    fn parse_primary(&mut self) -> ParseResult<Expr> {
        match self.peek().clone() {
            Token::IntLit(n) => {
                self.advance();
                Ok(Expr::IntLiteral(n))
            }
            Token::FloatLit(f) => {
                self.advance();
                Ok(Expr::FloatLiteral(f))
            }
            Token::StringLit(s) => {
                self.advance();
                Ok(Expr::StringLiteral(s))
            }
            Token::True => {
                self.advance();
                Ok(Expr::BoolLiteral(true))
            }
            Token::False => {
                self.advance();
                Ok(Expr::BoolLiteral(false))
            }
            Token::Null => {
                self.advance();
                Ok(Expr::Null)
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr(0)?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::Ident(name) if name == "List" && matches!(self.peek_at(1), Token::Dot) => {
                self.advance();
                self.advance(); // consume dot
                let method = self.expect_ident()?;
                if method == "of" {
                    self.expect(&Token::LAngle)?;
                    let inner = self.parse_type()?;
                    self.expect(&Token::RAngle)?;
                    self.expect(&Token::LParen)?;
                    let args = self.parse_arg_list()?;
                    self.expect(&Token::RParen)?;
                    Ok(Expr::ListOf(inner, args))
                } else {
                    Err(ParseError::new(
                        "expected 'of' after List.",
                        self.current_line(),
                        self.current_col(),
                    ))
                }
            }
            Token::New => {
                self.advance();
                match self.peek().clone() {
                    Token::Ident(name) if name == "List" => {
                        self.advance();
                        self.expect(&Token::LAngle)?;
                        let inner = self.parse_type()?;
                        self.expect(&Token::RAngle)?;
                        self.expect(&Token::LParen)?;
                        self.expect(&Token::RParen)?;
                        Ok(Expr::NewList(inner))
                    }
                    Token::Ident(class_name) => {
                        self.advance();
                        self.expect(&Token::LParen)?;
                        let args = self.parse_arg_list()?;
                        self.expect(&Token::RParen)?;
                        Ok(Expr::NewObject(class_name, args))
                    }
                    _ => Err(ParseError::new(
                        "expected class name after 'new'",
                        self.current_line(),
                        self.current_col(),
                    )),
                }
            }
            Token::Ident(name) if matches!(self.peek_at(1), Token::DoubleColon) => {
                self.advance();
                self.advance(); // consume DoubleColon
                let member = self.expect_ident()?;
                if self.eat_if(&Token::LParen) {
                    let args = self.parse_arg_list()?;
                    self.expect(&Token::RParen)?;
                    Ok(Expr::StaticCall(name, member, args, true))
                } else {
                    Ok(Expr::StaticCall(name, member, vec![], false))
                }
            }
            Token::Ident(name) => {
                self.advance();
                if self.eat_if(&Token::LParen) {
                    let args = self.parse_arg_list()?;
                    self.expect(&Token::RParen)?;
                    Ok(Expr::MethodCall(
                        Box::new(Expr::Ident("this".to_string())),
                        name,
                        args,
                    ))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            _ => Err(ParseError::new(
                &format!("unexpected token in expression: {:?}", self.peek()),
                self.current_line(),
                self.current_col(),
            )),
        }
    }

    // Parse a comma‑separated list of arguments (for function calls)
    fn parse_arg_list(&mut self) -> ParseResult<Vec<Expr>> {
        let mut args = Vec::new();
        while !matches!(self.peek(), Token::RParen | Token::Eof) {
            args.push(self.parse_expr(0)?);
            if !self.eat_if(&Token::Comma) {
                break;
            }
        }
        Ok(args)
    }

    // Expect an identifier token and return its string
    fn expect_ident(&mut self) -> ParseResult<String> {
        match self.peek().clone() {
            Token::Ident(s) => {
                self.advance();
                Ok(s)
            }
            other => Err(ParseError::new(
                &format!("expected identifier but got {:?}", other),
                self.current_line(),
                self.current_col(),
            )),
        }
    }

    // Expect the keyword 'super' (either as Token::Super or as Ident("super"))
    fn expect_keyword_super(&mut self) -> ParseResult<()> {
        match self.peek().clone() {
            Token::Super => {
                self.advance();
                Ok(())
            }
            Token::Ident(s) if s == "super" => {
                self.advance();
                Ok(())
            }
            other => Err(ParseError::new(
                &format!("expected 'super' but got {:?}", other),
                self.current_line(),
                self.current_col(),
            )),
        }
    }

    // Collect all tokens from the current position until newline or semicolon
    // and reconstruct a string representation (used for raw C++ fallback)
    fn collect_rest_of_line(&mut self) -> String {
        let mut parts = Vec::new();
        while !matches!(self.peek(), Token::Newline | Token::Semicolon | Token::Eof) {
            let tok_str = match self.peek().clone() {
                Token::Ident(s) => s,
                Token::StringLit(s) => format!("\"{}\"", s),
                Token::IntLit(n) => n.to_string(),
                Token::FloatLit(f) => f.to_string(),
                Token::LParen => "(".to_string(),
                Token::RParen => ")".to_string(),
                Token::LBrace => "{".to_string(),
                Token::RBrace => "}".to_string(),
                Token::LAngle => "<".to_string(),
                Token::RAngle => ">".to_string(),
                Token::LBracket => "[".to_string(),
                Token::RBracket => "]".to_string(),
                Token::Comma => ",".to_string(),
                Token::Dot => ".".to_string(),
                Token::Colon => ":".to_string(),
                Token::DoubleColon => "::".to_string(),
                Token::Equals => "=".to_string(),
                Token::EqEq => "==".to_string(),
                Token::NotEq => "!=".to_string(),
                Token::Bang => "!".to_string(),
                Token::Plus => "+".to_string(),
                Token::Minus => "-".to_string(),
                Token::Star => "*".to_string(),
                Token::Slash => "/".to_string(),
                Token::Percent => "%".to_string(),
                Token::Amp => "&".to_string(),
                Token::AmpAmp => "&&".to_string(),
                Token::Pipe => "|".to_string(),
                Token::PipePipe => "||".to_string(),
                Token::Arrow => "->".to_string(),
                Token::LtEq => "<=".to_string(),
                Token::GtEq => ">=".to_string(),
                Token::Semicolon => ";".to_string(),
                Token::True => "true".to_string(),
                Token::False => "false".to_string(),
                Token::Null => "nullptr".to_string(),
                _ => String::new(),
            };
            parts.push(tok_str);
            self.advance();
        }
        let mut result = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 && !part.is_empty() && !result.ends_with("::") && !part.starts_with("::") {
                result.push(' ');
            }
            result.push_str(part);
        }
        result
    }
}