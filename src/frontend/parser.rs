use std::collections::HashMap;
use std::io::Write;
use std::iter::Peekable;
use std::str::SplitWhitespace;
use std::any::Any;

use thiserror::Error;

use crate::frontend::{
    ast::*,
    lexer::{Lex, Ops, Token, Tokens},
};

lazy_static! {
    static ref OP_PRECEDENCE: HashMap<Ops, i32> = {
        let mut map = HashMap::new();
        map.insert(Ops::Plus, 20);
        map.insert(Ops::Minus, 20);
        map.insert(Ops::Mult, 40);
        map.insert(Ops::Div, 40);
        map.insert(Ops::Modulo, 40);
        map
    };
}

#[derive(Error, PartialEq, Debug)]
pub enum ParserError<'src> {
    #[error("Unexpected token: {0:?}")]
    UnexpectedToken(Token<'src>),

    #[error("Reached end of input expecting more")]
    UnexpectedEOI,

    #[error("Expected token: {0:?}")]
    ExpectedToken(Token<'src>),
}

type ParseResult<'src> = Result<Box<dyn AST>, ParserError<'src>>;

pub fn parse_extern<'src>(
    tokens: &mut Peekable<impl Iterator<Item = Token<'src>>>,
) -> Result<Box<Prototype>, ParserError<'src>> {
    let _keyword = tokens.next();
    parse_prototype(tokens)
}

pub fn parse_prototype<'src>(
    tokens: &mut Peekable<impl Iterator<Item = Token<'src>>>,
) -> Result<Box<Prototype>, ParserError<'src>> {
    let Some(Token::Identifier(name)) = tokens.next() else {
        return Err(ParserError::ExpectedToken(Token::Identifier(&"")));
    };

    tokens
        .next()
        .filter(|t| matches!(t, Token::OpenParen))
        .ok_or(ParserError::ExpectedToken(Token::OpenParen))?;

    let mut args = vec![];

    while let Some(Token::Identifier(s)) = tokens.peek() {
        args.push(s.to_string());
        let _ = tokens.next();
    }

    let _closed_paren = tokens
        .next()
        .filter(|t| matches!(t, Token::ClosedParen))
        .ok_or(ParserError::ExpectedToken(Token::ClosedParen))?;

    Ok(Box::new(Prototype {
        name: name.to_string(),
        args,
    }))
}

pub fn parse_definition<'src>(
    tokens: &mut Peekable<impl Iterator<Item = Token<'src>>>,
) -> ParseResult<'src> {
    // swallow the def keyword
    let _def = tokens.next();

    // try to parse prototype and body
    let proto = parse_prototype(tokens)?;
    let body = parse_expression(tokens)?;

    Ok(Box::new(Function { proto, body }))
}

pub fn parse_top_level_expr<'src>(
    tokens: &mut Peekable<impl Iterator<Item = Token<'src>>>,
) -> ParseResult<'src> {
    let expr = parse_expression(tokens)?;

    let proto = Box::new(Prototype {
        name: "<anonymous>".to_string(),
        args: vec![],
    });

    Ok(Box::new(Function { proto, body: expr }))
}

fn parse_primary<'src>(
    tokens: &mut Peekable<impl Iterator<Item = Token<'src>>>,
) -> ParseResult<'src> {
    match tokens.peek() {
        Some(Token::Identifier(_)) => parse_identifier_expr(tokens),

        Some(Token::Number(_)) => parse_number_expr(tokens),

        Some(Token::OpenParen) => parse_paren_expr(tokens),

        Some(unexpected) => Err(ParserError::UnexpectedToken(*unexpected)),

        None => Err(ParserError::UnexpectedEOI),
    }
}

fn parse_number_expr<'src>(tokens: &mut impl Iterator<Item = Token<'src>>) -> ParseResult<'src> {
    if let Some(Token::Number(num)) = tokens.next() {
        Ok(Box::new(NumberExpr(num)))
    } else {
        panic!("Expected next token to be number for parse_number_expr!")
    }
}

