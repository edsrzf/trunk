use std::{vec::IntoIter};
use std::iter::Peekable;
use trunk_lexer::{Token, TokenKind};
use crate::{Program, Statement, Block, Expression, ast::MethodFlag};

macro_rules! expect {
    ($actual:expr, $expected:pat, $out:expr, $message:literal) => {
        match $actual {
            Some(token) => match token.kind {
                $expected => $out,
                _ => return Err(ParseError::ExpectedToken($message.into()))
            },
            None => return Err(ParseError::ExpectedToken($message.into()))
        }
    };
    ($actual:expr, $expected:pat, $message:literal) => {
        match $actual {
            Some(token) => match token.kind {
                $expected => (),
                _ => return Err(ParseError::ExpectedToken($message.into()))
            },
            None => return Err(ParseError::ExpectedToken($message.into()))
        }
    };
}

pub struct Parser;

#[allow(dead_code)]
impl Parser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, tokens: Vec<Token>) -> Result<Program, ParseError> {
        let mut program = Program::new();
        let mut iter = tokens.into_iter().peekable();

        while let Some(t) = iter.next() {
            if let TokenKind::OpenTag(_) = t.kind {
                continue;
            }

            program.push(self.statement(t, &mut iter)?);
        }

        Ok(program)
    }

    #[allow(dead_code)]
    fn statement(&self, t: Token, tokens: &mut Peekable<IntoIter<Token>>) -> Result<Statement, ParseError> {
        Ok(match t.kind {
            TokenKind::InlineHtml(html) => Statement::InlineHtml(html),
            TokenKind::If => {
                expect!(tokens.next(), TokenKind::LeftParen, "expected (");

                let condition = self.expression(tokens, 0)?;

                expect!(tokens.next(), TokenKind::RightParen, "expected )");

                // TODO: Support one-liner if statements.
                expect!(tokens.next(), TokenKind::LeftBrace, "expected {");

                let mut then = Block::new();
                while let Some(t) = tokens.peek() && t.kind != TokenKind::RightBrace {
                    then.push(self.statement(tokens.next().unwrap(), tokens)?);
                }

                // TODO: Support one-liner if statements.
                expect!(tokens.next(), TokenKind::RightBrace, "expected }");

                Statement::If { condition, then }
            },
            TokenKind::Class => {
                let name = expect!(tokens.next(), TokenKind::Identifier(i), i, "expected class name");
                expect!(tokens.next(), TokenKind::LeftBrace, "expected left-brace");

                let mut body = Vec::new();
                while let Some(t) = tokens.peek() && t.kind != TokenKind::RightBrace {
                    let statement = match self.statement(tokens.next().unwrap(), tokens)? {
                        Statement::Function { name, params, body } => {
                            Statement::Method { name, params, body, flags: vec![] }
                        },
                        s @ Statement::Method { .. } => s,
                        _ => return Err(ParseError::InvalidClassStatement(format!("Classes can only contain properties, constants and methods.")))
                    };

                    body.push(statement);
                }

                expect!(tokens.next(), TokenKind::RightBrace, "expected right-brace");

                Statement::Class { name: name.into(), body }
            },
            TokenKind::Echo => {
                let mut values = Vec::new();
                while let Some(t) = tokens.peek() && t.kind != TokenKind::SemiColon {
                    values.push(self.expression(tokens, 0)?);

                    // `echo` supports multiple expressions separated by a comma.
                    // TODO: Disallow trailing commas when the next token is a semi-colon.
                    if let Some(t) = tokens.peek() && t.kind == TokenKind::Comma {
                        tokens.next();
                    }
                }
                expect!(tokens.next(), TokenKind::SemiColon, "expected semi-colon at the end of an echo statement");
                Statement::Echo { values }
            },
            TokenKind::Return => {
                if let Some(Token { kind: TokenKind::SemiColon, .. }) = tokens.peek() {
                    let ret = Statement::Return { value: None };
                    expect!(tokens.next(), TokenKind::SemiColon, "expected semi-colon at the end of return statement.");
                    ret
                } else {
                    let ret = Statement::Return { value: self.expression(tokens, 0).ok() };
                    expect!(tokens.next(), TokenKind::SemiColon, "expected semi-colon at the end of return statement.");
                    ret
                }
            },
            TokenKind::Function => {
                let name = expect!(tokens.next(), TokenKind::Identifier(i), i, "expected identifier");

                expect!(tokens.next(), TokenKind::LeftParen, "expected (");

                let mut params = Vec::new();

                while let Some(n) = tokens.peek() && n.kind != TokenKind::RightParen {
                    // TODO: Support variable types and default values.
                    params.push(expect!(tokens.next(), TokenKind::Variable(v), v, "expected variable").into());
                    
                    if let Some(Token { kind: TokenKind::Comma, .. }) = tokens.peek() {
                        tokens.next();
                    }
                }

                expect!(tokens.next(), TokenKind::RightParen, "expected )");

                // TODO: Support return types here.

                expect!(tokens.next(), TokenKind::LeftBrace, "expected {");

                let mut body = Block::new();

                while let Some(n) = tokens.peek() && n.kind != TokenKind::RightBrace {
                    body.push(self.statement(tokens.next().unwrap(), tokens)?);
                }

                expect!(tokens.next(), TokenKind::RightBrace, "expected }");

                Statement::Function { name: name.into(), params, body }
            },
            _ if is_method_visibility_modifier(&t.kind) => {
                let mut flags = vec![visibility_token_to_flag(&t.kind)];

                while let Some(t) = tokens.peek() && is_method_visibility_modifier(&t.kind) {
                    let next = tokens.next().unwrap();

                    flags.push(visibility_token_to_flag(&next.kind));
                }

                match self.statement(tokens.next().unwrap(), tokens)? {
                    Statement::Function { name, params, body } => {
                        Statement::Method { name, params, body, flags }
                    },
                    _ => return Err(ParseError::InvalidClassStatement("Classes can only contain properties, constants and methods.".into()))
                }
            },
            _ => todo!("unhandled token: {:?}", t)
        })
    }

    fn expression(&self, tokens: &mut Peekable<IntoIter<Token>>, bp: u8) -> Result<Expression, ParseError> {
        if tokens.peek().is_none() {
            return Err(ParseError::UnexpectedEndOfFile);
        }

        let t = tokens.next().unwrap();

        let mut lhs = match t.kind {
            TokenKind::Variable(v) => Expression::Variable(v),
            TokenKind::Int(i) => Expression::Int(i),
            TokenKind::Identifier(i) => Expression::Identifier(i),
            _ => todo!("lhs: {:?}", t.kind),
        };

        loop {
            let kind = match tokens.peek() {
                Some(Token { kind: TokenKind::SemiColon, .. }) | None => break,
                Some(Token { kind, .. }) => kind.clone(),
            };

            if let Some(lbp) = postfix_binding_power(&kind) {
                if lbp < bp {
                    break;
                }

                tokens.next();

                let op = kind.clone();
                lhs = self.postfix(tokens, lhs, &op)?;

                continue;
            }

            if let Some((lbp, rbp)) = infix_binding_power(&kind) {
                if lbp < bp {
                    break;
                }

                tokens.next();

                let op = kind.clone();
                let rhs = self.expression(tokens, rbp)?;

                lhs = infix(lhs, op, rhs);
                continue;
            }

            break;
        }

        Ok(lhs)
    }

    fn postfix(&self, tokens: &mut Peekable<IntoIter<Token>>, lhs: Expression, op: &TokenKind) -> Result<Expression, ParseError> {
        Ok(match op {
            TokenKind::LeftParen => {
                let mut args = Vec::new();
                while let Some(t) = tokens.peek() && t.kind != TokenKind::RightParen {
                    args.push(self.expression(tokens, 0)?);

                    if let Some(Token { kind: TokenKind::Comma, .. }) = tokens.peek() {
                        tokens.next();
                    }
                }

                expect!(tokens.next(), TokenKind::RightParen, "expected )");
    
                Expression::Call(Box::new(lhs), args)
            },
            _ => todo!("postfix: {:?}", op),
        })
    }
}

