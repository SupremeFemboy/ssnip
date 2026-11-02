use crate::color::Color;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table, Value};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputConfig {
    #[serde(default = "default_dir", alias = "save_dir")]
    pub dir: String,

    #[serde(default = "default_filename", alias = "filename_format")]
    pub filename: String,

    #[serde(default = "default_quality")]
    pub quality: u8,

    #[serde(default = "default_capture_cursor", alias = "cursor")]
    pub capture_cursor: bool,

    #[serde(default = "default_copy_to_clipboard", alias = "clipboard", alias = "copy_clipboard")]
    pub copy_to_clipboard: bool,
}

fn default_dir() -> String {
    "~/Pictures/Screenshots".to_string()
}

fn default_filename() -> String {
    "Screenshot_%Y-%m-%d_%H-%M-%S.png".to_string()
}

fn default_quality() -> u8 {
    100
}

fn default_capture_cursor() -> bool {
    false
}

fn default_copy_to_clipboard() -> bool {
    true
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            dir: default_dir(),
            filename: default_filename(),
            quality: default_quality(),
            capture_cursor: default_capture_cursor(),
            copy_to_clipboard: default_copy_to_clipboard(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DimConfig {
    #[serde(default = "default_dim_color", alias = "dim_color")]
    pub color: Color,

    #[serde(default)]
    pub factor: Option<f32>,
}

fn default_dim_color() -> Color {
    Color::rgba(0, 0, 0, 128)
}

impl Default for DimConfig {
    fn default() -> Self {
        Self {
            color: default_dim_color(),
            factor: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BorderConfig {
    #[serde(default = "default_border_color", alias = "border_color")]
    pub color: Color,

    #[serde(default = "default_border_width", alias = "border_width")]
    pub width: u32,
}

fn default_border_color() -> Color {
    Color::rgb(255, 255, 255)
}

fn default_border_width() -> u32 {
    2
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            color: default_border_color(),
            width: default_border_width(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebpConfig {
    #[serde(default = "default_webp_lossless")]
    pub lossless: bool,

    #[serde(default = "default_webp_quality")]
    pub quality: f32,

    #[serde(default = "default_webp_method")]
    pub method: i32,

    #[serde(default = "default_webp_exact")]
    pub exact: bool,
}

fn default_webp_lossless() -> bool {
    true
}

fn default_webp_quality() -> f32 {
    90.0
}

fn default_webp_method() -> i32 {
    1
}

fn default_webp_exact() -> bool {
    true
}

impl Default for WebpConfig {
    fn default() -> Self {
        Self {
            lossless: default_webp_lossless(),
            quality: default_webp_quality(),
            method: default_webp_method(),
            exact: default_webp_exact(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JpegConfig {
    #[serde(default = "default_jpeg_quality")]
    pub quality: u8,
}

fn default_jpeg_quality() -> u8 {
    100
}

impl Default for JpegConfig {
    fn default() -> Self {
        Self {
            quality: default_jpeg_quality(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PngConfig {
    #[serde(default = "default_png_compression")]
    pub compression: String,
}

fn default_png_compression() -> String {
    "default".to_string()
}

impl Default for PngConfig {
    fn default() -> Self {
        Self {
            compression: default_png_compression(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CompressionConfig {
    #[serde(default)]
    pub webp: Option<WebpConfig>,
    #[serde(default)]
    pub jpeg: Option<JpegConfig>,
    #[serde(default)]
    pub png: Option<PngConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct FormatOptions {
    pub webp: WebpConfig,
    pub jpeg: JpegConfig,
    pub png: PngConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub output: OutputConfig,

    #[serde(default)]
    pub dim: DimConfig,

    #[serde(default, alias = "frame", alias = "selection")]
    pub border: BorderConfig,

    #[serde(default)]
    pub webp: WebpConfig,

    #[serde(default)]
    pub jpeg: JpegConfig,

    #[serde(default)]
    pub png: PngConfig,

    #[serde(default, alias = "formats")]
    pub compression: Option<CompressionConfig>,

    // Flat compatibility fallbacks
    #[serde(default)]
    save_dir: Option<String>,
    #[serde(default)]
    filename_format: Option<String>,
    #[serde(default)]
    quality: Option<u8>,
    #[serde(default)]
    capture_cursor: Option<bool>,
    #[serde(default)]
    copy_to_clipboard: Option<bool>,
    #[serde(default)]
    dim_color: Option<Color>,
    #[serde(default)]
    dim_factor: Option<f32>,
    #[serde(default)]
    border_color: Option<Color>,
    #[serde(default)]
    border_width: Option<u32>,
}

impl Config {
    pub fn save_dir(&self) -> &str {
        self.save_dir.as_deref().unwrap_or(&self.output.dir)
    }

    pub fn filename_format(&self) -> &str {
        self.filename_format
            .as_deref()
            .unwrap_or(&self.output.filename)
    }

    pub fn quality(&self) -> u8 {
        self.quality.unwrap_or(self.output.quality).clamp(1, 100)
    }

    pub fn set_quality(&mut self, val: u8) {
        let q = val.clamp(1, 100);
        self.output.quality = q;
        self.quality = Some(q);
    }

    pub fn capture_cursor(&self) -> bool {
        self.capture_cursor.unwrap_or(self.output.capture_cursor)
    }

    pub fn set_capture_cursor(&mut self, val: bool) {
        self.output.capture_cursor = val;
        self.capture_cursor = Some(val);
    }

    pub fn copy_to_clipboard(&self) -> bool {
        self.copy_to_clipboard
            .unwrap_or(self.output.copy_to_clipboard)
    }

    pub fn set_copy_to_clipboard(&mut self, val: bool) {
        self.output.copy_to_clipboard = val;
        self.copy_to_clipboard = Some(val);
    }

    pub fn effective_dim_color(&self) -> Color {
        if let Some(factor) = self.dim_factor.or(self.dim.factor) {
            let factor = factor.clamp(0.0, 1.0);
            let alpha = ((1.0 - factor) * 255.0).round() as u8;
            let base = self.dim_color.unwrap_or(self.dim.color);
            Color {
                r: base.r,
                g: base.g,
                b: base.b,
                a: alpha,
            }
        } else {
            self.dim_color.unwrap_or(self.dim.color)
        }
    }

    pub fn border_color(&self) -> Color {
        self.border_color.unwrap_or(self.border.color)
    }

    pub fn border_width(&self) -> u32 {
        self.border_width.unwrap_or(self.border.width)
    }

    pub fn webp_config(&self) -> WebpConfig {
        self.compression
            .as_ref()
            .and_then(|c| c.webp.clone())
            .unwrap_or_else(|| self.webp.clone())
    }

    pub fn jpeg_config(&self) -> JpegConfig {
        let mut cfg = self.compression
            .as_ref()
            .and_then(|c| c.jpeg.clone())
            .unwrap_or_else(|| self.jpeg.clone());

        if let Some(q) = self.quality {
            cfg.quality = q.clamp(1, 100);
        } else if self.output.quality != 100 {
            cfg.quality = self.output.quality.clamp(1, 100);
        }
        cfg
    }

    pub fn png_config(&self) -> PngConfig {
        self.compression
            .as_ref()
            .and_then(|c| c.png.clone())
            .unwrap_or_else(|| self.png.clone())
    }

    pub fn format_options(&self) -> FormatOptions {
        FormatOptions {
            webp: self.webp_config(),
            jpeg: self.jpeg_config(),
            png: self.png_config(),
        }
    }

    pub fn load() -> Result<(Self, PathBuf), Box<dyn std::error::Error>> {
        let args: Vec<String> = std::env::args().collect();
        let mut custom_config_path = None;
        let mut cli_cursor = None;
        let mut cli_clipboard = None;
        let mut cli_quality = None;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "-c" | "--config" if i + 1 < args.len() => {
                    custom_config_path = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
                "-q" | "--quality" if i + 1 < args.len() => {
                    if let Ok(q) = args[i + 1].parse::<u8>() {
                        cli_quality = Some(q.clamp(1, 100));
                    }
                    i += 1;
                }
                "--cursor" | "-C" => {
                    cli_cursor = Some(true);
                }
                "--no-cursor" => {
                    cli_cursor = Some(false);
                }
                "--clipboard" | "-b" => {
                    cli_clipboard = Some(true);
                }
                "--no-clipboard" => {
                    cli_clipboard = Some(false);
                }
                _ => {}
            }
            i += 1;
        }

        let apply_cli_overrides = |config: &mut Config| {
            if let Some(c) = cli_cursor {
                config.set_capture_cursor(c);
            }
            if let Some(b) = cli_clipboard {
                config.set_copy_to_clipboard(b);
            }
            if let Some(q) = cli_quality {
                config.quality = Some(q);
                config.output.quality = q;
            }
        };

        // 1. Check custom path from CLI
        if let Some(custom_path) = custom_config_path {
            if custom_path.exists() {
                let _ = update_config_file_with_missing_defaults(&custom_path);
                let content = fs::read_to_string(&custom_path)?;
                let mut config: Config = toml::from_str(&content)?;
                apply_cli_overrides(&mut config);
                return Ok((config, custom_path));
            } else {
                return Err(format!(
                    "Specified configuration file not found: {:?}",
                    custom_path
                )
                .into());
            }
        }

        // 2. Check ~/.config/ssnip/config.toml
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let xdg_config = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(&home).join(".config"));
        let config_file = xdg_config.join("ssnip").join("config.toml");

        if config_file.exists() {
            let _ = update_config_file_with_missing_defaults(&config_file);
            let content = fs::read_to_string(&config_file)?;
            let mut config: Config = toml::from_str(&content)?;
            apply_cli_overrides(&mut config);
            return Ok((config, config_file));
        }

        // 3. Check ./ssnip.toml
        let local_config = PathBuf::from("ssnip.toml");
        if local_config.exists() {
            let _ = update_config_file_with_missing_defaults(&local_config);
            let content = fs::read_to_string(&local_config)?;
            let mut config: Config = toml::from_str(&content)?;
            apply_cli_overrides(&mut config);
            return Ok((config, local_config));
        }

        // 4. Create default config at ~/.config/ssnip/config.toml strictly only if it does not exist
        let mut default_config = Config::default();
        apply_cli_overrides(&mut default_config);

        if !config_file.exists() {
            if let Some(parent) = config_file.parent() {
                let _ = fs::create_dir_all(parent);
            }

            let template = format!(
                "# Storage settings\n\
                 [output]\n\
                 dir = \"{}\"\n\
                 filename = \"{}\"\n\
                 capture_cursor = {}\n\
                 copy_to_clipboard = {}\n\
                 \n\
                 # Dim overlay\n\
                 [dim]\n\
                 color = \"{}\"\n\
                 \n\
                 # Selection border\n\
                 [border]\n\
                 color = \"{}\"\n\
                 width = {}\n\
                 \n\
                 # WebP compression options (used when filename ends in .webp)\n\
                 [webp]\n\
                 lossless = {}\n\
                 quality = {:.1}\n\
                 method = {}\n\
                 exact = {}\n\
                 \n\
                 # JPEG compression options (used when filename ends in .jpg or .jpeg)\n\
                 [jpeg]\n\
                 quality = {}\n\
                 \n\
                 # PNG compression options (used when filename ends in .png)\n\
                 [png]\n\
                 compression = \"{}\"\n",
                default_config.save_dir(),
                default_config.filename_format(),
                default_config.capture_cursor(),
                default_config.copy_to_clipboard(),
                default_config.effective_dim_color(),
                default_config.border_color(),
                default_config.border_width(),
                default_config.webp.lossless,
                default_config.webp.quality,
                default_config.webp.method,
                default_config.webp.exact,
                default_config.jpeg.quality,
                default_config.png.compression
            );

            let _ = fs::write(&config_file, template);
        }

        Ok((default_config, config_file))
    }

    pub fn resolve_save_path(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let raw_dir = if self.save_dir().starts_with("~/") {
            format!("{}/{}", home, &self.save_dir()[2..])
        } else if self.save_dir() == "~" {
            home
        } else {
            self.save_dir().to_string()
        };

        let now = chrono::Local::now();
        let ym = now.format("%Y-%m").to_string();
        let target_dir = PathBuf::from(raw_dir).join(ym);

        if !target_dir.exists() {
            fs::create_dir_all(&target_dir)?;
        }

        let filename = now.format(self.filename_format()).to_string();
        let target_path = target_dir.join(filename);

        Ok(target_path)
    }
}

pub fn update_config_file_with_missing_defaults(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let mut doc: DocumentMut = match content.parse() {
        Ok(d) => d,
        Err(_) => return Ok(()),
    };

    let mut modified = false;
    let is_flat = doc.contains_key("save_dir") || doc.contains_key("dim_color");

    if is_flat {
        if !doc.contains_key("quality") {
            doc["quality"] = Item::Value(Value::from(100));
            modified = true;
        }
        if !doc.contains_key("copy_to_clipboard") && !doc.contains_key("clipboard") {
            doc["copy_to_clipboard"] = Item::Value(Value::from(true));
            modified = true;
        }
        if !doc.contains_key("capture_cursor") && !doc.contains_key("cursor") {
            doc["capture_cursor"] = Item::Value(Value::from(false));
            modified = true;
        }
    } else {
        if !doc.contains_key("output") {
            let mut table = Table::new();
            table["dir"] = Item::Value(Value::from("~/Pictures/Screenshots"));
            table["filename"] = Item::Value(Value::from("Screenshot_%Y-%m-%d_%H-%M-%S.png"));
            table["quality"] = Item::Value(Value::from(100));
            table["capture_cursor"] = Item::Value(Value::from(false));
            table["copy_to_clipboard"] = Item::Value(Value::from(true));
            doc["output"] = Item::Table(table);
            modified = true;
        } else if let Some(output) = doc.get_mut("output").and_then(|i| i.as_table_mut()) {
            if !output.contains_key("dir") && !output.contains_key("save_dir") {
                output["dir"] = Item::Value(Value::from("~/Pictures/Screenshots"));
                modified = true;
            }
            if !output.contains_key("filename") && !output.contains_key("filename_format") {
                output["filename"] = Item::Value(Value::from("Screenshot_%Y-%m-%d_%H-%M-%S.png"));
                modified = true;
            }
            if !output.contains_key("quality") {
                output["quality"] = Item::Value(Value::from(100));
                modified = true;
            }
            if !output.contains_key("capture_cursor") && !output.contains_key("cursor") {
                output["capture_cursor"] = Item::Value(Value::from(false));
                modified = true;
            }
            if !output.contains_key("copy_to_clipboard") && !output.contains_key("clipboard") {
                output["copy_to_clipboard"] = Item::Value(Value::from(true));
                modified = true;
            }
        }

        if !doc.contains_key("dim") {
            let mut table = Table::new();
            table["color"] = Item::Value(Value::from("#00000080"));
            doc["dim"] = Item::Table(table);
            modified = true;
        } else if let Some(dim) = doc.get_mut("dim").and_then(|i| i.as_table_mut())
            && !dim.contains_key("color") && !dim.contains_key("dim_color") {
                dim["color"] = Item::Value(Value::from("#00000080"));
                modified = true;
        }

        if !doc.contains_key("border") {
            let mut table = Table::new();
            table["color"] = Item::Value(Value::from("#ffffff"));
            table["width"] = Item::Value(Value::from(2));
            doc["border"] = Item::Table(table);
            modified = true;
        } else if let Some(border) = doc.get_mut("border").and_then(|i| i.as_table_mut()) {
            if !border.contains_key("color") && !border.contains_key("border_color") {
                border["color"] = Item::Value(Value::from("#ffffff"));
                modified = true;
            }
            if !border.contains_key("width") && !border.contains_key("border_width") {
                border["width"] = Item::Value(Value::from(2));
                modified = true;
            }
        }

        if !doc.contains_key("webp") && !doc.contains_key("compression") {
            let mut table = Table::new();
            table["lossless"] = Item::Value(Value::from(true));
            table["quality"] = Item::Value(Value::from(90.0));
            table["method"] = Item::Value(Value::from(1));
            table["exact"] = Item::Value(Value::from(true));
            doc["webp"] = Item::Table(table);
            modified = true;
        }

        if !doc.contains_key("jpeg") && !doc.contains_key("compression") {
            let mut table = Table::new();
            table["quality"] = Item::Value(Value::from(100));
            doc["jpeg"] = Item::Table(table);
            modified = true;
        }

        if !doc.contains_key("png") && !doc.contains_key("compression") {
            let mut table = Table::new();
            table["compression"] = Item::Value(Value::from("default"));
            doc["png"] = Item::Table(table);
            modified = true;
        }
    }

    if modified {
        fs::write(path, doc.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.effective_dim_color(), Color::rgba(0, 0, 0, 128));
        assert_eq!(cfg.border_width(), 2);
        assert_eq!(cfg.border_color(), Color::rgb(255, 255, 255));
        assert_eq!(cfg.save_dir(), "~/Pictures/Screenshots");
        assert!(!cfg.capture_cursor());
        assert!(cfg.copy_to_clipboard());
        assert!(cfg.filename_format().contains("%Y"));
        assert!(cfg.webp.lossless);
        assert_eq!(cfg.jpeg.quality, 100);
        assert_eq!(cfg.png.compression, "default");
    }

    #[test]
    fn test_deserialize_categorized() {
        let toml_str = r##"
            [output]
            dir = "/tmp/my_shots"
            filename = "Shot_%H_%M.webp"
            capture_cursor = true
            copy_to_clipboard = false

            [dim]
            color = "#000000aa"

            [border]
            color = "#00ffcc"
            width = 3

            [webp]
            lossless = false
            quality = 95.0
            method = 6
            exact = false

            [jpeg]
            quality = 98

            [png]
            compression = "best"
        "##;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.save_dir(), "/tmp/my_shots");
        assert_eq!(cfg.filename_format(), "Shot_%H_%M.webp");
        assert!(cfg.capture_cursor());
        assert!(!cfg.copy_to_clipboard());
        assert_eq!(cfg.effective_dim_color(), Color::rgba(0, 0, 0, 170));
        assert_eq!(cfg.border_color(), Color::rgb(0, 255, 204));
        assert_eq!(cfg.border_width(), 3);
        assert!(!cfg.webp.lossless);
        assert_eq!(cfg.webp.quality, 95.0);
        assert_eq!(cfg.webp.method, 6);
        assert!(!cfg.webp.exact);
        assert_eq!(cfg.jpeg.quality, 98);
        assert_eq!(cfg.png.compression, "best");
    }

    #[test]
    fn test_deserialize_nested_compression() {
        let toml_str = r##"
            [compression.webp]
            lossless = true
            quality = 85.0
            method = 5

            [compression.jpeg]
            quality = 90
        "##;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        let webp_cfg = cfg.webp_config();
        assert!(webp_cfg.lossless);
        assert_eq!(webp_cfg.quality, 85.0);
        assert_eq!(webp_cfg.method, 5);
        assert_eq!(cfg.jpeg_config().quality, 90);
    }

    #[test]
    fn test_resolve_save_path() {
        let mut cfg = Config::default();
        cfg.output.dir = "/tmp/ssnip_test_dir".to_string();
        cfg.output.filename = "Shot_%Y%m%d.webp".to_string();

        let path = cfg.resolve_save_path().unwrap();
        let now = chrono::Local::now();
        let ym = now.format("%Y-%m").to_string();

        let parent_dir = path.parent().unwrap();
        assert!(parent_dir.ends_with(&ym));
        assert!(path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("Shot_"));
        assert!(path.to_str().unwrap().ends_with(".webp"));
    }

    #[test]
    fn test_auto_add_missing_parameters_preserves_custom_values() {
        let temp_dir = std::env::temp_dir();
        let test_config_path = temp_dir.join("ssnip_test_auto_add_subgroups.toml");

        let original_user_toml = r##"# Custom comment
[output]
dir = "~/MyCustomScreenshots"
filename = "MyShot_%Y.webp"

# User dim
[dim]
color = "#12345678"
"##;
        fs::write(&test_config_path, original_user_toml).unwrap();

        update_config_file_with_missing_defaults(&test_config_path).unwrap();

        let updated_content = fs::read_to_string(&test_config_path).unwrap();
        let _ = fs::remove_file(&test_config_path);

        assert!(updated_content.contains("# Custom comment"));
        assert!(updated_content.contains("~/MyCustomScreenshots"));
        assert!(updated_content.contains("MyShot_%Y.webp"));
        assert!(updated_content.contains("#12345678"));

        assert!(updated_content.contains("[webp]"));
        assert!(updated_content.contains("[jpeg]"));
        assert!(updated_content.contains("[png]"));
    }
}
