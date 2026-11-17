# SPDX-License-Identifier: Apache-2.0
"""Generate ignored incremental admission-only candidate; never edits live files."""
from pathlib import Path
import difflib
import hashlib
import json

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
SOURCE = HERE / 'source'
OUT = HERE / 'admission-only-source'
changes = {}

def read(name, live=False):
    return (ROOT if live else SOURCE).joinpath(name).read_text(encoding='utf-8')

def save(name, old, new):
    base = HERE / 'admission-only-base' / name
    base.parent.mkdir(parents=True, exist_ok=True)
    base.write_text(old, encoding='utf-8', newline='\n')
    path = OUT / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(new, encoding='utf-8', newline='\n')
    changes[name] = (old, new)

name = 'crates/airk-store/src/quota.rs'
old = read(name, True)
new = old.replace('    fn lock_before(&self, deadline: Option<Instant>) -> Result<LedgerLock> {', '''    fn lock_before(&self, deadline: Option<Instant>) -> Result<LedgerLock> {
        self.lock_selected(deadline, false)
    }
    fn lock_selected(&self, deadline: Option<Instant>, fail_fast: bool) -> Result<LedgerLock> {''')
new = new.replace('        admission::acquire(&file, deadline)?;', '        if fail_fast { file.try_lock()?; } else { admission::acquire(&file, deadline)?; }')
new = new.replace('                if deadline.is_none() {\n                    fs::remove_file(entry.path())?;', '                if deadline.is_none() && !fail_fast {\n                    fs::remove_file(entry.path())?;')
needle = '        let _lock = self.lock_before(deadline)?;\n        let policy = self.read_policy()?;\n        let records = if deadline.is_some() {'
assert new.count(needle) == 1
new = new.replace(needle, '''        let lock = self.lock_before(deadline)?;
        self.reserve_locked(collection, bytes, deadline, available, deadline.is_none(), lock)
    }
    /// Optional caller has admitted this exact attempt. Lock contention is
    /// fail-fast; accepted bounded inventory is not an accounting wait interval.
    /// No recovery or ordinary caller deadline policy is changed.
    pub(crate) fn try_reserve_without_wait(&self, collection: &str, bytes: u64) -> Result<Reservation> {
        crate::validate_collection(collection)?;
        ensure!(bytes > 0, "quota reservation must be positive");
        let lock = self.lock_selected(None, true)?;
        self.reserve_locked(collection, bytes, None, false, false, lock)
    }
    fn reserve_locked(&self, collection: &str, bytes: u64, deadline: Option<Instant>,
        available: bool, recover: bool, _lock: LedgerLock) -> Result<Reservation> {
        let policy = self.read_policy()?;
        let records = if !recover {''')
new = new.replace('        let policy = self.read_policy()?;\n        let records = if !recover {', '''        #[cfg(test)]
        if !recover && deadline.is_none() { OPTIONAL_INVENTORY_HOOK.with(|hook| { if let Some(run)=hook.get() { run(); } }); }
        let policy = self.read_policy()?;
        let records = if !recover {''')
new += '''
#[cfg(test)]
thread_local! { static OPTIONAL_INVENTORY_HOOK: std::cell::Cell<Option<fn()>> = const { std::cell::Cell::new(None) }; }
#[cfg(test)]
pub(crate) fn with_optional_inventory_hook<T>(hook: fn(), work: impl FnOnce()->T)->T {
    struct Restore(Option<fn()>);
    impl Drop for Restore { fn drop(&mut self) { OPTIONAL_INVENTORY_HOOK.with(|hook|hook.set(self.0)); } }
    let _restore=Restore(OPTIONAL_INVENTORY_HOOK.with(|slot|slot.replace(Some(hook))));
    work()
}
'''
save(name, old, new)

