use anyhow::Result;
use resvg::{tiny_skia, usvg};

pub struct Rasterizer {
    font_db: usvg::fontdb::Database,
}

impl Rasterizer {
    pub fn new() -> Self {
        let mut fontdb = usvg::fontdb::Database::new();
        fontdb.load_system_fonts();
        fontdb.load_fonts_dir("src/fonts");
        Self { font_db: fontdb }
    }

    pub fn render(&self, svg_data: &str) -> Result<tiny_skia::Pixmap, anyhow::Error> {
        let options = usvg::Options {
            fontdb: std::sync::Arc::new(self.font_db.clone()),
            ..Default::default()
        };

        let tree = usvg::Tree::from_str(svg_data, &options)?;

        let pixmap_size = tree.size().to_int_size();
        let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
            .ok_or_else(|| anyhow::anyhow!("Failed to create pixmap"))?;

        resvg::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );

        Ok(pixmap)
    }
}

fn main() -> Result<()> {
    let svg_template = std::fs::read_to_string("card.svg")?;

    let svg_filled = svg_template
        .replace("{{name}}", "livecards")
        .replace("{{description}}", "A project to generate repository cards.")
        .replace("{{language}}", "Rust")
        .replace("{{language_color}}", "#f1e05a")
        .replace("{{stars}}", "123")
        .replace("{{forks}}", "45");

    let rasterizer = Rasterizer::new();

    let pixmap = rasterizer.render(&svg_filled)?;

    pixmap.save_png("card.png")?;

    println!("Successfully generated card.png.");

    Ok(())
}