#[allow(dead_code)]
mod convert;
mod dom;
mod engine;
mod exception;
mod host;
mod install;

#[allow(non_snake_case, dead_code, unused_variables, clippy::all)]
mod bindings {
    use web_dom::NodeId;

    use crate::convert::{self, Global};
    use crate::dom::Api;
    use crate::engine::{Context, JsObject, JsResult, JsValue};
    use crate::exception::Exception;
    use crate::host::{self, Host};
    use crate::install;

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

use web_dom::Document;

use crate::engine::{Context, JsObject, Source};
use crate::host::Host;

pub struct Script {
    context: Context,
    wrappers: Vec<Option<JsObject>>,
    prototypes: Vec<JsObject>,
    console: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub mutated: bool,
    pub error: Option<String>,
}

impl Script {
    pub fn new() -> Result<Script, String> {
        let mut context = Context::default();
        let prototypes = install::interfaces(&mut context).map_err(|error| error.to_string())?;
        Ok(Script {
            context,
            wrappers: Vec::new(),
            prototypes,
            console: Vec::new(),
        })
    }

    pub fn run(&mut self, document: &mut Document, source: &str) -> Outcome {
        host::enter(Host {
            document: std::mem::take(document),
            wrappers: std::mem::take(&mut self.wrappers),
            prototypes: std::mem::take(&mut self.prototypes),
            console: std::mem::take(&mut self.console),
            mutated: false,
        });
        let mut error = self
            .context
            .eval(Source::from_bytes(source))
            .err()
            .map(|error| error.to_string());
        if let Err(failed) = self.context.run_jobs() {
            error.get_or_insert_with(|| failed.to_string());
        }
        let mut outcome = Outcome {
            mutated: false,
            error,
        };
        if let Some(host) = host::leave() {
            *document = host.document;
            self.wrappers = host.wrappers;
            self.prototypes = host.prototypes;
            self.console = host.console;
            outcome.mutated = host.mutated;
        }
        outcome
    }

    pub fn take_console(&mut self) -> Vec<String> {
        std::mem::take(&mut self.console)
    }
}
