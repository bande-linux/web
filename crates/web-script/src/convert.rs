use boa_engine::object::builtins::JsArray;
use web_dom::NodeId;

use crate::bindings::Interface;
use crate::dom::implements;
use crate::engine::{Context, Finalize, JsData, JsNativeError, JsObject, JsResult, JsValue, Trace};
use crate::host;

#[derive(Trace, Finalize, JsData)]
pub struct Wrapper {
    #[unsafe_ignore_trace]
    pub node: NodeId,
}

pub struct Global;

pub fn argument(arguments: &[JsValue], index: usize) -> JsValue {
    arguments.get(index).cloned().unwrap_or_default()
}

pub fn string(value: &JsValue, context: &mut Context) -> JsResult<String> {
    Ok(value.to_string(context)?.to_std_string_escaped())
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

fn wrapped(value: &JsValue) -> Option<NodeId> {
    let object = value.as_object()?;
    let wrapper = object.downcast_ref::<Wrapper>()?;
    Some(wrapper.node)
}

pub fn node(value: &JsValue, interface: Interface) -> JsResult<NodeId> {
    let node = wrapped(value).ok_or_else(|| {
        JsNativeError::typ().with_message(format!("the value is not a {}", interface.name()))
    })?;
    if host::with(|host| implements(&host.document, node, interface))? {
        Ok(node)
    } else {
        Err(JsNativeError::typ()
            .with_message(format!("the value is not a {}", interface.name()))
            .into())
    }
}

pub fn this_node(this: &JsValue, interface: Interface) -> JsResult<NodeId> {
    node(this, interface).map_err(|_| {
        JsNativeError::typ()
            .with_message("Illegal invocation")
            .into()
    })
}

pub fn wrap(node: NodeId, context: &mut Context) -> JsResult<JsObject> {
    let _ = context;
    host::with(|host| {
        if let Some(wrapper) = host.wrapper(node) {
            return Some(wrapper.clone());
        }
        let interface = crate::dom::interface_of(&host.document, node)?;
        let prototype = host.prototype(interface)?.clone();
        let object = JsObject::from_proto_and_data(prototype, Wrapper { node });
        let index = node.index();
        if host.wrappers.len() <= index {
            host.wrappers.resize(index + 1, None);
        }
        if let Some(slot) = host.wrappers.get_mut(index) {
            *slot = Some(object.clone());
        }
        Some(object)
    })?
    .ok_or_else(|| {
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
        Ok(JsValue::from(crate::engine::JsString::from(self.as_str())))
    }
}

impl ToJs for JsValue {
    fn to_js(self, _context: &mut Context) -> JsResult<JsValue> {
        Ok(self)
    }
}

impl ToJs for Global {
    fn to_js(self, context: &mut Context) -> JsResult<JsValue> {
        Ok(JsValue::from(context.global_object()))
    }
}

impl ToJs for NodeId {
    fn to_js(self, context: &mut Context) -> JsResult<JsValue> {
        Ok(JsValue::from(wrap(self, context)?))
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
