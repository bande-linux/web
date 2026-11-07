use crate::bindings::{Interface, install_members, install_namespaces};
use crate::engine::{
    Context, FunctionObjectBuilder, JsNativeError, JsObject, JsResult, JsString, JsValue, Native,
    NativeFunction, PropertyDescriptor,
};

fn function(name: &str, native: Native, length: usize, context: &mut Context) -> JsObject {
    FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(native))
        .name(JsString::from(name))
        .length(length)
        .build()
        .into()
}

pub fn attribute(
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

pub fn method(
    target: &JsObject,
    name: &str,
    native: Native,
    length: usize,
    context: &mut Context,
) -> JsResult<()> {
    let value = function(name, native, length, context);
    target.define_property_or_throw(
        JsString::from(name),
        PropertyDescriptor::builder()
            .value(value)
            .writable(true)
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    )?;
    Ok(())
}

pub fn constant(
    target: &JsObject,
    constructor: Option<&JsObject>,
    name: &str,
    value: i32,
    context: &mut Context,
) -> JsResult<()> {
    for object in std::iter::once(target).chain(constructor) {
        object.define_property_or_throw(
            JsString::from(name),
            PropertyDescriptor::builder()
                .value(value)
                .writable(false)
                .enumerable(true)
                .configurable(false)
                .build(),
            context,
        )?;
    }
    Ok(())
}

pub fn namespace(global: &JsObject, name: &str, context: &mut Context) -> JsResult<JsObject> {
    let object = JsObject::with_object_proto(context.intrinsics());
    hidden(global, name, JsValue::from(object.clone()), context)?;
    Ok(object)
}

fn hidden(target: &JsObject, name: &str, value: JsValue, context: &mut Context) -> JsResult<()> {
    target.define_property_or_throw(
        JsString::from(name),
        PropertyDescriptor::builder()
            .value(value)
            .writable(true)
            .enumerable(false)
            .configurable(true)
            .build(),
        context,
    )?;
    Ok(())
}

fn illegal(_this: &JsValue, _arguments: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    Err(JsNativeError::typ()
        .with_message("Illegal constructor")
        .into())
}

pub fn interfaces(context: &mut Context) -> JsResult<Vec<JsObject>> {
    let global = context.global_object();
    let object_prototype = context.intrinsics().constructors().object().prototype();
    let mut prototypes: Vec<JsObject> = Vec::with_capacity(Interface::ALL.len());
    for &interface in Interface::ALL {
        let parent = interface
            .parent()
            .and_then(|parent| Interface::ALL.iter().position(|known| *known == parent))
            .and_then(|index| prototypes.get(index).cloned())
            .unwrap_or_else(|| object_prototype.clone());
        let prototype = JsObject::with_null_proto();
        prototype.set_prototype(Some(parent));
        let constructor =
            FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(illegal))
                .name(JsString::from(interface.name()))
                .length(0)
                .constructor(true)
                .build();
        let constructor: JsObject = constructor.into();
        constructor.define_property_or_throw(
            JsString::from("prototype"),
            PropertyDescriptor::builder()
                .value(prototype.clone())
                .writable(false)
                .enumerable(false)
                .configurable(false)
                .build(),
            context,
        )?;
        hidden(
            &prototype,
            "constructor",
            JsValue::from(constructor.clone()),
            context,
        )?;
        hidden(
            &global,
            interface.name(),
            JsValue::from(constructor.clone()),
            context,
        )?;
        if interface == Interface::Window {
            global.set_prototype(Some(prototype.clone()));
            install_members(interface, &global, Some(&constructor), context)?;
        } else {
            install_members(interface, &prototype, Some(&constructor), context)?;
        }
        prototypes.push(prototype);
    }
    install_namespaces(&global, context)?;
    Ok(prototypes)
}
