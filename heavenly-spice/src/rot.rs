//! SPICE-style frame-rotation matrices (`[θ]ᵢ` rotates the axes by θ).

pub(crate) type Mat3 = [[f64; 3]; 3];

pub(crate) fn axis(i: u8, theta: f64) -> Mat3 {
    let (s, c) = (libm::sin(theta), libm::cos(theta));
    match i {
        1 => [[1.0, 0.0, 0.0], [0.0, c, s], [0.0, -s, c]],
        2 => [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]],
        _ => [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]],
    }
}

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
