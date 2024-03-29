// Copyright 2024 LangVM Project
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0
// that can be found in the LICENSE file and https://mozilla.org/MPL/2.0/.

use std::collections::HashMap;

use crate::parser::AST::{Expr, Field, FieldList, FuncDecl, FuncType, Ident, ImportDecl, Node, Stmt, StmtBlock, StructType, TraitType, Type};
use crate::parser::Diagnosis::{SyntaxError, UnexpectedNodeError};
use crate::parser::Token::{KeywordLookup, Token, TokenKind};
use crate::scanner::BasicScanner::{BasicScanner, BasicScannerError, NewBufferScanner};
use crate::scanner::BasicToken::BasicTokenKind;
use crate::scanner::Position::Position;
use crate::scanner::PosRange::PosRange;
use crate::tag_matches;

pub enum ParserError {
    ScannerError(BasicScannerError),
}

pub struct Parser {
    pub Scanner: BasicScanner,

    pub KeywordLookup: HashMap<String, TokenKind>,

    pub ReachedEOF: bool,
    pub Token: Token,

    pub CompleteSemicolon: bool,

    pub QuoteStack: Vec<TokenKind>,

    pub SyntaxErrors: Vec<SyntaxError>,
}

pub fn NewParser(buffer: Vec<char>) -> Parser {
    Parser {
        Scanner: BasicScanner {
            BufferScanner: NewBufferScanner(buffer),
            Delimiters: vec!['(', ')', '[', ']', '{', '}', ',', ';', '/'],
            Whitespaces: vec![' ', '\t', '\r'],
        },
        KeywordLookup: KeywordLookup(),
        ReachedEOF: false,
        Token: Token::default(),

        CompleteSemicolon: false,

        QuoteStack: vec![],

        SyntaxErrors: vec![],
    }
}

macro_rules! begin_end {
    ($self: expr, $begin: expr) => {
        PosRange{ Begin: $begin, End: $self.GetPos() }
    };
}

macro_rules! parse_list {
    ($self: expr, $unit: ty, $parser: ident, $delimiter: expr, $term: expr) => { if true {
        let mut list: Vec<$unit> = vec![];
        loop {
            list.push($self.$parser()?);
            match &$self.Token.Kind {
                it if tag_matches!(it, &$delimiter) => {
                    $self.Scan()?;
                    match &$self.Token.Kind {
                        it if tag_matches!(it, &$term) => { break; }
                        _ => {}
                    }
                }
                it if tag_matches!(it, &$term) => { break; }
                _ => {}
            }
        }
        list
    } else { panic!() }};
}

impl Parser {
    pub fn GetPos(&self) -> Position { self.Scanner.GetPos() }

    pub fn Scan(&mut self) -> Result<(), ParserError> {
        let bt = match self.Scanner.Scan() {
            Ok(token) => { token }
            Err(err) => { return Err(ParserError::ScannerError(err)); }
        };

        match self.Token.Kind {
            TokenKind::NEWLINE => {
                if self.CompleteSemicolon {
                    self.CompleteSemicolon = false;
                    self.Token = Token {
                        Pos: bt.Pos,
                        Kind: TokenKind::SEMICOLON,
                        Literal: vec![';'],
                    };
                    return Ok(());
                }
            }
            _ => {}
        }

        self.CompleteSemicolon = false;

        macro_rules! lookup {
            ($e: ident) => {{
                let s = bt.Literal.iter().collect::<String>();
                if self.KeywordLookup.contains_key(&s) {
                    self.KeywordLookup.get(&s).unwrap().clone()
                } else {
                    TokenKind::$e
                }
            }};
        }

        let kind = match bt.Kind {
            BasicTokenKind::Ident | BasicTokenKind::Operator | BasicTokenKind::Delimiter => {
                let k = lookup!(Operator);
                match k {
                    TokenKind::Ident
                    | TokenKind::RPAREN
                    | TokenKind::RBRACE
                    | TokenKind::RETURN => { self.CompleteSemicolon = true }
                    _ => {}
                }
                k
            }
            BasicTokenKind::Int(format) => { TokenKind::Int(format) }
            BasicTokenKind::Float => { TokenKind::Float } // TODO
            BasicTokenKind::String => { TokenKind::String }
            BasicTokenKind::Char => { TokenKind::Char }
            BasicTokenKind::Comment => { return self.Scan(); }
        };

        match kind {
            TokenKind::LPAREN => { self.QuoteStack.push(TokenKind::RPAREN) }
            TokenKind::LBRACE => { self.QuoteStack.push(TokenKind::RBRACE) }
            TokenKind::LBRACK => { self.QuoteStack.push(TokenKind::RBRACK) }
            _ => {}
        }

        self.Token = Token {
            Pos: bt.Pos,
            Kind: kind,
            Literal: bt.Literal,
        };

        Ok(())
    }

