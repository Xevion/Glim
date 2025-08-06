use glim::colors::{count_languages, get_color};

#[test]
fn test_count_languages() {
    assert!(
        count_languages() > 100,
        "Expected at least 100 languages, got {}",
        count_languages()
    );
}

#[test]
fn test_common_languages_exist() {
    assert!(get_color("Rust").is_some());
    assert!(get_color("JavaScript").is_some());
    assert!(get_color("Python").is_some());
}

#[test]
fn test_color_mapping() {
    // Test known language colors
    assert_eq!(get_color("Rust"), Some("#dea584".to_string()));
    assert_eq!(get_color("JavaScript"), Some("#f1e05a".to_string()));
    assert_eq!(get_color("Python"), Some("#3572A5".to_string()));

    // Test unknown language returns None
    assert_eq!(get_color("UnknownLanguage"), None);
}
