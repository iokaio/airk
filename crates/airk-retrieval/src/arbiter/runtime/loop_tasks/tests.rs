// SPDX-License-Identifier: Apache-2.0
use super::*;

/// A catalog Context descriptor re-emitted under the current compiler policy,
/// as a loop profile emitter would start from it.
fn current_context() -> Result<Profile> {
    let mut profile = catalog_profiles()?
        .into_iter()
        .find(|profile| profile.model == ModelChoice::ContextQwenCompact)
        .context("catalog Context profile")?;
    profile.policy = crate::arbiter::compiler::qwen::policy_digest()?;
    profile.task_policies = BTreeMap::from([(
        Task::ContextAction,
        crate::arbiter::context_action::policy_digest()?,
    )]);
    profile.tasks = BTreeSet::from([Task::ContextAction]);
    Ok(profile)
}

/// A profile that `admit_loop_tasks` produced is a valid Core profile, can
/// be qualified and enters a private evaluation catalog. Before Core let a
/// Context profile declare the card tasks, each of these refused it with
/// "Arbiter profile has unsupported stage task", so no loop profile could be
/// loaded on any device.
#[test]
fn a_loop_profile_is_a_valid_profile_and_enters_a_catalog() -> Result<()> {
    let mut looped = current_context()?;
    admit_loop_tasks(&mut looped)?;
    looped.validate()?;
    looped.validate_stage(Stage::Context)?;
    validate_selected_policies(&looped)?;
    ExecutionProfile::for_qualification(looped.clone())?;
    #[cfg(feature = "test-support")]
    {
        // An evaluation catalog holds candidates, which carry no final
        // qualification.
        let mut candidate = looped.clone();
        candidate.qualification = None;
        // SAFETY: a unit value stands in for the service lease; the catalog
        // only holds it, and this test opens no scorer.
        let catalog =
            unsafe { EvaluationCatalog::from_native_lease(Arc::new(()), vec![candidate.clone()])? };
        assert_eq!(catalog.profiles().len(), 1);
        assert_eq!(catalog.profiles()[0].tasks, looped.tasks);
        catalog.execution_profile(candidate.manifest)?;
    }
    // A Focus profile that declares a card task is refused by Core itself.
    let mut focus = catalog_profiles()?
        .into_iter()
        .find(|profile| profile.model == ModelChoice::FocusQwenReranker06B)
        .context("catalog Focus profile")?;
    focus.tasks.insert(Task::ContextSplit);
    focus.task_policies.insert(
        Task::ContextSplit,
        crate::arbiter::loop_cards::task_policy(Task::ContextSplit)?,
    );
    let error = focus.validate().unwrap_err().to_string();
    assert!(
        error.contains("Arbiter profile has unsupported stage task"),
        "{error}"
    );
    // A Context profile that declares a task of no current stage is too.
    let mut unknown = looped;
    unknown.tasks.insert(Task::ContextNextRead);
    unknown.task_policies.insert(
        Task::ContextNextRead,
        crate::arbiter::tasks::policy_digest()?,
    );
    let error = unknown.validate().unwrap_err().to_string();
    assert!(
        error.contains("Arbiter profile has unsupported stage task"),
        "{error}"
    );
    Ok(())
}

#[test]
fn context_profiles_admit_one_turn_or_exactly_the_four_loop_tasks() -> Result<()> {
    let one_turn = current_context()?;
    validate_selected_policies(&one_turn)?;
    assert!(!declares_card_tasks(&one_turn));
    let mut looped = one_turn.clone();
    let raw = admit_loop_tasks(&mut looped)?;
    validate_selected_policies(&looped)?;
    assert_eq!(looped.tasks.len(), 4);
    assert!(looped.tasks.iter().all(|task| is_card_task(*task)));
    assert_eq!(looped.calibration, LoopCalibrationSet::digest(&raw));

    let mut mixed = looped.clone();
    mixed.tasks.insert(Task::ContextAction);
    mixed.task_policies.insert(
        Task::ContextAction,
        crate::arbiter::context_action::policy_digest()?,
    );
    assert!(
        validate_selected_policies(&mixed).is_err(),
        "never both forms"
    );

    let mut partial = looped.clone();
    partial.tasks.remove(&Task::ContextFollowUp);
    partial.task_policies.remove(&Task::ContextFollowUp);
    assert!(
        validate_selected_policies(&partial).is_err(),
        "all four or none"
    );

    let mut stale = looped.clone();
    stale.task_policies.insert(
        Task::ContextPlan,
        crate::arbiter::loop_cards::task_policy(Task::ContextSplit)?,
    );
    assert!(
        validate_selected_policies(&stale).is_err(),
        "exact policy per task"
    );

    let mut focus = catalog_profiles()?
        .into_iter()
        .find(|profile| profile.model == ModelChoice::FocusQwenReranker06B)
        .context("catalog Focus profile")?;
    assert!(
        admit_loop_tasks(&mut focus).is_err(),
        "Focus never scores cards"
    );
    Ok(())
}

#[test]
fn calibration_set_is_authenticated_unruled_and_one_entry_per_card_task() -> Result<()> {
    let mut profile = current_context()?;
    let raw = admit_loop_tasks(&mut profile)?;
    let set = calibration_set(&profile, |maximum| {
        ensure!(raw.len() as u64 <= maximum, "set exceeds its read bound");
        Ok(raw.clone())
    })?
    .context("loop profile has a calibration set")?;
    assert!(
        set.is_unruled(),
        "the shipped set abstains on every decision"
    );
    let tasks: Vec<Task> = set.entries.iter().map(|entry| entry.task).collect();
    assert_eq!(
        tasks,
        vec![
            Task::ContextSplit,
            Task::ContextPlan,
            Task::ContextCoverage,
            Task::ContextFollowUp
        ]
    );
    for entry in &set.entries {
        assert_eq!(
            entry.task_policy,
            crate::arbiter::loop_cards::task_policy(entry.task)?
        );
    }
    let mut tampered = raw.clone();
    let at = tampered.len() - 3;
    tampered[at] ^= 1;
    assert!(
        calibration_set(&profile, |_| Ok(tampered)).is_err(),
        "bytes other than the profile's one Calibration asset refuse"
    );
    let one_turn = current_context()?;
    assert!(
        calibration_set(&one_turn, |_| anyhow::bail!("never read"))?.is_none(),
        "a one-turn profile reads no loop calibration"
    );
    Ok(())
}
