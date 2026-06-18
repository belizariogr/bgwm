use std::path::PathBuf;
use std::sync::OnceLock;
use tray_icon::Icon;

type RgbaIcon = (u32, u32, Vec<u8>);

#[derive(Debug, Clone, Copy)]
struct BBox {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl BBox {
    fn width(self) -> u32 {
        self.x1 - self.x0 + 1
    }

    fn height(self) -> u32 {
        self.y1 - self.y0 + 1
    }
}

#[derive(Clone)]
struct DigitAsset {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    bbox: BBox,
}

struct TrayAssets {
    border: (u32, u32, Vec<u8>),
    digits: [DigitAsset; 10],
}

static TRAY_ASSETS: OnceLock<TrayAssets> = OnceLock::new();

const DIGIT_GAP: u32 = 0;
const CANVAS_PADDING: u32 = 1;

pub fn workspace_icon(workspace: u32) -> Result<Icon, tray_icon::Error> {
    let assets = TRAY_ASSETS.get_or_init(|| load_tray_assets().unwrap_or_else(|_| empty_assets()));
    if assets.border.2.is_empty() {
        return Err(tray_icon::Error::OsError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "tray icon assets not found",
        )));
    }

    let (width, height, rgba) = compose_workspace_icon(workspace, assets);
    Icon::from_rgba(rgba, width, height).map_err(|e| {
        tray_icon::Error::OsError(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        ))
    })
}

fn compose_workspace_icon(workspace: u32, assets: &TrayAssets) -> RgbaIcon {
    let label = workspace.to_string();
    let digit_indices: Vec<usize> = label
        .bytes()
        .filter_map(|b| (b as char).to_digit(10))
        .map(|d| d as usize)
        .collect();

    let (canvas_w, canvas_h, mut canvas) = assets.border.clone();
    if digit_indices.is_empty() {
        return (canvas_w, canvas_h, canvas);
    }

    let glyphs: Vec<&DigitAsset> = digit_indices
        .iter()
        .map(|&idx| &assets.digits[idx])
        .collect();

    let gap_total = DIGIT_GAP * glyphs.len().saturating_sub(1) as u32;
    let natural_width: u32 = glyphs.iter().map(|g| g.bbox.width()).sum::<u32>() + gap_total;
    let natural_height = glyphs.iter().map(|g| g.bbox.height()).max().unwrap_or(0);

    let inner_w = canvas_w.saturating_sub(CANVAS_PADDING * 2);
    let inner_h = canvas_h.saturating_sub(CANVAS_PADDING * 2);
    let scale = (inner_w as f32 / natural_width as f32)
        .min(inner_h as f32 / natural_height as f32)
        .min(1.0);

    let scaled_width = (natural_width as f32 * scale).round() as u32;
    let scaled_height = (natural_height as f32 * scale).round() as u32;
    let start_x = CANVAS_PADDING + (inner_w.saturating_sub(scaled_width)) / 2;
    let start_y = CANVAS_PADDING + (inner_h.saturating_sub(scaled_height)) / 2;

    let mut cursor_x = start_x as f32;
    let base_y = start_y as f32;

    for (idx, glyph) in glyphs.iter().enumerate() {
        let dst_x = cursor_x - glyph.bbox.x0 as f32 * scale;
        let dst_y = base_y - glyph.bbox.y0 as f32 * scale;
        blit_digit_scaled(&mut canvas, canvas_w, canvas_h, glyph, dst_x, dst_y, scale);

        if idx + 1 < glyphs.len() {
            cursor_x += glyph.bbox.width() as f32 * scale;
            cursor_x += DIGIT_GAP as f32 * scale;
        }
    }

    (canvas_w, canvas_h, canvas)
}

fn blit_digit_scaled(
    canvas: &mut [u8],
    canvas_w: u32,
    canvas_h: u32,
    digit: &DigitAsset,
    dst_x: f32,
    dst_y: f32,
    scale: f32,
) {
    let src_w = digit.width as i32;
    let src_h = digit.height as i32;

    for sy in 0..src_h {
        for sx in 0..src_w {
            let src_idx = ((sy * src_w + sx) * 4) as usize;
            let alpha = digit.rgba[src_idx + 3];
            if alpha == 0 {
                continue;
            }

            let px = dst_x + sx as f32 * scale;
            let py = dst_y + sy as f32 * scale;
            if scale >= 0.99 {
                blit_pixel(canvas, canvas_w, canvas_h, px.round() as i32, py.round() as i32, &digit.rgba[src_idx..src_idx + 4]);
            } else {
                blit_pixel(
                    canvas,
                    canvas_w,
                    canvas_h,
                    px.floor() as i32,
                    py.floor() as i32,
                    &digit.rgba[src_idx..src_idx + 4],
                );
            }
        }
    }
}

