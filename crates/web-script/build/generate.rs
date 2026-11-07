use std::fmt::Write;

use crate::idl::{Argument, Definition, Error, Kind, Member, Type, extended_value};

pub fn snake(name: &str) -> String {
    let mut out = String::new();
    let mut previous_lower = false;
    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            if previous_lower {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_lower = false;
        } else {
            out.push(ch);
            previous_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    match out.as_str() {
        "type" | "self" | "match" | "ref" | "in" | "loop" | "where" | "use" | "for" | "as"
        | "mod" | "move" | "async" | "await" | "fn" | "enum" | "struct" | "trait" | "impl"
        | "let" | "const" | "static" | "dyn" | "final" | "override" | "try" | "do" | "macro"
        | "typeof" | "yield" | "box" | "abstract" | "priv" | "virtual" | "if" | "else"
        | "while" | "return" | "break" | "continue" | "crate" | "super" | "true" | "false" => {
            format!("{out}_")
        }
        _ => out,
    }
}

fn upper_first(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
        .unwrap_or_default()
}

struct Context<'a> {
    definitions: &'a [Definition],
    unions: std::cell::RefCell<Vec<String>>,
}

impl Context<'_> {
    fn find(&self, name: &str) -> Option<&Definition> {
        self.definitions
            .iter()
            .find(|definition| definition.name == name)
    }

    fn kind(&self, name: &str) -> Option<Kind> {
        self.find(name).map(|definition| definition.kind)
    }

    fn is_node(&self, name: &str) -> bool {
        let mut current = Some(name.to_owned());
        while let Some(known) = current {
            if known == "Node" {
                return true;
            }
            current = self.find(&known).and_then(|definition| definition.parent.clone());
        }
        false
    }

    fn is_global(&self, name: &str) -> bool {
        self.find(name)
            .is_some_and(|definition| definition.attribute("Global").is_some())
    }

    fn resolve(&self, ty: &Type) -> Type {
        if let Type::Named { name, nullable } = ty
            && let Some(alias) = self.find(name).and_then(|definition| definition.alias.as_ref())
        {
            return self.resolve(alias).with_nullable(*nullable || alias.nullable());
        }
        ty.clone()
    }

    fn base(&self, name: &str) -> Result<String, Error> {
        Ok(match name {
            "DOMString" | "USVString" | "ByteString" | "CSSOMString" => "String".to_owned(),
            "boolean" => "bool".to_owned(),
            "byte" => "i8".to_owned(),
            "octet" => "u8".to_owned(),
            "unsigned short" => "u16".to_owned(),
            "short" => "i16".to_owned(),
            "unsigned long" => "u32".to_owned(),
            "long" => "i32".to_owned(),
            "long long" => "i64".to_owned(),
            "unsigned long long" => "u64".to_owned(),
            "double" | "unrestricted double" | "float" | "unrestricted float" => "f64".to_owned(),
            "any" => "JsValue".to_owned(),
            "object" => "JsObject".to_owned(),
            "undefined" => "()".to_owned(),
            other if self.is_global(other) => "Global".to_owned(),
            other if self.is_node(other) => "NodeId".to_owned(),
            other => match self.kind(other) {
                Some(Kind::Interface | Kind::Callback | Kind::CallbackInterface) => {
                    "JsObject".to_owned()
                }
                Some(Kind::Enum) => "String".to_owned(),
                Some(Kind::Dictionary) => other.to_owned(),
                _ => return Err(Error(format!("unsupported type {other}"))),
            },
        })
    }

    fn rust(&self, ty: &Type) -> Result<String, Error> {
        let ty = self.resolve(ty);
        let (base, nullable) = match &ty {
            Type::Sequence { inner, nullable } => (format!("Vec<{}>", self.rust(inner)?), *nullable),
            Type::Named { name, nullable } => (self.base(name)?, *nullable),
            Type::Union { members, nullable } => (self.union(members)?, *nullable),
        };
        Ok(if nullable {
            format!("Option<{base}>")
        } else {
            base
        })
    }

    fn variant(&self, ty: &Type) -> String {
        match self.resolve(ty) {
            Type::Named { name, .. } => upper_first(&name.replace(' ', "_")),
            Type::Sequence { .. } => "Sequence".to_owned(),
            Type::Union { .. } => "Union".to_owned(),
        }
    }

    fn union(&self, members: &[Type]) -> Result<String, Error> {
        let name = format!(
            "Union{}",
            members
                .iter()
                .map(|member| self.variant(member))
                .collect::<Vec<_>>()
                .join("Or")
        );
        if self.unions.borrow().iter().any(|known| known.starts_with(&format!("pub enum {name} "))) {
            return Ok(name);
        }
        let mut code = format!("pub enum {name} {{\n");
        for member in members {
            let _ = writeln!(code, "    {}({}),", self.variant(member), self.rust(member)?);
        }
        code.push_str("}\n\n");
        let _ = writeln!(
            code,
            "impl {name} {{\n    pub fn from_js(value: &JsValue, context: &mut Context) -> JsResult<Self> {{"
        );
        let resolved: Vec<Type> = members.iter().map(|member| self.resolve(member)).collect();
        let named = |ty: &Type| match ty {
            Type::Named { name, .. } => Some(name.clone()),
            _ => None,
        };
        for member in &resolved {
            if let Some(name) = named(member)
                && self.kind(&name) == Some(Kind::Dictionary)
            {
                let _ = writeln!(
                    code,
                    "        if value.is_null_or_undefined() {{ return Ok(Self::{}({name}::from_js(value, context)?)); }}",
                    self.variant(member)
                );
            }
        }
        for member in &resolved {
            if let Some(name) = named(member)
                && (self.is_node(&name)
                    || (self.kind(&name) == Some(Kind::Interface) && !self.is_global(&name)))
            {
                let _ = writeln!(
                    code,
                    "        if convert::implements(value, Interface::{name}) {{ return Ok(Self::{}({})); }}",
                    self.variant(member),
                    self.convert(member, "value", &[])?
                );
            }
        }
        for member in &resolved {
            match member {
                Type::Sequence { .. } => {
                    let _ = writeln!(
                        code,
                        "        if value.is_object() {{ return Ok(Self::Sequence({})); }}",
                        self.convert(member, "value", &[])?
                    );
                }
                Type::Named { name, .. } => match self.kind(name) {
                    Some(Kind::Callback) => {
                        let _ = writeln!(
                            code,
                            "        if value.is_callable() {{ return Ok(Self::{}({})); }}",
                            self.variant(member),
                            self.convert(member, "value", &[])?
                        );
                    }
                    Some(Kind::Dictionary | Kind::CallbackInterface) => {
                        let _ = writeln!(
                            code,
                            "        if value.is_object() {{ return Ok(Self::{}({})); }}",
                            self.variant(member),
                            self.convert(member, "value", &[])?
                        );
                    }
                    _ => {}
                },
                Type::Union { .. } => {}
            }
        }
        let numeric = |name: &str| {
            matches!(
                name,
                "byte" | "octet" | "short" | "unsigned short" | "long" | "unsigned long"
                    | "long long" | "unsigned long long" | "double" | "unrestricted double"
                    | "float" | "unrestricted float"
            )
        };
        for member in &resolved {
            if let Some(name) = named(member) {
                if name == "boolean" {
                    let _ = writeln!(
                        code,
                        "        if value.is_boolean() {{ return Ok(Self::Boolean(value.to_boolean())); }}"
                    );
                } else if numeric(&name) {
                    let _ = writeln!(
                        code,
                        "        if value.is_number() {{ return Ok(Self::{}({})); }}",
                        self.variant(member),
                        self.convert(member, "value", &[])?
                    );
                }
            }
        }
        let fallback = resolved
            .iter()
            .find(|member| {
                named(member).is_some_and(|name| {
                    matches!(name.as_str(), "DOMString" | "USVString" | "ByteString" | "CSSOMString")
                        || self.kind(&name) == Some(Kind::Enum)
                })
            })
            .or_else(|| resolved.iter().find(|member| named(member).is_some_and(|name| numeric(&name))))
            .or_else(|| resolved.iter().find(|member| named(member).as_deref() == Some("boolean")));
        match fallback {
            Some(member) => {
                let _ = writeln!(
                    code,
                    "        Ok(Self::{}({}))",
                    self.variant(member),
                    self.convert(member, "value", &[])?
                );
            }
            None => code.push_str(
                "        Err(JsNativeError::typ().with_message(\"the value matches no member of the union\").into())\n",
            ),
        }
        code.push_str("    }\n}\n\n");
        self.unions.borrow_mut().push(code);
        Ok(name)
    }

    fn convert(&self, ty: &Type, value: &str, extended: &[String]) -> Result<String, Error> {
        let ty = self.resolve(ty);
        let plain = match &ty {
            Type::Sequence { inner, .. } => {
                let item = self.convert(inner, "item", &[])?;
                format!("convert::sequence({value}, context, |item, context| Ok({item}))?")
            }
            Type::Union { members, .. } => {
                format!("{}::from_js({value}, context)?", self.union(members)?)
            }
            Type::Named { name, .. } => match name.as_str() {
                "DOMString" | "CSSOMString" if extended_value(extended, "LegacyNullToEmptyString").is_some() => {
                    format!("convert::string_or_empty({value}, context)?")
                }
                "DOMString" | "USVString" | "ByteString" | "CSSOMString" => {
                    format!("convert::string({value}, context)?")
                }
                "boolean" => format!("convert::boolean({value})"),
                "byte" => format!("convert::long({value}, context)? as i8"),
                "octet" => format!("convert::long({value}, context)? as u8"),
                "unsigned short" => format!("convert::unsigned_short({value}, context)?"),
                "short" => format!("convert::short({value}, context)?"),
                "unsigned long" => format!("convert::unsigned_long({value}, context)?"),
                "long" => format!("convert::long({value}, context)?"),
                "long long" => format!("convert::long_long({value}, context)?"),
                "unsigned long long" => format!("convert::long_long({value}, context)? as u64"),
                "double" | "float" => format!("convert::double({value}, context)?"),
                "unrestricted double" | "unrestricted float" => {
                    format!("convert::unrestricted_double({value}, context)?")
                }
                "any" => format!("({value}).clone()"),
                "object" => format!("convert::any_object({value})?"),
                other if self.is_node(other) => {
                    format!("convert::node({value}, Interface::{other})?")
                }
                other => match self.kind(other) {
                    Some(Kind::Interface) => {
                        format!("convert::object({value}, Interface::{other})?")
                    }
                    Some(Kind::Callback) => format!("convert::callback({value})?"),
                    Some(Kind::CallbackInterface) => format!("convert::any_object({value})?"),
                    Some(Kind::Enum) => format!("convert::string({value}, context)?"),
                    Some(Kind::Dictionary) => format!("{other}::from_js({value}, context)?"),
                    _ => return Err(Error(format!("unsupported argument type {other}"))),
                },
            },
        };
        Ok(if ty.nullable() {
            format!(
                "{{ let nullable: &JsValue = {value}; if nullable.is_null_or_undefined() {{ None }} else {{ let value = nullable; Some({}) }} }}",
                self.convert(&ty.clone().with_nullable(false), "value", extended)?
            )
        } else {
            plain
        })
    }

    fn default(&self, ty: &Type, default: &str) -> Result<String, Error> {
        let ty = self.resolve(ty);
        Ok(match (&ty, default) {
            (Type::Named { name, .. }, "null") if name == "any" => "JsValue::null()".to_owned(),
            (_, "null") => "None".to_owned(),
            (Type::Union { .. } | Type::Named { .. }, "{}") => {
                format!("{}::from_js(&JsValue::undefined(), context)?", self.rust(&ty)?)
            }
            (_, "[]") => "Vec::new()".to_owned(),
            (Type::Union { members, .. }, value) => {
                let name = self.union(members)?;
                format!("{name}::Boolean({value})")
            }
            (_, value) if value.starts_with('"') => {
                let text = format!("{value}.to_owned()");
                if ty.nullable() { format!("Some({text})") } else { text }
            }
            (_, value) => {
                if ty.nullable() {
                    format!("Some({value})")
                } else {
                    value.to_owned()
                }
            }
        })
    }

    fn argument_type(&self, argument: &Argument) -> Result<String, Error> {
        let rust = self.rust(&argument.ty)?;
        Ok(if argument.variadic {
            format!("Vec<{rust}>")
        } else if argument.optional && argument.default.is_none() && !argument.ty.nullable() {
            format!("Option<{rust}>")
        } else {
            rust
        })
    }

    fn argument_value(&self, index: usize, argument: &Argument) -> Result<String, Error> {
        let value = format!("&convert::argument(arguments, {index})");
        if argument.variadic {
            let item = self.convert(&argument.ty, "item", &argument.extended)?;
            return Ok(format!(
                "{{ let mut items = Vec::new(); for item in arguments.get({index}..).unwrap_or_default() {{ items.push({item}); }} items }}"
            ));
        }
        if argument.optional {
            let converted = self.convert(&argument.ty, "value", &argument.extended)?;
            return Ok(match &argument.default {
                Some(default) => format!(
                    "{{ let value = {value}; if value.is_undefined() {{ {} }} else {{ {converted} }} }}",
                    self.default(&argument.ty, default)?
                ),
                None if argument.ty.nullable() => {
                    format!("{{ let value = {value}; {converted} }}")
                }
                None => format!(
                    "{{ let value = {value}; if value.is_undefined() {{ None }} else {{ Some({converted}) }} }}"
                ),
            });
        }
        self.convert(&argument.ty, &value, &argument.extended)
    }

    fn dictionary_fields(&self, definition: &Definition) -> Vec<Argument> {
        let mut fields = definition
            .parent
            .as_ref()
            .and_then(|parent| self.find(parent))
            .map(|parent| self.dictionary_fields(parent))
            .unwrap_or_default();
        fields.extend(definition.fields.iter().cloned());
        fields
    }

    fn dictionary(&self, definition: &Definition, out: &mut String) -> Result<(), Error> {
        let fields = self.dictionary_fields(definition);
        let name = &definition.name;
        let _ = writeln!(out, "pub struct {name} {{");
        for field in &fields {
            let _ = writeln!(out, "    pub {}: {},", snake(&field.name), self.argument_type(field)?);
        }
        let _ = writeln!(
            out,
            "}}\n\nimpl {name} {{\n    pub fn from_js(value: &JsValue, context: &mut Context) -> JsResult<Self> {{\n        let object = if value.is_null_or_undefined() {{ None }} else {{ Some(convert::any_object(value)?) }};"
        );
        let mut sorted: Vec<&Argument> = fields.iter().collect();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        for field in sorted {
            let variable = snake(&field.name);
            let converted = self.convert(&field.ty, "value", &field.extended)?;
            let fallback = match &field.default {
                Some(default) => self.default(&field.ty, default)?,
                None if field.optional => "None".to_owned(),
                None => format!(
                    "return Err(JsNativeError::typ().with_message(\"{} is required\").into())",
                    field.name
                ),
            };
            let present = if field.optional && field.default.is_none() && !field.ty.nullable() {
                format!("Some({converted})")
            } else {
                converted
            };
            let _ = writeln!(
                out,
                "        let value = &match &object {{ Some(object) => object.get(JsString::from(\"{}\"), context)?, None => JsValue::undefined() }};\n        let {variable} = if value.is_undefined() {{ {fallback} }} else {{ {present} }};",
                field.name
            );
        }
        let names: Vec<String> = fields.iter().map(|field| snake(&field.name)).collect();
        let _ = writeln!(out, "        Ok({name} {{ {} }})\n    }}\n}}\n", names.join(", "));
        Ok(())
    }

    fn this(&self, definition: &Definition, is_static: bool) -> (&'static str, String) {
        if is_static || definition.kind == Kind::Namespace || self.is_global(&definition.name) {
            ("()", "{ let _ = this; }".to_owned())
        } else if self.is_node(&definition.name) {
            (
                "NodeId",
                format!("convert::this_node(this, Interface::{})?", definition.name),
            )
        } else {
            (
                "JsObject",
                format!("convert::this_object(this, Interface::{}, context)?", definition.name),
            )
        }
    }
}

