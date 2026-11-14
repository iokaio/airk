// SPDX-License-Identifier: Apache-2.0
//! One automatic admission policy; request dispatch/replay is outside this gate.
use crate::contract::{ErrorLine, ExitClass};
use airk_core::install_record::StartMode;

pub(super) fn admit(
    probe: impl FnOnce() -> Result<(), ErrorLine>,
    start: impl FnOnce(StartMode, ErrorLine) -> Result<(), ErrorLine>,
) -> Result<(), ErrorLine> {
    match probe() {
        Ok(()) => Ok(()),
        Err(line) if line.class == ExitClass::ServiceMissing => start(StartMode::Automatic, line),
        Err(line) => Err(line),
    }
}

#[cfg(test)]
type Probe = Box<dyn FnMut(&std::path::Path) -> Result<(), ErrorLine>>;
#[cfg(test)]
thread_local! { static PROBE: std::cell::RefCell<Option<Probe>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub(super) fn intercept(control: &std::path::Path) -> Option<Result<(), ErrorLine>> {
    PROBE.with_borrow_mut(|probe| probe.as_mut().map(|probe| probe(control)))
}

#[cfg(test)]
pub(super) fn with_probe<R>(
    probe: impl FnMut(&std::path::Path) -> Result<(), ErrorLine> + 'static,
    run: impl FnOnce() -> R,
) -> R {
    struct Restore(Option<Probe>);
    impl Drop for Restore {
        fn drop(&mut self) {
            PROBE.with_borrow_mut(|probe| *probe = self.0.take());
        }
    }
    let previous = PROBE.with_borrow_mut(|current| current.replace(Box::new(probe)));
    let _restore = Restore(previous);
    run()
}

#[cfg(test)]
mod tests;
