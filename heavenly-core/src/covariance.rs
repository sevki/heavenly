//! Uncertainty of a state: a 6×6 position/velocity covariance, and how it
//! moves with the state between frames.

use crate::{Error, FrameId, FrameResolver, StateVector, Transform, Vec3};

/// A 6×6 matrix, row-major: rows and columns are `x, y, z, vx, vy, vz`.
pub type Matrix6 = [[f64; 6]; 6];

/// The covariance of a state's position (m) and velocity (m/s): a symmetric,
/// positive semi-definite 6×6 matrix. Rows and columns are
/// `x, y, z, vx, vy, vz` in the frame of the state it belongs to (see
/// [`UncertainState`]), so its units are m², m²/s and m²/s².
///
/// Covariances in conjunction data messages are usually given in the
/// radial–transverse–normal frame; [`Covariance::from_rtn`] and
/// [`Covariance::to_rtn`] convert.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Covariance {
    m: Matrix6,
}

fn mul(a: &Matrix6, b: &Matrix6) -> Matrix6 {
    let mut o = [[0.0; 6]; 6];
    for (i, row) in o.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            *x = (0..6).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}

fn transpose(a: &Matrix6) -> Matrix6 {
    let mut o = [[0.0; 6]; 6];
    for (i, row) in a.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            o[j][i] = *x;
        }
    }
    o
}

/// `diag(q, q)` for a 3×3 rotation `q`.
fn block_rotation(q: [[f64; 3]; 3]) -> Matrix6 {
    let mut b = [[0.0; 6]; 6];
    for i in 0..3 {
        for j in 0..3 {
            b[i][j] = q[i][j];
            b[i + 3][j + 3] = q[i][j];
        }
    }
    b
}

impl Covariance {
    /// A covariance from a full matrix. Fails with [`Error::NonFinite`] on
    /// non-finite entries and [`Error::InvalidInput`] if the matrix is not
    /// symmetric (to 1e-9 of the diagonal scale) or not positive
    /// semi-definite. The two triangles are averaged.
    pub fn new(m: Matrix6) -> Result<Covariance, Error> {
        if m.iter().flatten().any(|x| !x.is_finite()) {
            return Err(Error::NonFinite);
        }
        for i in 0..6 {
            for j in 0..i {
                let scale = libm::sqrt(libm::fabs(m[i][i] * m[j][j]));
                if libm::fabs(m[i][j] - m[j][i]) > 1e-9 * scale + f64::MIN_POSITIVE {
                    return Err(Error::InvalidInput("a covariance must be symmetric"));
                }
            }
        }
        Covariance::symmetrised(m)
    }

    // Averages the two triangles, then checks that the result is positive
    // semi-definite. For products that are symmetric by construction
    // (`J C Jᵀ`, sums), where rounding alone breaks the symmetry.
    fn symmetrised(m: Matrix6) -> Result<Covariance, Error> {
        if m.iter().flatten().any(|x| !x.is_finite()) {
            return Err(Error::NonFinite);
        }
        let mut s = m;
        for i in 0..6 {
            for j in 0..i {
                let avg = 0.5 * (m[i][j] + m[j][i]);
                s[i][j] = avg;
                s[j][i] = avg;
            }
        }
        // Positive semi-definite: an LDLᵀ factorisation whose pivots are
        // not negative (beyond rounding).
        let scale = (0..6).map(|i| libm::fabs(s[i][i])).fold(0.0, f64::max);
        let mut l = [[0.0; 6]; 6];
        let mut d = [0.0; 6];
        for j in 0..6 {
            d[j] = s[j][j] - (0..j).map(|k| l[j][k] * l[j][k] * d[k]).sum::<f64>();
            if d[j] < -1e-10 * scale {
                return Err(Error::InvalidInput(
                    "a covariance must be positive semi-definite",
                ));
            }
            for i in j + 1..6 {
                let v = s[i][j] - (0..j).map(|k| l[i][k] * l[j][k] * d[k]).sum::<f64>();
                l[i][j] = if d[j] > 1e-14 * scale { v / d[j] } else { 0.0 };
            }
        }
        Ok(Covariance { m: s })
    }

    /// Independent errors with these standard deviations: position (m) and
    /// velocity (m/s) along each axis.
    pub fn diagonal(
        position_sigma_m: [f64; 3],
        velocity_sigma_m_s: [f64; 3],
    ) -> Result<Covariance, Error> {
        let mut m = [[0.0; 6]; 6];
        for i in 0..3 {
            m[i][i] = position_sigma_m[i] * position_sigma_m[i];
            m[i + 3][i + 3] = velocity_sigma_m_s[i] * velocity_sigma_m_s[i];
        }
        Covariance::new(m)
    }