name = 'crates/airk-store/src/usage.rs'
old = read(name)
new = old.replace('    remaining: Duration,\n    active: Option<Instant>,', '    remaining: std::sync::Mutex<Duration>,')
new = new.replace('/// Request-owned cumulative admission allowance. Paused query time is excluded.\n/// Neither the allowance nor its active interval is cloned/refilled.', '/// Request-owned cumulative contention-wait allowance. Computation is excluded.\n/// No allowance is cloned/refilled; accepted operations settle normally.')
new = new.replace('Self { remaining: allowance, active: Some(Instant::now()), cancelled }', 'Self { remaining: std::sync::Mutex::new(allowance), cancelled }')
start = new.index('    pub fn pause(&mut self)')
end = new.index('\n}\n\npub fn unix_ms', start)
new = new[:start] + '''    fn remaining(&self) -> Result<Duration> {
        ensure!(!self.cancelled.is_cancelled(), "usage accounting cancelled");
        let left = *self.remaining.lock().unwrap_or_else(|error| error.into_inner());
        ensure!(!left.is_zero(), "usage accounting admission budget exhausted");
        Ok(left)
    }
    fn check(&self) -> Result<()> { self.remaining().map(|_| ()) }
    fn charge_wait(&self, elapsed: Duration) {
        let mut left = self.remaining.lock().unwrap_or_else(|error| error.into_inner());
        *left = left.saturating_sub(elapsed);
    }
    fn blocked<T>(&self, work: impl FnOnce() -> T) -> T {
        struct Charge<'a>(&'a AccountingBudget, Instant);
        impl Drop for Charge<'_> { fn drop(&mut self) { self.0.charge_wait(self.1.elapsed()); } }
        let _charge = Charge(self, Instant::now());
        work()
    }
''' + new[end:]
needle = '''fn lock_before(path: PathBuf, started: Instant, budget: Duration,
    check: &dyn Fn() -> Result<()>, poll_cancellation: bool) -> Result<UsageLock> {
    check()?;'''
assert needle in new
new = new.replace(needle, '''fn lock_before(path: PathBuf, started: Instant, budget: Duration,
    check: &dyn Fn() -> Result<()>, poll_cancellation: bool) -> Result<UsageLock> {
    lock_inner(path, started, budget, check, poll_cancellation, None)
}
fn lock_accounted(path: PathBuf, accounting: &AccountingBudget) -> Result<UsageLock> {
    lock_inner(path, Instant::now(), accounting.remaining()?, &|| accounting.check(), true, Some(accounting))
}
fn lock_inner(path: PathBuf, started: Instant, budget: Duration,
    check: &dyn Fn() -> Result<()>, poll_cancellation: bool, accounting: Option<&AccountingBudget>) -> Result<UsageLock> {
    check()?;''')
new = new.replace('Some(wait::acquire_checked(&path, started, budget, check, poll_cancellation)?)', '''Some(if accounting.is_some() {
            wait::acquire_accounted(&path, started, budget, check, poll_cancellation, accounting)?
        } else { wait::acquire_checked(&path, started, budget, check, poll_cancellation)? })''')
needle = '''            Err(std::fs::TryLockError::WouldBlock) if started.elapsed() < budget => {
                std::thread::sleep(
                    Duration::from_millis(2).min(budget.saturating_sub(started.elapsed())),
                );
            }'''
assert needle in new
new = new.replace(needle, '''            Err(std::fs::TryLockError::WouldBlock) if accounting.is_some() || started.elapsed() < budget => {
                let left = match accounting { Some(value) => value.remaining()?,
                    None => budget.saturating_sub(started.elapsed()) };
                ensure!(!left.is_zero(), "usage state is busy; observation may be dropped");
                let sleep = || std::thread::sleep(Duration::from_millis(2).min(left));
                if let Some(value) = accounting { value.blocked(sleep); } else { sleep(); }
            }''')
import re
new, count = re.subn(r'            let \(started, allowance\) = budget.window\(\)\?;\n            lock_before\(([^\n]+), started, allowance, &\|\| budget.check\(\), true\)\?', r'            lock_accounted(\1, budget)?', new)
assert count == 2, count
new = new.replace('''                let (started, allowance) = budget.window()?;
                lock_before(root.join("usage-quota.lock"), started, allowance, &|| budget.check(), true)?''', '''                lock_accounted(root.join("usage-quota.lock"), budget)?''')
new = new.replace('|remaining| quota.try_reserve(collection, bytes, remaining)', '|remaining| if admission.is_some() { quota.try_reserve_without_wait(collection, bytes) } else { quota.try_reserve(collection, bytes, remaining) }')
new = new.replace(') && started.elapsed() < CONTENTION_BUDGET =>', ') && (admission.is_some() || started.elapsed() < CONTENTION_BUDGET) =>')
new = new.replace('                    std::thread::sleep(Duration::from_millis(2).min(remaining));', '''                    let sleep = || std::thread::sleep(Duration::from_millis(2).min(remaining));
                    if let Some(budget) = admission { budget.blocked(sleep); } else { sleep(); }''')
