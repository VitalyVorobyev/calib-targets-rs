use crate::rectify::GrayImageView;

#[inline]
fn get_gray(img: &GrayImageView<'_>, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= img.width as i32 || y >= img.height as i32 {
        return 0;
    }
    img.data[y as usize * img.width + x as usize]
}

fn otsu_threshold_patch(img: &GrayImageView<'_>, x0: i32, y0: i32, w: i32, h: i32) -> u8 {
    let mut hist = [0u32; 256];
    let mut count = 0u32;
    for yy in 0..h {
        for xx in 0..w {
            let v = get_gray(img, x0 + xx, y0 + yy);
            hist[v as usize] += 1;
            count += 1;
        }
    }
    if count == 0 {
        return 127;
    }

    let total = count as f64;
    let mut sum_total = 0f64;
    for i in 0..256 {
        sum_total += (i as f64) * (hist[i] as f64);
    }

    let mut sum_b = 0f64;
    let mut w_b = 0f64;
    let mut best_var = -1f64;
    let mut best_t = 127u8;

    for t in 0..256 {
        w_b += hist[t] as f64;
        if w_b < 1.0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f < 1.0 {
            break;
        }

        sum_b += (t as f64) * (hist[t] as f64);
        let m_b = sum_b / w_b;
        let m_f = (sum_total - sum_b) / w_f;
        let var_between = w_b * w_f * (m_b - m_f) * (m_b - m_f);

        if var_between > best_var {
            best_var = var_between;
            best_t = t as u8;
        }
    }
    best_t
}

/// Read a 4x4 marker inside square (sx,sy) in rectified image.
/// Returns (code_u16, border_score) with black=1 bits, row-major.
pub fn read_aruco_4x4_from_square(
    rect: &GrayImageView<'_>,
    sx: i32,
    sy: i32,
    px_per_square: f32,
    border_bits: usize,  // usually 1
    cell_size_px: usize, // e.g. 10..20
    inset_frac: f32,     // e.g. 0.10 (ignore edges of the square)
) -> Option<(u16, f32)> {
    let bits = 4usize;
    let cells = bits + 2 * border_bits;
    let marker_side = (cells * cell_size_px) as i32;

    // square ROI in rectified pixels
    let s = px_per_square;
    let x0 = (sx as f32 * s + inset_frac * s).round() as i32;
    let y0 = (sy as f32 * s + inset_frac * s).round() as i32;
    let side = ((1.0 - 2.0 * inset_frac) * s).round() as i32;
    if side < 8 {
        return None;
    }

    // We sample directly from the rectified square by mapping canonical marker coords -> rect coords.
    // This avoids allocating a warped patch.
    let thr = otsu_threshold_patch(rect, x0, y0, side, side);

    // cell center mapping (canonical cell grid -> rect square)
    let cs = cell_size_px as f32;
    let scale = (side as f32) / (marker_side as f32);

    let mut border_ok = 0u32;
    let mut border_total = 0u32;

    let mut code: u16 = 0;

    for cy in 0..cells {
        for cx in 0..cells {
            // canonical coords in marker pixels at cell center
            let u = (cx as f32 + 0.5) * (cell_size_px as f32);
            let v = (cy as f32 + 0.5) * (cell_size_px as f32);

            // map to rect coords inside this square ROI
            let rx = x0 as f32 + u * scale;
            let ry = y0 as f32 + v * scale;

            // small 3x3 mean for robustness
            let mut sum = 0u32;
            let mut cnt = 0u32;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    sum += get_gray(rect, (rx as i32) + dx, (ry as i32) + dy) as u32;
                    cnt += 1;
                }
            }
            let m = (sum / cnt) as u8;
            let is_black = m < thr;

            let is_border = cx == 0 || cy == 0 || cx + 1 == cells || cy + 1 == cells;
            if is_border {
                border_total += 1;
                if is_black {
                    border_ok += 1;
                }
            } else {
                // inner 4x4 bits
                let bx = cx - border_bits;
                let by = cy - border_bits;
                let bit = if is_black { 1u16 } else { 0u16 };
                let idx = (by * bits + bx) as u16; // row-major
                code |= bit << idx;
            }
        }
    }

    let border_score = border_ok as f32 / border_total.max(1) as f32;
    if border_score < 0.85 {
        return None;
    }

    Some((code, border_score))
}
