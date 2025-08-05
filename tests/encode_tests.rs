use glim::encode::{
    create_encoder, AvifEncoder, Encoder, EncoderType, GifEncoder, IcoEncoder, ImageFormat,
    JpegEncoder, PngEncoder, SvgEncoder, WebPEncoder,
};
use std::io::Cursor;

#[tokio::test]
async fn test_image_format_mime_types() {
    let test_cases = [
        (ImageFormat::Png, "image/png"),
        (ImageFormat::WebP, "image/webp"),
        (ImageFormat::Jpeg, "image/jpeg"),
        (ImageFormat::Svg, "image/svg+xml"),
        (ImageFormat::Avif, "image/avif"),
        (ImageFormat::Gif, "image/gif"),
        (ImageFormat::Ico, "image/x-icon"),
    ];

    for (format, expected_mime) in test_cases {
        assert_eq!(format.mime_type(), expected_mime);
    }
}

#[tokio::test]
async fn test_image_format_extensions() {
    let test_cases = [
        (ImageFormat::Png, "png"),
        (ImageFormat::WebP, "webp"),
        (ImageFormat::Jpeg, "jpg"),
        (ImageFormat::Svg, "svg"),
        (ImageFormat::Avif, "avif"),
        (ImageFormat::Gif, "gif"),
        (ImageFormat::Ico, "ico"),
    ];

    for (format, expected_ext) in test_cases {
        assert_eq!(format.extension(), expected_ext);
    }
}

#[tokio::test]
async fn test_encoder_creation() {
    let test_cases = [
        (ImageFormat::Png, false),  // Should fail with invalid SVG
        (ImageFormat::WebP, false), // Should fail with invalid SVG
        (ImageFormat::Jpeg, false), // Should fail with invalid SVG
        (ImageFormat::Svg, true),   // Should work for SVG
        (ImageFormat::Avif, false), // Should fail with invalid SVG
        (ImageFormat::Gif, false),  // Should fail with invalid SVG
        (ImageFormat::Ico, false),  // Should fail with invalid SVG
    ];

    for (format, should_succeed) in test_cases {
        let encoder = create_encoder(format);
        let mut cursor = Cursor::new(Vec::new());
        let result = encoder.encode("test", &mut cursor, None);
        assert_eq!(result.is_ok(), should_succeed);
    }
}

#[tokio::test]
async fn test_svg_encoder() {
    let encoder = SvgEncoder::new();
    let mut output = Cursor::new(Vec::new());
    let test_svg = "<svg><text>Hello World</text></svg>";

    let result = encoder.encode(test_svg, &mut output, None);
    assert!(result.is_ok());

    let output_data = output.into_inner();
    assert_eq!(output_data, test_svg.as_bytes());
}

#[tokio::test]
async fn test_png_encoder_creation() {
    let encoder = PngEncoder::new();
    let mut cursor = Cursor::new(Vec::new());
    assert!(encoder
        .encode("<invalid>svg</invalid>", &mut cursor, None)
        .is_err());
}

#[tokio::test]
async fn test_webp_encoder_creation() {
    let encoder = WebPEncoder::new();
    let mut cursor = Cursor::new(Vec::new());
    assert!(encoder
        .encode("<invalid>svg</invalid>", &mut cursor, None)
        .is_err());
}

#[tokio::test]
async fn test_jpeg_encoder_creation() {
    let encoder = JpegEncoder::new();
    let mut cursor = Cursor::new(Vec::new());
    assert!(encoder
        .encode("<invalid>svg</invalid>", &mut cursor, None)
        .is_err());
}

#[tokio::test]
async fn test_png_error_handling() {
    test_single_encoder_error_handling(EncoderType::Png(PngEncoder::new()), "PNG").await;
}

#[tokio::test]
async fn test_webp_error_handling() {
    test_single_encoder_error_handling(EncoderType::WebP(WebPEncoder::new()), "WebP").await;
}

#[tokio::test]
async fn test_jpeg_error_handling() {
    test_single_encoder_error_handling(EncoderType::Jpeg(JpegEncoder::new()), "JPEG").await;
}

#[tokio::test]
async fn test_avif_error_handling() {
    test_single_encoder_error_handling(EncoderType::Avif(AvifEncoder::new()), "AVIF").await;
}

#[tokio::test]
async fn test_gif_error_handling() {
    test_single_encoder_error_handling(EncoderType::Gif(GifEncoder::new()), "GIF").await;
}

#[tokio::test]
async fn test_ico_error_handling() {
    test_single_encoder_error_handling(EncoderType::Ico(IcoEncoder::new()), "ICO").await;
}

async fn test_single_encoder_error_handling(encoder: EncoderType, name: &str) {
    let mut output = Cursor::new(Vec::new());
    let result = encoder.encode("<invalid>svg</invalid>", &mut output, None);

    assert!(
        result.is_err(),
        "{} encoder should fail with invalid SVG",
        name
    );

    let error = result.unwrap_err();
    assert!(format!("{:?}", error).contains("Image"));
}