start = new.index('    #[test]\n    fn paused_query_interval_does_not_refill_or_consume_accounting_allowance')
end = new.index('\n    #[test]', start + 12)
new = new[:start] + '''    #[test]
    fn only_actual_waits_consume_one_shared_allowance() -> Result<()> {
        let budget=AccountingBudget::new(Duration::from_millis(10));
        budget.charge_wait(Duration::from_millis(4));
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(budget.remaining()?,Duration::from_millis(6));
        budget.charge_wait(Duration::from_millis(4));
        assert_eq!(budget.remaining()?,Duration::from_millis(2));
        budget.charge_wait(Duration::from_millis(2));
        assert!(budget.check().is_err());
        Ok(())
    }
''' + new[end:]
# Existing held-guard fixture uses the private window solely to start its lock attempt.
new = new.replace('let (started, allowance) = budget.window()?;', 'let (started, allowance) = (Instant::now(), budget.remaining()?);')
needle='    #[test]\n    fn admitted_budgeted_record_preserves_default_measurement_values()'
assert needle in new
new = new.replace(needle, '''    #[test]
    fn uncontended_accepted_quota_inventory_over_ten_ms_preserves_record_coverage() -> Result<()> {
        let (_owner,directory,scope)=setup()?;
        let store=UsageStore::open(&directory,&scope,&UsagePolicy::default())?.unwrap();
        let event=observation(&scope,80);
        let budget=AccountingBudget::new(Duration::from_millis(10));
        let started=Instant::now();
        crate::quota::with_optional_inventory_hook(||std::thread::sleep(Duration::from_millis(20)),
            ||store.record_budgeted(&event,&budget))?;
        assert!(started.elapsed()>=Duration::from_millis(20));
        assert_eq!(budget.remaining()?,Duration::from_millis(10),"accepted inventory is not lock waiting");
        let saved=UsageStore::read(&directory,&scope,0,unix_ms()+DAY_MS)?;
        assert_eq!(saved.lifetime.context("record missing after slow uncontended inventory")?.calls,1);
        Ok(())
    }

''' + needle)
save(name, old, new)

name = 'crates/airk-store/src/usage/wait.rs'
old = read(name)
needle = '''    check: &dyn Fn() -> Result<()>, poll_cancellation: bool) -> Result<Turn> {
    check()?;'''
assert needle in old
new = old.replace(needle, '''    check: &dyn Fn() -> Result<()>, poll_cancellation: bool) -> Result<Turn> {
    acquire_accounted(path, started, budget, check, poll_cancellation, None)
}
pub(super) fn acquire_accounted(path: &Path, started: Instant, budget: Duration,
    check: &dyn Fn() -> Result<()>, poll_cancellation: bool,
    accounting: Option<&super::AccountingBudget>) -> Result<Turn> {
    check()?;''', 1)
new = new.replace('Turn::enqueue(queue)?.wait_checked(started, budget, check, poll_cancellation)', 'Turn::enqueue(queue)?.wait_accounted(started, budget, check, poll_cancellation, accounting)')
needle = '''        check: &dyn Fn() -> Result<()>, poll_cancellation: bool) -> Result<Self> {
        {'''
assert needle in new
new = new.replace(needle, '''        check: &dyn Fn() -> Result<()>, poll_cancellation: bool) -> Result<Self> {
        self.wait_accounted(started, budget, check, poll_cancellation, None)
    }
    fn wait_accounted(self, started: Instant, budget: Duration,
        check: &dyn Fn() -> Result<()>, poll_cancellation: bool,
        accounting: Option<&super::AccountingBudget>) -> Result<Self> {
        {''')
new = new.replace('    fn wait_checked(self, started:', '    #[cfg(test)]\n    fn wait_checked(self, started:')
new = new.replace('let remaining = budget.saturating_sub(started.elapsed());', '''let remaining = match accounting {
                    Some(value) => match value.remaining() { Ok(left) => left, Err(error) => { drop(order); return Err(error); } },
                    None => budget.saturating_sub(started.elapsed()) };''')
needle = '''                (order, _) = self
                    .queue
                    .changed
                    .wait_timeout(order, if poll_cancellation { remaining.min(Duration::from_millis(2)) } else { remaining })
                    .unwrap_or_else(|error| error.into_inner());'''
assert needle in new
new = new.replace(needle, '''                let waiting = || self.queue.changed
                    .wait_timeout(order, if poll_cancellation { remaining.min(Duration::from_millis(2)) } else { remaining })
                    .unwrap_or_else(|error| error.into_inner());
                (order, _) = match accounting { Some(value) => value.blocked(waiting), None => waiting() };''')
save(name, old, new)

name='crates/airk-kernel/src/usage.rs'
old=read(name)
new=old.replace('        span.accounting.pause();\n','').replace('        span.accounting.resume();\n','')
new=new.replace('        let mut span = RequestSpan {','        let span = RequestSpan {')
new=new.replace('        mut span: RequestSpan,','        span: RequestSpan,')
new=new.replace('        use BaselineUnavailable as Missing;', '''        #[cfg(test)]
        BASELINE_DELAY.with(|delay| { if let Some(duration)=delay.take() { std::thread::sleep(duration); } });
        use BaselineUnavailable as Missing;''',1)
