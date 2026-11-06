//! Shared artwork decode/scaling extracted from the original image loader.
//! Copyright (c) Carmine Paolino. MIT; see the repository LICENSE.
//! Call on a worker: allocating, decoding and resizing are not drawing work.

pub const COVER_TEXTURE: egui::TextureOptions =
    egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear));

#[derive(Clone, Copy)]
pub struct DecodeLimits {
    pub encoded_bytes: usize,
    pub side: u32,
    pub allocation_bytes: u64,
}
pub fn decode(bytes: &[u8], limits: DecodeLimits) -> Result<image::DynamicImage, String> {
    if bytes.len() > limits.encoded_bytes {
        return Err("artwork is too large".to_string());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut bound = image::Limits::default();
    bound.max_image_width = Some(limits.side);
    bound.max_image_height = Some(limits.side);
    bound.max_alloc = Some(limits.allocation_bytes);
    reader.limits(bound);
    reader.decode().map_err(|error| error.to_string())
}

/// Preserve aspect ratio and original small images. A cheap first reduction
/// bounds the Lanczos pass, as in the original loader.
pub fn scaled(image: image::DynamicImage, pixels: u32) -> egui::ColorImage {
    let pixels = pixels.clamp(1, 8192);
    let longer = image.width().max(image.height());
    let image = if longer > pixels {
        let scale = pixels as f32 / longer as f32;
        let width = ((image.width() as f32 * scale).round() as u32).max(1);
        let height = ((image.height() as f32 * scale).round() as u32).max(1);
        let image = if longer > pixels * 2 {
            image.thumbnail(width * 2, height * 2)
        } else {
            image
        };
        image
            .resize_exact(width, height, image::imageops::FilterType::Lanczos3)
            .to_rgba8()
    } else {
        image.to_rgba8()
    };
    egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_limits_precede_decode_and_scaling_preserves_aspect_and_small_sources() {
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2400, 2)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let limits = DecodeLimits {
            encoded_bytes: 512 * 1024,
            side: 1024,
            allocation_bytes: 8 * 1024 * 1024,
        };
        assert!(decode(encoded.get_ref(), limits).is_err());
        assert!(decode(&vec![0; 512 * 1024 + 1], limits).is_err());
        assert_eq!(
            scaled(image::DynamicImage::new_rgb8(800, 400), 250).size,
            [250, 125]
        );
        assert_eq!(
            scaled(image::DynamicImage::new_rgb8(40, 20), 250).size,
            [40, 20]
        );
    }
}
