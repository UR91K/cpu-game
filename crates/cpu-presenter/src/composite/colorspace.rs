#[inline(always)]
pub fn rgb2yiq(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    (
        0.2989 * r + 0.5870 * g + 0.1140 * b,
        0.5959 * r - 0.2744 * g - 0.3216 * b,
        0.2115 * r - 0.5229 * g + 0.3114 * b,
    )
}

#[inline(always)]
pub fn yiq2rgb(y: f32, i: f32, q: f32) -> (f32, f32, f32) {
    (
        y + 0.9560 * i + 0.6210 * q,
        y - 0.2720 * i - 0.6474 * q,
        y - 1.1060 * i + 1.7046 * q,
    )
}

#[inline(always)]
pub fn mix_mat_mul(y: f32, i: f32, q: f32, mat: &[[f32; 3]; 3]) -> (f32, f32, f32) {
    (
        mat[0][0] * y + mat[0][1] * i + mat[0][2] * q,
        mat[1][0] * y + mat[1][1] * i + mat[1][2] * q,
        mat[2][0] * y + mat[2][1] * i + mat[2][2] * q,
    )
}