new += '''
#[cfg(test)]
thread_local! { static BASELINE_DELAY: std::cell::Cell<Option<std::time::Duration>> = const { std::cell::Cell::new(None) }; }
'''
needle='    #[test]\n    fn comparison_expires_after_current_publication()'
assert needle in new
new=new.replace(needle,'''    #[test]
    fn slow_uncontended_default_baseline_preserves_record_and_comparison_coverage()->Result<()> {
        let root=tempfile::tempdir()?;
        std::fs::write(root.path().join("memo.txt"),"original baseline context\\n".repeat(80))?;
        let kernel=crate::Kernel::open(root.path(),None)?;
        kernel.configure_vectors(false)?;kernel.index()?;
        let response=kernel.read_file("memo.txt",1,1,100)?;
        let logical=serde_json::to_value(&response)?;
        let expected=kernel.usage_baseline(&logical,Some(response.generation),ResponseRepresentation::LogicalResultJson);
        assert!(expected.is_ok());
        let span=kernel.usage_span(UsageOperation::ReadFile,ClientFamily::Codex);
        assert!(span.enabled&&span.store.is_some(),"actual default admission must create a recorder");
        let started=Instant::now();
        BASELINE_DELAY.with(|delay|delay.set(Some(std::time::Duration::from_millis(20))));
        kernel.finish_usage(span,Some(&logical),400,false,ResponseRepresentation::LogicalResultJson);
        assert!(started.elapsed()>=std::time::Duration::from_millis(20));
        let report=kernel.usage_report(30)?;
        assert_eq!(report.report.eligible_evidence_calls,1,"uncontended computation dropped the event");
        assert_eq!(report.report.missing_baseline_calls,0);
        assert_eq!(report.report.mismatched_baseline_calls,0);
        assert!(report.report.baseline_unavailable.is_empty());
        assert_eq!(report.lifetime.context("lifetime event missing")?.calls,1);
        Ok(())
    }

''' + needle)
save(name,old,new)

# Deterministic clock is test-only; real lock/queue/quota transitions remain real.
name='crates/airk-store/src/usage.rs'
old,new=changes[name]
new=new.replace('    cancelled: AccountingCancellation,\n}', '''    cancelled: AccountingCancellation,
    #[cfg(test)]
    wait_clock: Option<std::sync::Arc<dyn Fn(&'static str,bool)->Instant+Send+Sync>>,
}''',1)
new=new.replace('Self { remaining: std::sync::Mutex::new(allowance), cancelled }', '''Self { remaining: std::sync::Mutex::new(allowance), cancelled,
            #[cfg(test)] wait_clock: None }''')
new=new.replace('''    fn blocked<T>(&self, work: impl FnOnce() -> T) -> T {
        struct Charge<'a>(&'a AccountingBudget, Instant);
        impl Drop for Charge<'_> { fn drop(&mut self) { self.0.charge_wait(self.1.elapsed()); } }
        let _charge = Charge(self, Instant::now());''', '''    fn wait_now(&self, stage: &'static str, finished: bool) -> Instant {
        #[cfg(test)] if let Some(clock)=&self.wait_clock { return clock(stage,finished); }
        let _=(stage,finished); Instant::now()
    }
    fn blocked<T>(&self, stage: &'static str, work: impl FnOnce() -> T) -> T {
        struct Charge<'a>(&'a AccountingBudget, &'static str, Instant);
        impl Drop for Charge<'_> { fn drop(&mut self) { self.0.charge_wait(self.0.wait_now(self.1,true).saturating_duration_since(self.2)); } }
        let _charge = Charge(self, stage, self.wait_now(stage,false));''')
new=new.replace('value.blocked(sleep)','value.blocked("os",sleep)').replace('budget.blocked(sleep)','budget.blocked("quota",sleep)')
needle='        let started = Instant::now();\n        let reservation = loop {'
assert needle in new
new=new.replace(needle,'''        let started = Instant::now();
        #[cfg(test)] let started=QUOTA_RETRY_START.with(|value|value.get().unwrap_or(started));
        let reservation = loop {''',1)