    pub fn Report(&mut self, e: SyntaxError) {
        self.SyntaxErrors.push(e);
    }

    pub fn ReportAndRecover(&mut self, e: SyntaxError) -> Result<(), ParserError> {
        self.SyntaxErrors.push(e);

        if self.QuoteStack.len() != 0 {
            while !tag_matches!(&self.Token.Kind, &self.QuoteStack.pop().unwrap()) {
                self.Scan()?;
            }
        }

        Ok(())
    }

    pub fn MatchTerm(&mut self, term: TokenKind) -> Result<Token, ParserError> {
        let token = self.Token.clone();
        self.Scan()?;
        if tag_matches!(&token.Kind, &term) {
            self.Report(SyntaxError::UnexpectedNode(UnexpectedNodeError { Want: Node::TokenKind(term), Have: Node::TokenKind(token.Kind) }));
            Ok(Token::default())
        } else {
            Ok(token)
        }
    }
}

impl Parser {
    pub fn ExpectIdent(&mut self) -> Result<Ident, ParserError> {
        let token = self.Token.clone();

        match token.Kind {
            TokenKind::Ident => {
                self.Scan()?;
                Ok(Ident { Pos: token.Pos, Token: token.clone() })
            }
            _ => {
                self.ReportAndRecover(SyntaxError::UnexpectedNode(UnexpectedNodeError { Want: Node::TokenKind(TokenKind::Ident), Have: Node::Token(self.Token.clone()) }))?;
                Ok(Ident::default())
            }
        }
    }

    pub fn ExpectField(&mut self) -> Result<Field, ParserError> {
        let begin = self.GetPos();

        Ok(Field {
            Name: self.ExpectIdent()?,
            Type: self.ExpectType()?,
            Pos: begin_end!(self, begin),
        })
    }

    pub fn ExpectFieldList(&mut self, delimiter: TokenKind, term: TokenKind) -> Result<FieldList, ParserError> {
        let begin = self.GetPos();

        Ok(FieldList {
            FieldList: parse_list!(self, Field, ExpectField, delimiter, term),
            Pos: begin_end!(self, begin),
        })
    }

    pub fn ExpectType(&mut self) -> Result<Type, ParserError> {
        Ok(match self.Token.Kind {
            TokenKind::STRUCT => { Type::StructType(Box::new(self.ExpectStructType()?)) }
            TokenKind::TRAIT => { Type::TraitType(Box::new(self.ExpectTraitType()?)) }
            _ => {
                self.ReportAndRecover(SyntaxError::UnexpectedNode(UnexpectedNodeError { Want: todo!(), Have: todo!() }))?;
                Type::None
            }
        })
    }

    pub fn ExpectFuncType(&mut self) -> Result<FuncType, ParserError> {
        let begin = self.GetPos();

        self.MatchTerm(TokenKind::LPAREN)?;

        let params = self.ExpectFieldList(TokenKind::COMMA, TokenKind::RPAREN)?;

        Ok(match self.Token.Kind {
            TokenKind::PASS => {
                FuncType {
                    Params: params,
                    Result: self.ExpectType()?,
                    Pos: begin_end!(self, begin),
                }
            }
            _ => {
                FuncType {
                    Params: params,
                    Result: Type::None,
                    Pos: begin_end!(self, begin),
                }
            }
        })
    }

