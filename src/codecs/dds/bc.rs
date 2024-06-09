use crate::codecs::dds::convert::snorm8_to_unorm8;

use super::convert::{x4_to_x8, B5G6R5};

/// Decodes a BC1 block into 16 RGBA pixels.
pub(crate) fn decode_bc1_block(block_bytes: [u8; 8]) -> [[u8; 4]; 16] {
    // https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-block-compression#bc1
    let color0_u16 = u16::from_le_bytes([block_bytes[0], block_bytes[1]]);
    let color1_u16 = u16::from_le_bytes([block_bytes[2], block_bytes[3]]);

    let c0_bgr = B5G6R5::from_u16(color0_u16);
    let c1_bgr = B5G6R5::from_u16(color1_u16);

    let c0 = c0_bgr.to_rgba8();
    let c1 = c1_bgr.to_rgba8();

    let mut pixels: [[u8; 4]; 16] = Default::default();

    let (c2, c3) = if color0_u16 > color1_u16 {
        (
            c0_bgr.blend_rgba8(c1_bgr, 1.0 / 3.0),
            c0_bgr.blend_rgba8(c1_bgr, 2.0 / 3.0),
        )
    } else {
        (
            c0_bgr.blend_rgba8(c1_bgr, 1.0 / 2.0),
            [0, 0, 0, 0], // transparent
        )
    };

    let lut = [c0, c1, c2, c3];
    let indexes = u32::from_le_bytes([
        block_bytes[4],
        block_bytes[5],
        block_bytes[6],
        block_bytes[7],
    ]);
    for (i, pixel) in pixels.iter_mut().enumerate() {
        let index = (indexes >> (i * 2)) & 0b11;
        *pixel = lut[index as usize];
    }

    pixels
}

/// Decodes a BC2 block into 16 RGBA pixels.
pub(crate) fn decode_bc2_block(block_bytes: [u8; 16]) -> [[u8; 4]; 16] {
    // https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-block-compression#bc2
    let alpha_bytes: [u8; 8] = block_bytes[0..8].try_into().unwrap();
    let bc1_bytes: [u8; 8] = block_bytes[8..16].try_into().unwrap();
    let mut pixels = decode_bc1_block(bc1_bytes);

    for i in 0..4 {
        let alpha_byte_high = alpha_bytes[i * 2];
        let alpha_byte_low = alpha_bytes[i * 2 + 1];
        let alpha = [
            alpha_byte_high & 0xF,
            alpha_byte_high >> 4,
            alpha_byte_low & 0xF,
            alpha_byte_low >> 4,
        ]
        .map(|a4| x4_to_x8(a4 as u16));

        for (j, &alpha) in alpha.iter().enumerate() {
            pixels[i * 4 + j][3] = alpha;
        }
    }

    pixels
}

/// Decodes a BC3 block into 16 RGBA pixels.
pub(crate) fn decode_bc3_block(block_bytes: [u8; 16]) -> [[u8; 4]; 16] {
    // https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-block-compression#bc3
    let alpha_bytes: [u8; 8] = block_bytes[0..8].try_into().unwrap();
    let bc1_bytes: [u8; 8] = block_bytes[8..16].try_into().unwrap();

    let mut pixels = decode_bc1_block(bc1_bytes);
    let alpha = decode_bc4_unsigned_block(alpha_bytes);

    for i in 0..4 {
        for j in 0..4 {
            pixels[i * 4 + j][3] = alpha[i * 4 + j][0];
        }
    }

    pixels
}

/// Decodes a BC4 UNORM block of into 16 grayscale pixels.
pub(crate) fn decode_bc4_unsigned_block(block_bytes: [u8; 8]) -> [[u8; 1]; 16] {
    // https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-block-compression#bc4
    let c0 = block_bytes[0];
    let c1 = block_bytes[1];

    let (c2, c3, c4, c5, c6, c7) = if c0 > c1 {
        // 6 interpolated colors

        #[inline(always)]
        fn interpolate(c0: u8, c1: u8, c1_factor: u16) -> u8 {
            // Same tricks as in x5_to_x8
            //   round(c0*(7-f) + c1*f / 7)
            //   floor(c0*(7-f) + c1*f / 7 + 0.5)
            //   floor((c0*(7-f) + c1*f + 7/2) / 7)
            //   floor((c0*(7-f)*2 + c1*f*2 + 7) / 14)
            //   (c0*(7-f)*2 + c1*f*2 + 7) / 14
            ((c0 as u16 * (7 - c1_factor) * 2 + c1 as u16 * c1_factor * 2 + 7) / 14) as u8
        }
        (
            interpolate(c0, c1, 1),
            interpolate(c0, c1, 2),
            interpolate(c0, c1, 3),
            interpolate(c0, c1, 4),
            interpolate(c0, c1, 5),
            interpolate(c0, c1, 6),
        )
    } else {
        // 4 interpolated colors

        #[inline(always)]
        fn interpolate(c0: u8, c1: u8, c1_factor: u16) -> u8 {
            // See above
            ((c0 as u16 * (5 - c1_factor) * 2 + c1 as u16 * c1_factor * 2 + 5) / 10) as u8
        }
        (
            interpolate(c0, c1, 1),
            interpolate(c0, c1, 2),
            interpolate(c0, c1, 3),
            interpolate(c0, c1, 4),
            0,
            255,
        )
    };

    let mut pixels: [[u8; 1]; 16] = Default::default();

    let lut = [c0, c1, c2, c3, c4, c5, c6, c7];
    let indexes0 = u32::from_le_bytes([block_bytes[2], block_bytes[3], block_bytes[4], 0]);
    let indexes1 = u32::from_le_bytes([block_bytes[5], block_bytes[6], block_bytes[7], 0]);
    for (i, indexes) in [indexes0, indexes1].into_iter().enumerate() {
        for j in 0..8 {
            let index = (indexes >> (j * 3)) & 0b111;
            pixels[i * 8 + j][0] = lut[index as usize];
        }
    }

    pixels
}

