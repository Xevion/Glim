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
    let invalid_extensions = ["invalid", "", "txt", "pdf", "doc", "xls"];

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