    /// The full matrix.
    pub fn matrix(&self) -> Matrix6 {
        self.m
    }

    /// The position block (m²).
    pub fn position(&self) -> [[f64; 3]; 3] {
        let m = &self.m;
        [
            [m[0][0], m[0][1], m[0][2]],
            [m[1][0], m[1][1], m[1][2]],
            [m[2][0], m[2][1], m[2][2]],
        ]
    }

    /// Standard deviations of the six components: position (m) along each
    /// axis, then velocity (m/s).
    pub fn sigmas(&self) -> [f64; 6] {
        core::array::from_fn(|i| libm::sqrt(self.m[i][i]))
    }

    /// `J C Jᵀ`: the covariance of `J·x` for a linear map `J`, such as a
    /// frame change or a state transition matrix.
    pub fn transformed(&self, j: &Matrix6) -> Result<Covariance, Error> {
        Covariance::symmetrised(mul(&mul(j, &self.m), &transpose(j)))
    }

    /// The sum of two covariances: that of the difference of two independent
    /// states, as in a conjunction.
    pub fn plus(&self, other: &Covariance) -> Result<Covariance, Error> {
        let mut m = self.m;
        for (row, orow) in m.iter_mut().zip(other.m.iter()) {
            for (x, o) in row.iter_mut().zip(orow) {
                *x += o;
            }
        }
        Covariance::symmetrised(m)
    }

    /// The radial–transverse–normal axes of `state` as rows: radial along
    /// the position, normal along the angular momentum, transverse
    /// completing the right-handed set (along the velocity for a circular
    /// orbit).
    pub fn rtn_axes(state: &StateVector) -> Result<[[f64; 3]; 3], Error> {
        let r = state
            .position_m
            .normalized()
            .ok_or(Error::InvalidInput("RTN needs a non-zero position"))?;
        let n = state
            .position_m
            .cross(state.velocity_m_s)
            .normalized()
            .ok_or(Error::InvalidInput(
                "RTN needs position and velocity that are not parallel",
            ))?;
        let t = n.cross(r);
        Ok([r.to_array(), t.to_array(), n.to_array()])
    }

    /// This covariance (in `state`'s frame) in `state`'s radial–transverse–
    /// normal axes. Velocity components are the frame's velocity resolved
    /// along those axes, as in CCSDS conjunction data messages.
    pub fn to_rtn(&self, state: &StateVector) -> Result<Covariance, Error> {
        self.transformed(&block_rotation(Covariance::rtn_axes(state)?))
    }

    /// A covariance given in `state`'s radial–transverse–normal axes, in
    /// `state`'s frame. The inverse of [`to_rtn`](Self::to_rtn).
    pub fn from_rtn(rtn: &Covariance, state: &StateVector) -> Result<Covariance, Error> {
        let q = Covariance::rtn_axes(state)?;
        let qt = [
            [q[0][0], q[1][0], q[2][0]],
            [q[0][1], q[1][1], q[2][1]],
            [q[0][2], q[1][2], q[2][2]],
        ];
        rtn.transformed(&block_rotation(qt))
    }
}

/// The Jacobian of a frame transform acting on `(position, velocity)`:
/// `[[R, 0], [[ω]× R, R]]`. Transforms are affine in the state, so this is
/// exact, and it is the same at every state.
pub fn transform_jacobian(t: &Transform) -> Matrix6 {
    let r = t.rotation.to_matrix();
    let w = t.angular_velocity_rad_s;
    // [ω]× R, column by column: ω × (column of R).
    let mut j = block_rotation(r);
    for c in 0..3 {
        let col = Vec3::new(r[0][c], r[1][c], r[2][c]);
        let wc = w.cross(col);
        j[3][c] = wc.x;
        j[4][c] = wc.y;
        j[5][c] = wc.z;
    }
    j
}

/// A state with its covariance, both in the state's frame and at its epoch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UncertainState {
    pub state: StateVector,
    pub covariance: Covariance,
}

impl UncertainState {
    pub const fn new(state: StateVector, covariance: Covariance) -> UncertainState {
        UncertainState { state, covariance }
    }

    /// The state and its covariance in another frame. The covariance goes
    /// through the transform's exact Jacobian, so a rotating frame mixes
    /// position uncertainty into velocity uncertainty, as it should.
    pub fn to_frame<R: FrameResolver + ?Sized>(
        &self,
        destination: FrameId,
        resolver: &R,
    ) -> Result<UncertainState, R::Error>
    where
        R::Error: From<Error>,
    {
        let t = resolver.transform(self.state.frame, destination, self.state.epoch)?;
        let state = t.apply(&self.state)?;
        let covariance = self
            .covariance
            .transformed(&transform_jacobian(&t.transform))?;
        Ok(UncertainState { state, covariance })
    }
}
