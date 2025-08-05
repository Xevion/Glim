//! Build script to generate language color mappings from GitHub Linguist.
//!
//! This script downloads the latest language definitions from GitHub Linguist
//! and generates a static map of language names to their hex colors.

use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::OnceLock;

use phf_codegen::Map;
use saphyr::{LoadableYamlNode, Yaml};

// We'll extract the color field manually from the YAML structure
// instead of using Serde deserialize

const LANGUAGES_URL: &str =
    "https://raw.githubusercontent.com/github-linguist/linguist/master/lib/linguist/languages.yml";
const CACHE_FILE: &str = "languages.yml";
const ETAG_FILE: &str = "languages.etag";

/// Lazy-evaluated verbose flag
static VERBOSE: OnceLock<bool> = OnceLock::new();

/// Checks if verbose output is enabled via --verbose flag (lazy evaluated)
fn is_verbose() -> bool {
    *VERBOSE.get_or_init(|| env::args().any(|arg| arg == "--verbose"))
}

/// Prints verbose message only if --verbose flag is set
fn verbose_println(message: &str) {
    if is_verbose() {
        println!("cargo:warning={}", message);
    }
}

/// Downloads and caches the languages.yml file from GitHub Linguist.
/// Uses conditional requests with If-None-Match for efficient caching.
fn download_languages_yml(out_path: &Path) -> String {
    let cache_path = out_path.join(CACHE_FILE);
    let etag_path = out_path.join(ETAG_FILE);

    let mut headers = reqwest::header::HeaderMap::new();
    if let Ok(token) = env::var("GITHUB_TOKEN") {
        headers.insert(
            "Authorization",
            format!("Bearer {}", token).parse().unwrap(),
        );
    }

    let client = reqwest::blocking::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();

    // Check if we have cached files
    let has_cache = cache_path.exists() && etag_path.exists();

    // Read cached ETag if available
    let cached_etag = if has_cache {
        fs::read_to_string(&etag_path).ok()
    } else {
        None
    };

    // Make conditional request
    let mut request = client.get(LANGUAGES_URL);

    // Add If-None-Match header if we have a cached ETag
    if let Some(etag) = &cached_etag {
        request = request.header("If-None-Match", format!("\"{}\"", etag.trim()));
    }

    let response = request.send().unwrap();
    let status = response.status();

    if status.as_u16() == 304 {
        // Content unchanged, use cached version
        verbose_println("Using cached languages.yml (304 Not Modified)");
        fs::read_to_string(&cache_path).unwrap()
    } else {
        // Content changed or no cache, download new version
        verbose_println("Downloading languages.yml from GitHub Linguist...");

        // Get the new ETag from the response
        if let Some(etag) = response.headers().get("etag") {
            if let Ok(etag_str) = etag.to_str() {
                // Cache the ETag (remove quotes)
                let etag_clean = etag_str.trim_matches('"');
                fs::write(&etag_path, etag_clean).unwrap();
            }
        }

        let content = response.text().unwrap();

        // Cache the content
        fs::write(&cache_path, &content).unwrap();
        content
    }
}

/// Extracts color information from a language mapping
fn extract_color_from_language(lang_mapping: &saphyr::Yaml) -> Option<String> {
    lang_mapping.as_mapping().and_then(|mapping| {
        mapping.iter().find_map(|(key, value)| {
            key.as_str()
                .filter(|&k| k == "color")
                .and_then(|_| value.as_str())
                .map(|color| format!("\"{}\"", color))
        })
    })
}

/// Generates the colors.rs file from the languages YAML content.
fn generate_colors_rs(out_path: &Path, languages_yml: &str) {
    let path = out_path.join("colors.rs");
    let mut file = BufWriter::new(File::create(&path).unwrap());

    // Parse YAML using Saphyr
    let docs = Yaml::load_from_str(languages_yml).unwrap();
    let yaml_root = &docs[0]; // Get the first (and only) YAML document

    let mut color_map = Map::new();
    let mut color_strings: Vec<(String, String)> = Vec::new();

    // Extract color mappings from the YAML structure
    if let Some(mapping) = yaml_root.as_mapping() {
        for (name_yaml, lang_yaml) in mapping {
            if let Some(name) = name_yaml.as_str() {
                if let Some(color) = extract_color_from_language(lang_yaml) {
                    color_strings.push((name.to_string(), color));
                }
            }
        }
    }

    // Build the PHF map from the collected colors
    for (name, color) in color_strings.iter() {
        color_map.entry(name, color);
    }

    writeln!(
        &mut file,
        "static COLORS: phf::Map<&'static str, &'static str> = \n{};\n",
        color_map.build()
    )
    .unwrap();
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);

    // Download and cache the languages YAML file
    let languages_yml = download_languages_yml(out_path);

    // Generate the colors.rs file
    generate_colors_rs(out_path, &languages_yml);
}
