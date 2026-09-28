use crate::preferences::Settings;
use gtk4::cairo;
use gtk4::gdk;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::graphene;
use gtk4::prelude::*;
use std::path::PathBuf;

#[derive(Clone, PartialEq)]
struct BackgroundCacheKey {
    wallpaper_path: PathBuf,
    width: i32,
    height: i32,
    blur_bits: u64,
    brightness_bits: u64,
    saturation_bits: u64,
    grain_bits: u64,
}

impl BackgroundCacheKey {
    fn new(
        wallpaper_path: PathBuf,
        width: f32,
        height: f32,
        blur_percent: f64,
        brightness: f64,
        saturation_raw: f64,
        grain_percent: u32,
    ) -> Self {
        Self {
            wallpaper_path,
            width: width as i32,
            height: height as i32,
            blur_bits: blur_percent.to_bits(),
            brightness_bits: brightness.to_bits(),
            saturation_bits: saturation_raw.to_bits(),
            grain_bits: (grain_percent as f64).to_bits(),
        }
    }
}

#[derive(Default)]
pub(super) struct BackgroundRenderer {
    wallpaper_texture: Option<(PathBuf, gdk::Texture)>,
    grain_tile: Option<cairo::ImageSurface>,
    background_cache: Option<(BackgroundCacheKey, gdk::Texture)>,
}

impl BackgroundRenderer {
    /// Малює кешований композит шпалери. `false` означає, що Canvas має
    /// намалювати свій звичайний суцільний колір тла.
    pub(super) fn draw(
        &mut self,
        snapshot: &gtk4::Snapshot,
        widget: &gtk4::Widget,
        settings: Option<&Settings>,
        width: f32,
        height: f32,
    ) -> bool {
        let Some(settings) = settings else {
            return false;
        };
        let Some((path, texture)) = self.effective_wallpaper_texture(settings) else {
            return false;
        };

        let blur_percent = settings.wallpaper_blur();
        let brightness = settings.wallpaper_brightness();
        let saturation_raw = settings.wallpaper_saturation();
        let grain_percent = settings.wallpaper_grain();
        let key = BackgroundCacheKey::new(
            path,
            width,
            height,
            blur_percent,
            brightness,
            saturation_raw,
            grain_percent,
        );

        if let Some((cached_key, cached_texture)) = self.background_cache.as_ref() {
            if *cached_key == key {
                snapshot.append_texture(
                    cached_texture,
                    &graphene::Rect::new(0.0, 0.0, width, height),
                );
                return true;
            }
        }

        let grain_percent = grain_percent as f64;
        match self.render_background_texture(
            widget,
            &texture,
            width,
            height,
            blur_percent,
            brightness,
            saturation_raw,
            grain_percent,
        ) {
            Some(baked) => {
                snapshot.append_texture(&baked, &graphene::Rect::new(0.0, 0.0, width, height));
                self.background_cache = Some((key, baked));
            }
            None => self.paint_wallpaper_background(
                snapshot,
                &texture,
                width,
                height,
                blur_percent,
                brightness,
                saturation_raw,
                grain_percent,
            ),
        }
        true
    }

    fn effective_wallpaper_texture(
        &mut self,
        settings: &Settings,
    ) -> Option<(PathBuf, gdk::Texture)> {
        if !settings.use_desktop_wallpaper() {
            return None;
        }

        let path = crate::preferences::settings::desktop_wallpaper_path()?;
        if let Some((cached_path, texture)) = self.wallpaper_texture.as_ref() {
            if *cached_path == path {
                return Some((path, texture.clone()));
            }
        }

        let pixbuf = Pixbuf::from_file(&path).ok()?;
        let texture = gdk::Texture::for_pixbuf(&pixbuf);
        self.wallpaper_texture = Some((path.clone(), texture.clone()));
        Some((path, texture))
    }

