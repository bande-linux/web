use std::fmt::Write;

use crate::idl::{Argument, Definition, Error, Kind, Member, Type};

const NODE_ROOT: &str = "EventTarget";

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
        "type" | "self" | "match" | "ref" | "in" | "loop" => format!("{out}_"),
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
}

impl Context<'_> {
    fn is_node(&self, name: &str) -> bool {
        name == "Node"
            || self
                .ancestors(name)
                .iter()
                .any(|ancestor| ancestor == "Node")
    }

    fn ancestors(&self, name: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut current = self
            .definitions
            .iter()
            .find(|definition| definition.name == name)
            .and_then(|definition| definition.parent.clone());
        while let Some(parent) = current {
            current = self
                .definitions
                .iter()
                .find(|definition| definition.name == parent)
                .and_then(|definition| definition.parent.clone());
            found.push(parent);
        }
        found
    }

    fn is_global(&self, name: &str) -> bool {
        self.definitions.iter().any(|definition| {
            definition.name == name
                && definition
                    .extended
                    .iter()
                    .any(|attribute| attribute.starts_with("Global"))
        })
    }

    fn rust(&self, ty: &Type) -> Result<String, Error> {
        Ok(match ty {
            Type::Sequence { inner, nullable } => {
                let inner = format!("Vec<{}>", self.rust(inner)?);
                if *nullable {
                    format!("Option<{inner}>")
                } else {
                    inner
                }
            }
            Type::Named { name, nullable } => {
                let base = match name.as_str() {
                    "DOMString" | "USVString" => "String".to_owned(),
                    "boolean" => "bool".to_owned(),
                    "unsigned short" => "u16".to_owned(),
                    "short" => "i16".to_owned(),
                    "unsigned long" => "u32".to_owned(),
                    "long" => "i32".to_owned(),
                    "double" | "unrestricted double" => "f64".to_owned(),
                    "any" => "JsValue".to_owned(),
                    "undefined" => "()".to_owned(),
                    other if self.is_global(other) => "Global".to_owned(),
                    other if self.is_node(other) => "NodeId".to_owned(),
                    other => return Err(Error(format!("unsupported type {other}"))),
                };
                if *nullable {
                    format!("Option<{base}>")
                } else {
                    base
                }
            }
        })
    }

    fn convert(&self, ty: &Type, value: &str) -> Result<String, Error> {
        Ok(match ty {
            Type::Sequence { .. } => {
                return Err(Error("sequence arguments are not supported yet".into()));
            }
            Type::Named { name, nullable } => {
                let plain = match name.as_str() {
                    "DOMString" | "USVString" => format!("convert::string({value}, context)?"),
                    "boolean" => format!("convert::boolean({value})"),
                    "unsigned short" => format!("convert::unsigned_short({value}, context)?"),
                    "short" => format!("convert::short({value}, context)?"),
                    "unsigned long" => format!("convert::unsigned_long({value}, context)?"),
                    "long" => format!("convert::long({value}, context)?"),
                    "double" => format!("convert::double({value}, context)?"),
                    "unrestricted double" => {
                        format!("convert::unrestricted_double({value}, context)?")
                    }
                    "any" => format!("{value}.clone()"),
                    other if self.is_node(other) => {
                        format!("convert::node({value}, Interface::{other})?")
                    }
                    other => return Err(Error(format!("unsupported argument type {other}"))),
                };
                if *nullable {
                    let inner = self.convert(
                        &Type::Named {
                            name: name.clone(),
                            nullable: false,
                        },
                        "nullable",
                    )?;
                    format!(
                        "{{ let nullable: &JsValue = {value}; if nullable.is_null_or_undefined() {{ None }} else {{ Some({inner}) }} }}"
                    )
                } else {
                    plain
                }
            }
        })
    }

    fn this(&self, interface: &Definition) -> (&'static str, &'static str) {
        if interface.kind == Kind::Namespace || self.is_global(&interface.name) {
            ("()", "{ let _ = this; }")
        } else {
            ("NodeId", "convert::this_node(this, Interface::%)?")
        }
    }
}

