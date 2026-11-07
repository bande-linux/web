pub use boa_engine::builtins::promise::OperationType;
pub use boa_engine::context::HostHooks;
pub use boa_engine::job::{Job, NativeJob};
pub use boa_engine::object::builtins::JsArray;
pub use boa_engine::object::{FunctionObjectBuilder, JsObject};
pub use boa_engine::property::{PropertyDescriptor, PropertyKey};
pub use boa_engine::{
    Context, JsData, JsError, JsNativeError, JsResult, JsString, JsSymbol, JsValue, NativeFunction,
    Source, js_string,
};
pub use boa_gc::{Finalize, Trace};

pub type Native = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;

pub fn text(value: &str) -> JsValue {
    JsValue::from(JsString::from(value))
}

pub fn property(object: &JsObject, name: &str, context: &mut Context) -> JsResult<JsValue> {
    object.get(JsString::from(name), context)
}
