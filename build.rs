//! Build script to generate language color mappings from GitHub Linguist.
//!
//! This script downloads the latest language definitions from GitHub Linguist
//! and generates a static map of language names to their hex colors.

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use phf_codegen::Map;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Language {
    color: Option<String>,
}

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

    let languages: HashMap<String, Language> = serde_yaml::from_str(&languages_yml).unwrap();

    let mut color_map = Map::new();
    for (name, lang) in languages {
        if let Some(color) = lang.color {
            color_map.entry(name, &format!("\"{}\"", color));
        }
    }

    writeln!(
        &mut file,
        "static COLORS: phf::Map<&'static str, &'static str> = \n{};\n",
        color_map.build()
    )
    .unwrap();
}
