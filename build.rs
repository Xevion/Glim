//! Build script to generate language color mappings from GitHub Linguist.
//!
//! This script downloads the latest language definitions from GitHub Linguist
//! and generates a static map of language names to their hex colors.

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use phf_codegen::Map;
use saphyr::{LoadableYamlNode, Yaml};

// We'll extract the color field manually from the YAML structure
// instead of using Serde deserialize

fn main() {
    let path = Path::new(&env::var("OUT_DIR").unwrap()).join("colors.rs");
    let mut file = BufWriter::new(File::create(&path).unwrap());

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

    let languages_yml = client
        .get("https://raw.githubusercontent.com/github-linguist/linguist/master/lib/linguist/languages.yml")
        .send()
        .unwrap()
        .text()
        .unwrap();

    // Parse YAML using Saphyr
    let docs = Yaml::load_from_str(&languages_yml).unwrap();
    let yaml_root = &docs[0]; // Get the first (and only) YAML document

    let mut color_map = Map::new();

    // Iterate through the mapping manually
    if let Some(mapping) = yaml_root.as_mapping() {
        for (name_yaml, lang_yaml) in mapping {
            if let Some(name) = name_yaml.as_str() {
                // Check if the language has a color field
                if let Some(lang_mapping) = lang_yaml.as_mapping() {
                    for (key_yaml, value_yaml) in lang_mapping {
                        if let Some(key_str) = key_yaml.as_str() {
                            if key_str == "color" {
                                if let Some(color) = value_yaml.as_str() {
                                    color_map.entry(name.to_string(), &format!("\"{}\"", color));
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    writeln!(
        &mut file,
        "static COLORS: phf::Map<&'static str, &'static str> = \n{};\n",
        color_map.build()
    )
    .unwrap();
}
