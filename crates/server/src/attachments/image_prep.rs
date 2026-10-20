use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

/// An uploaded image as composer keeps it: turned upright (EXIF orientation), shrunk to
/// `max_edge` on its long side, and re-encoded, which drops EXIF and other metadata (location,
/// camera). JPEGs stay JPEG; everything else becomes PNG. Returns the bytes, MIME type and
/// file extension.
pub fn prepare_image(
    bytes: &[u8],
    max_edge: u32,
) -> Result<(Vec<u8>, &'static str, &'static str), String> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let jpeg = reader.format() == Some(ImageFormat::Jpeg);
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| format!("not a readable image: {e}"))?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    if image.width().max(image.height()) > max_edge {
        image = image.resize(max_edge, max_edge, FilterType::Lanczos3);
    }
    let mut out = Vec::new();
    if jpeg {
        let encoder = JpegEncoder::new_with_quality(&mut out, 85);
        image
            .to_rgb8()
            .write_with_encoder(encoder)
            .map_err(|e| e.to_string())?;
        Ok((out, "image/jpeg", "jpg"))
    } else {
        image
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok((out, "image/png", "png"))
    }
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgb};

    use super::*;

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let image = ImageBuffer::from_fn(width, height, |x, _| Rgb([(x % 255) as u8, 80, 160]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(image)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Jpeg)
            .unwrap();
        out
    }

    #[test]
    fn shrinks_the_long_edge_and_keeps_jpeg() {
        let (bytes, mime, ext) = prepare_image(&jpeg(3000, 1000), 1500).unwrap();
        assert_eq!((mime, ext), ("image/jpeg", "jpg"));
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!((back.width(), back.height()), (1500, 500));
    }

    #[test]
    fn small_images_keep_their_size() {
        let (bytes, _, _) = prepare_image(&jpeg(200, 100), 1500).unwrap();
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!((back.width(), back.height()), (200, 100));
    }

    #[test]
    fn exif_is_dropped() {
        // A JPEG with an APP1 Exif segment right after SOI.
        let plain = jpeg(10, 10);
        let exif = b"Exif\0\0MM\0*\0\0\0\x08\0\0secret-gps";
        let mut tagged = vec![0xff, 0xd8, 0xff, 0xe1];
        tagged.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        tagged.extend_from_slice(exif);
        tagged.extend_from_slice(&plain[2..]);
        let (bytes, _, _) = prepare_image(&tagged, 1500).unwrap();
        assert!(!bytes.windows(10).any(|w| w == b"secret-gps"));
        assert!(!bytes.windows(4).any(|w| w == b"Exif"));
    }

    #[test]
    fn garbage_is_refused() {
        assert!(prepare_image(b"\x89PNG\r\n\x1a\nnope", 1500).is_err());
    }
}
