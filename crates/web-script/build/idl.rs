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
    Mixin,
    Namespace,
    Dictionary,
    Callback,
    CallbackInterface,
    Enum,
    Typedef,
}

#[derive(Debug)]
pub struct Definition {
    pub kind: Kind,
    pub name: String,
    pub parent: Option<String>,
    pub extended: Vec<String>,
    pub members: Vec<Member>,
    pub fields: Vec<Argument>,
    pub alias: Option<Type>,
}

impl Definition {
    pub fn attribute(&self, name: &str) -> Option<&str> {
        extended_value(&self.extended, name)
    }
}

pub fn extended_value<'a>(extended: &'a [String], name: &str) -> Option<&'a str> {
    extended.iter().find_map(|attribute| {
        if attribute == name {
            Some("")
        } else {
            attribute
                .strip_prefix(name)
                .and_then(|rest| rest.strip_prefix('='))
        }
    })
}

#[derive(Clone, Debug)]
pub enum Type {
    Named { name: String, nullable: bool },
    Sequence { inner: Box<Type>, nullable: bool },
    Union { members: Vec<Type>, nullable: bool },
}

impl Type {
    pub fn with_nullable(self, value: bool) -> Type {
        match self {
            Type::Named { name, .. } => Type::Named {
                name,
                nullable: value,
            },
            Type::Sequence { inner, .. } => Type::Sequence {
                inner,
                nullable: value,
            },
            Type::Union { members, .. } => Type::Union {
                members,
                nullable: value,
            },
        }
    }