fn blit_pixel(canvas: &mut [u8], canvas_w: u32, canvas_h: u32, x: i32, y: i32, src: &[u8]) {
    if x < 0 || y < 0 || x >= canvas_w as i32 || y >= canvas_h as i32 {
        return;
    }

    let idx = ((y as u32 * canvas_w + x as u32) * 4) as usize;
    let alpha = src[3] as f32 / 255.0;
    if alpha <= 0.0 {
        return;
    }

    if alpha >= 1.0 {
        canvas[idx..idx + 4].copy_from_slice(src);
        return;
    }

    let inv = 1.0 - alpha;
    for channel in 0..3 {
        canvas[idx + channel] =
            (src[channel] as f32 * alpha + canvas[idx + channel] as f32 * inv).round() as u8;
    }
    canvas[idx + 3] = 255;
}

fn load_tray_assets() -> Result<TrayAssets, tray_icon::Error> {
    let base = assets_dir().join("tray").join("ref");
    let border = load_rgba_from_png(&base.join("border.png"))?;

    let mut digits = [const { None }; 10];
    for (digit, slot) in digits.iter_mut().enumerate() {
        let (width, height, rgba) = load_rgba_from_png(&base.join(format!("{digit}.png")))?;
        let bbox = opaque_bbox(&rgba, width, height);
        *slot = Some(DigitAsset {
            width,
            height,
            rgba,
            bbox,
        });
    }

    Ok(TrayAssets {
        border,
        digits: digits.map(|d| d.expect("digit asset")),
    })
}

fn empty_assets() -> TrayAssets {
    TrayAssets {
        border: (0, 0, Vec::new()),
        digits: std::array::from_fn(|_| DigitAsset {
            width: 0,
            height: 0,
            rgba: Vec::new(),
            bbox: BBox {
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 0,
            },
        }),
    }
}

fn opaque_bbox(rgba: &[u8], width: u32, height: u32) -> BBox {
    let mut bbox = BBox {
        x0: width,
        y0: height,
        x1: 0,
        y1: 0,
    };
    let mut found = false;

    for y in 0..height {
        for x in 0..width {
            let alpha = rgba[((y * width + x) * 4 + 3) as usize];
            if alpha > 0 {
                found = true;
                bbox.x0 = bbox.x0.min(x);
                bbox.y0 = bbox.y0.min(y);
                bbox.x1 = bbox.x1.max(x);
                bbox.y1 = bbox.y1.max(y);
            }
        }
    }

    if found {
        bbox
    } else {
        BBox {
            x0: 0,
            y0: 0,
            x1: width.saturating_sub(1),
            y1: height.saturating_sub(1),
        }
    }
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
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join("assets");
            if bundled.join("tray").join("ref").join("border.png").is_file() {
                return bundled;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn assets() -> TrayAssets {
        load_tray_assets().expect("tray assets should load in tests")
    }

    #[test]
    fn single_digit_icon_uses_border_canvas() {
        let (w, h, _) = compose_workspace_icon(3, &assets());
        assert_eq!((w, h), (18, 18));
    }

    #[test]
    fn double_digit_icon_uses_border_canvas() {
        let (w, h, _) = compose_workspace_icon(10, &assets());
        assert_eq!((w, h), (18, 18));
    }

    #[test]
    fn double_digit_icon_has_more_opaque_pixels_than_single() {
        let (_, _, one) = compose_workspace_icon(1, &assets());
        let (_, _, ten) = compose_workspace_icon(10, &assets());
        let count = |rgba: &[u8]| rgba.chunks(4).filter(|px| px[3] > 0).count();
        assert!(count(&ten) > count(&one));
    }

    #[test]
    fn workspace_icon_builds_for_two_digits() {
        workspace_icon(12).expect("icon for workspace 12 should build");
    }

    #[test]
    #[ignore = "manual visual check: writes PNG previews to target/tray_previews/"]
    fn dump_preview_icons() {
        use std::fs;
        use image::RgbaImage;

        let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("tray_previews");
        fs::create_dir_all(&out_dir).expect("create preview dir");

        let assets = assets();
        for ws in [1_u32, 9, 10, 11, 12, 23] {
            let (w, h, rgba) = compose_workspace_icon(ws, &assets);
            let img = RgbaImage::from_raw(w, h, rgba).expect("icon rgba");
            img.save(out_dir.join(format!("ws_{ws}.png")))
                .expect("write preview png");
        }
    }
}
