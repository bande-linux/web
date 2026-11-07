use crate::bindings::Interface;
use crate::convert::{Payload, Platform};
use crate::engine::{JsError, JsNativeError, JsObject, JsValue};
use crate::host;

#[derive(Debug)]
pub enum Exception {
    Dom(&'static str, String),
    Type(String),
    Range(String),
    Js(JsError),
}

impl Exception {
    pub fn dom(name: &'static str, message: impl Into<String>) -> Self {
        Exception::Dom(name, message.into())
    }

    pub fn type_error(message: impl Into<String>) -> Self {
        Exception::Type(message.into())
    }
}

impl From<JsError> for Exception {
    fn from(error: JsError) -> Self {
        Exception::Js(error)
    }
}

pub fn code_of(name: &str) -> u16 {
    match name {
        "IndexSizeError" => 1,
        "HierarchyRequestError" => 3,
        "WrongDocumentError" => 4,
        "InvalidCharacterError" => 5,
        "NoModificationAllowedError" => 7,
        "NotFoundError" => 8,
        "NotSupportedError" => 9,
        "InUseAttributeError" => 10,
        "InvalidStateError" => 11,
        "SyntaxError" => 12,
        "InvalidModificationError" => 13,
        "NamespaceError" => 14,
        "InvalidAccessError" => 15,
        "TypeMismatchError" => 17,
        "SecurityError" => 18,
        "NetworkError" => 19,
        "AbortError" => 20,
        "URLMismatchError" => 21,
        "QuotaExceededError" => 22,
        "TimeoutError" => 23,
        "InvalidNodeTypeError" => 24,
        "DataCloneError" => 25,
        _ => 0,
    }
}

pub fn dom_exception(name: &str, message: &str) -> Option<JsObject> {
    let prototype = host::with(|host| host.prototype(Interface::DOMException))
        .ok()
        .flatten()?;
    Some(JsObject::from_proto_and_data(
        prototype,
        Platform::new(
            Interface::DOMException,
            Payload::Exception {
                name: name.to_owned(),
                message: message.to_owned(),
            },
        ),
    ))
}

impl From<Exception> for JsError {
    fn from(exception: Exception) -> Self {
        match exception {
            Exception::Dom(name, message) => match dom_exception(name, &message) {
                Some(object) => JsError::from_opaque(JsValue::from(object)),
                None => JsNativeError::error()
                    .with_message(format!("{name}: {message}"))
                    .into(),
            },
            Exception::Type(message) => JsNativeError::typ().with_message(message).into(),
            Exception::Range(message) => JsNativeError::range().with_message(message).into(),
            Exception::Js(error) => error,
        }
    }
}
