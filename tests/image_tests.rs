use livecards::colors::get_color;

#[tokio::test]
async fn test_color_mapping() {
    // Test known language colors
    assert_eq!(get_color("Rust"), Some("#dea584".to_string()));
    assert_eq!(get_color("JavaScript"), Some("#f1e05a".to_string()));
    assert_eq!(get_color("Python"), Some("#3572A5".to_string()));

    // Test unknown language returns None
    assert_eq!(get_color("UnknownLanguage"), None);
}
