include!(concat!(env!("OUT_DIR"), "/colors.rs"));

pub fn get_color(lang: &str) -> Option<String> {
    COLORS.get(lang).map(|s| s.to_string())
}