fn argument_value(
    index: usize,
    argument: &Argument,
    context: &Context<'_>,
) -> Result<String, Error> {
    let value = format!("&convert::argument(arguments, {index})");
    if argument.variadic {
        return Ok(format!(
            "arguments.get({index}..).unwrap_or_default().to_vec()"
        ));
    }
    if argument.optional {
        let converted = context.convert(&argument.ty, "value")?;
        return Ok(match &argument.default {
            Some(default) => format!(
                "{{ let value = {value}; if value.is_undefined() {{ {default} }} else {{ {converted} }} }}"
            ),
            None => format!(
                "{{ let value = {value}; if value.is_undefined() {{ None }} else {{ Some({converted}) }} }}"
            ),
        });
    }
    context.convert(&argument.ty, &value)
}

fn argument_type(argument: &Argument, context: &Context<'_>) -> Result<String, Error> {
    let rust = context.rust(&argument.ty)?;
    Ok(if argument.variadic {
        "Vec<JsValue>".to_owned()
    } else if argument.optional && argument.default.is_none() {
        format!("Option<{rust}>")
    } else {
        rust
    })
}

pub fn generate(definitions: &[Definition]) -> Result<String, Error> {
    let context = Context { definitions };
    let mut out = String::new();
    let interfaces: Vec<&Definition> = definitions
        .iter()
        .filter(|definition| definition.kind == Kind::Interface)
        .collect();
    let push = |out: &mut String, text: &str| out.push_str(text);
    push(
        &mut out,
        "#[derive(Clone, Copy, PartialEq, Eq, Debug)]\npub enum Interface {\n",
    );
    for interface in &interfaces {
        let _ = writeln!(out, "    {},", interface.name);
    }
    push(
        &mut out,
        "}\n\nimpl Interface {\n    pub const ALL: &[Interface] = &[\n",
    );
    for interface in &interfaces {
        let _ = writeln!(out, "        Interface::{},", interface.name);
    }
    push(
        &mut out,
        "    ];\n\n    pub fn name(self) -> &'static str {\n        match self {\n",
    );
    for interface in &interfaces {
        let _ = writeln!(
            out,
            "            Interface::{0} => \"{0}\",",
            interface.name
        );
    }
    push(
        &mut out,
        "        }\n    }\n\n    pub fn parent(self) -> Option<Interface> {\n        match self {\n",
    );
    for interface in &interfaces {
        let parent = interface
            .parent
            .as_ref()
            .map_or("None".to_owned(), |parent| {
                format!("Some(Interface::{parent})")
            });
        let _ = writeln!(
            out,
            "            Interface::{} => {parent},",
            interface.name
        );
    }
    push(
        &mut out,
        "        }\n    }\n\n    pub fn is_node(self) -> bool {\n        matches!(self, ",
    );
    let nodes: Vec<String> = interfaces
        .iter()
        .filter(|interface| context.is_node(&interface.name) || interface.name == NODE_ROOT)
        .map(|interface| format!("Interface::{}", interface.name))
        .collect();
    out.push_str(&nodes.join(" | "));
    push(&mut out, ")\n    }\n}\n\n");

    for definition in definitions {
        let (this_type, this_convert) = context.this(definition);
        let this_convert = this_convert.replace('%', &definition.name);
        let module = snake(&definition.name);
        let trait_name = format!("{}Api", upper_first(&definition.name));
        let _ = writeln!(out, "pub trait {trait_name} {{");
        let mut functions = String::new();
        let mut installs = String::new();
        for member in &definition.members {
            match member {
                Member::Constant { name, value } => {
                    let _ = writeln!(
                        installs,
                        "    install::constant(target, constructor, \"{name}\", {value}, context)?;"
                    );
                }
                Member::Attribute { name, ty, readonly } => {
                    let rust = context.rust(ty)?;
                    let field = snake(name);
                    let _ = writeln!(
                        out,
                        "    fn {field}(host: &mut Host, this: {this_type}) -> Result<{rust}, Exception>;"
                    );
                    let getter = format!("{module}__get_{field}");
                    let _ = writeln!(
                        functions,
                        "fn {getter}(this: &JsValue, _arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {{\n    let this = {this_convert};\n    let result = host::with(|host| <Api as {iface}>::{field}(host, this))??;\n    convert::to_js(result, context)\n}}\n",
                        iface = trait_name,
                    );
                    let setter = if *readonly {
                        "None".to_owned()
                    } else {
                        let name = format!("{module}__set_{field}");
                        let converted = context.convert(ty, "&convert::argument(arguments, 0)")?;
                        let _ = writeln!(
                            out,
                            "    fn set_{field}(host: &mut Host, this: {this_type}, value: {rust}) -> Result<(), Exception>;"
                        );
                        let _ = writeln!(
                            functions,
                            "fn {name}(this: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {{\n    let this = {this_convert};\n    let value = {converted};\n    host::with(|host| <Api as {iface}>::set_{field}(host, this, value))??;\n    Ok(JsValue::undefined())\n}}\n",
                            iface = trait_name,
                        );
                        format!("Some({name})")
                    };
                    let _ = writeln!(
                        installs,
                        "    install::attribute(target, \"{name}\", {getter}, {setter}, context)?;"
                    );
                }
                Member::Operation {
                    name,
                    returns,
                    arguments,
                } => {
                    let rust = context.rust(returns)?;
                    let function = snake(name);
                    let mut parameters = String::new();
                    let mut conversions = String::new();
                    let mut names = String::new();
                    for (index, argument) in arguments.iter().enumerate() {
                        let variable = snake(&argument.name);
                        let _ = write!(
                            parameters,
                            ", {variable}: {}",
                            argument_type(argument, &context)?
                        );
                        let _ = writeln!(
                            conversions,
                            "    let {variable} = {};",
                            argument_value(index, argument, &context)?
                        );
                        let _ = write!(names, ", {variable}");
                    }
                    let _ = writeln!(
                        out,
                        "    fn {function}(host: &mut Host, this: {this_type}{parameters}) -> Result<{rust}, Exception>;"
                    );
                    let native = format!("{module}__{function}");
                    let unused = if arguments.is_empty() { "_" } else { "" };
                    let _ = writeln!(
                        functions,
                        "fn {native}(this: &JsValue, {unused}arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {{\n    let this = {this_convert};\n{conversions}    let result = host::with(|host| <Api as {iface}>::{function}(host, this{names}))??;\n    convert::to_js(result, context)\n}}\n",
                        iface = trait_name,
                    );
                    let length = arguments
                        .iter()
                        .filter(|argument| !argument.optional && !argument.variadic)
                        .count();
                    let _ = writeln!(
                        installs,
                        "    install::method(target, \"{name}\", {native}, {length}, context)?;"
                    );
                }
            }
        }
        out.push_str("}\n\n");
        out.push_str(&functions);
        let _ = writeln!(
            out,
            "pub fn install_{module}(target: &JsObject, constructor: Option<&JsObject>, context: &mut Context) -> JsResult<()> {{\n    let _ = (target, constructor, &context);\n{installs}    Ok(())\n}}\n"
        );
    }

    out.push_str("pub fn install_members(interface: Interface, target: &JsObject, constructor: Option<&JsObject>, context: &mut Context) -> JsResult<()> {\n    match interface {\n");
    for interface in &interfaces {
        let _ = writeln!(
            out,
            "        Interface::{} => install_{}(target, constructor, context),",
            interface.name,
            snake(&interface.name)
        );
    }
    out.push_str("    }\n}\n\npub fn install_namespaces(global: &JsObject, context: &mut Context) -> JsResult<()> {\n");
    for namespace in definitions
        .iter()
        .filter(|definition| definition.kind == Kind::Namespace)
    {
        let _ = writeln!(
            out,
            "    let namespace = install::namespace(global, \"{}\", context)?;\n    install_{}(&namespace, None, context)?;",
            namespace.name,
            snake(&namespace.name)
        );
    }
    out.push_str("    Ok(())\n}\n");
    Ok(out)
}