const NATIVE: &str = "(this: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue>";

fn reflect_kind(ty: &Type, extended: &[String]) -> Result<&'static str, Error> {
    let Type::Named { name, nullable } = ty else {
        return Err(Error("reflected attributes are simple".into()));
    };
    Ok(match (name.as_str(), nullable) {
        ("boolean", _) => "boolean",
        ("long", _) => "long",
        ("unsigned long", _) => "unsigned",
        ("DOMString", true) => "nullable",
        ("USVString", _) => "url",
        _ if extended_value(extended, "URL").is_some() => "url",
        ("DOMString", _) => "string",
        (other, _) => return Err(Error(format!("cannot reflect {other}"))),
    })
}

fn reflect_rust(kind: &str) -> &'static str {
    match kind {
        "boolean" => "bool",
        "long" => "i32",
        "unsigned" => "u32",
        "nullable" => "Option<String>",
        _ => "String",
    }
}

pub fn generate(definitions: &[Definition]) -> Result<String, Error> {
    let context = Context {
        definitions,
        unions: std::cell::RefCell::new(Vec::new()),
    };
    let mut out = String::new();
    let interfaces: Vec<&Definition> = definitions
        .iter()
        .filter(|definition| definition.kind == Kind::Interface)
        .collect();
    out.push_str("#[derive(Clone, Copy, PartialEq, Eq, Debug)]\n#[repr(u16)]\npub enum Interface {\n");
    for interface in &interfaces {
        let _ = writeln!(out, "    {},", interface.name);
    }
    out.push_str("}\n\nimpl Interface {\n    pub fn parent(self) -> Option<Interface> {\n        match self {\n");
    for interface in &interfaces {
        let parent = interface
            .parent
            .as_ref()
            .map_or("None".to_owned(), |parent| format!("Some(Interface::{parent})"));
        let _ = writeln!(out, "            Interface::{} => {parent},", interface.name);
    }
    out.push_str("        }\n    }\n\n    pub fn name(self) -> &'static str {\n        match self {\n");
    for interface in &interfaces {
        let _ = writeln!(out, "            Interface::{0} => \"{0}\",", interface.name);
    }
    out.push_str("        }\n    }\n}\n\npub fn html_interface(name: &str) -> Option<Interface> {\n    Some(match name {\n");
    for interface in &interfaces {
        let tags: Vec<String> = interface
            .extended
            .iter()
            .filter_map(|attribute| attribute.strip_prefix("Tag="))
            .map(|tag| format!("\"{tag}\""))
            .collect();
        if !tags.is_empty() {
            let _ = writeln!(out, "        {} => Interface::{},", tags.join(" | "), interface.name);
        }
    }
    out.push_str("        _ => return None,\n    })\n}\n\n");

    let mut specs = String::new();
    let mut namespaces = String::new();
    for definition in definitions {
        match definition.kind {
            Kind::Dictionary => {
                context.dictionary(definition, &mut out)?;
                continue;
            }
            Kind::Interface | Kind::Namespace => {}
            _ => continue,
        }
        let module = snake(&definition.name).trim_end_matches('_').to_owned();
        let trait_name = format!("{}Api", upper_first(&definition.name));
        let mut trait_body = String::new();
        let mut functions = String::new();
        let mut members = String::new();
        let mut constructor = "None".to_owned();
        let mut iterable = false;
        for member in &definition.members {
            match member {
                Member::Constant { name, value } => {
                    let _ = writeln!(members, "        Member::Constant {{ name: \"{name}\", value: {value}i64 as f64 }},");
                }
                Member::Iterable => iterable = true,
                Member::Constructor { arguments } => {
                    let mut parameters = String::new();
                    let mut conversions = String::new();
                    let mut names = String::new();
                    for (index, argument) in arguments.iter().enumerate() {
                        let variable = snake(&argument.name);
                        let _ = write!(parameters, ", {variable}: {}", context.argument_type(argument)?);
                        let _ = writeln!(conversions, "    let {variable} = {};", context.argument_value(index, argument)?);
                        let _ = write!(names, ", {variable}");
                    }
                    let _ = writeln!(trait_body, "    fn constructor(context: &mut Context, prototype: JsObject{parameters}) -> Result<JsObject, Exception>;");
                    let required = arguments.iter().filter(|argument| !argument.optional && !argument.variadic).count();
                    let _ = writeln!(
                        functions,
                        "fn {module}__constructor{NATIVE} {{\n    convert::required(arguments, {required}, \"{name}\")?;\n    let prototype = convert::new_target_prototype(this, Interface::{name}, context)?;\n{conversions}    Ok(JsValue::from(<Api as {trait_name}>::constructor(context, prototype{names})?))\n}}\n",
                        name = definition.name,
                    );
                    constructor = format!("Some(({module}__constructor, {required}))");
                }
                Member::Attribute { name, ty, readonly, stringifier, extended } => {
                    let field = snake(name);
                    let getter = format!("{module}__get_{field}");
                    let (this_type, this_convert) = context.this(definition, false);
                    let mut setter = "None".to_owned();
                    let resolved = context.resolve(ty);
                    let handler = matches!(&resolved, Type::Named { name, .. } if name == "EventHandler");
                    if handler {
                        let event = name.trim_start_matches("on");
                        let _ = writeln!(
                            functions,
                            "fn {getter}{NATIVE} {{\n    crate::events::handler_get(this, \"{event}\", context)\n}}\n\nfn {module}__set_{field}{NATIVE} {{\n    crate::events::handler_set(this, \"{event}\", &convert::argument(arguments, 0), context)\n}}\n"
                        );
                        setter = format!("Some({module}__set_{field})");
                    } else if extended_value(extended, "EventProp").is_some() {
                        let _ = writeln!(
                            functions,
                            "fn {getter}{NATIVE} {{\n    let _ = arguments;\n    crate::events::prop(this, \"{name}\", Interface::{interface}, context)\n}}\n",
                            interface = definition.name,
                        );
                    } else if let Some(content) = extended_value(extended, "Reflect") {
                        let content = if content.is_empty() { name.to_ascii_lowercase() } else { content.to_owned() };
                        let kind = reflect_kind(&resolved, extended)?;
                        let _ = writeln!(
                            functions,
                            "fn {getter}{NATIVE} {{\n    let _ = arguments;\n    let this = {this_convert};\n    let result = host::with(|host| crate::reflect::get_{kind}(host, this, \"{content}\"))?;\n    convert::to_js(result, context)\n}}\n"
                        );
                        if !readonly {
                            let converted = context.convert(ty, "&convert::argument(arguments, 0)", extended)?;
                            let _ = writeln!(
                                functions,
                                "fn {module}__set_{field}{NATIVE} {{\n    let this = {this_convert};\n    let value: {} = {converted};\n    host::with(|host| crate::reflect::set_{kind}(host, this, \"{content}\", value))??;\n    convert::after(context)?;\n    Ok(JsValue::undefined())\n}}\n",
                                reflect_rust(kind)
                            );
                            setter = format!("Some({module}__set_{field})");
                        }
                    } else {
                        let rust = context.rust(ty)?;
                        let script = extended_value(extended, "Script").is_some();
                        let (receiver, call) = if script { ("context: &mut Context", "context") } else { ("host: &mut Host", "host") };
                        let _ = writeln!(trait_body, "    fn {field}({receiver}, this: {this_type}) -> Result<{rust}, Exception>;");
                        let invoke = if script {
                            format!("<Api as {trait_name}>::{field}(context, this)?")
                        } else {
                            format!("host::with(|host| <Api as {trait_name}>::{field}(host, this))??")
                        };
                        let _ = call;
                        let _ = writeln!(
                            functions,
                            "fn {getter}{NATIVE} {{\n    let _ = arguments;\n    let this = {this_convert};\n    let result = {invoke};\n    convert::to_js(result, context)\n}}\n"
                        );
                        if let Some(forward) = extended_value(extended, "PutForwards") {
                            let _ = writeln!(
                                functions,
                                "fn {module}__set_{field}{NATIVE} {{\n    convert::put_forward(this, \"{name}\", \"{forward}\", &convert::argument(arguments, 0), context)\n}}\n"
                            );
                            setter = format!("Some({module}__set_{field})");
                        } else if !readonly {
                            let converted = context.convert(ty, "&convert::argument(arguments, 0)", extended)?;
                            let _ = writeln!(trait_body, "    fn set_{field}({receiver}, this: {this_type}, value: {rust}) -> Result<(), Exception>;");
                            let invoke = if script {
                                format!("<Api as {trait_name}>::set_{field}(context, this, value)?")
                            } else {
                                format!("host::with(|host| <Api as {trait_name}>::set_{field}(host, this, value))??")
                            };
                            let _ = writeln!(
                                functions,
                                "fn {module}__set_{field}{NATIVE} {{\n    convert::required(arguments, 1, \"{name}\")?;\n    let this = {this_convert};\n    let value = {converted};\n    {invoke};\n    convert::after(context)?;\n    Ok(JsValue::undefined())\n}}\n"
                            );
                            setter = format!("Some({module}__set_{field})");
                        }
                    }
                    let _ = writeln!(members, "        Member::Attribute {{ name: \"{name}\", get: {getter}, set: {setter} }},");
                    if *stringifier {
                        let _ = writeln!(members, "        Member::Method {{ name: \"toString\", native: {getter}, length: 0 }},");
                    }
                }
                Member::Operation { name, returns, arguments, is_static, extended } => {
                    let rust = context.rust(returns)?;
                    let function = snake(name);
                    let (this_type, this_convert) = context.this(definition, *is_static);
                    let mut parameters = String::new();
                    let mut conversions = String::new();
                    let mut names = String::new();
                    for (index, argument) in arguments.iter().enumerate() {
                        let variable = snake(&argument.name);
                        let _ = write!(parameters, ", {variable}: {}", context.argument_type(argument)?);
                        let _ = writeln!(conversions, "    let {variable} = {};", context.argument_value(index, argument)?);
                        let _ = write!(names, ", {variable}");
                    }
                    let script = extended_value(extended, "Script").is_some();
                    let receiver = if script { "context: &mut Context" } else { "host: &mut Host" };
                    let _ = writeln!(trait_body, "    fn {function}({receiver}, this: {this_type}{parameters}) -> Result<{rust}, Exception>;");
                    let native = if *is_static { format!("{module}__static_{function}") } else { format!("{module}__{function}") };
                    let required = arguments.iter().filter(|argument| !argument.optional && !argument.variadic).count();
                    let invoke = if script {
                        format!("<Api as {trait_name}>::{function}(context, this{names})?")
                    } else {
                        format!("host::with(|host| <Api as {trait_name}>::{function}(host, this{names}))??")
                    };
                    let _ = writeln!(
                        functions,
                        "fn {native}{NATIVE} {{\n    let this = {this_convert};\n    convert::required(arguments, {required}, \"{name}\")?;\n{conversions}    let result = {invoke};\n    convert::after(context)?;\n    convert::to_js(result, context)\n}}\n"
                    );
                    let kind = if *is_static { "Static" } else { "Method" };
                    let _ = writeln!(members, "        Member::{kind} {{ name: \"{name}\", native: {native}, length: {required} }},");
                }
            }
        }
        if !trait_body.is_empty() {
            let _ = writeln!(out, "pub trait {trait_name} {{\n{trait_body}}}\n");
        }
        out.push_str(&functions);
        if definition.kind == Kind::Namespace {
            let _ = writeln!(namespaces, "    NamespaceSpec {{ name: \"{}\", members: &[\n{members}    ] }},", definition.attribute("Alias").unwrap_or(&definition.name));
        } else {
            let _ = writeln!(
                specs,
                "    Spec {{ interface: Interface::{name}, name: \"{name}\", constructor: {constructor}, global: {global}, visible: {visible}, iterable: {iterable}, members: &[\n{members}    ] }},",
                name = definition.name,
                global = context.is_global(&definition.name),
                visible = definition.attribute("LegacyNoInterfaceObject").is_none(),
            );
        }
    }
    let _ = writeln!(out, "pub static SPECS: &[Spec] = &[\n{specs}];\n\npub static NAMESPACES: &[NamespaceSpec] = &[\n{namespaces}];\n");
    for code in context.unions.borrow().iter() {
        out.push_str(code);
    }
    Ok(out)
}
