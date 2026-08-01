#![cfg(feature = "serde")]

use std::{fs, path::PathBuf};

use rill_ml::ValidateState;
use rill_ml::drift::{
    Adwin, AdwinConfig, AdwinPortableStateV1, Kswin, KswinConfig, KswinPortableStateV1,
    PageHinkley, PageHinkleyConfig, PageHinkleyPortableStateV1,
};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/state/portable-v1")
        .join(format!("{name}.json"));
    fs::read_to_string(path).unwrap()
}

#[test]
fn load_portable_v1_page_hinkley() {
    let state: PageHinkleyPortableStateV1 = serde_json::from_str(&fixture("page_hinkley")).unwrap();
    state.validate_state().unwrap();
    let restored = PageHinkley::restore_state_v1(PageHinkleyConfig::default(), state).unwrap();
    assert_eq!(restored.export_state_v1().samples, 3);
}

#[test]
fn load_portable_v1_adwin() {
    let state: AdwinPortableStateV1 = serde_json::from_str(&fixture("adwin")).unwrap();
    state.validate_state().unwrap();
    let restored = Adwin::restore_state_v1(AdwinConfig::default(), state).unwrap();
    assert_eq!(restored.export_state_v1().window, [0.0, 1.0, 2.0]);
}

#[test]
fn load_portable_v1_kswin() {
    let state: KswinPortableStateV1 = serde_json::from_str(&fixture("kswin")).unwrap();
    state.validate_state().unwrap();
    let restored = Kswin::restore_state_v1(KswinConfig::default(), state).unwrap();
    assert_eq!(restored.export_state_v1().current_window, [1.0, 2.0]);
}

#[test]
fn portable_v1_fixtures_reject_unknown_fields() {
    let json = fixture("page_hinkley").replace("\n}", ",\n  \"unknown\": true\n}");
    assert!(serde_json::from_str::<PageHinkleyPortableStateV1>(&json).is_err());
}
