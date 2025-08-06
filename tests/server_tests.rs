use glim::image;

#[test]
fn test_parse_extension_valid_formats() {
    let test_cases = [
        ("png", glim::encode::ImageFormat::Png),
        ("PNG", glim::encode::ImageFormat::Png),
        ("webp", glim::encode::ImageFormat::WebP),
        ("jpeg", glim::encode::ImageFormat::Jpeg),
        ("jpg", glim::encode::ImageFormat::Jpeg),
        ("svg", glim::encode::ImageFormat::Svg),
        ("avif", glim::encode::ImageFormat::Avif),
        ("gif", glim::encode::ImageFormat::Gif),
        ("ico", glim::encode::ImageFormat::Ico),
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
        ("PNG", glim::encode::ImageFormat::Png),
        ("Png", glim::encode::ImageFormat::Png),
        ("pNg", glim::encode::ImageFormat::Png),
        ("WEBP", glim::encode::ImageFormat::WebP),
        ("JPEG", glim::encode::ImageFormat::Jpeg),
        ("JPG", glim::encode::ImageFormat::Jpeg),
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
    let (repo_name, format) = glim::server::parse_repo_name_and_format("next.js");
    assert_eq!(repo_name, "next.js");
    assert_eq!(format, None);

    let (repo_name, format) = glim::server::parse_repo_name_and_format("config.xml");
    assert_eq!(repo_name, "config.xml");
    assert_eq!(format, None);
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
        let (repo_name, format) = glim::server::parse_repo_name_and_format(input);
        assert_eq!(repo_name, expected);
        assert_eq!(format, None);
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

#[test]
fn test_parse_scale_parameter() {
    use glim::server::{parse_scale_parameter, ImageQuery};

    // Test valid scale parameters
    let query = ImageQuery {
        scale: Some("1.5".to_string()),
        s: None,
    };
    assert_eq!(parse_scale_parameter(&query), Some(1.5));

    let query = ImageQuery {
        scale: None,
        s: Some("2.0".to_string()),
    };
    assert_eq!(parse_scale_parameter(&query), Some(2.0));

    // Test fallback from scale to s
    let query = ImageQuery {
        scale: None,
        s: Some("1.2".to_string()),
    };
    assert_eq!(parse_scale_parameter(&query), Some(1.2));

    // Test invalid parameters
    let query = ImageQuery {
        scale: Some("0.05".to_string()), // Below minimum - gets clamped to 0.1
        s: None,
    };
    assert_eq!(parse_scale_parameter(&query), Some(0.1));

    let query = ImageQuery {
        scale: Some("12345678901".to_string()), // Too long after trimming (>10 chars)
        s: None,
    };
    assert_eq!(parse_scale_parameter(&query), None);

    let query = ImageQuery {
        scale: Some("abc".to_string()), // Invalid number
        s: None,
    };
    assert_eq!(parse_scale_parameter(&query), None);

    // Test no parameters
    let query = ImageQuery {
        scale: None,
        s: None,
    };
    assert_eq!(parse_scale_parameter(&query), None);
}

#[test]
fn test_scale_parameter_length_validation() {
    use glim::server::{parse_scale_parameter, ImageQuery};

    // Test that trailing zeros are trimmed correctly
    let query = ImageQuery {
        scale: Some("1.2000".to_string()),
        s: None,
    };
    assert_eq!(parse_scale_parameter(&query), Some(1.2));

    // Test that long strings are rejected (>10 chars after trimming)
    let query = ImageQuery {
        scale: Some("1.2345678901".to_string()),
        s: None,
    };
    assert_eq!(parse_scale_parameter(&query), None);
}

#[test]
fn test_parse_address_components_ipv6() {
    use glim::server::parse_address_components;
    use std::net::{IpAddr, SocketAddr};

    // Test IPv6 addresses without ports
    let result = parse_address_components("[::]");
    assert!(
        result.as_ref().is_ok()
            && matches!(
                result.as_ref().unwrap().as_enum(),
                terrors::E3::B(IpAddr::V6(_))
            ),
        "Expected Ok(IpAddr::V6(_)), got {:?}",
        result
    );

    let result = parse_address_components("[::1]");
    assert!(
        result.as_ref().is_ok()
            && matches!(
                result.as_ref().unwrap().as_enum(),
                terrors::E3::B(IpAddr::V6(_))
            ),
        "Expected Ok(IpAddr::V6(_)), got {:?}",
        result
    );

    // Test IPv6 addresses with ports
    let result = parse_address_components("[::]:8080");
    assert!(
        result.as_ref().is_ok()
            && matches!(
                result.as_ref().unwrap().as_enum(),
                terrors::E3::A(SocketAddr::V6(_))
            ),
        "Expected Ok(SocketAddr::V6(_)), got {:?}",
        result
    );

    let result = parse_address_components("[::1]:3000");
    assert!(
        result.as_ref().is_ok()
            && matches!(
                result.as_ref().unwrap().as_enum(),
                terrors::E3::A(SocketAddr::V6(_))
            ),
        "Expected Ok(SocketAddr::V6(_)), got {:?}",
        result
    );

    // Test IPv6 addresses with empty port (should be treated as no port)
    let result = parse_address_components("[::]:");
    assert!(
        result.as_ref().is_ok()
            && matches!(
                result.as_ref().unwrap().as_enum(),
                terrors::E3::B(IpAddr::V6(_))
            ),
        "Expected Ok(IpAddr::V6(_)), got {:?}",
        result
    );
}

#[test]
fn test_parse_address_components_ipv4() {
    use glim::server::parse_address_components;
    use std::net::{IpAddr, SocketAddr};

    // Test IPv4 addresses without ports
    let result = parse_address_components("127.0.0.1");
    assert!(
        result.as_ref().is_ok()
            && matches!(
                result.as_ref().unwrap().as_enum(),
                terrors::E3::B(IpAddr::V4(_))
            ),
        "Expected Ok(IpAddr::V4(_)), got {:?}",
        result
    );

    // Test IPv4 addresses with ports
    let result = parse_address_components("127.0.0.1:8080");
    assert!(
        result.as_ref().is_ok()
            && matches!(
                result.as_ref().unwrap().as_enum(),
                terrors::E3::A(SocketAddr::V4(_))
            ),
        "Expected Ok(SocketAddr::V4(_)), got {:?}",
        result
    );

    // Test just port
    let result = parse_address_components("8080");
    assert!(
        result.as_ref().is_ok()
            && matches!(result.as_ref().unwrap().as_enum(), terrors::E3::C(8080)),
        "Expected Ok(8080), got {:?}",
        result
    );

    let result = parse_address_components(":8080");
    assert!(
        result.as_ref().is_ok()
            && matches!(result.as_ref().unwrap().as_enum(), terrors::E3::C(8080)),
        "Expected Ok(8080), got {:?}",
        result
    );
}

#[test]
fn test_parse_address_components_invalid() {
    use glim::server::parse_address_components;

    // Test invalid IPv6 addresses
    let result = parse_address_components("[invalid]");
    assert!(result.is_err());

    let result = parse_address_components("[::]:invalid");
    assert!(result.is_err());

    // Test invalid IPv4 addresses
    let result = parse_address_components("256.256.256.256");
    assert!(result.is_err());

    let result = parse_address_components("127.0.0.1:99999");
    assert!(result.is_err());

    // Test invalid port
    let result = parse_address_components("99999");
    assert!(result.is_err());

    // Test empty input
    let result = parse_address_components("");
    assert!(result.is_err());
}

#[test]
fn test_debug_ipv6_parsing() {
    use glim::server::parse_address_components;
    use std::net::Ipv6Addr;
    use std::str::FromStr;

    println!("Testing [::] parsing...");
    let result = parse_address_components("[::]");
    println!("Result: {:?}", result);

    // Also test the raw Ipv6Addr parsing
    println!("Testing raw :: parsing...");
    let ipv6_result = Ipv6Addr::from_str("::");
    println!("Ipv6Addr result: {:?}", ipv6_result);
}
