use std::path::PathBuf;
use std::sync::OnceLock;
use tray_icon::Icon;

type RgbaIcon = (u32, u32, Vec<u8>);

static ICON_CACHE: OnceLock<Vec<RgbaIcon>> = OnceLock::new();

pub fn workspace_icon(workspace: u32) -> Result<Icon, tray_icon::Error> {
    let icons = ICON_CACHE.get_or_init(|| load_all_icons().unwrap_or_default());
    // Assets are named 0.png–9.png matching the 1-based workspace label shown to the user.
    let index = workspace.min(9) as usize;
    let (width, height, rgba) = icons.get(index).ok_or_else(|| {
        tray_icon::Error::OsError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "tray icon assets not found",
        ))
    })?;
    Icon::from_rgba(rgba.clone(), *width, *height).map_err(|e| {
        tray_icon::Error::OsError(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        ))
    })
}

fn load_all_icons() -> Result<Vec<RgbaIcon>, tray_icon::Error> {
    let base = assets_dir().join("tray").join("ref");
    let mut icons = Vec::with_capacity(10);
    for digit in 0..=9 {
        let path = base.join(format!("{digit}.png"));
        icons.push(load_rgba_from_png(&path)?);
    }
    Ok(icons)
}

fn load_rgba_from_png(path: &PathBuf) -> Result<RgbaIcon, tray_icon::Error> {
    let image = image::open(path).map_err(|e| {
        tray_icon::Error::OsError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            e.to_string(),
        ))
    })?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok((width, height, rgba.into_raw()))
}

fn assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}
