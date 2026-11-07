use std::fmt;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Interface,
    Namespace,
}

#[derive(Debug)]
pub struct Definition {
    pub kind: Kind,
    pub name: String,
    pub parent: Option<String>,
    pub extended: Vec<String>,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug)]
pub enum Type {
    Named { name: String, nullable: bool },
    Sequence { inner: Box<Type>, nullable: bool },
}

#[derive(Debug)]
pub struct Argument {
    pub name: String,
    pub ty: Type,
    pub optional: bool,
    pub variadic: bool,
    pub default: Option<String>,
}

#[derive(Debug)]
pub enum Member {
    Constant {
        name: String,
        value: String,
    },
    Attribute {
        name: String,
        ty: Type,
        readonly: bool,
    },
    Operation {
        name: String,
        returns: Type,
        arguments: Vec<Argument>,
    },
}

#[derive(Clone, PartialEq, Debug)]
enum Token {
    Word(String),
    Number(String),
    Text(String),
    Symbol(char),
    Ellipsis,
}

fn tokenize(source: &str) -> Result<Vec<Token>, Error> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
        } else if ch == '/' {
            chars.next();
            match chars.next() {
                Some('/') => {
                    for ch in chars.by_ref() {
                        if ch == '\n' {
                            break;
                        }
                    }
                }
                Some('*') => {
                    let mut previous = ' ';
                    for ch in chars.by_ref() {
                        if previous == '*' && ch == '/' {
                            break;
                        }
                        previous = ch;
                    }
                }
                _ => return Err(Error("stray '/'".into())),
            }
        } else if ch.is_ascii_alphabetic() || ch == '_' {
            let mut word = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    word.push(ch);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Word(word));
        } else if ch.is_ascii_digit() || ch == '-' {
            let mut number = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' {
                    number.push(ch);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Number(number));
        } else if ch == '"' {
            chars.next();
            let mut text = String::new();
            for ch in chars.by_ref() {
                if ch == '"' {
                    break;
                }
                text.push(ch);
            }
            tokens.push(Token::Text(text));
        } else if ch == '.' {
            for _ in 0..3 {
                if chars.next() != Some('.') {
                    return Err(Error("expected '...'".into()));
                }
            }
            tokens.push(Token::Ellipsis);
        } else {
            chars.next();
            tokens.push(Token::Symbol(ch));
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Result<Token, Error> {
        let token = self
            .tokens
            .get(self.at)
            .cloned()
            .ok_or_else(|| Error("unexpected end".into()))?;
        self.at += 1;
        Ok(token)
    }

    fn symbol(&mut self, symbol: char) -> Result<(), Error> {
        match self.next()? {
            Token::Symbol(found) if found == symbol => Ok(()),
            other => Err(Error(format!("expected '{symbol}', found {other:?}"))),
        }
    }

    fn eat_symbol(&mut self, symbol: char) -> bool {
        if self.peek() == Some(&Token::Symbol(symbol)) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if matches!(self.peek(), Some(Token::Word(found)) if found == word) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self) -> Result<String, Error> {
        match self.next()? {
            Token::Word(word) => Ok(word),
            other => Err(Error(format!("expected a name, found {other:?}"))),
        }
    }

    fn extended(&mut self) -> Result<Vec<String>, Error> {
        let mut attributes = Vec::new();
        if !self.eat_symbol('[') {
            return Ok(attributes);
        }
        let mut depth = 0;
        let mut current = String::new();
        loop {
            match self.next()? {
                Token::Symbol(']') if depth == 0 => break,
                Token::Symbol(',') if depth == 0 => attributes.push(std::mem::take(&mut current)),
                Token::Symbol(open @ ('(' | '[')) => {
                    depth += 1;
                    current.push(open);
                }
                Token::Symbol(close @ (')' | ']')) => {
                    depth -= 1;
                    current.push(close);
                }
                Token::Word(word) | Token::Number(word) | Token::Text(word) => {
                    current.push_str(&word)
                }
                Token::Symbol(symbol) => current.push(symbol),
                Token::Ellipsis => current.push_str("..."),
            }
        }
        if !current.is_empty() {
            attributes.push(current);
        }
        Ok(attributes)
    }

    fn ty(&mut self) -> Result<Type, Error> {
        let ty = if self.eat_word("sequence") {
            self.symbol('<')?;
            let inner = self.ty()?;
            self.symbol('>')?;
            Type::Sequence {
                inner: Box::new(inner),
                nullable: false,
            }
        } else {
            let mut name = self.word()?;
            for prefix in ["unsigned", "unrestricted"] {
                if name == prefix {
                    name = format!("{prefix} {}", self.word()?);
                }
            }
            if name.ends_with("long") && self.eat_word("long") {
                name.push_str(" long");
            }
            Type::Named {
                name,
                nullable: false,
            }
        };
        Ok(if self.eat_symbol('?') {
            match ty {
                Type::Named { name, .. } => Type::Named {
                    name,
                    nullable: true,
                },
                Type::Sequence { inner, .. } => Type::Sequence {
                    inner,
                    nullable: true,
                },
            }
        } else {
            ty
        })
    }

    fn arguments(&mut self) -> Result<Vec<Argument>, Error> {
        self.symbol('(')?;
        let mut arguments = Vec::new();
        if self.eat_symbol(')') {
            return Ok(arguments);
        }
        loop {
            self.extended()?;
            let optional = self.eat_word("optional");
            let ty = self.ty()?;
            let variadic = self.peek() == Some(&Token::Ellipsis);
            if variadic {
                self.at += 1;
            }
            let name = self.word()?;
            let default = if self.eat_symbol('=') {
                Some(match self.next()? {
                    Token::Word(word) | Token::Number(word) => word,
                    Token::Text(text) => format!("\"{text}\""),
                    other => return Err(Error(format!("bad default {other:?}"))),
                })
            } else {
                None
            };
            arguments.push(Argument {
                name,
                ty,
                optional,
                variadic,
                default,
            });
            if self.eat_symbol(')') {
                return Ok(arguments);
            }
            self.symbol(',')?;
        }
    }

    fn member(&mut self) -> Result<Member, Error> {
        self.extended()?;
        if self.eat_word("const") {
            self.ty()?;
            let name = self.word()?;
            self.symbol('=')?;
            let value = match self.next()? {
                Token::Number(number) | Token::Word(number) => number,
                other => return Err(Error(format!("bad constant {other:?}"))),
            };
            self.symbol(';')?;
            return Ok(Member::Constant { name, value });
        }
        let readonly = self.eat_word("readonly");
        if self.eat_word("attribute") {
            let ty = self.ty()?;
            let name = self.word()?;
            self.symbol(';')?;
            return Ok(Member::Attribute { name, ty, readonly });
        }
        let returns = self.ty()?;
        let name = self.word()?;
        let arguments = self.arguments()?;
        self.symbol(';')?;
        Ok(Member::Operation {
            name,
            returns,
            arguments,
        })
    }

    fn definition(&mut self) -> Result<(Definition, bool), Error> {
        let extended = self.extended()?;
        let partial = self.eat_word("partial");
        let kind = match self.word()?.as_str() {
            "interface" => Kind::Interface,
            "namespace" => Kind::Namespace,
            other => return Err(Error(format!("unsupported definition {other}"))),
        };
        let name = self.word()?;
        let parent = if self.eat_symbol(':') {
            Some(self.word()?)
        } else {
            None
        };
        self.symbol('{')?;
        let mut members = Vec::new();
        while !self.eat_symbol('}') {
            members.push(self.member()?);
        }
        self.symbol(';')?;
        Ok((
            Definition {
                kind,
                name,
                parent,
                extended,
                members,
            },
            partial,
        ))
    }
}

pub fn parse(source: &str, definitions: &mut Vec<Definition>) -> Result<(), Error> {
    let mut parser = Parser {
        tokens: tokenize(source)?,
        at: 0,
    };
    while parser.peek().is_some() {
        let (definition, partial) = parser.definition()?;
        match definitions
            .iter_mut()
            .find(|known| known.name == definition.name)
        {
            Some(known) if partial => known.members.extend(definition.members),
            Some(_) => return Err(Error(format!("{} is defined twice", definition.name))),
            None => definitions.push(definition),
        }
    }
    Ok(())
}
