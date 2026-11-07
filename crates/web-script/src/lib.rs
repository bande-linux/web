mod convert;
mod dom;
mod engine;
mod event_loop;
mod events;
mod exception;
mod host;
mod install;
mod reflect;
mod window;

#[allow(non_snake_case, dead_code, unused_variables, unused_imports, clippy::all)]
mod bindings {
    use web_dom::NodeId;

    use crate::convert::{self, Global};
    use crate::dom::Api;
    use crate::engine::{Context, JsNativeError, JsObject, JsResult, JsString, JsValue};
    use crate::exception::Exception;
    use crate::host::{self, Host};
    use crate::install::{Member, NamespaceSpec, Spec};

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use event_loop::{Click, Config, Outcome, Script};
pub use host::Viewport;
