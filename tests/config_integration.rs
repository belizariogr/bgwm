use bgwm::config::{load_from_path, save_to_path, Config};
use tempfile::tempdir;

#[test]
fn config_round_trip_on_disk() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    let config = Config::default();

    save_to_path(&config, &path).unwrap();
    let loaded = load_from_path(&path).unwrap();
    assert_eq!(config, loaded);
}