    fn render_background_texture(
        &mut self,
        widget: &gtk4::Widget,
        texture: &gdk::Texture,
        width: f32,
        height: f32,
        blur_percent: f64,
        brightness: f64,
        saturation_raw: f64,
        grain_percent: f64,
    ) -> Option<gdk::Texture> {
        let renderer = widget.native()?.renderer()?;
        let baking = gtk4::Snapshot::new();
        self.paint_wallpaper_background(
            &baking,
            texture,
            width,
            height,
            blur_percent,
            brightness,
            saturation_raw,
            grain_percent,
        );
        let node = baking.to_node()?;
        Some(renderer.render_texture(&node, Some(&graphene::Rect::new(0.0, 0.0, width, height))))
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_wallpaper_background(
        &mut self,
        snapshot: &gtk4::Snapshot,
        texture: &gdk::Texture,
        width: f32,
        height: f32,
        blur_percent: f64,
        brightness: f64,
        saturation_raw: f64,
        grain_percent: f64,
    ) {
        let texture_width = texture.width() as f32;
        let texture_height = texture.height() as f32;
        if texture_width <= 0.0 || texture_height <= 0.0 || width <= 0.0 || height <= 0.0 {
            return;
        }

        let blur_radius = blur_percent.clamp(0.0, 100.0) as f32;
        let blurred = blur_radius > 0.0;
        let blur_margin = if blurred { blur_radius * 2.0 } else { 0.0 };
        let scale =
            ((width + blur_margin) / texture_width).max((height + blur_margin) / texture_height);
        let draw_width = texture_width * scale;
        let draw_height = texture_height * scale;
        let offset_x = (width - draw_width) / 2.0;
        let offset_y = (height - draw_height) / 2.0;

        snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, width, height));
        if blurred {
            snapshot.push_blur(blur_radius.into());
        }

        let saturation = saturation_raw.clamp(0.0, 2.0) as f32;
        let saturated = (saturation - 1.0).abs() > f32::EPSILON;
        if saturated {
            let (matrix, offset) = saturation_color_matrix(saturation);
            snapshot.push_color_matrix(&matrix, &offset);
        }
        snapshot.append_texture(
            texture,
            &graphene::Rect::new(offset_x, offset_y, draw_width, draw_height),
        );
        if saturated {
            snapshot.pop();
        }
        if blurred {
            snapshot.pop();
        }

        if brightness.abs() > f64::EPSILON {
            let alpha = (brightness.abs() / 100.0).clamp(0.0, 1.0) as f32;
            let color = if brightness < 0.0 {
                gdk::RGBA::new(0.0, 0.0, 0.0, alpha)
            } else {
                gdk::RGBA::new(1.0, 1.0, 1.0, alpha)
            };
            snapshot.append_color(&color, &graphene::Rect::new(0.0, 0.0, width, height));
        }

        if grain_percent > 0.0 {
            const MAX_GRAIN_ALPHA: f64 = 0.35;
            let alpha = (grain_percent.clamp(0.0, 100.0) / 100.0) * MAX_GRAIN_ALPHA;
            let cairo = snapshot.append_cairo(&graphene::Rect::new(0.0, 0.0, width, height));
            cairo.rectangle(0.0, 0.0, width as f64, height as f64);
            cairo.clip();
            let pattern = cairo::SurfacePattern::create(&self.grain_tile());
            pattern.set_extend(cairo::Extend::Repeat);
            cairo.set_source_rgba(0.0, 0.0, 0.0, alpha);
            let _ = cairo.mask(&pattern);
        }

        snapshot.pop();
    }

    fn grain_tile(&mut self) -> cairo::ImageSurface {
        if let Some(tile) = self.grain_tile.as_ref() {
            return tile.clone();
        }

        const SIZE: i32 = 64;
        let mut state: u32 = 0x9E37_79B9;
        let mut next_u8 = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 24) as u8
        };

        let mut surface = cairo::ImageSurface::create(cairo::Format::A8, SIZE, SIZE)
            .expect("не вдалося створити поверхню шуму для \"Зернистості\"");
        {
            let stride = surface.stride() as usize;
            let mut data = surface
                .data()
                .expect("не вдалося отримати доступ до пікселів поверхні шуму");
            for y in 0..SIZE as usize {
                for x in 0..SIZE as usize {
                    data[y * stride + x] = next_u8();
                }
            }
        }
        surface.mark_dirty();
        self.grain_tile = Some(surface.clone());
        surface
    }
}

fn saturation_color_matrix(saturation: f32) -> (graphene::Matrix, graphene::Vec4) {
    let a = 0.213 + 0.787 * saturation;
    let b = 0.715 - 0.715 * saturation;
    let c = 0.072 - 0.072 * saturation;
    let d = 0.213 - 0.213 * saturation;
    let e = 0.715 + 0.285 * saturation;
    let f = 0.072 - 0.072 * saturation;
    let g = 0.213 - 0.213 * saturation;
    let h = 0.715 - 0.715 * saturation;
    let i = 0.072 + 0.928 * saturation;

    let v0 = graphene::Vec4::new(a, d, g, 0.0);
    let v1 = graphene::Vec4::new(b, e, h, 0.0);
    let v2 = graphene::Vec4::new(c, f, i, 0.0);
    let v3 = graphene::Vec4::new(0.0, 0.0, 0.0, 1.0);
    let matrix = graphene::Matrix::from_vec4(&v0, &v1, &v2, &v3);
    (matrix, graphene::Vec4::new(0.0, 0.0, 0.0, 0.0))
}
