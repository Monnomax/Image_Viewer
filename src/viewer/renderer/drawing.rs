use crate::preferences::Settings;
use gtk4::gdk;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;

pub(super) fn draw_image_shadow(
    snapshot: &gtk4::Snapshot,
    texture: &gdk::Texture,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    opacity: f64,
    settings: Option<&Settings>,
    corner_radius: f64,
) {
    let Some(settings) = settings else {
        return;
    };
    if !settings.shadow_enabled() || opacity <= 0.0 {
        return;
    }

    let dx = settings.shadow_offset_x();
    let dy = settings.shadow_offset_y();
    let blur_radius = settings.shadow_blur_radius().clamp(0.0, 100.0);
    let spread = settings.shadow_spread_radius();
    let opacity_factor = (settings.shadow_opacity() / 100.0).clamp(0.0, 1.0) as f32;
    let color = {
        let color = settings.shadow_color();
        gdk::RGBA::new(
            color.red(),
            color.green(),
            color.blue(),
            color.alpha() * opacity_factor,
        )
    };

    let texture_width = texture.width() as f64 * scale;
    let texture_height = texture.height() as f64 * scale;
    let width = texture_width + spread * 2.0;
    let height = texture_height + spread * 2.0;
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    let bounds = graphene::Rect::new(
        (offset_x - spread) as f32,
        (offset_y - spread) as f32,
        width as f32,
        height as f32,
    );

    let shadow = gsk::Shadow::new(color, dx as f32, dy as f32, blur_radius as f32);
    let opacity_applied = opacity < 1.0;
    if opacity_applied {
        snapshot.push_opacity(opacity);
    }
    snapshot.push_shadow(&[shadow]);

    if corner_radius > 0.0 {
        let shadow_radius = (corner_radius + spread) as f32;
        let corner = graphene::Size::new(shadow_radius, shadow_radius);
        let rounded = gsk::RoundedRect::new(bounds, corner, corner, corner, corner);
        snapshot.push_rounded_clip(&rounded);
        snapshot.append_texture(texture, &bounds);
        snapshot.pop();
    } else {
        snapshot.append_texture(texture, &bounds);
    }

    snapshot.pop();
    if opacity_applied {
        snapshot.pop();
    }
}

pub(super) fn draw_texture(
    snapshot: &gtk4::Snapshot,
    texture: &gdk::Texture,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    opacity: f64,
    corner_radius: f64,
) {
    if opacity <= 0.0 {
        return;
    }

    let texture_width = texture.width() as f64 * scale;
    let texture_height = texture.height() as f64 * scale;
    let opacity_applied = opacity < 1.0;
    if opacity_applied {
        snapshot.push_opacity(opacity);
    }

    let clipped = corner_radius > 0.0;
    if clipped {
        let bounds = graphene::Rect::new(
            offset_x as f32,
            offset_y as f32,
            texture_width as f32,
            texture_height as f32,
        );
        let corner = graphene::Size::new(corner_radius as f32, corner_radius as f32);
        let rounded = gsk::RoundedRect::new(bounds, corner, corner, corner, corner);
        snapshot.push_rounded_clip(&rounded);
    }

    snapshot.save();
    snapshot.translate(&graphene::Point::new(offset_x as f32, offset_y as f32));
    snapshot.scale(scale as f32, scale as f32);
    snapshot.append_texture(
        texture,
        &graphene::Rect::new(0.0, 0.0, texture.width() as f32, texture.height() as f32),
    );
    snapshot.restore();

    if clipped {
        snapshot.pop();
    }
    if opacity_applied {
        snapshot.pop();
    }
}
