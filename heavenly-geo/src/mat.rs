//! Row-major 3×3 matrix helpers using SOFA's *frame-rotation* convention:
//! `rx`, `ry`, `rz` pre-multiply by the matrix that rotates the coordinate
//! axes (not the vector) by the given angle.

pub(crate) type Mat3 = [[f64; 3]; 3];

pub(crate) const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

pub(crate) fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut r = [[0.0; 3]; 3];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    r
}

pub(crate) fn transpose(a: &Mat3) -> Mat3 {
    let mut r = [[0.0; 3]; 3];
    for (i, row) in a.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            r[j][i] = *v;
        }
    }
    r
}

/// `Rx(φ) · r`.
pub(crate) fn rx(phi: f64, r: &Mat3) -> Mat3 {
    let (s, c) = (libm::sin(phi), libm::cos(phi));
    mul(&[[1.0, 0.0, 0.0], [0.0, c, s], [0.0, -s, c]], r)
}

/// `Ry(θ) · r`.
pub(crate) fn ry(theta: f64, r: &Mat3) -> Mat3 {
    let (s, c) = (libm::sin(theta), libm::cos(theta));
    mul(&[[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]], r)
}

/// `Rz(ψ) · r`.
pub(crate) fn rz(psi: f64, r: &Mat3) -> Mat3 {
    let (s, c) = (libm::sin(psi), libm::cos(psi));
    mul(&[[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]], r)
}

pub(crate) const DEG: f64 = core::f64::consts::PI / 180.0;
pub(crate) const ARCSEC: f64 = DEG / 3600.0;
pub(crate) const TAU: f64 = 2.0 * core::f64::consts::PI;