fn parse_identifier_expr<'src>(
    tokens: &mut Peekable<impl Iterator<Item = Token<'src>>>,
) -> ParseResult<'src> {
    let name = match tokens.next() {
        Some(Token::Identifier(name)) => name,
        _unexpected => panic!("Expected"),
    };

    // Call Expression
    if let Some(Token::OpenParen) = tokens.peek() {
        let _open_paren = tokens.next();

        let mut arglist = vec![];

        loop {
            if let Some(Token::ClosedParen) = tokens.peek() {
                break;
            }

            parse_expression(tokens).map(|arg_expr| arglist.push(arg_expr))?;

            if let Some(Token::Comma) = tokens.peek() {
                tokens.next();
                continue;
            }
        }

        let _closed_paren = tokens.next();

        Ok(Box::new(CallExpr {
            name: name.to_string(),
            args: arglist,
        }))
    } else {
        // Variable Expression
        Ok(Box::new(VariableExpr {
            name: name.to_string(),
        }))
    }
}

fn parse_paren_expr<'src>(
    tokens: &mut Peekable<impl Iterator<Item = Token<'src>>>,
) -> ParseResult<'src> {
    let _paren = tokens.next();

    let expr = parse_expression(tokens);

    match tokens.next() {
        Some(Token::ClosedParen) => expr,
        Some(unexpected) => Err(ParserError::UnexpectedToken(unexpected)),
        None => Err(ParserError::UnexpectedEOI),
    }
}

fn parse_expression<'src>(
    tokens: &mut Peekable<impl Iterator<Item = Token<'src>>>,
) -> ParseResult<'src> {
    let lhs = parse_primary(tokens)?;

    parse_binop_rhs(tokens, lhs, 0)
}

fn get_operator_precedence(token: Token) -> i32 {
    if let Token::Operator(operator) = token {
        OP_PRECEDENCE[&operator]
    } else {
        -1
    }
}

fn parse_binop_rhs<'src>(
    tokens: &mut Peekable<impl Iterator<Item = Token<'src>>>,
    mut lhs: Box<dyn AST>,
    expr_prec: i32,
) -> ParseResult<'src> {
    loop {
        let tok_prec = match tokens.peek().copied() {
            Some(token) => get_operator_precedence(token),
            None => return Err(ParserError::UnexpectedEOI),
        };

        if tok_prec < expr_prec {
            return Ok(lhs);
        }

        let Some(next_tok @ Token::Operator(op)) = tokens.next() else {
            panic!("Should be operator here!")
        };

        let mut rhs = parse_primary(tokens)?;

        let next_prec = get_operator_precedence(next_tok);

        if tok_prec < next_prec {
            rhs = parse_binop_rhs(tokens, rhs, tok_prec + 1)?;
        }

        lhs = Box::new(BinaryExpr {
            op,
            left: lhs,
            right: rhs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ops::*;
    use Token::*;

    macro_rules! ast_node {
        ( $node:expr ) => {
            Box::new($node) as Box<dyn AST>
        };
    }

    #[test]
    fn parsing_primary_expressions() {
        let mut input = " 3.14; ";
        let mut ast = input.parse_into_ast(parse_primary);

        assert_eq!(ast, Ok(ast_node!(NumberExpr::new(3.14))));

        input = " 2 + 3; ";
        ast = input.parse_into_ast(parse_expression);

        assert_eq!(
            ast,
            Ok(ast_node!(BinaryExpr::new(
                Ops::Plus,
                ast_node!(NumberExpr::new(2.0)),
                ast_node!(NumberExpr::new(3.0)),
            )))
        );

        input = " var1 * var2; ";
        ast = input.parse_into_ast(parse_expression);

        assert_eq!(
            ast,
            Ok(ast_node!(BinaryExpr::new(
                Ops::Mult,
                ast_node!(VariableExpr::new("var1".to_string())),
                ast_node!(VariableExpr::new("var2".to_string())),
            )))
        );
    }

    #[test]
    fn parsing_binorphs() {}

    fn parsing_functions() {}
}