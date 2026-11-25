// SPDX-License-Identifier: Apache-2.0
//! Literal stored bytes for the decision "Service-wide GPU use is opt-in" in
//! `arbiter/CONTRACT.md`. Every expectation is written out by hand.
use super::*;

const OFF: &str = r#"{"allow_gpu":false,"query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#;
const EARLIER_ENABLED: &str = r#"{"allow_gpu":true,"query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#;
const RESET: &str = r#"{"allow_gpu":false,"gpu_opt_in":"default_reset","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#;
const CHOSEN: &str = r#"{"allow_gpu":true,"gpu_opt_in":"user_choice","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#;

fn stored(bytes: &str) -> ResourceConfiguration {
    serde_json::from_str(bytes).unwrap()
}

fn bytes(configuration: &ResourceConfiguration) -> String {
    serde_json::to_string(configuration).unwrap()
}

#[test]
fn a_new_configuration_is_off_and_keeps_the_earlier_bytes() {
    let new = ResourceConfiguration::default();
    assert!(!new.allow_gpu);
    assert_eq!(new.gpu_opt_in, None);
    new.validate().unwrap();
    assert_eq!(bytes(&new), OFF);
    assert_eq!(bytes(&new.stored_under_gpu_opt_in()), OFF);
}

#[test]
fn a_stored_configuration_without_the_field_is_off_without_a_notice() {
    for without_field in [
        r#"{"query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
        // The shape stored before concurrent requests became configurable.
        r#"{"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
    ] {
        let loaded = stored(without_field).stored_under_gpu_opt_in();
        assert!(!loaded.allow_gpu, "{without_field}");
        assert_eq!(loaded.gpu_opt_in, None, "{without_field}");
        loaded.validate().unwrap();
        assert_eq!(bytes(&loaded), OFF, "{without_field}");
    }
}

#[test]
fn an_enabled_value_from_the_earlier_default_resets_once_and_reports() {
    // A carrier holds the stored bytes unchanged: they parse, validate and
    // encode back to the same bytes.
    let earlier = stored(EARLIER_ENABLED);
    earlier.validate().unwrap();
    assert_eq!(bytes(&earlier), EARLIER_ENABLED);

    let reset = earlier.stored_under_gpu_opt_in();
    assert!(!reset.allow_gpu);
    assert_eq!(reset.gpu_opt_in, Some(GpuOptIn::DefaultReset));
    reset.validate().unwrap();
    assert_eq!(bytes(&reset), RESET);

    // Reading the result again changes nothing: there is no second reset.
    assert_eq!(bytes(&reset.stored_under_gpu_opt_in()), RESET);
    assert_eq!(bytes(&stored(RESET).stored_under_gpu_opt_in()), RESET);
}

#[test]
fn a_stored_off_value_stays_off_without_a_notice() {
    let loaded = stored(OFF).stored_under_gpu_opt_in();
    assert!(!loaded.allow_gpu);
    assert_eq!(loaded.gpu_opt_in, None);
    assert_eq!(bytes(&loaded), OFF);
}

#[test]
fn the_users_choice_after_the_reset_is_kept_without_a_second_reset() {
    let chosen = stored(RESET).with_gpu_choice(true);
    chosen.validate().unwrap();
    assert_eq!(bytes(&chosen), CHOSEN);

    let reloaded = stored(CHOSEN).stored_under_gpu_opt_in();
    assert!(reloaded.allow_gpu);
    assert_eq!(reloaded.gpu_opt_in, Some(GpuOptIn::UserChoice));
    assert_eq!(bytes(&reloaded), CHOSEN);

    // Turning it off again, or answering the notice with off, leaves no record.
    assert_eq!(bytes(&stored(CHOSEN).with_gpu_choice(false)), OFF);
    assert_eq!(bytes(&stored(RESET).with_gpu_choice(false)), OFF);
    assert_eq!(bytes(&stored(OFF).with_gpu_choice(false)), OFF);
    assert_eq!(bytes(&stored(OFF).with_gpu_choice(true)), CHOSEN);
}

