use livecards::image;

#[test]
fn test_parse_extension_valid_formats() {
    let test_cases = [
        ("png", livecards::encode::ImageFormat::Png),
        ("PNG", livecards::encode::ImageFormat::Png),
        ("webp", livecards::encode::ImageFormat::WebP),
        ("jpeg", livecards::encode::ImageFormat::Jpeg),
        ("jpg", livecards::encode::ImageFormat::Jpeg),
        ("svg", livecards::encode::ImageFormat::Svg),
        ("avif", livecards::encode::ImageFormat::Avif),
        ("gif", livecards::encode::ImageFormat::Gif),
        ("ico", livecards::encode::ImageFormat::Ico),
    ];

    for (extension, expected_format) in test_cases {
        assert_eq!(image::parse_extension(extension), Some(expected_format));
    }
}

#[test]
fn test_parse_extension_invalid_formats() {
    let invalid_extensions = ["invalid", "", "txt", "pdf", "doc", "xls", "xml"];

    for extension in invalid_extensions {
        assert_eq!(image::parse_extension(extension), None);
    }
}

#[test]
fn test_parse_extension_case_insensitive() {
    let test_cases = [
        ("PNG", livecards::encode::ImageFormat::Png),
        ("Png", livecards::encode::ImageFormat::Png),
        ("pNg", livecards::encode::ImageFormat::Png),
        ("WEBP", livecards::encode::ImageFormat::WebP),
        ("JPEG", livecards::encode::ImageFormat::Jpeg),
        ("JPG", livecards::encode::ImageFormat::Jpeg),
    ];

    for (extension, expected_format) in test_cases {
        assert_eq!(image::parse_extension(extension), Some(expected_format));
    }
}

#[test]
fn test_unsupported_extension_handling() {
    // Test that .xml extension is not supported by parse_extension
    assert_eq!(image::parse_extension("xml"), None);
    assert_eq!(image::parse_extension("XML"), None);
    assert_eq!(image::parse_extension("Xml"), None);

    // Test that unsupported extensions are ignored and treated as part of repo name
    // This allows repositories like "vercel/next.js" to work normally
    let (repo_name, format) = livecards::server::parse_repo_name_and_format("next.js");
    assert_eq!(repo_name, "next.js");
    assert_eq!(format, livecards::encode::ImageFormat::Png);

    let (repo_name, format) = livecards::server::parse_repo_name_and_format("config.xml");
    assert_eq!(repo_name, "config.xml");
    assert_eq!(format, livecards::encode::ImageFormat::Png);
}

#[test]
fn test_real_world_repository_names() {
    // Test real-world repository names that contain dots
    let test_cases = [
        ("next.js", "next.js"),
        ("react.js", "react.js"),
        ("config.xml", "config.xml"),
        ("package.json", "package.json"),
        ("README.md", "README.md"),
        ("Dockerfile", "Dockerfile"),
    ];

    for (input, expected) in test_cases {
        let (repo_name, format) = livecards::server::parse_repo_name_and_format(input);
        assert_eq!(repo_name, expected);
        assert_eq!(format, livecards::encode::ImageFormat::Png);
    }
}

#[test]
fn test_error_response_structure() {
    // Test that our error response structure can be serialized
    use serde_json;

    #[derive(serde::Serialize)]
    struct ErrorResponse {
        error: String,
        message: String,
        status: u16,
    }

    let error = ErrorResponse {
        error: "repository_error".to_string(),
        message: "Failed to get repository info: Repository not found".to_string(),
        status: 404,
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("repository_error"));
    assert!(json.contains("Failed to get repository info"));
    assert!(json.contains("404"));
}