    pub fn ExpectStructType(&mut self) -> Result<StructType, ParserError> {
        let begin = self.GetPos();

        self.MatchTerm(TokenKind::STRUCT)?;
        self.MatchTerm(TokenKind::LBRACE)?;

        let name = self.ExpectIdent()?;

        let fieldList = self.ExpectFieldList(TokenKind::SEMICOLON, TokenKind::RBRACE)?;

        Ok(StructType { Pos: begin_end!(self, begin), Name: name, FieldList: fieldList })
    }

    pub fn ExpectTraitType(&mut self) -> Result<TraitType, ParserError> {
        let begin = self.GetPos();

        self.MatchTerm(TokenKind::TRAIT)?;

        let name = self.ExpectIdent()?;

        Ok(TraitType {
            Name: name,
            Pos: begin_end!(self, begin),
        })
    }

    pub fn ExpectImportDecl(&mut self) -> Result<ImportDecl, ParserError> {
        let begin = self.GetPos();

        self.MatchTerm(TokenKind::IMPORT)?;
        Ok(match self.Token.Kind {
            TokenKind::Ident => {
                ImportDecl {
                    Alias: Some(self.ExpectIdent()?),
                    Canonical: self.MatchTerm(TokenKind::String)?,
                    Pos: begin_end!(self, begin),
                }
            }
            TokenKind::String => {
                ImportDecl {
                    Alias: None,
                    Canonical: self.MatchTerm(TokenKind::String)?,
                    Pos: begin_end!(self, begin),
                }
            }
            _ => {
                self.ReportAndRecover(SyntaxError::UnexpectedNode(UnexpectedNodeError {
                    Want: Node::TokenKind(TokenKind::String),
                    Have: Node::Token(self.Token.clone()),
                }))?;
                ImportDecl::default()
            }
        })
    }

    pub fn ExpectFuncDecl(&mut self) -> Result<FuncDecl, ParserError> {
        let begin = self.GetPos();

        self.MatchTerm(TokenKind::FUNC)?;

        let name = self.ExpectIdent()?;

        self.MatchTerm(TokenKind::LPAREN)?;

        let params = match self.Token.Kind {
            TokenKind::RPAREN => {
                FieldList { Pos: begin_end!(self, self.GetPos()), FieldList: vec![] }
            }
            _ => {
                self.ExpectFieldList(TokenKind::COMMA, TokenKind::RPAREN)?
            }
        };

        let typ = match self.Token.Kind {
            TokenKind::PASS => {
                self.Scan()?;
                FuncType {
                    Pos: begin_end!(self, begin),
                    Params: params,
                    Result: self.ExpectType()?,
                }
            }
            _ => {
                FuncType {
                    Pos: begin_end!(self, begin),
                    Params: params,
                    Result: Type::None,
                }
            }
        };

        Ok(FuncDecl {
            Name: name,
            Type: typ,
            Pos: begin_end!(self, begin.clone()),
        })
    }

    pub fn ExpectExpr(&mut self) -> Result<Expr, ParserError> {
        Ok(match self.Token.Kind {
            _ => { todo!() }
        })
    }

    pub fn ExpectStmt(&mut self) -> Result<Stmt, ParserError> {
        let expr = self.ExpectExpr()?;

        Ok(match self.Token.Kind {
            _ => { todo!() }
        })
    }

    pub fn ExpectStmtBlock(&mut self) -> Result<StmtBlock, ParserError> {
        let begin = self.GetPos();

        Ok(StmtBlock {
            StmtList: parse_list!(self, Stmt, ExpectStmt, TokenKind::SEMICOLON, TokenKind::None),
            Expr: Expr::None, // TODO
            Pos: begin_end!(self, begin),
        })
    }
}