fn is_method_visibility_modifier(kind: &TokenKind) -> bool {
    [TokenKind::Public, TokenKind::Protected, TokenKind::Private, TokenKind::Static].contains(kind)
}

fn visibility_token_to_flag(kind: &TokenKind) -> MethodFlag {
    match kind {
        TokenKind::Public => MethodFlag::Public,
        TokenKind::Protected => MethodFlag::Protected,
        TokenKind::Private => MethodFlag::Private,
        TokenKind::Static => MethodFlag::Static,
        _ => unreachable!()
    }
}

fn infix(lhs: Expression, op: TokenKind, rhs: Expression) -> Expression {
    Expression::Infix(Box::new(lhs), op.into(), Box::new(rhs))
}

fn infix_binding_power(t: &TokenKind) -> Option<(u8, u8)> {
    Some(match t {
        TokenKind::Plus | TokenKind::Minus => (11, 12),
        TokenKind::LessThan => (9, 10),
        _ => return None,
    })
}

fn postfix_binding_power(t: &TokenKind) -> Option<u8> {
    Some(match t {
        TokenKind::LeftParen => 19,
        _ => return None
    })
}

#[derive(Debug)]
pub enum ParseError {
    ExpectedToken(String),
    UnexpectedEndOfFile,
    InvalidClassStatement(String),
}

