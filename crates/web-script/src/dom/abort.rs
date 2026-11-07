use crate::bindings::*;
use crate::convert::{Payload, Platform};
use crate::dom::Api;
use crate::dom::tree::Result;
use crate::engine::{Context, Finalize, JsObject, JsValue, Trace};
use crate::exception::{Exception, dom_exception};
use crate::host;

#[derive(Trace, Finalize, Default)]
pub struct Signal {
    pub aborted: bool,
    pub reason: Option<JsValue>,
    pub dependents: Vec<JsObject>,
    pub algorithms: Vec<JsObject>,
}

fn new_signal(prototype: JsObject) -> JsObject {
    JsObject::from_proto_and_data(
        prototype,
        Platform::new(
            Interface::AbortSignal,
            Payload::Signal(Box::default()),
        ),
    )
}

fn signal_prototype() -> Result<JsObject> {
    host::with(|host| host.prototype(Interface::AbortSignal))?
        .ok_or_else(|| Exception::type_error("no realm"))
}

fn with_signal<R>(object: &JsObject, f: impl FnOnce(&mut Signal) -> R) -> Option<R> {
    let mut platform = object.downcast_mut::<Platform>()?;
    match &mut platform.payload {
        Payload::Signal(signal) => Some(f(signal)),
        _ => None,
    }
}

fn abort_reason(reason: Option<JsValue>) -> JsValue {
    match reason.filter(|reason| !reason.is_undefined()) {
        Some(reason) => reason,
        None => dom_exception("AbortError", "signal is aborted without reason")
            .map_or_else(JsValue::undefined, JsValue::from),
    }
}

pub fn signal_abort(context: &mut Context, signal: &JsObject, reason: JsValue) -> Result<()> {
    if with_signal(signal, |state| state.aborted) != Some(false) {
        return Ok(());
    }
    let dependents = with_signal(signal, |state| {
        state.aborted = true;
        state.reason = Some(reason.clone());
        std::mem::take(&mut state.dependents)
    })
    .unwrap_or_default();
    let mut to_fire = vec![signal.clone()];
    for dependent in dependents {
        if with_signal(&dependent, |state| {
            let fresh = !state.aborted;
            if fresh {
                state.aborted = true;
                state.reason = Some(reason.clone());
            }
            fresh
        }) == Some(true)
        {
            to_fire.push(dependent);
        }
    }
    for target in to_fire {
        crate::events::fire_simple(context, &target, "abort")?;
    }
    Ok(())
}

pub fn is_aborted(signal: &JsObject) -> bool {
    with_signal(signal, |state| state.aborted).unwrap_or(false)
}

pub fn fire_timeout(context: &mut Context, signal: &JsObject) -> Result<()> {
    let reason = dom_exception("TimeoutError", "the signal timed out")
        .map_or_else(JsValue::undefined, JsValue::from);
    signal_abort(context, signal, reason)
}

impl AbortControllerApi for Api {
    fn constructor(_context: &mut Context, prototype: JsObject) -> Result<JsObject> {
        let signal = new_signal(signal_prototype()?);
        Ok(JsObject::from_proto_and_data(
            prototype,
            Platform::new(Interface::AbortController, Payload::Controller(signal)),
        ))
    }

    fn signal(_context: &mut Context, this: JsObject) -> Result<JsObject> {
        let platform = this.downcast_ref::<Platform>();
        match platform.as_deref().map(|platform| &platform.payload) {
            Some(Payload::Controller(signal)) => Ok(signal.clone()),
            _ => Err(Exception::type_error("Illegal invocation")),
        }
    }

    fn abort(context: &mut Context, this: JsObject, reason: Option<JsValue>) -> Result<()> {
        let signal = <Api as AbortControllerApi>::signal(context, this)?;
        signal_abort(context, &signal, abort_reason(reason))
    }
}

impl AbortSignalApi for Api {
    fn abort(_context: &mut Context, _this: (), reason: Option<JsValue>) -> Result<JsObject> {
        let signal = new_signal(signal_prototype()?);
        let reason = abort_reason(reason);
        with_signal(&signal, |state| {
            state.aborted = true;
            state.reason = Some(reason);
        });
        Ok(signal)
    }

    fn timeout(_context: &mut Context, _this: (), milliseconds: u64) -> Result<JsObject> {
        let signal = new_signal(signal_prototype()?);
        let held = signal.clone();
        host::with(|host| crate::event_loop::schedule_signal(host, held, milliseconds as f64))?;
        Ok(signal)
    }

    fn any(_context: &mut Context, _this: (), signals: Vec<JsObject>) -> Result<JsObject> {
        let result = new_signal(signal_prototype()?);
        for source in &signals {
            if is_aborted(source) {
                let reason = with_signal(source, |state| state.reason.clone()).flatten();
                with_signal(&result, |state| {
                    state.aborted = true;
                    state.reason = reason;
                });
                return Ok(result);
            }
        }
        for source in signals {
            with_signal(&source, |state| state.dependents.push(result.clone()));
        }
        Ok(result)
    }

    fn aborted(_host: &mut crate::host::Host, this: JsObject) -> Result<bool> {
        Ok(is_aborted(&this))
    }

    fn reason(_host: &mut crate::host::Host, this: JsObject) -> Result<JsValue> {
        Ok(with_signal(&this, |state| state.reason.clone())
            .flatten()
            .unwrap_or_default())
    }

    fn throw_if_aborted(_context: &mut Context, this: JsObject) -> Result<()> {
        match with_signal(&this, |state| {
            if state.aborted {
                state.reason.clone()
            } else {
                None
            }
        })
        .flatten()
        {
            Some(reason) => Err(Exception::Js(boa_engine::JsError::from_opaque(reason))),
            None => Ok(()),
        }
    }
}
