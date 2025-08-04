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

    let languages_yml = reqwest::blocking::get(
        "https://raw.githubusercontent.com/github-linguist/linguist/master/lib/linguist/languages.yml",
    )
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