#[cfg(test)]
mod tests {
    use trunk_lexer::Lexer;
    use crate::{Statement, Block, Param, Expression, ast::{InfixOp, MethodFlag}};
    use super::Parser;

    macro_rules! function {
        ($name:literal, $params:expr, $body:expr) => {
            Statement::Function {
                name: $name.to_string().into(),
                params: $params.to_vec().into_iter().map(|p: &str| Param::from(p)).collect::<Vec<Param>>(),
                body: $body.to_vec(),
            }
        };
    }

    macro_rules! class {
        ($name:literal) => {
            Statement::Class {
                name: $name.to_string().into(),
                body: vec![],
            }
        };
        ($name:literal, $body:expr) => {
            Statement::Class {
                name: $name.to_string().into(),
                body: $body.to_vec(),
            }
        };
    }

    macro_rules! method {
        ($name:literal, $params:expr, $flags:expr, $body:expr) => {
            Statement::Method {
                name: $name.to_string().into(),
                params: $params.to_vec().into_iter().map(|p: &str| Param::from(p)).collect::<Vec<Param>>(),
                flags: $flags.to_vec(),
                body: $body.to_vec(),
            }
        };
    }

    #[test]
    fn empty_fn() {
        assert_ast("<?php function foo() {}", &[
            function!("foo", &[], &[]),
        ]);
    }

    #[test]
    fn empty_fn_with_params() {
        assert_ast("<?php function foo($n) {}", &[
            function!("foo", &["n"], &[]),
        ]);

        assert_ast("<?php function foo($n, $m) {}", &[
            function!("foo", &["n", "m"], &[]),
        ]);
    }

    #[test]
    fn fib() {
        assert_ast("\
        <?php

        function fib($n) {
            if ($n < 2) {
                return $n;
            }

            return fib($n - 1) + fib($n - 2);
        }", &[
            function!("fib", &["n"], &[
                Statement::If {
                    condition: Expression::Infix(
                        Box::new(Expression::Variable("n".into())),
                        InfixOp::LessThan,
                        Box::new(Expression::Int(2)),
                    ),
                    then: vec![
                        Statement::Return { value: Some(Expression::Variable("n".into())) }
                    ],
                },
                Statement::Return {
                    value: Some(Expression::Infix(
                        Box::new(Expression::Call(
                            Box::new(Expression::Identifier("fib".into())),
                            vec![
                                Expression::Infix(
                                    Box::new(Expression::Variable("n".into())),
                                    InfixOp::Sub,
                                    Box::new(Expression::Int(1)),
                                )
                            ]
                        )),
                        InfixOp::Add,
                        Box::new(Expression::Call(
                            Box::new(Expression::Identifier("fib".into())),
                            vec![
                                Expression::Infix(
                                    Box::new(Expression::Variable("n".into())),
                                    InfixOp::Sub,
                                    Box::new(Expression::Int(2)),
                                )
                            ]
                        )),
                    ))
                }
            ])
        ]);
    }

    #[test]
    fn echo() {
        assert_ast("<?php echo 1;", &[
            Statement::Echo {
                values: vec![
                    Expression::Int(1),
                ]
            }
        ]);
    }

    #[test]
    fn empty_class() {
        assert_ast("<?php class Foo {}", &[
            class!("Foo")
        ]);
    }

    #[test]
    fn class_with_basic_method() {
        assert_ast("\
        <?php
        
        class Foo {
            function bar() {
                echo 1;
            }
        }
        ", &[
            class!("Foo", &[
                method!("bar", &[], &[], &[
                    Statement::Echo { values: vec![
                        Expression::Int(1),
                    ] }
                ])
            ])
        ]);
    }

    #[test]
    fn class_with_method_visibility() {
        assert_ast("\
        <?php
        
        class Foo {
            public function bar() {
                echo 1;
            }

            private static function baz() {}
        }
        ", &[
            class!("Foo", &[
                method!("bar", &[], &[
                    MethodFlag::Public,
                ], &[
                    Statement::Echo { values: vec![
                        Expression::Int(1),
                    ] }
                ]),
                method!("baz", &[], &[
                    MethodFlag::Private,
                    MethodFlag::Static,
                ], &[])
            ])
        ]);
    }

    fn assert_ast(source: &str, expected: &[Statement]) {
        let mut lexer = Lexer::new(None);
        let tokens = lexer.tokenize(source).unwrap();

        let parser = Parser::new();
        let ast = parser.parse(tokens).unwrap();

        assert_eq!(ast, expected);
    }
}