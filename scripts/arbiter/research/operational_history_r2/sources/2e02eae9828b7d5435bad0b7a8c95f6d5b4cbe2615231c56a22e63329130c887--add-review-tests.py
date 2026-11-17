# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f16b-draft/source/crates/airk-cli/src/unified_mcp/explorer_adapter.rs')
s=p.read_text(encoding='utf-8').replace('secondary: anyhow::Error','_secondary: anyhow::Error').replace('ExchangeFailure { primary, secondary }','ExchangeFailure { primary, _secondary: secondary }')
needle='    async fn exercise_bundle(package_variable: &str) -> Result<()> {'
t='''    #[tokio::test]
    #[ignore = "requires compiled real Assistance bundle and approved Node; explicit source fence qualification"]
    async fn removal_while_post_initialize_scope_check_is_held_never_calls_old_binding() -> Result<()> {
        let (_directory, descriptor) = real_bundle_fixture("AIRK_TEST_ASSISTANCE_PACKAGE")?;
        let owner = Arc::new(Owner::default());
        let (entered, observed) = tokio::sync::oneshot::channel();
        let (release, held) = std::sync::mpsc::channel();
        let entered = Mutex::new(Some(entered));
        let held = Mutex::new(Some(held));
        let removed = Arc::new(AtomicBool::new(false));
        let check_removed = removed.clone();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let running = owner.clone();
        let task = tokio::spawn(async move {
            running.call(move || {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 { return Ok(Ok((None, None))); }
                entered.lock().unwrap().take().unwrap().send(()).unwrap();
                held.lock().unwrap().take().unwrap().recv().unwrap();
                assert!(check_removed.load(Ordering::SeqCst));
                Ok(Err(json!({"isError":true,"source_removed":true})))
            }, descriptor, "airk_explorer_templates_list".into(), Map::new(), airk_core::usage::ClientFamily::Cli).await
        });
        observed.await?;
        // The real child has completed initialize; the request is not yet sent.
        assert_eq!(owner.establishments.load(Ordering::SeqCst), 1);
        removed.store(true, Ordering::SeqCst);
        release.send(())?;
        assert_eq!(task.await??, json!({"isError":true,"source_removed":true}));
        assert!(owner.adapter.lock().unwrap().is_none());
        owner.shutdown().await?;
        Ok(())
    }
    #[test]
    fn exchange_keeps_first_failure_and_actual_writer_cause() {
        let error = settle_exchange(Err(anyhow::anyhow!("original reply timeout")), Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe).into()), false).unwrap_err();
        assert_eq!(error.to_string(), "original reply timeout");
        let error = settle_exchange(Err(anyhow::anyhow!("input event")), Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()), true).unwrap_err();
        assert_eq!(error.downcast_ref::<std::io::Error>().unwrap().kind(), std::io::ErrorKind::PermissionDenied);
    }
    #[test]
    fn child_work_budget_starts_after_delayed_preparation_without_renewal() {
        // Test-owned clock instants: preparation took an hour; no sleep/OS call.
        let preparation = Instant::now();
        let spawn = preparation + Duration::from_secs(3600);
        let deadline = child_work_deadline(spawn);
        assert_eq!(deadline.duration_since(spawn), Duration::from_secs(15));
        let initialized = spawn + Duration::from_secs(6);
        assert_eq!(deadline.duration_since(initialized), Duration::from_secs(9));
    }
'''
assert needle in s;s=s.replace(needle,t+needle)
p.write_text(s,encoding='utf-8',newline='\n')