/// Decodes a BC4 SNORM block of into 16 grayscale pixels.
pub(crate) fn decode_bc4_signed_block(block_bytes: [u8; 8]) -> [[u8; 1]; 16] {
    // https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-block-compression#bc4
    let red0 = block_bytes[0];
    let red1 = block_bytes[1];

    let c0 = snorm8_to_unorm8(red0);
    let c1 = snorm8_to_unorm8(red1);

    // exact f32 values of c0 and c1
    let c0_f = red0.wrapping_add(128).saturating_sub(1) as f32 / 254.0 * 255.0;
    let c1_f = red1.wrapping_add(128).saturating_sub(1) as f32 / 254.0 * 255.0;

    fn interpolate(red0: f32, red1: f32, blend: f32) -> u8 {
        (red0 * (1.0 - blend) + red1 * blend).round() as u8
    }
    let (c2, c3, c4, c5, c6, c7) = if c0 > c1 {
        // 6 interpolated colors
        (
            interpolate(c0_f, c1_f, 1.0 / 7.0),
            interpolate(c0_f, c1_f, 2.0 / 7.0),
            interpolate(c0_f, c1_f, 3.0 / 7.0),
            interpolate(c0_f, c1_f, 4.0 / 7.0),
            interpolate(c0_f, c1_f, 5.0 / 7.0),
            interpolate(c0_f, c1_f, 6.0 / 7.0),
        )
    } else {
        // 4 interpolated colors
        (
            interpolate(c0_f, c1_f, 1.0 / 5.0),
            interpolate(c0_f, c1_f, 2.0 / 5.0),
            interpolate(c0_f, c1_f, 3.0 / 5.0),
            interpolate(c0_f, c1_f, 4.0 / 5.0),
            0,
            255,
        )
    };

    let mut pixels: [[u8; 1]; 16] = Default::default();

    let lut = [c0, c1, c2, c3, c4, c5, c6, c7];
    let indexes0 = u32::from_le_bytes([block_bytes[2], block_bytes[3], block_bytes[4], 0]);
    let indexes1 = u32::from_le_bytes([block_bytes[5], block_bytes[6], block_bytes[7], 0]);
    for (i, indexes) in [indexes0, indexes1].into_iter().enumerate() {
        for j in 0..8 {
            let index = (indexes >> (j * 3)) & 0b111;
            pixels[i * 8 + j][0] = lut[index as usize];
        }
    }

    pixels
}

/// Decodes a BC5 UNORM block into 16 RGB pixels.
pub(crate) fn decode_bc5_unsigned_block(block_bytes: [u8; 16]) -> [[u8; 3]; 16] {
    let red = decode_bc4_unsigned_block(block_bytes[0..8].try_into().unwrap());
    let green = decode_bc4_unsigned_block(block_bytes[8..16].try_into().unwrap());

    let mut pixels: [[u8; 3]; 16] = Default::default();
    for (i, pixel) in pixels.iter_mut().enumerate() {
        pixel[0] = red[i][0];
        pixel[1] = green[i][0];
        pixel[2] = 0;
    }

    pixels
}

/// Decodes a BC5 UNORM block into 16 RGB pixels.
pub(crate) fn decode_bc5_signed_block(block_bytes: [u8; 16]) -> [[u8; 3]; 16] {
    let red = decode_bc4_signed_block(block_bytes[0..8].try_into().unwrap());
    let green = decode_bc4_signed_block(block_bytes[8..16].try_into().unwrap());

    let mut pixels: [[u8; 3]; 16] = Default::default();
    for (i, pixel) in pixels.iter_mut().enumerate() {
        pixel[0] = red[i][0];
        pixel[1] = green[i][0];
        pixel[2] = 128;
    }

    pixels
}

/// Decodes a BC7 block into 16 RGBA pixels.
pub(crate) fn decode_bc7_block(block_bytes: [u8; 16]) -> [[u8; 4]; 16] {
    todo!()
}
