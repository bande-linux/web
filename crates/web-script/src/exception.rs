use crate::engine::{JsError, JsNativeError};

#[derive(Debug)]
pub enum Exception {
    Dom(&'static str, String),
    Type(String),
}

impl Exception {
    pub fn dom(name: &'static str, message: impl Into<String>) -> Self {
        Exception::Dom(name, message.into())
    }
}

impl From<Exception> for JsError {
    fn from(exception: Exception) -> Self {
        match exception {
            Exception::Dom(name, message) => JsNativeError::error()
                .with_message(format!("{name}: {message}"))
                .into(),
            Exception::Type(message) => JsNativeError::typ().with_message(message).into(),
        }
    }
}
