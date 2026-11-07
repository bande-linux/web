use web_dom::NodeId;

use crate::bindings::Interface;
use crate::dom::collections::Collection;
use crate::engine::{
    Context, Finalize, JsArray, JsData, JsNativeError, JsObject, JsResult, JsString, JsValue, Trace,
};
use crate::events::{EventState, Listener};
use crate::host;

#[derive(Trace, Finalize, JsData)]
pub struct Platform {
    #[unsafe_ignore_trace]
    pub interface: Interface,
    pub payload: Payload,
    pub listeners: Option<Box<Vec<Listener>>>,
}

impl Platform {
    pub fn new(interface: Interface, payload: Payload) -> Self {
        Platform {
            interface,
            payload,
            listeners: None,
        }
    }
}

#[derive(Trace, Finalize)]
pub enum Payload {
    Plain,
    Node(#[unsafe_ignore_trace] NodeId),
    Event(Box<EventState>),
    Exception {
        name: String,
        message: String,
    },
    Collection(Box<Collection>),
    Controller(JsObject),
    Signal(Box<crate::dom::abort::Signal>),
    Range(#[unsafe_ignore_trace] std::rc::Rc<std::cell::RefCell<crate::dom::ranges::RangeState>>),
    Static(#[unsafe_ignore_trace] crate::dom::ranges::RangeState),
    Traversal(Box<crate::dom::traversal::Traversal>),
    Observer(#[unsafe_ignore_trace] u64),
    Record(Box<crate::dom::observer::Record>),
    Implementation(#[unsafe_ignore_trace] NodeId),
}

pub struct Global;

pub fn argument(arguments: &[JsValue], index: usize) -> JsValue {
    arguments.get(index).cloned().unwrap_or_default()
}

pub fn required(arguments: &[JsValue], count: usize, name: &str) -> JsResult<()> {
    if arguments.len() < count {
        return Err(JsNativeError::typ()
            .with_message(format!(
                "{name}: {count} argument(s) required, {} present",
                arguments.len()
            ))
            .into());
    }
    Ok(())
}

pub fn after(context: &mut Context) -> JsResult<()> {
    if host::pending() {
        crate::event_loop::settle_pending(context)?;
    }
    Ok(())
}

pub fn js_to_string(string: &JsString) -> String {
    string.to_std_string_lossy()
}

pub fn string(value: &JsValue, context: &mut Context) -> JsResult<String> {
    Ok(js_to_string(&value.to_string(context)?))
}

pub fn string_or_empty(value: &JsValue, context: &mut Context) -> JsResult<String> {
    if value.is_null() {
        return Ok(String::new());
    }
    string(value, context)
}

pub fn boolean(value: &JsValue) -> bool {
    value.to_boolean()
}

pub fn unsigned_short(value: &JsValue, context: &mut Context) -> JsResult<u16> {
    value.to_uint16(context)
}

pub fn short(value: &JsValue, context: &mut Context) -> JsResult<i16> {
    value.to_int16(context)
}

pub fn unsigned_long(value: &JsValue, context: &mut Context) -> JsResult<u32> {
    value.to_u32(context)
}

pub fn long(value: &JsValue, context: &mut Context) -> JsResult<i32> {
    value.to_i32(context)
}

pub fn long_long(value: &JsValue, context: &mut Context) -> JsResult<i64> {
    let number = value.to_number(context)?;
    Ok(if number.is_finite() { number as i64 } else { 0 })
}

pub fn unrestricted_double(value: &JsValue, context: &mut Context) -> JsResult<f64> {
    value.to_number(context)
}

pub fn double(value: &JsValue, context: &mut Context) -> JsResult<f64> {
    let number = value.to_number(context)?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(JsNativeError::typ()
            .with_message("the value is not a finite number")
            .into())
    }
}

pub fn any_object(value: &JsValue) -> JsResult<JsObject> {
    value.as_object().map(|object| object.clone()).ok_or_else(|| {
        JsNativeError::typ()
            .with_message("the value is not an object")
            .into()
    })
}

pub fn callback(value: &JsValue) -> JsResult<JsObject> {
    match value.as_object() {
        Some(object) if object.is_callable() => Ok(object.clone()),
        _ => Err(JsNativeError::typ()
            .with_message("the value is not callable")
            .into()),
    }
}

pub fn sequence<T>(
    value: &JsValue,
    context: &mut Context,
    mut convert: impl FnMut(&JsValue, &mut Context) -> JsResult<T>,
) -> JsResult<Vec<T>> {
    let object = any_object(value)?;
    let iterator_method = object.get(boa_engine::JsSymbol::iterator(), context)?;
    if !iterator_method.is_callable() {
        return Err(JsNativeError::typ()
            .with_message("the value is not iterable")
            .into());
    }
    let mut records = value.get_iterator(boa_engine::builtins::iterable::IteratorHint::Sync, context)?;
    let mut items = Vec::new();
    while let Some(item) = records.step_value(context)? {
        items.push(convert(&item, context)?);
    }
    Ok(items)
}

pub fn implements_interface(known: Interface, wanted: Interface) -> bool {
    let mut current = Some(known);
    while let Some(interface) = current {
        if interface == wanted {
            return true;
        }
        current = interface.parent();
    }
    false
}

pub fn implements(value: &JsValue, wanted: Interface) -> bool {
    value
        .as_object()
        .and_then(|object| {
            object
                .downcast_ref::<Platform>()
                .map(|platform| implements_interface(platform.interface, wanted))
        })
        .unwrap_or(false)
}

pub fn node(value: &JsValue, interface: Interface) -> JsResult<NodeId> {
    node_of(value, interface).ok_or_else(|| {
        JsNativeError::typ()
            .with_message(format!("the value is not a {}", interface.name()))
            .into()
    })
}

pub fn node_of(value: &JsValue, interface: Interface) -> Option<NodeId> {
    let object = value.as_object()?;
    let platform = object.downcast_ref::<Platform>()?;
    match platform.payload {
        Payload::Node(node) if implements_interface(platform.interface, interface) => Some(node),
        _ => None,
    }
}

pub fn this_node(this: &JsValue, interface: Interface) -> JsResult<NodeId> {
    node_of(this, interface).ok_or_else(|| {
        JsNativeError::typ()
            .with_message("Illegal invocation")
            .into()
    })
}

pub fn object(value: &JsValue, interface: Interface) -> JsResult<JsObject> {
    if implements(value, interface) {
        any_object(value)
    } else if interface == Interface::EventTarget
        && value.as_object().is_some_and(|object| host::is_global(&object))
    {
        any_object(value)
    } else {
        Err(JsNativeError::typ()
            .with_message(format!("the value is not a {}", interface.name()))
            .into())
    }
}

pub fn this_object(this: &JsValue, interface: Interface, context: &mut Context) -> JsResult<JsObject> {
    let illegal = || -> boa_engine::JsError {
        JsNativeError::typ()
            .with_message("Illegal invocation")
            .into()
    };
    let Some(object) = this.as_object().map(|object| object.clone()) else {
        if this.is_null_or_undefined() && interface == Interface::EventTarget {
            return Ok(context.global_object());
        }
        return Err(illegal());
    };
    if let Some(platform) = object.downcast_ref::<Platform>() {
        return if implements_interface(platform.interface, interface) {
            drop(platform);
            Ok(object)
        } else {
            Err(illegal())
        };
    }
    if interface == Interface::EventTarget && host::is_global(&object) {
        return Ok(object);
    }
    if let Some(target) = crate::dom::collections::proxy_target(&object, context)?
        && implements(&JsValue::from(target.clone()), interface)
    {
        return Ok(target);
    }
    Err(illegal())
}

pub fn new_target_prototype(
    new_target: &JsValue,
    interface: Interface,
    context: &mut Context,
) -> JsResult<JsObject> {
    let Some(target) = new_target.as_object() else {
        return Err(JsNativeError::typ()
            .with_message(format!(
                "{} must be called with new",
                interface.name()
            ))
            .into());
    };
    if let Some(prototype) = target.get(boa_engine::js_string!("prototype"), context)?.as_object() {
        return Ok(prototype.clone());
    }
    host::with(|host| host.prototype(interface))?.ok_or_else(|| {
        JsNativeError::error()
            .with_message("no prototype")
            .into()
    })
}

pub fn put_forward(
    this: &JsValue,
    attribute: &str,
    forward: &str,
    value: &JsValue,
    context: &mut Context,
) -> JsResult<JsValue> {
    let object = any_object(this)?;
    let target = object.get(JsString::from(attribute), context)?;
    let target = any_object(&target)?;
    target.set(JsString::from(forward), value.clone(), true, context)?;
    Ok(JsValue::undefined())
}

pub fn wrap(node: NodeId) -> JsResult<JsObject> {
    host::with(|host| host.wrap(node))?.ok_or_else(|| {
        JsNativeError::error()
            .with_message("the node has no interface")
            .into()
    })
}

pub fn wrap_with_prototype(node: NodeId, prototype: JsObject) -> JsResult<JsObject> {
    host::with(|host| host.wrap_with_prototype(node, prototype))?.ok_or_else(|| {
        JsNativeError::error()
            .with_message("the node has no interface")
            .into()
    })
}

pub trait ToJs {
    fn to_js(self, context: &mut Context) -> JsResult<JsValue>;
}

pub fn to_js<T: ToJs>(value: T, context: &mut Context) -> JsResult<JsValue> {
    value.to_js(context)
}

impl ToJs for () {
    fn to_js(self, _context: &mut Context) -> JsResult<JsValue> {
        Ok(JsValue::undefined())
    }
}

impl ToJs for bool {
    fn to_js(self, _context: &mut Context) -> JsResult<JsValue> {
        Ok(JsValue::from(self))
    }
}

macro_rules! number {
    ($($ty:ty),*) => {
        $(impl ToJs for $ty {
            fn to_js(self, _context: &mut Context) -> JsResult<JsValue> {
                Ok(JsValue::from(self))
            }
        })*
    };
}

number!(u16, i16, u32, i32, f64);

impl ToJs for String {
    fn to_js(self, _context: &mut Context) -> JsResult<JsValue> {
        Ok(crate::engine::text(&self))
    }
}

impl ToJs for JsValue {
    fn to_js(self, _context: &mut Context) -> JsResult<JsValue> {
        Ok(self)
    }
}

impl ToJs for JsObject {
    fn to_js(self, _context: &mut Context) -> JsResult<JsValue> {
        Ok(JsValue::from(self))
    }
}

impl ToJs for Global {
    fn to_js(self, context: &mut Context) -> JsResult<JsValue> {
        Ok(JsValue::from(context.global_object()))
    }
}

impl ToJs for NodeId {
    fn to_js(self, _context: &mut Context) -> JsResult<JsValue> {
        Ok(JsValue::from(wrap(self)?))
    }
}

impl<T: ToJs> ToJs for Option<T> {
    fn to_js(self, context: &mut Context) -> JsResult<JsValue> {
        match self {
            Some(value) => value.to_js(context),
            None => Ok(JsValue::null()),
        }
    }
}

impl<T: ToJs> ToJs for Vec<T> {
    fn to_js(self, context: &mut Context) -> JsResult<JsValue> {
        let values = self
            .into_iter()
            .map(|value| value.to_js(context))
            .collect::<JsResult<Vec<JsValue>>>()?;
        Ok(JsValue::from(JsArray::from_iter(values, context)))
    }
}
