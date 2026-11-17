// SPDX-License-Identifier: Apache-2.0
//! A successful native push always owns exactly one restoration attempt.
use super::*;
pub(super) trait Stack {
    fn push(&self, handle: usize) -> Result<()>;
    fn pop(&self) -> Result<usize>;
    fn poison_and_retain(&self);
}
struct Restore<'a, T: Stack> {
    stack: &'a T,
    expected: usize,
    attempted: bool,
}
impl<T: Stack> Restore<'_, T> {
    fn finish(&mut self) -> Result<()> {
        self.attempted = true;
        let result = self.stack.pop().and_then(|actual| {
            ensure!(actual == self.expected, "native CUDA context stack changed");
            Ok(())
        });
        if result.is_err() {
            self.stack.poison_and_retain();
        }
        result
    }
}
impl<T: Stack> Drop for Restore<'_, T> {
    fn drop(&mut self) {
        if !self.attempted {
            let _ = self.finish();
        }
    }
}
pub(super) fn run<T: Stack, V>(
    stack: &T,
    handle: usize,
    call: impl FnOnce() -> Result<V>,
) -> Result<V> {
    stack.push(handle)?;
    let mut restore = Restore {
        stack,
        expected: handle,
        attempted: false,
    };
    let result = call();
    let restored = restore.finish();
    match (result, restored) {
        (Ok(v), Ok(())) => Ok(v),
        (Err(e), Ok(())) | (Ok(_), Err(e)) => Err(e),
        (Err(e), Err(r)) => Err(preserve_primary(
            e,
            format!("context restoration also failed: {r:#}"),
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Injected {
        push_ok: bool,
        pop: Option<usize>,
        pops: Cell<usize>,
        retained: Cell<bool>,
    }
    impl Stack for Injected {
        fn push(&self, _: usize) -> Result<()> {
            ensure!(self.push_ok, "push failed");
            Ok(())
        }
        fn pop(&self) -> Result<usize> {
            self.pops.set(self.pops.get() + 1);
            self.pop.context("pop failed")
        }
        fn poison_and_retain(&self) {
            self.retained.set(true);
        }
    }
    #[test]
    fn failed_push_never_pops_and_call_error_restores_once() {
        let x = Injected {
            push_ok: false,
            pop: Some(7),
            pops: Cell::new(0),
            retained: Cell::new(false),
        };
        assert!(run(&x, 7, || Ok(())).is_err());
        assert_eq!(x.pops.get(), 0);
        let x = Injected { push_ok: true, ..x };
        assert!(run(&x, 7, || -> Result<()> { anyhow::bail!("original") }).is_err());
        assert_eq!(x.pops.get(), 1);
        assert!(!x.retained.get());
    }
    #[test]
    fn restore_error_wrong_handle_and_panic_preserve_exactly_one_attempt() {
        for pop in [None, Some(8)] {
            let x = Injected {
                push_ok: true,
                pop,
                pops: Cell::new(0),
                retained: Cell::new(false),
            };
            let e = run(&x, 7, || -> Result<()> {
                Err(
                    std::io::Error::new(std::io::ErrorKind::ConnectionReset, "first failure")
                        .into(),
                )
            })
            .unwrap_err();
            assert!(e.to_string().contains("first failure"));
            assert!(e.to_string().contains("context restoration also failed"));
            assert_eq!(
                e.downcast_ref::<std::io::Error>().unwrap().kind(),
                std::io::ErrorKind::ConnectionReset
            );
            assert!(x.retained.get());
            assert_eq!(x.pops.get(), 1);
        }
        let x = Injected {
            push_ok: true,
            pop: Some(7),
            pops: Cell::new(0),
            retained: Cell::new(false),
        };
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(
                &x,
                7,
                || -> Result<()> { panic!("original panic") }
            )))
            .is_err()
        );
        assert_eq!(x.pops.get(), 1);
        assert!(!x.retained.get());
    }
}
