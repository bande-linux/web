pub use boa_engine::object::{FunctionObjectBuilder, JsObject};
pub use boa_engine::property::PropertyDescriptor;
pub use boa_engine::{
    Context, JsData, JsError, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source,
};
pub use boa_gc::{Finalize, Trace};

pub type Native = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;
