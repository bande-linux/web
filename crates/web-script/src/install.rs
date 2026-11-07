use crate::bindings::{Interface, NAMESPACES, SPECS};
use crate::engine::{
    Context, FunctionObjectBuilder, JsNativeError, JsObject, JsResult, JsString, JsSymbol, JsValue,
    Native, NativeFunction, PropertyDescriptor,
};

pub enum Member {
    Constant {
        name: &'static str,
        value: f64,
    },
    Attribute {
        name: &'static str,
        get: Native,
        set: Option<Native>,
    },
    Method {
        name: &'static str,
        native: Native,
        length: usize,
    },
    Static {
        name: &'static str,
        native: Native,
        length: usize,
    },
}

pub struct Spec {
    pub interface: Interface,
    pub name: &'static str,
    pub constructor: Option<(Native, usize)>,
    pub global: bool,
    pub visible: bool,
    pub iterable: bool,
    pub members: &'static [Member],
}

pub struct NamespaceSpec {
    pub name: &'static str,
    pub members: &'static [Member],
}

fn function(name: &str, native: Native, length: usize, context: &mut Context) -> JsObject {
    FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(native))
        .name(JsString::from(name))
        .length(length)
        .build()
        .into()
}

fn attribute(
    target: &JsObject,
    name: &str,
    getter: Native,
    setter: Option<Native>,
    context: &mut Context,
) -> JsResult<()> {
    let get = function(&format!("get {name}"), getter, 0, context);
    let mut descriptor = PropertyDescriptor::builder()
        .get(get)
        .enumerable(true)
        .configurable(true);
    descriptor = match setter {
        Some(setter) => descriptor.set(function(&format!("set {name}"), setter, 1, context)),
        None => descriptor.set(JsValue::undefined()),
    };
    target.define_property_or_throw(JsString::from(name), descriptor.build(), context)?;
    Ok(())
}

fn data(
    target: &JsObject,
    key: impl Into<crate::engine::PropertyKey>,
    value: JsValue,
    writable: bool,
    enumerable: bool,
    configurable: bool,
    context: &mut Context,
) -> JsResult<()> {
    target.define_property_or_throw(
        key,
        PropertyDescriptor::builder()
            .value(value)
            .writable(writable)
            .enumerable(enumerable)
            .configurable(configurable)
            .build(),
        context,
    )?;
    Ok(())
}

fn members(
    target: &JsObject,
    constructor: Option<&JsObject>,
    list: &[Member],
    context: &mut Context,
) -> JsResult<()> {
    for member in list {
        match member {
            Member::Constant { name, value } => {
                for object in std::iter::once(target).chain(constructor) {
                    data(object, JsString::from(*name), JsValue::from(*value), false, true, false, context)?;
                }
            }
            Member::Attribute { name, get, set } => attribute(target, name, *get, *set, context)?,
            Member::Method { name, native, length } => {
                let value = function(name, *native, *length, context);
                data(target, JsString::from(*name), JsValue::from(value), true, true, true, context)?;
            }
            Member::Static { name, native, length } => {
                let value = function(name, *native, *length, context);
                if let Some(constructor) = constructor {
                    data(constructor, JsString::from(*name), JsValue::from(value), true, true, true, context)?;
                }
            }
        }
    }
    Ok(())
}

fn illegal(_this: &JsValue, _arguments: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    Err(JsNativeError::typ()
        .with_message("Illegal constructor")
        .into())
}

fn array_method(name: &str, context: &mut Context) -> JsResult<JsValue> {
    let prototype = context.intrinsics().constructors().array().prototype();
    prototype.get(JsString::from(name), context)
}

pub fn interfaces(context: &mut Context) -> JsResult<Vec<JsObject>> {
    let global = context.global_object();
    let object_prototype = context.intrinsics().constructors().object().prototype();
    let mut prototypes = Vec::with_capacity(SPECS.len());
    let mut constructors: Vec<JsObject> = Vec::with_capacity(SPECS.len());
    for spec in SPECS {
        let (native, length) = spec.constructor.unwrap_or((illegal, 0));
        let constructor: JsObject =
            FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(native))
                .name(JsString::from(spec.name))
                .length(length)
                .constructor(true)
                .build()
                .into();
        let prototype = JsObject::with_null_proto();
        data(&constructor, crate::engine::js_string!("prototype"), JsValue::from(prototype.clone()), false, false, false, context)?;
        data(&prototype, crate::engine::js_string!("constructor"), JsValue::from(constructor.clone()), true, false, true, context)?;
        data(&prototype, JsSymbol::to_string_tag(), crate::engine::text(spec.name), false, false, true, context)?;
        if spec.visible {
            data(&global, JsString::from(spec.name), JsValue::from(constructor.clone()), true, false, true, context)?;
        }
        prototypes.push(prototype);
        constructors.push(constructor);
    }
    for (index, spec) in SPECS.iter().enumerate() {
        let parent = spec.interface.parent().map(|parent| parent as usize);
        let prototype = prototypes.get(index).cloned();
        let constructor = constructors.get(index).cloned();
        let (Some(prototype), Some(constructor)) = (prototype, constructor) else {
            continue;
        };
        let parent_prototype = parent.and_then(|parent| prototypes.get(parent).cloned());
        let is_exception = spec.interface == Interface::DOMException;
        let chained = if is_exception {
            context.intrinsics().constructors().error().prototype()
        } else {
            parent_prototype.unwrap_or_else(|| object_prototype.clone())
        };
        prototype.set_prototype(Some(chained));
        if let Some(parent) = parent.and_then(|parent| constructors.get(parent).cloned()) {
            constructor.set_prototype(Some(parent));
        }
        if spec.global {
            global.set_prototype(Some(prototype.clone()));
            members(&global, Some(&constructor), spec.members, context)?;
        } else {
            members(&prototype, Some(&constructor), spec.members, context)?;
        }
        if spec.iterable {
            let values = array_method("values", context)?;
            data(&prototype, JsSymbol::iterator(), values.clone(), true, false, true, context)?;
            data(&prototype, crate::engine::js_string!("values"), values, true, true, true, context)?;
            for name in ["keys", "entries", "forEach"] {
                let value = array_method(name, context)?;
                data(&prototype, JsString::from(name), value, true, true, true, context)?;
            }
        }
    }
    for namespace in NAMESPACES {
        let object = JsObject::with_object_proto(context.intrinsics());
        data(&global, JsString::from(namespace.name), JsValue::from(object.clone()), true, false, true, context)?;
        members(&object, None, namespace.members, context)?;
    }
    Ok(prototypes)
}