new+='''
#[cfg(test)]
thread_local! { static QUOTA_RETRY_START: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) }; }
'''
needle='    #[test]\n    fn only_actual_waits_consume_one_shared_allowance()'
assert needle in new
new=new.replace(needle,'''    #[test]
    fn real_fifo_os_and_quota_stages_share_wait_allowance_without_wall_refill()->Result<()> {
        use std::sync::{Arc,atomic::{AtomicU64,AtomicU8,Ordering}};
        let (root,directory,scope)=setup()?;
        let store=UsageStore::open_best_effort(&directory,&scope,&UsagePolicy::default())?.unwrap();
        let UsageLock{_file: held_file,_turn: held_turn}=operation_guard(&directory)?;
        let quota=OpenOptions::new().read(true).write(true).open(root.path().join(".quota-1.2/ledger.lock"))?;
        quota.lock()?;
        let ticks=Arc::new(AtomicU64::new(0));let seen=Arc::new(AtomicU8::new(0));
        let (entered,stages)=std::sync::mpsc::channel();
        let origin=Instant::now();let clock_ticks=ticks.clone();
        let mut budget=AccountingBudget::new(Duration::from_millis(10));
        budget.wait_clock=Some(Arc::new(move|stage,finished|{
            let now=origin+Duration::from_millis(clock_ticks.load(Ordering::SeqCst));
            if !finished {
                let bit=match stage{"fifo"=>1,"os"=>2,"quota"=>4,_=>unreachable!()};
                if seen.fetch_or(bit,Ordering::SeqCst)&bit==0 {entered.send(stage).unwrap();}
            } now
        }));
        let event=observation(&scope,79);
        std::thread::scope(|threads|->Result<()> {
            let (done,finished)=std::sync::mpsc::channel();
            struct CancelOnExit(AccountingCancellation);
            impl Drop for CancelOnExit {fn drop(&mut self){self.0.cancel();}}
            let _cancel=CancelOnExit(budget.cancellation());
            let worker=threads.spawn(||{
                // An old legacy 2 s wall guard must not suppress this next real
                // quota retry when the shared wait allowance still remains.
                QUOTA_RETRY_START.with(|value|value.set(Some(Instant::now()-Duration::from_secs(3))));
                let result=store.record_budgeted(&event,&budget);
                let _=done.send(());result
            });
            assert_eq!(stages.recv_timeout(Duration::from_secs(5))?,"fifo");
            ticks.store(4,Ordering::SeqCst);drop(held_turn);
            assert_eq!(stages.recv_timeout(Duration::from_secs(5))?,"os");
            assert_eq!(budget.remaining()?,Duration::from_millis(6));
            ticks.store(8,Ordering::SeqCst);held_file.unlock()?;
            assert_eq!(stages.recv_timeout(Duration::from_secs(5))?,"quota");
            assert_eq!(budget.remaining()?,Duration::from_millis(2));
            ticks.store(10,Ordering::SeqCst);
            if let Err(timeout)=finished.recv_timeout(Duration::from_secs(5)) {
                budget.cancellation().cancel();
                let _=quota.unlock();
                let primary=anyhow::Error::new(timeout).context("staged recorder did not finish after allowance exhaustion");
                return match worker.join(){Ok(_)=>Err(primary),Err(_)=>Err(primary.context("staged worker also panicked during drain"))};
            }
            assert!(worker.join().expect("staged recorder panicked").is_err());
            assert_eq!(*budget.remaining.lock().unwrap(),Duration::ZERO);
            Ok(())
        })?;
        assert!(budget.check().is_err());drop(quota);drop(held_file);
        let before=UsageStore::read(&directory,&scope,0,unix_ms()+DAY_MS)?;
        assert!(before.lifetime.is_none_or(|report|report.calls==0));
        store.record(&event)?;store.record(&event)?;
        let after=UsageStore::read(&directory,&scope,0,unix_ms()+DAY_MS)?;
        assert_eq!(after.lifetime.context("ordinary retry missing")?.calls,1);
        Ok(())
    }

''' + needle)
save(name,old,new)
name='crates/airk-store/src/usage/wait.rs'
old,new=changes[name]
new=new.replace('value.blocked(waiting)','value.blocked("fifo",waiting)')
save(name,old,new)

patch = ''.join(''.join(difflib.unified_diff(old.splitlines(True), new.splitlines(True),
    fromfile='a/'+name, tofile='b/'+name)) for name,(old,new) in changes.items())
(HERE / 'admission-only-after-accounting.patch').write_text(patch, encoding='utf-8', newline='\n')
(HERE / 'ADMISSION-ONLY-BASE.json').write_text(json.dumps({
    'status':'ignored/uncompiled/unexecuted',
    'base':'original F07 accounting mirrors plus live quota.rs captured at generation',
    'files':{name:hashlib.sha256(old.encode()).hexdigest() for name,(old,_) in changes.items()}
},indent=2)+'\n',encoding='utf-8',newline='\n')