#[test]
fn a_replacement_takes_the_gpu_value_and_the_service_keeps_the_record() {
    let eight = r#"{"allow_gpu":false,"query_concurrency":8,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#;
    // Leaving GPU use off does not answer a pending notice.
    assert_eq!(
        bytes(&stored(eight).replacing(&stored(RESET))),
        r#"{"allow_gpu":false,"gpu_opt_in":"default_reset","query_concurrency":8,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#
    );
    assert_eq!(bytes(&stored(RESET).replacing(&stored(RESET))), RESET);
    // Stating `allow_gpu: true` is the user's choice, with or without a record.
    assert_eq!(
        bytes(&stored(EARLIER_ENABLED).replacing(&stored(RESET))),
        CHOSEN
    );
    assert_eq!(bytes(&stored(CHOSEN).replacing(&stored(OFF))), CHOSEN);
    assert_eq!(bytes(&stored(CHOSEN).replacing(&stored(CHOSEN))), CHOSEN);
    // A request cannot create a notice, and off replaces an earlier choice.
    assert_eq!(bytes(&stored(RESET).replacing(&stored(OFF))), OFF);
    assert_eq!(bytes(&stored(RESET).replacing(&stored(CHOSEN))), OFF);
    assert_eq!(bytes(&stored(OFF).replacing(&stored(CHOSEN))), OFF);
    assert_eq!(bytes(&stored(eight).replacing(&stored(OFF))), eight);
}

#[test]
fn contradictory_records_and_unknown_values_are_refused() {
    for contradictory in [
        r#"{"allow_gpu":true,"gpu_opt_in":"default_reset","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
        r#"{"allow_gpu":false,"gpu_opt_in":"user_choice","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
        r#"{"gpu_opt_in":"user_choice","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
    ] {
        assert!(stored(contradictory).validate().is_err(), "{contradictory}");
        assert!(
            stored(contradictory)
                .stored_under_gpu_opt_in()
                .validate()
                .is_err(),
            "{contradictory}"
        );
    }
    for unknown in [
        r#"{"allow_gpu":true,"gpu_opt_in":"enabled","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
        r#"{"allow_gpu":true,"gpu_opt_in":true,"query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
        r#"{"allow_gpu":true,"gpu_choice":"user_choice","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
    ] {
        assert!(
            serde_json::from_str::<ResourceConfiguration>(unknown).is_err(),
            "{unknown}"
        );
    }
}

#[test]
fn embedding_gpu_defaults_to_allowed_and_legacy_replacement_preserves_a_choice() {
    let legacy = stored(OFF);
    let disabled = stored(
        r#"{"allow_gpu":false,"allow_embedding_gpu":false,"query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
    );
    assert!(legacy.embedding_gpu_allowed());
    assert!(!legacy.allow_gpu);
    assert_eq!(bytes(&legacy), OFF);
    assert!(!disabled.embedding_gpu_allowed());
    assert!(
        !disabled
            .clone()
            .with_gpu_choice(true)
            .embedding_gpu_allowed()
    );
    assert!(!legacy.clone().replacing(&disabled).embedding_gpu_allowed());

    let enabled = stored(
        r#"{"allow_gpu":false,"allow_embedding_gpu":true,"query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
    );
    assert!(enabled.replacing(&disabled).embedding_gpu_allowed());
    assert!(!disabled.replacing(&legacy).embedding_gpu_allowed());
}

#[test]
fn the_setting_request_carries_only_the_boolean() {
    assert!(
        serde_json::from_str::<SetGpuAllowed>(r#"{"allow_gpu":true}"#)
            .unwrap()
            .allow_gpu
    );
    assert!(
        !serde_json::from_str::<SetGpuAllowed>(r#"{"allow_gpu":false}"#)
            .unwrap()
            .allow_gpu
    );
    for refused in [
        r#"{"allow_gpu":false,"preset":"balanced"}"#,
        r#"{"allow_gpu":true,"gpu_opt_in":"user_choice"}"#,
        r#"{"allow_gpu":"true"}"#,
        r#"{}"#,
    ] {
        assert!(
            serde_json::from_str::<SetGpuAllowed>(refused).is_err(),
            "{refused}"
        );
    }
}