    pub fn nullable(&self) -> bool {
        match self {
            Type::Named { nullable, .. }
            | Type::Sequence { nullable, .. }
            | Type::Union { nullable, .. } => *nullable,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Argument {
    pub name: String,
    pub ty: Type,
    pub optional: bool,
    pub variadic: bool,
    pub default: Option<String>,
    pub extended: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum Member {
    Constant {
        name: String,
        value: String,
    },
    Attribute {
        name: String,
        ty: Type,
        readonly: bool,
        stringifier: bool,
        extended: Vec<String>,
    },
    Operation {
        name: String,
        returns: Type,
        arguments: Vec<Argument>,
        is_static: bool,
        extended: Vec<String>,
    },
    Constructor {
        arguments: Vec<Argument>,
    },
    Iterable,
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
                Token::Symbol(open @ '(') => {
                    depth += 1;
                    current.push(open);
                }
                Token::Symbol(close @ ')') => {
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
        self.extended()?;
        let ty = if self.eat_symbol('(') {
            let mut members = vec![self.ty()?];
            while self.eat_word("or") {
                members.push(self.ty()?);
            }
            self.symbol(')')?;
            Type::Union {
                members,
                nullable: false,
            }
        } else if self.eat_word("sequence") {
            self.symbol('<')?;
            let inner = self.ty()?;
            self.symbol('>')?;
            Type::Sequence {
                inner: Box::new(inner),
                nullable: false,
            }
        } else if self.eat_word("Promise") {
            self.symbol('<')?;
            self.ty()?;
            self.symbol('>')?;
            Type::Named {
                name: "any".to_owned(),
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
            ty.with_nullable(true)
        } else {
            ty
        })
    }

    fn default(&mut self) -> Result<Option<String>, Error> {
        if !self.eat_symbol('=') {
            return Ok(None);
        }
        Ok(Some(match self.next()? {
            Token::Word(word) | Token::Number(word) => word,
            Token::Text(text) => format!("\"{text}\""),
            Token::Symbol('{') => {
                self.symbol('}')?;
                "{}".to_owned()
            }
            Token::Symbol('[') => {
                self.symbol(']')?;
                "[]".to_owned()
            }
            other => return Err(Error(format!("bad default {other:?}"))),
        }))
    }

    fn arguments(&mut self) -> Result<Vec<Argument>, Error> {
        self.symbol('(')?;
        let mut arguments = Vec::new();
        if self.eat_symbol(')') {
            return Ok(arguments);
        }
        loop {
            let extended = self.extended()?;
            let optional = self.eat_word("optional");
            let ty = self.ty()?;
            let variadic = self.peek() == Some(&Token::Ellipsis);
            if variadic {
                self.at += 1;
            }
            let name = self.word()?;
            let default = self.default()?;
            arguments.push(Argument {
                name,
                ty,
                optional,
                variadic,
                default,
                extended,
            });
            if self.eat_symbol(')') {
                return Ok(arguments);
            }
            self.symbol(',')?;
        }
    }

    fn member(&mut self) -> Result<Member, Error> {
        let extended = self.extended()?;
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
        if self.eat_word("constructor") {
            let arguments = self.arguments()?;
            self.symbol(';')?;
            return Ok(Member::Constructor { arguments });
        }
        if self.eat_word("iterable") {
            self.symbol('<')?;
            self.ty()?;
            self.symbol('>')?;
            self.symbol(';')?;
            return Ok(Member::Iterable);
        }
        let stringifier = self.eat_word("stringifier");
        let is_static = self.eat_word("static");
        self.eat_word("getter");
        let readonly = self.eat_word("readonly");
        if self.eat_word("attribute") {
            let ty = self.ty()?;
            let name = self.word()?;
            self.symbol(';')?;
            return Ok(Member::Attribute {
                name,
                ty,
                readonly,
                stringifier,
                extended,
            });
        }
        let returns = self.ty()?;
        let name = self.word()?;
        let arguments = self.arguments()?;
        self.symbol(';')?;
        Ok(Member::Operation {
            name,
            returns,
            arguments,
            is_static,
            extended,
        })
    }

    fn fields(&mut self) -> Result<Vec<Argument>, Error> {
        let mut fields = Vec::new();
        while !self.eat_symbol('}') {
            let extended = self.extended()?;
            let required = self.eat_word("required");
            let ty = self.ty()?;
            let name = self.word()?;
            let default = self.default()?;
            self.symbol(';')?;
            fields.push(Argument {
                name,
                ty,
                optional: !required,
                variadic: false,
                default,
                extended,
            });
        }
        self.symbol(';')?;
        Ok(fields)
    }

    fn definition(&mut self, includes: &mut Vec<(String, String)>) -> Result<Option<(Definition, bool)>, Error> {
        let extended = self.extended()?;
        let partial = self.eat_word("partial");
        let mut definition = Definition {
            kind: Kind::Interface,
            name: String::new(),
            parent: None,
            extended,
            members: Vec::new(),
            fields: Vec::new(),
            alias: None,
        };
        let first = self.word()?;
        match first.as_str() {
            "interface" | "namespace" => {
                definition.kind = if first == "namespace" {
                    Kind::Namespace
                } else if self.eat_word("mixin") {
                    Kind::Mixin
                } else {
                    Kind::Interface
                };
            }
            "dictionary" => {
                definition.kind = Kind::Dictionary;
                definition.name = self.word()?;
                if self.eat_symbol(':') {
                    definition.parent = Some(self.word()?);
                }
                self.symbol('{')?;
                definition.fields = self.fields()?;
                return Ok(Some((definition, partial)));
            }
            "callback" => {
                if self.eat_word("interface") {
                    definition.kind = Kind::CallbackInterface;
                    definition.name = self.word()?;
                    self.symbol('{')?;
                    while !self.eat_symbol('}') {
                        self.member()?;
                    }
                    self.symbol(';')?;
                } else {
                    definition.kind = Kind::Callback;
                    definition.name = self.word()?;
                    self.symbol('=')?;
                    self.ty()?;
                    self.arguments()?;
                    self.symbol(';')?;
                }
                return Ok(Some((definition, false)));
            }
            "enum" => {
                definition.kind = Kind::Enum;
                definition.name = self.word()?;
                self.symbol('{')?;
                while !self.eat_symbol('}') {
                    self.next()?;
                }
                self.symbol(';')?;
                return Ok(Some((definition, false)));
            }
            "typedef" => {
                definition.kind = Kind::Typedef;
                definition.alias = Some(self.ty()?);
                definition.name = self.word()?;
                self.symbol(';')?;
                return Ok(Some((definition, false)));
            }
            other => {
                if self.eat_word("includes") {
                    let mixin = self.word()?;
                    self.symbol(';')?;
                    includes.push((other.to_owned(), mixin));
                    return Ok(None);
                }
                return Err(Error(format!("unsupported definition {other}")));
            }
        }
        definition.name = self.word()?;
        if self.eat_symbol(':') {
            definition.parent = Some(self.word()?);
        }
        self.symbol('{')?;
        while !self.eat_symbol('}') {
            definition.members.push(self.member()?);
        }
        self.symbol(';')?;
        Ok(Some((definition, partial)))
    }
}

pub struct Parsed {
    pub definitions: Vec<Definition>,
    includes: Vec<(String, String)>,
}

impl Parsed {
    pub fn new() -> Self {
        Parsed {
            definitions: Vec::new(),
            includes: Vec::new(),
        }
    }

    pub fn parse(&mut self, source: &str) -> Result<(), Error> {
        let mut parser = Parser {
            tokens: tokenize(source)?,
            at: 0,
        };
        while parser.peek().is_some() {
            let Some((definition, partial)) = parser.definition(&mut self.includes)? else {
                continue;
            };
            match self
                .definitions
                .iter_mut()
                .find(|known| known.name == definition.name)
            {
                Some(known) if partial => known.members.extend(definition.members),
                Some(_) => return Err(Error(format!("{} is defined twice", definition.name))),
                None => self.definitions.push(definition),
            }
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<Vec<Definition>, Error> {
        for (interface, mixin) in std::mem::take(&mut self.includes) {
            let members = self
                .definitions
                .iter()
                .find(|definition| definition.name == mixin && definition.kind == Kind::Mixin)
                .map(|definition| definition.members.clone())
                .ok_or_else(|| Error(format!("no mixin {mixin}")))?;
            let target = self
                .definitions
                .iter_mut()
                .find(|definition| definition.name == interface)
                .ok_or_else(|| Error(format!("no interface {interface}")))?;
            target.members.extend(members);
        }
        Ok(self.definitions)
    }
}
