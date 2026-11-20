//! SGP4/SDP4: the propagator for NORAD two-line element sets (TLEs).
//!
//! This is a port of David Vallado's reference implementation ("Revisiting
//! Spacetrack Report #3", AIAA 2006-6753, revision of 2020). It includes the
//! deep-space branch: lunar–solar perturbations, and 12-hour and
//! synchronous resonance integration. The tests compare it with Vallado's C++
//! (via the `sgp4` Python package) over the official verification set.
//!
//! Output is in the TEME frame (`FrameId::TEME`), in km and km/s, at a time
//! `tsince` minutes from the TLE epoch. A TLE is only meaningful through SGP4:
//! its elements are SGP4 mean elements, not osculating ones, so do not feed
//! them to a two-body propagator.

// Constants are transcribed digit-for-digit from the reference implementation.
#![allow(clippy::excessive_precision)]

use heavenly_core::{Epoch, Error, FrameId, FrameResolver, Locator, StateVector, Vec3};

use crate::tle::Tle;

const TWOPI: f64 = 2.0 * core::f64::consts::PI;
const PI: f64 = core::f64::consts::PI;
const DEG2RAD: f64 = PI / 180.0;
const X2O3: f64 = 2.0 / 3.0;

/// Earth gravity model used by SGP4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Gravity {
    /// WGS-72 with the historical rounded `xke`.
    Wgs72Old,
    /// WGS-72: the model TLEs are generated with, and the usual choice.
    #[default]
    Wgs72,
    /// WGS-84.
    Wgs84,
}

/// Operation mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OpsMode {
    /// Vallado's improved mode, the reference default.
    #[default]
    Improved,
    /// The original AFSPC mode, for matching legacy operational output.
    Afspc,
}

/// Why SGP4 could not produce a state, matching the reference error codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sgp4Error {
    /// 1: mean eccentricity out of range (≥ 1 or < −0.001).
    MeanEccentricity,
    /// 2: mean motion became negative.
    MeanMotion,
    /// 3: perturbed eccentricity out of range.
    PerturbedEccentricity,
    /// 4: semi-latus rectum negative.
    SemiLatusRectum,
    /// 6: the orbit radius is below one Earth radius: the satellite has decayed.
    Decayed,
}

impl Sgp4Error {
    /// The reference implementation's numeric error code.
    pub fn code(self) -> u8 {
        match self {
            Sgp4Error::MeanEccentricity => 1,
            Sgp4Error::MeanMotion => 2,
            Sgp4Error::PerturbedEccentricity => 3,
            Sgp4Error::SemiLatusRectum => 4,
            Sgp4Error::Decayed => 6,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Sgp4Error::MeanEccentricity => "SGP4: mean eccentricity out of range",
            Sgp4Error::MeanMotion => "SGP4: mean motion is negative",
            Sgp4Error::PerturbedEccentricity => "SGP4: perturbed eccentricity out of range",
            Sgp4Error::SemiLatusRectum => "SGP4: semi-latus rectum is negative",
            Sgp4Error::Decayed => "SGP4: the satellite has decayed",
        }
    }
}

impl From<Sgp4Error> for Error {
    fn from(e: Sgp4Error) -> Error {
        Error::InvalidInput(e.message())
    }
}

impl core::fmt::Display for Sgp4Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} (error {})", self.message(), self.code())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Sgp4Error {}

#[derive(Clone, Copy, Debug)]
struct GravConst {
    radius_km: f64,
    xke: f64,
    j2: f64,
    j3oj2: f64,
    j4: f64,
}

impl Gravity {
    fn constants(self) -> GravConst {
        let (mu, re, j2, j3, j4, xke): (f64, f64, f64, f64, f64, Option<f64>) = match self {
            Gravity::Wgs72Old => (
                398600.79964,
                6378.135,
                0.001082616,
                -0.00000253881,
                -0.00000165597,
                Some(0.0743669161),
            ),
            Gravity::Wgs72 => (
                398600.8,
                6378.135,
                0.001082616,
                -0.00000253881,
                -0.00000165597,
                None,
            ),
            Gravity::Wgs84 => (
                398600.5,
                6378.137,
                0.00108262998905,
                -0.00000253215306,
                -0.00000161098761,
                None,
            ),
        };
        let xke = xke.unwrap_or_else(|| 60.0 / libm::sqrt(re * re * re / mu));
        GravConst {
            radius_km: re,
            xke,
            j2,
            j3oj2: j3 / j2,
            j4,
        }
    }
}

/// Deep-space (SDP4) coefficients.
#[derive(Clone, Copy, Debug, Default)]
struct Deep {
    irez: u8,
    d2201: f64,
    d2211: f64,
    d3210: f64,
    d3222: f64,
    d4410: f64,
    d4422: f64,
    d5220: f64,
    d5232: f64,
    d5421: f64,
    d5433: f64,
    dedt: f64,
    del1: f64,
    del2: f64,
    del3: f64,
    didt: f64,
    dmdt: f64,
    dnodt: f64,
    domdt: f64,
    e3: f64,
    ee2: f64,
    peo: f64,
    pgho: f64,
    pho: f64,
    pinco: f64,
    plo: f64,
    se2: f64,
    se3: f64,
    sgh2: f64,
    sgh3: f64,
    sgh4: f64,
    sh2: f64,
    sh3: f64,
    si2: f64,
    si3: f64,
    sl2: f64,
    sl3: f64,
    sl4: f64,
    xfact: f64,
    xgh2: f64,
    xgh3: f64,
    xgh4: f64,
    xh2: f64,
    xh3: f64,
    xi2: f64,
    xi3: f64,
    xl2: f64,
    xl3: f64,
    xl4: f64,
    xlamo: f64,
    zmol: f64,
    zmos: f64,
}

/// An initialised SGP4 propagator for one TLE.
#[derive(Clone, Debug)]
pub struct Sgp4 {
    grav: GravConst,
    opsmode: OpsMode,
    deep_space: bool,
    isimp: bool,
    epoch: Epoch,
    sgp4_epoch: f64,
    // Mean elements at epoch.
    bstar: f64,
    ecco: f64,
    argpo: f64,
    inclo: f64,
    mo: f64,
    no_unkozai: f64,
    nodeo: f64,
    // Near-earth coefficients.
    aycof: f64,
    con41: f64,
    cc1: f64,
    cc4: f64,
    cc5: f64,
    d2: f64,
    d3: f64,
    d4: f64,
    delmo: f64,
    eta: f64,
    argpdot: f64,
    omgcof: f64,
    sinmao: f64,
    t2cof: f64,
    t3cof: f64,
    t4cof: f64,
    t5cof: f64,
    x1mth2: f64,
    x7thm1: f64,
    mdot: f64,
    nodedot: f64,
    xlcof: f64,
    xmcof: f64,
    nodecf: f64,
    gsto: f64,
    deep: Deep,
}

/// `fmod` with C semantics (the reference implementation's angle wrapping).
fn fmod(a: f64, b: f64) -> f64 {
    a % b
}

/// Greenwich mean sidereal time (IAU 1982) for a UT1 Julian date, in rad.
pub(crate) fn gstime(jdut1: f64) -> f64 {
    let tut1 = (jdut1 - 2451545.0) / 36525.0;
    let mut temp = -6.2e-6 * tut1 * tut1 * tut1
        + 0.093104 * tut1 * tut1
        + (876600.0 * 3600.0 + 8640184.812866) * tut1
        + 67310.54841;
    temp = fmod(temp * DEG2RAD / 240.0, TWOPI);
    if temp < 0.0 {
        temp += TWOPI;
    }
    temp
}

impl Sgp4 {
    /// Initialises SGP4 for a TLE with the given gravity model and mode.
    ///
    /// Fails if the elements are unusable at epoch, for example because the
    /// satellite has already decayed.
    pub fn new(tle: &Tle, gravity: Gravity, opsmode: OpsMode) -> Result<Sgp4, Error> {
        let xpdotp = 1440.0 / TWOPI;
        Ok(Sgp4::from_elements(
            gravity,
            opsmode,
            tle.epoch()?,
            tle.days_since_1950(),
            tle.bstar,
            tle.eccentricity,
            tle.arg_perigee_deg * DEG2RAD,
            tle.inclination_deg * DEG2RAD,
            tle.mean_anomaly_deg * DEG2RAD,
            tle.mean_motion_rev_day / xpdotp,
            tle.raan_deg * DEG2RAD,
        )?)
    }

    /// Initialises SGP4 from an OMM, following the reference
    /// implementation's own OMM reader (`sgp4.omm.initialize`): the mean
    /// motion is converted as `n / 720 · π`, and the epoch is counted in
    /// microseconds, which round slightly differently from a TLE's.
    #[cfg(feature = "alloc")]
    pub fn from_omm(omm: &crate::Omm, gravity: Gravity, opsmode: OpsMode) -> Result<Sgp4, Error> {
        Ok(Sgp4::from_elements(
            gravity,
            opsmode,
            omm.epoch,
            omm.days_since_1950(),
            omm.bstar,
            omm.eccentricity,
            omm.arg_pericenter_deg * DEG2RAD,
            omm.inclination_deg * DEG2RAD,
            omm.mean_anomaly_deg * DEG2RAD,
            omm.mean_motion_rev_day / 720.0 * PI,
            omm.raan_deg * DEG2RAD,
        )?)
    }

    /// The TLE epoch.
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// The TLE epoch in SGP4's own count: days since 1949-12-31 00:00 UTC.
    pub fn sgp4_epoch(&self) -> f64 {
        self.sgp4_epoch
    }

    /// Whether the deep-space (SDP4) branch is used (period ≥ 225 min).
    pub fn is_deep_space(&self) -> bool {
        self.deep_space
    }

    #[allow(clippy::too_many_arguments)]
    fn from_elements(
        gravity: Gravity,
        opsmode: OpsMode,
        epoch_utc: Epoch,
        epoch: f64,
        bstar: f64,
        ecco: f64,
        argpo: f64,
        inclo: f64,
        mo: f64,
        no_kozai: f64,
        nodeo: f64,
    ) -> Result<Sgp4, Sgp4Error> {
        let g = gravity.constants();
        let (re, xke, j2, j3oj2, j4) = (g.radius_km, g.xke, g.j2, g.j3oj2, g.j4);
        let temp4 = 1.5e-12;
        let ss = 78.0 / re + 1.0;
        let qzms2ttemp = (120.0 - 78.0) / re;
        let qzms2t = qzms2ttemp * qzms2ttemp * qzms2ttemp * qzms2ttemp;

        // initl
        let eccsq = ecco * ecco;
        let omeosq = 1.0 - eccsq;
        let rteosq = libm::sqrt(omeosq);
        let cosio = libm::cos(inclo);
        let cosio2 = cosio * cosio;
        let ak = libm::pow(xke / no_kozai, X2O3);
        let d1 = 0.75 * j2 * (3.0 * cosio2 - 1.0) / (rteosq * omeosq);
        let mut del = d1 / (ak * ak);
        let adel = ak * (1.0 - del * del - del * (1.0 / 3.0 + 134.0 * del * del / 81.0));
        del = d1 / (adel * adel);
        let no_unkozai = no_kozai / (1.0 + del);
        let ao = libm::pow(xke / no_unkozai, X2O3);
        let sinio = libm::sin(inclo);
        let po = ao * omeosq;
        let con42 = 1.0 - 5.0 * cosio2;
        let con41 = -con42 - cosio2 - cosio2;
        let posq = po * po;
        let rp = ao * (1.0 - ecco);
        let gsto = match opsmode {
            OpsMode::Afspc => {
                let ts70 = epoch - 7305.0;
                let ds70 = libm::floor(ts70 + 1.0e-8);
                let tfrac = ts70 - ds70;
                let c1 = 1.72027916940703639e-2;
                let thgr70 = 1.7321343856509374;
                let fk5r = 5.07551419432269442e-15;
                let c1p2p = c1 + TWOPI;
                let mut gsto = fmod(
                    thgr70 + c1 * ds70 + c1p2p * tfrac + ts70 * ts70 * fk5r,
                    TWOPI,
                );
                if gsto < 0.0 {
                    gsto += TWOPI;
                }
                gsto
            }
            OpsMode::Improved => gstime(epoch + 2433281.5),
        };

        let mut s = Sgp4 {
            grav: g,
            opsmode,
            deep_space: false,
            isimp: false,
            epoch: epoch_utc,
            sgp4_epoch: epoch,
            bstar,
            ecco,
            argpo,
            inclo,
            mo,
            no_unkozai,
            nodeo,
            aycof: 0.0,
            con41,
            cc1: 0.0,
            cc4: 0.0,
            cc5: 0.0,
            d2: 0.0,
            d3: 0.0,
            d4: 0.0,
            delmo: 0.0,
            eta: 0.0,
            argpdot: 0.0,
            omgcof: 0.0,
            sinmao: 0.0,
            t2cof: 0.0,
            t3cof: 0.0,
            t4cof: 0.0,
            t5cof: 0.0,
            x1mth2: 0.0,
            x7thm1: 0.0,
            mdot: 0.0,
            nodedot: 0.0,
            xlcof: 0.0,
            xmcof: 0.0,
            nodecf: 0.0,
            gsto,
            deep: Deep::default(),
        };

        if omeosq >= 0.0 || no_unkozai >= 0.0 {
            s.isimp = rp < 220.0 / re + 1.0;
            let mut sfour = ss;
            let mut qzms24 = qzms2t;
            let perige = (rp - 1.0) * re;
            if perige < 156.0 {
                sfour = perige - 78.0;
                if perige < 98.0 {
                    sfour = 20.0;
                }
                let q = (120.0 - sfour) / re;
                qzms24 = q * q * q * q;
                sfour = sfour / re + 1.0;
            }
            let pinvsq = 1.0 / posq;
            let tsi = 1.0 / (ao - sfour);
            s.eta = ao * ecco * tsi;
            let etasq = s.eta * s.eta;
            let eeta = ecco * s.eta;
            let psisq = libm::fabs(1.0 - etasq);
            let coef = qzms24 * libm::pow(tsi, 4.0);
            let coef1 = coef / libm::pow(psisq, 3.5);
            let cc2 = coef1
                * no_unkozai
                * (ao * (1.0 + 1.5 * etasq + eeta * (4.0 + etasq))
                    + 0.375 * j2 * tsi / psisq * con41 * (8.0 + 3.0 * etasq * (8.0 + etasq)));
            s.cc1 = bstar * cc2;
            let mut cc3 = 0.0;
            if ecco > 1.0e-4 {
                cc3 = -2.0 * coef * tsi * j3oj2 * no_unkozai * sinio / ecco;
            }
            s.x1mth2 = 1.0 - cosio2;
            s.cc4 = 2.0
                * no_unkozai
                * coef1
                * ao
                * omeosq
                * (s.eta * (2.0 + 0.5 * etasq) + ecco * (0.5 + 2.0 * etasq)
                    - j2 * tsi / (ao * psisq)
                        * (-3.0 * con41 * (1.0 - 2.0 * eeta + etasq * (1.5 - 0.5 * eeta))
                            + 0.75
                                * s.x1mth2
                                * (2.0 * etasq - eeta * (1.0 + etasq))
                                * libm::cos(2.0 * argpo)));
            s.cc5 = 2.0 * coef1 * ao * omeosq * (1.0 + 2.75 * (etasq + eeta) + eeta * etasq);
            let cosio4 = cosio2 * cosio2;
            let temp1 = 1.5 * j2 * pinvsq * no_unkozai;
            let temp2 = 0.5 * temp1 * j2 * pinvsq;
            let temp3 = -0.46875 * j4 * pinvsq * pinvsq * no_unkozai;
            s.mdot = no_unkozai
                + 0.5 * temp1 * rteosq * con41
                + 0.0625 * temp2 * rteosq * (13.0 - 78.0 * cosio2 + 137.0 * cosio4);
            s.argpdot = -0.5 * temp1 * con42
                + 0.0625 * temp2 * (7.0 - 114.0 * cosio2 + 395.0 * cosio4)
                + temp3 * (3.0 - 36.0 * cosio2 + 49.0 * cosio4);
            let xhdot1 = -temp1 * cosio;
            s.nodedot = xhdot1
                + (0.5 * temp2 * (4.0 - 19.0 * cosio2) + 2.0 * temp3 * (3.0 - 7.0 * cosio2))
                    * cosio;
            let xpidot = s.argpdot + s.nodedot;
            s.omgcof = bstar * cc3 * libm::cos(argpo);
            s.xmcof = 0.0;
            if ecco > 1.0e-4 {
                s.xmcof = -X2O3 * coef * bstar / eeta;
            }
            s.nodecf = 3.5 * omeosq * xhdot1 * s.cc1;
            s.t2cof = 1.5 * s.cc1;
            s.xlcof = if libm::fabs(cosio + 1.0) > 1.5e-12 {
                -0.25 * j3oj2 * sinio * (3.0 + 5.0 * cosio) / (1.0 + cosio)
            } else {
                -0.25 * j3oj2 * sinio * (3.0 + 5.0 * cosio) / temp4
            };
            s.aycof = -0.5 * j3oj2 * sinio;
            let delmotemp = 1.0 + s.eta * libm::cos(mo);
            s.delmo = delmotemp * delmotemp * delmotemp;
            s.sinmao = libm::sin(mo);
            s.x7thm1 = 7.0 * cosio2 - 1.0;

            if TWOPI / no_unkozai >= 225.0 {
                s.deep_space = true;
                s.isimp = true;
                let tc = 0.0;
                let ds = dscom(epoch, ecco, argpo, tc, inclo, nodeo, no_unkozai);
                s.deep.e3 = ds.e3;
                s.deep.ee2 = ds.ee2;
                s.deep.peo = ds.peo;
                s.deep.pgho = ds.pgho;
                s.deep.pho = ds.pho;
                s.deep.pinco = ds.pinco;
                s.deep.plo = ds.plo;
                s.deep.se2 = ds.se2;
                s.deep.se3 = ds.se3;
                s.deep.sgh2 = ds.sgh2;
                s.deep.sgh3 = ds.sgh3;
                s.deep.sgh4 = ds.sgh4;
                s.deep.sh2 = ds.sh2;
                s.deep.sh3 = ds.sh3;
                s.deep.si2 = ds.si2;
                s.deep.si3 = ds.si3;
                s.deep.sl2 = ds.sl2;
                s.deep.sl3 = ds.sl3;
                s.deep.sl4 = ds.sl4;
                s.deep.xgh2 = ds.xgh2;
                s.deep.xgh3 = ds.xgh3;
                s.deep.xgh4 = ds.xgh4;
                s.deep.xh2 = ds.xh2;
                s.deep.xh3 = ds.xh3;
                s.deep.xi2 = ds.xi2;
                s.deep.xi3 = ds.xi3;
                s.deep.xl2 = ds.xl2;
                s.deep.xl3 = ds.xl3;
                s.deep.xl4 = ds.xl4;
                s.deep.zmol = ds.zmol;
                s.deep.zmos = ds.zmos;
                // dpper with init = 'y' leaves the elements unchanged.
                dsinit(&mut s, &ds, xke, xpidot, eccsq);
            }

            if !s.isimp {
                let cc1sq = s.cc1 * s.cc1;
                s.d2 = 4.0 * ao * tsi * cc1sq;
                let temp = s.d2 * tsi * s.cc1 / 3.0;
                s.d3 = (17.0 * ao + sfour) * temp;
                s.d4 = 0.5 * temp * ao * tsi * (221.0 * ao + 31.0 * sfour) * s.cc1;
                s.t3cof = s.d2 + 2.0 * cc1sq;
                s.t4cof = 0.25 * (3.0 * s.d3 + s.cc1 * (12.0 * s.d2 + 10.0 * cc1sq));
                s.t5cof = 0.2
                    * (3.0 * s.d4
                        + 12.0 * s.cc1 * s.d3
                        + 6.0 * s.d2 * s.d2
                        + 15.0 * cc1sq * (2.0 * s.d2 + cc1sq));
            }
        }
        // The reference initialiser propagates to epoch to report errors.
        s.propagate(0.0)?;
        Ok(s)
    }

    /// Position (km) and velocity (km/s) in TEME, `tsince_min` minutes after
    /// the TLE epoch.
    pub fn propagate(&self, tsince_min: f64) -> Result<([f64; 3], [f64; 3]), Sgp4Error> {
        let GravConst {
            radius_km: re,
            xke,
            j2,
            j3oj2,
            ..
        } = self.grav;
        let temp4 = 1.5e-12;
        let vkmpersec = re * xke / 60.0;
        let t = tsince_min;

        let xmdf = self.mo + self.mdot * t;
        let argpdf = self.argpo + self.argpdot * t;
        let nodedf = self.nodeo + self.nodedot * t;
        let mut argpm = argpdf;
        let mut mm = xmdf;
        let t2 = t * t;
        let mut nodem = nodedf + self.nodecf * t2;
        let mut tempa = 1.0 - self.cc1 * t;
        let mut tempe = self.bstar * self.cc4 * t;
        let mut templ = self.t2cof * t2;

        if !self.isimp {
            let delomg = self.omgcof * t;
            let delmtemp = 1.0 + self.eta * libm::cos(xmdf);
            let delm = self.xmcof * (delmtemp * delmtemp * delmtemp - self.delmo);
            let temp = delomg + delm;
            mm = xmdf + temp;
            argpm = argpdf - temp;
            let t3 = t2 * t;
            let t4 = t3 * t;
            tempa = tempa - self.d2 * t2 - self.d3 * t3 - self.d4 * t4;
            tempe += self.bstar * self.cc5 * (libm::sin(mm) - self.sinmao);
            templ = templ + self.t3cof * t3 + t4 * (self.t4cof + t * self.t5cof);
        }

        let mut nm = self.no_unkozai;
        let mut em = self.ecco;
        let mut inclm = self.inclo;
        if self.deep_space {
            let r = dspace(self, t, em, argpm, inclm, mm, nodem, nm);
            em = r.em;
            argpm = r.argpm;
            inclm = r.inclm;
            mm = r.mm;
            nodem = r.nodem;
            nm = r.nm;
        }

        if nm <= 0.0 {
            return Err(Sgp4Error::MeanMotion);
        }
        let am = libm::pow(xke / nm, X2O3) * tempa * tempa;
        nm = xke / libm::pow(am, 1.5);
        em -= tempe;
        if !(-0.001..1.0).contains(&em) {
            return Err(Sgp4Error::MeanEccentricity);
        }
        if em < 1.0e-6 {
            em = 1.0e-6;
        }
        mm += self.no_unkozai * templ;
        let mut xlm = mm + argpm + nodem;
        nodem = fmod(nodem, TWOPI);
        argpm = fmod(argpm, TWOPI);
        xlm = fmod(xlm, TWOPI);
        mm = fmod(xlm - argpm - nodem, TWOPI);

        let sinim = libm::sin(inclm);
        let cosim = libm::cos(inclm);
        let mut ep = em;
        let mut xincp = inclm;
        let mut argpp = argpm;
        let mut nodep = nodem;
        let mut mp = mm;
        let mut sinip = sinim;
        let mut cosip = cosim;
        let (mut aycof, mut xlcof) = (self.aycof, self.xlcof);
        let (mut con41, mut x1mth2, mut x7thm1) = (self.con41, self.x1mth2, self.x7thm1);

        if self.deep_space {
            dpper(
                &self.deep,
                t,
                false,
                &mut ep,
                &mut xincp,
                &mut nodep,
                &mut argpp,
                &mut mp,
                self.opsmode,
            );
            if xincp < 0.0 {
                xincp = -xincp;
                nodep += PI;
                argpp -= PI;
            }
            if !(0.0..=1.0).contains(&ep) {
                return Err(Sgp4Error::PerturbedEccentricity);
            }
            // Long-period periodics depend on the perturbed inclination.
            sinip = libm::sin(xincp);
            cosip = libm::cos(xincp);
            aycof = -0.5 * j3oj2 * sinip;
            xlcof = if libm::fabs(cosip + 1.0) > 1.5e-12 {
                -0.25 * j3oj2 * sinip * (3.0 + 5.0 * cosip) / (1.0 + cosip)
            } else {
                -0.25 * j3oj2 * sinip * (3.0 + 5.0 * cosip) / temp4
            };
        }

        let axnl = ep * libm::cos(argpp);
        let temp = 1.0 / (am * (1.0 - ep * ep));
        let aynl = ep * libm::sin(argpp) + temp * aycof;
        let xl = mp + argpp + nodep + temp * xlcof * axnl;

        // Kepler's equation.
        let u = fmod(xl - nodep, TWOPI);
        let mut eo1 = u;
        let mut tem5: f64 = 9999.9;
        let mut ktr = 1;
        let (mut sineo1, mut coseo1) = (0.0, 0.0);
        while libm::fabs(tem5) >= 1.0e-12 && ktr <= 10 {
            sineo1 = libm::sin(eo1);
            coseo1 = libm::cos(eo1);
            tem5 = 1.0 - coseo1 * axnl - sineo1 * aynl;
            tem5 = (u - aynl * coseo1 + axnl * sineo1 - eo1) / tem5;
            if libm::fabs(tem5) >= 0.95 {
                tem5 = if tem5 > 0.0 { 0.95 } else { -0.95 };
            }
            eo1 += tem5;
            ktr += 1;
        }

        // Short-period periodics.
        let ecose = axnl * coseo1 + aynl * sineo1;
        let esine = axnl * sineo1 - aynl * coseo1;
        let el2 = axnl * axnl + aynl * aynl;
        let pl = am * (1.0 - el2);
        if pl < 0.0 {
            return Err(Sgp4Error::SemiLatusRectum);
        }
        let rl = am * (1.0 - ecose);
        let rdotl = libm::sqrt(am) * esine / rl;
        let rvdotl = libm::sqrt(pl) / rl;
        let betal = libm::sqrt(1.0 - el2);
        let temp = esine / (1.0 + betal);
        let sinu = am / rl * (sineo1 - aynl - axnl * temp);
        let cosu = am / rl * (coseo1 - axnl + aynl * temp);
        let mut su = libm::atan2(sinu, cosu);
        let sin2u = (cosu + cosu) * sinu;
        let cos2u = 1.0 - 2.0 * sinu * sinu;
        let temp = 1.0 / pl;
        let temp1 = 0.5 * j2 * temp;
        let temp2 = temp1 * temp;

        if self.deep_space {
            let cosisq = cosip * cosip;
            con41 = 3.0 * cosisq - 1.0;
            x1mth2 = 1.0 - cosisq;
            x7thm1 = 7.0 * cosisq - 1.0;
        }
        let mrt = rl * (1.0 - 1.5 * temp2 * betal * con41) + 0.5 * temp1 * x1mth2 * cos2u;
        su -= 0.25 * temp2 * x7thm1 * sin2u;
        let xnode = nodep + 1.5 * temp2 * cosip * sin2u;
        let xinc = xincp + 1.5 * temp2 * cosip * sinip * cos2u;
        let mvt = rdotl - nm * temp1 * x1mth2 * sin2u / xke;
        let rvdot = rvdotl + nm * temp1 * (x1mth2 * cos2u + 1.5 * con41) / xke;

        let (sinsu, cossu) = (libm::sin(su), libm::cos(su));
        let (snod, cnod) = (libm::sin(xnode), libm::cos(xnode));
        let (sini, cosi) = (libm::sin(xinc), libm::cos(xinc));
        let xmx = -snod * cosi;
        let xmy = cnod * cosi;
        let ux = xmx * sinsu + cnod * cossu;
        let uy = xmy * sinsu + snod * cossu;
        let uz = sini * sinsu;
        let vx = xmx * cossu - cnod * sinsu;
        let vy = xmy * cossu - snod * sinsu;
        let vz = sini * cossu;

        if mrt < 1.0 {
            return Err(Sgp4Error::Decayed);
        }
        let mr = mrt * re;
        Ok((
            [mr * ux, mr * uy, mr * uz],
            [
                (mvt * ux + rvdot * vx) * vkmpersec,
                (mvt * uy + rvdot * vy) * vkmpersec,
                (mvt * uz + rvdot * vz) * vkmpersec,
            ],
        ))
    }

    /// Minutes from the TLE epoch to `epoch`, as SGP4 counts them: in UTC
    /// days of 86400 s, so a leap second in between is not counted (as in
    /// the reference implementation, which works with UTC Julian dates).
    pub fn minutes_since_epoch(&self, epoch: Epoch) -> Result<f64, Error> {
        let leaps = epoch.tai_minus_utc_seconds()? - self.epoch.tai_minus_utc_seconds()?;
        Ok((epoch.seconds_since(self.epoch) - leaps as f64) / 60.0)
    }

    /// The state at `epoch` in TEME, in metres and m/s.
    pub fn state(&self, epoch: Epoch) -> Result<StateVector, Error> {
        let tsince = self.minutes_since_epoch(epoch)?;
        let (r, v) = self.propagate(tsince)?;
        Ok(StateVector::new(
            FrameId::TEME,
            epoch,
            Vec3::from(r) * 1e3,
            Vec3::from(v) * 1e3,
        ))
    }
}

impl Locator for Sgp4 {
    type Error = Error;

    fn state_at<R: FrameResolver + ?Sized>(
        &self,
        epoch: Epoch,
        frame: FrameId,
        resolver: &R,
    ) -> Result<StateVector, Error>
    where
        Error: From<R::Error>,
    {
        Ok(self.state(epoch)?.to_frame(frame, resolver)?)
    }
}

/// Outputs of `dscom` needed later.
struct Dscom {
    sinim: f64,
    cosim: f64,
    emsq: f64,
    s1: f64,
    s2: f64,
    s3: f64,
    s4: f64,
    s5: f64,
    ss1: f64,
    ss2: f64,
    ss3: f64,
    ss4: f64,
    ss5: f64,
    sz1: f64,
    sz3: f64,
    sz11: f64,
    sz13: f64,
    sz21: f64,
    sz23: f64,
    sz31: f64,
    sz33: f64,
    z1: f64,
    z3: f64,
    z11: f64,
    z13: f64,
    z21: f64,
    z23: f64,
    z31: f64,
    z33: f64,
    e3: f64,
    ee2: f64,
    peo: f64,
    pgho: f64,
    pho: f64,
    pinco: f64,
    plo: f64,
    se2: f64,
    se3: f64,
    sgh2: f64,
    sgh3: f64,
    sgh4: f64,
    sh2: f64,
    sh3: f64,
    si2: f64,
    si3: f64,
    sl2: f64,
    sl3: f64,
    sl4: f64,
    xgh2: f64,
    xgh3: f64,
    xgh4: f64,
    xh2: f64,
    xh3: f64,
    xi2: f64,
    xi3: f64,
    xl2: f64,
    xl3: f64,
    xl4: f64,
    zmol: f64,
    zmos: f64,
    em: f64,
    nm: f64,
}

/// Lunar–solar terms for the deep-space branch.
fn dscom(epoch: f64, ep: f64, argpp: f64, tc: f64, inclp: f64, nodep: f64, np: f64) -> Dscom {
    let zes = 0.01675;
    let zel = 0.05490;
    let c1ss = 2.9864797e-6;
    let c1l = 4.7968065e-7;
    let zsinis = 0.39785416;
    let zcosis = 0.91744867;
    let zcosgs = 0.1945905;
    let zsings = -0.98088458;

    let nm = np;
    let em = ep;
    let snodm = libm::sin(nodep);
    let cnodm = libm::cos(nodep);
    let sinomm = libm::sin(argpp);
    let cosomm = libm::cos(argpp);
    let sinim = libm::sin(inclp);
    let cosim = libm::cos(inclp);
    let emsq = em * em;
    let betasq = 1.0 - emsq;
    let rtemsq = libm::sqrt(betasq);

    let day = epoch + 18261.5 + tc / 1440.0;
    let xnodce = fmod(4.5236020 - 9.2422029e-4 * day, TWOPI);
    let stem = libm::sin(xnodce);
    let ctem = libm::cos(xnodce);
    let zcosil = 0.91375164 - 0.03568096 * ctem;
    let zsinil = libm::sqrt(1.0 - zcosil * zcosil);
    let zsinhl = 0.089683511 * stem / zsinil;
    let zcoshl = libm::sqrt(1.0 - zsinhl * zsinhl);
    let gam = 5.8351514 + 0.0019443680 * day;
    let mut zx = 0.39785416 * stem / zsinil;
    let zy = zcoshl * ctem + 0.91744867 * zsinhl * stem;
    zx = libm::atan2(zx, zy);
    zx = gam + zx - xnodce;
    let zcosgl = libm::cos(zx);
    let zsingl = libm::sin(zx);

    let (mut zcosg, mut zsing, mut zcosi, mut zsini, mut zcosh, mut zsinh) =
        (zcosgs, zsings, zcosis, zsinis, cnodm, snodm);
    let mut cc = c1ss;
    let xnoi = 1.0 / nm;

    // Solar terms (pass 1) are saved in the s* / sz* variables; the lunar
    // terms (pass 2) stay in s* / z*.
    let mut saved = [0.0f64; 19];
    let mut cur = [0.0f64; 19];
    for lsflg in 1..=2 {
        let a1 = zcosg * zcosh + zsing * zcosi * zsinh;
        let a3 = -zsing * zcosh + zcosg * zcosi * zsinh;
        let a7 = -zcosg * zsinh + zsing * zcosi * zcosh;
        let a8 = zsing * zsini;
        let a9 = zsing * zsinh + zcosg * zcosi * zcosh;
        let a10 = zcosg * zsini;
        let a2 = cosim * a7 + sinim * a8;
        let a4 = cosim * a9 + sinim * a10;
        let a5 = -sinim * a7 + cosim * a8;
        let a6 = -sinim * a9 + cosim * a10;

        let x1 = a1 * cosomm + a2 * sinomm;
        let x2 = a3 * cosomm + a4 * sinomm;
        let x3 = -a1 * sinomm + a2 * cosomm;
        let x4 = -a3 * sinomm + a4 * cosomm;
        let x5 = a5 * sinomm;
        let x6 = a6 * sinomm;
        let x7 = a5 * cosomm;
        let x8 = a6 * cosomm;

        let z31 = 12.0 * x1 * x1 - 3.0 * x3 * x3;
        let z32 = 24.0 * x1 * x2 - 6.0 * x3 * x4;
        let z33 = 12.0 * x2 * x2 - 3.0 * x4 * x4;
        let mut z1 = 3.0 * (a1 * a1 + a2 * a2) + z31 * emsq;
        let mut z2 = 6.0 * (a1 * a3 + a2 * a4) + z32 * emsq;
        let mut z3 = 3.0 * (a3 * a3 + a4 * a4) + z33 * emsq;
        let z11 = -6.0 * a1 * a5 + emsq * (-24.0 * x1 * x7 - 6.0 * x3 * x5);
        let z12 = -6.0 * (a1 * a6 + a3 * a5)
            + emsq * (-24.0 * (x2 * x7 + x1 * x8) - 6.0 * (x3 * x6 + x4 * x5));
        let z13 = -6.0 * a3 * a6 + emsq * (-24.0 * x2 * x8 - 6.0 * x4 * x6);
        let z21 = 6.0 * a2 * a5 + emsq * (24.0 * x1 * x5 - 6.0 * x3 * x7);
        let z22 = 6.0 * (a4 * a5 + a2 * a6)
            + emsq * (24.0 * (x2 * x5 + x1 * x6) - 6.0 * (x4 * x7 + x3 * x8));
        let z23 = 6.0 * a4 * a6 + emsq * (24.0 * x2 * x6 - 6.0 * x4 * x8);
        z1 = z1 + z1 + betasq * z31;
        z2 = z2 + z2 + betasq * z32;
        z3 = z3 + z3 + betasq * z33;
        let s3 = cc * xnoi;
        let s2 = -0.5 * s3 / rtemsq;
        let s4 = s3 * rtemsq;
        let s1 = -15.0 * em * s4;
        let s5 = x1 * x3 + x2 * x4;
        let s6 = x2 * x3 + x1 * x4;
        let s7 = x2 * x4 - x1 * x3;

        cur = [
            s1, s2, s3, s4, s5, s6, s7, z1, z2, z3, z11, z12, z13, z21, z22, z23, z31, z32, z33,
        ];
        if lsflg == 1 {
            saved = cur;
            zcosg = zcosgl;
            zsing = zsingl;
            zcosi = zcosil;
            zsini = zsinil;
            zcosh = zcoshl * cnodm + zsinhl * snodm;
            zsinh = snodm * zcoshl - cnodm * zsinhl;
            cc = c1l;
        }
    }
    let [ss1, ss2, ss3, ss4, ss5, ss6, ss7, sz1, sz2, sz3, sz11, sz12, sz13, sz21, sz22, sz23, sz31, sz32, sz33] =
        saved;
    let [s1, s2, s3, s4, s5, s6, s7, z1, z2, z3, z11, z12, z13, z21, z22, z23, z31, z32, z33] = cur;

    let zmol = fmod(4.7199672 + 0.22997150 * day - gam, TWOPI);
    let zmos = fmod(6.2565837 + 0.017201977 * day, TWOPI);

    Dscom {
        sinim,
        cosim,
        emsq,
        s1,
        s2,
        s3,
        s4,
        s5,
        ss1,
        ss2,
        ss3,
        ss4,
        ss5,
        sz1,
        sz3,
        sz11,
        sz13,
        sz21,
        sz23,
        sz31,
        sz33,
        z1,
        z3,
        z11,
        z13,
        z21,
        z23,
        z31,
        z33,
        // Solar terms.
        se2: 2.0 * ss1 * ss6,
        se3: 2.0 * ss1 * ss7,
        si2: 2.0 * ss2 * sz12,
        si3: 2.0 * ss2 * (sz13 - sz11),
        sl2: -2.0 * ss3 * sz2,
        sl3: -2.0 * ss3 * (sz3 - sz1),
        sl4: -2.0 * ss3 * (-21.0 - 9.0 * emsq) * zes,
        sgh2: 2.0 * ss4 * sz32,
        sgh3: 2.0 * ss4 * (sz33 - sz31),
        sgh4: -18.0 * ss4 * zes,
        sh2: -2.0 * ss2 * sz22,
        sh3: -2.0 * ss2 * (sz23 - sz21),
        // Lunar terms.
        ee2: 2.0 * s1 * s6,
        e3: 2.0 * s1 * s7,
        xi2: 2.0 * s2 * z12,
        xi3: 2.0 * s2 * (z13 - z11),
        xl2: -2.0 * s3 * z2,
        xl3: -2.0 * s3 * (z3 - z1),
        xl4: -2.0 * s3 * (-21.0 - 9.0 * emsq) * zel,
        xgh2: 2.0 * s4 * z32,
        xgh3: 2.0 * s4 * (z33 - z31),
        xgh4: -18.0 * s4 * zel,
        xh2: -2.0 * s2 * z22,
        xh3: -2.0 * s2 * (z23 - z21),
        peo: 0.0,
        pgho: 0.0,
        pho: 0.0,
        pinco: 0.0,
        plo: 0.0,
        zmol,
        zmos,
        em,
        nm,
    }
}

/// Lunar–solar periodics.
#[allow(clippy::too_many_arguments)]
fn dpper(
    d: &Deep,
    t: f64,
    init: bool,
    ep: &mut f64,
    inclp: &mut f64,
    nodep: &mut f64,
    argpp: &mut f64,
    mp: &mut f64,
    opsmode: OpsMode,
) {
    let zns = 1.19459e-5;
    let zes = 0.01675;
    let znl = 1.5835218e-4;
    let zel = 0.05490;

    let mut zm = d.zmos + zns * t;
    if init {
        zm = d.zmos;
    }
    let mut zf = zm + 2.0 * zes * libm::sin(zm);
    let mut sinzf = libm::sin(zf);
    let mut f2 = 0.5 * sinzf * sinzf - 0.25;
    let mut f3 = -0.5 * sinzf * libm::cos(zf);
    let ses = d.se2 * f2 + d.se3 * f3;
    let sis = d.si2 * f2 + d.si3 * f3;
    let sls = d.sl2 * f2 + d.sl3 * f3 + d.sl4 * sinzf;
    let sghs = d.sgh2 * f2 + d.sgh3 * f3 + d.sgh4 * sinzf;
    let shs = d.sh2 * f2 + d.sh3 * f3;
    zm = d.zmol + znl * t;
    if init {
        zm = d.zmol;
    }
    zf = zm + 2.0 * zel * libm::sin(zm);
    sinzf = libm::sin(zf);
    f2 = 0.5 * sinzf * sinzf - 0.25;
    f3 = -0.5 * sinzf * libm::cos(zf);
    let sel = d.ee2 * f2 + d.e3 * f3;
    let sil = d.xi2 * f2 + d.xi3 * f3;
    let sll = d.xl2 * f2 + d.xl3 * f3 + d.xl4 * sinzf;
    let sghl = d.xgh2 * f2 + d.xgh3 * f3 + d.xgh4 * sinzf;
    let shll = d.xh2 * f2 + d.xh3 * f3;
    let mut pe = ses + sel;
    let mut pinc = sis + sil;
    let mut pl = sls + sll;
    let mut pgh = sghs + sghl;
    let mut ph = shs + shll;

    if !init {
        pe -= d.peo;
        pinc -= d.pinco;
        pl -= d.plo;
        pgh -= d.pgho;
        ph -= d.pho;
        *inclp += pinc;
        *ep += pe;
        let sinip = libm::sin(*inclp);
        let cosip = libm::cos(*inclp);
        if *inclp >= 0.2 {
            ph /= sinip;
            pgh -= cosip * ph;
            *argpp += pgh;
            *nodep += ph;
            *mp += pl;
        } else {
            // Lyddane modification for low inclinations.
            let sinop = libm::sin(*nodep);
            let cosop = libm::cos(*nodep);
            let mut alfdp = sinip * sinop;
            let mut betdp = sinip * cosop;
            let dalf = ph * cosop + pinc * cosip * sinop;
            let dbet = -ph * sinop + pinc * cosip * cosop;
            alfdp += dalf;
            betdp += dbet;
            *nodep = fmod(*nodep, TWOPI);
            if *nodep < 0.0 && opsmode == OpsMode::Afspc {
                *nodep += TWOPI;
            }
            let mut xls = *mp + *argpp + cosip * *nodep;
            let dls = pl + pgh - pinc * *nodep * sinip;
            xls += dls;
            let xnoh = *nodep;
            *nodep = libm::atan2(alfdp, betdp);
            if *nodep < 0.0 && opsmode == OpsMode::Afspc {
                *nodep += TWOPI;
            }
            if libm::fabs(xnoh - *nodep) > PI {
                if *nodep < xnoh {
                    *nodep += TWOPI;
                } else {
                    *nodep -= TWOPI;
                }
            }
            *mp += pl;
            *argpp = xls - *mp - cosip * *nodep;
        }
    }
}

/// Deep-space secular rates and resonance coefficients.
fn dsinit(s: &mut Sgp4, ds: &Dscom, xke: f64, xpidot: f64, eccsq: f64) {
    let q22 = 1.7891679e-6;
    let q31 = 2.1460748e-6;
    let q33 = 2.2123015e-7;
    let root22 = 1.7891679e-6;
    let root44 = 7.3636953e-9;
    let root54 = 2.1765803e-9;
    let rptim = 4.37526908801129966e-3;
    let root32 = 3.7393792e-7;
    let root52 = 1.1428639e-7;
    let znl = 1.5835218e-4;
    let zns = 1.19459e-5;

    let (cosim, sinim) = (ds.cosim, ds.sinim);
    let mut emsq = ds.emsq;
    let nm = ds.nm;
    let mut em = ds.em;
    let inclm = s.inclo;
    let tc = 0.0;
    let d = &mut s.deep;

    d.irez = 0;
    if 0.0034906585 < nm && nm < 0.0052359877 {
        d.irez = 1;
    }
    if (8.26e-3..=9.24e-3).contains(&nm) && em >= 0.5 {
        d.irez = 2;
    }

    // Solar terms.
    let ses = ds.ss1 * zns * ds.ss5;
    let sis = ds.ss2 * zns * (ds.sz11 + ds.sz13);
    let sls = -zns * ds.ss3 * (ds.sz1 + ds.sz3 - 14.0 - 6.0 * emsq);
    let sghs = ds.ss4 * zns * (ds.sz31 + ds.sz33 - 6.0);
    let mut shs = -zns * ds.ss2 * (ds.sz21 + ds.sz23);
    if !(5.2359877e-2..=PI - 5.2359877e-2).contains(&inclm) {
        shs = 0.0;
    }
    if sinim != 0.0 {
        shs /= sinim;
    }
    let sgs = sghs - cosim * shs;

    // Lunar terms.
    d.dedt = ses + ds.s1 * znl * ds.s5;
    d.didt = sis + ds.s2 * znl * (ds.z11 + ds.z13);
    d.dmdt = sls - znl * ds.s3 * (ds.z1 + ds.z3 - 14.0 - 6.0 * emsq);
    let sghl = ds.s4 * znl * (ds.z31 + ds.z33 - 6.0);
    let mut shll = -znl * ds.s2 * (ds.z21 + ds.z23);
    if !(5.2359877e-2..=PI - 5.2359877e-2).contains(&inclm) {
        shll = 0.0;
    }
    d.domdt = sgs + sghl;
    d.dnodt = shs;
    if sinim != 0.0 {
        d.domdt -= cosim / sinim * shll;
        d.dnodt += shll / sinim;
    }

    let theta = fmod(s.gsto + tc * rptim, TWOPI);

    if d.irez != 0 {
        let aonv = libm::pow(nm / xke, X2O3);

        if d.irez == 2 {
            let cosisq = cosim * cosim;
            em = s.ecco;
            let emsqo = emsq;
            emsq = eccsq;
            let eoc = em * emsq;
            let g201 = -0.306 - (em - 0.64) * 0.440;
            let (g211, g310, g322, g410, g422, g520);
            if em <= 0.65 {
                g211 = 3.616 - 13.2470 * em + 16.2900 * emsq;
                g310 = -19.302 + 117.3900 * em - 228.4190 * emsq + 156.5910 * eoc;
                g322 = -18.9068 + 109.7927 * em - 214.6334 * emsq + 146.5816 * eoc;
                g410 = -41.122 + 242.6940 * em - 471.0940 * emsq + 313.9530 * eoc;
                g422 = -146.407 + 841.8800 * em - 1629.014 * emsq + 1083.4350 * eoc;
                g520 = -532.114 + 3017.977 * em - 5740.032 * emsq + 3708.2760 * eoc;
            } else {
                g211 = -72.099 + 331.819 * em - 508.738 * emsq + 266.724 * eoc;
                g310 = -346.844 + 1582.851 * em - 2415.925 * emsq + 1246.113 * eoc;
                g322 = -342.585 + 1554.908 * em - 2366.899 * emsq + 1215.972 * eoc;
                g410 = -1052.797 + 4758.686 * em - 7193.992 * emsq + 3651.957 * eoc;
                g422 = -3581.690 + 16178.110 * em - 24462.770 * emsq + 12422.520 * eoc;
                g520 = if em > 0.715 {
                    -5149.66 + 29936.92 * em - 54087.36 * emsq + 31324.56 * eoc
                } else {
                    1464.74 - 4664.75 * em + 3763.64 * emsq
                };
            }
            let (g533, g521, g532);
            if em < 0.7 {
                g533 = -919.22770 + 4988.6100 * em - 9064.7700 * emsq + 5542.21 * eoc;
                g521 = -822.71072 + 4568.6173 * em - 8491.4146 * emsq + 5337.524 * eoc;
                g532 = -853.66600 + 4690.2500 * em - 8624.7700 * emsq + 5341.4 * eoc;
            } else {
                g533 = -37995.780 + 161616.52 * em - 229838.20 * emsq + 109377.94 * eoc;
                g521 = -51752.104 + 218913.95 * em - 309468.16 * emsq + 146349.42 * eoc;
                g532 = -40023.880 + 170470.89 * em - 242699.48 * emsq + 115605.82 * eoc;
            }
            let sini2 = sinim * sinim;
            let f220 = 0.75 * (1.0 + 2.0 * cosim + cosisq);
            let f221 = 1.5 * sini2;
            let f321 = 1.875 * sinim * (1.0 - 2.0 * cosim - 3.0 * cosisq);
            let f322 = -1.875 * sinim * (1.0 + 2.0 * cosim - 3.0 * cosisq);
            let f441 = 35.0 * sini2 * f220;
            let f442 = 39.3750 * sini2 * sini2;
            let f522 = 9.84375
                * sinim
                * (sini2 * (1.0 - 2.0 * cosim - 5.0 * cosisq)
                    + 0.33333333 * (-2.0 + 4.0 * cosim + 6.0 * cosisq));
            let f523 = sinim
                * (4.92187512 * sini2 * (-2.0 - 4.0 * cosim + 10.0 * cosisq)
                    + 6.56250012 * (1.0 + 2.0 * cosim - 3.0 * cosisq));
            let f542 = 29.53125
                * sinim
                * (2.0 - 8.0 * cosim + cosisq * (-12.0 + 8.0 * cosim + 10.0 * cosisq));
            let f543 = 29.53125
                * sinim
                * (-2.0 - 8.0 * cosim + cosisq * (12.0 + 8.0 * cosim - 10.0 * cosisq));
            let xno2 = nm * nm;
            let ainv2 = aonv * aonv;
            let mut temp1 = 3.0 * xno2 * ainv2;
            let mut temp = temp1 * root22;
            d.d2201 = temp * f220 * g201;
            d.d2211 = temp * f221 * g211;
            temp1 *= aonv;
            temp = temp1 * root32;
            d.d3210 = temp * f321 * g310;
            d.d3222 = temp * f322 * g322;
            temp1 *= aonv;
            temp = 2.0 * temp1 * root44;
            d.d4410 = temp * f441 * g410;
            d.d4422 = temp * f442 * g422;
            temp1 *= aonv;
            temp = temp1 * root52;
            d.d5220 = temp * f522 * g520;
            d.d5232 = temp * f523 * g532;
            temp = 2.0 * temp1 * root54;
            d.d5421 = temp * f542 * g521;
            d.d5433 = temp * f543 * g533;
            d.xlamo = fmod(s.mo + s.nodeo + s.nodeo - theta - theta, TWOPI);
            d.xfact = s.mdot + d.dmdt + 2.0 * (s.nodedot + d.dnodt - rptim) - s.no_unkozai;
            emsq = emsqo;
        }

        if d.irez == 1 {
            let g200 = 1.0 + emsq * (-2.5 + 0.8125 * emsq);
            let g310 = 1.0 + 2.0 * emsq;
            let g300 = 1.0 + emsq * (-6.0 + 6.60937 * emsq);
            let f220 = 0.75 * (1.0 + cosim) * (1.0 + cosim);
            let f311 = 0.9375 * sinim * sinim * (1.0 + 3.0 * cosim) - 0.75 * (1.0 + cosim);
            let mut f330 = 1.0 + cosim;
            f330 = 1.875 * f330 * f330 * f330;
            let mut del1 = 3.0 * nm * nm * aonv * aonv;
            d.del2 = 2.0 * del1 * f220 * g200 * q22;
            d.del3 = 3.0 * del1 * f330 * g300 * q33 * aonv;
            del1 = del1 * f311 * g310 * q31 * aonv;
            d.del1 = del1;
            d.xlamo = fmod(s.mo + s.nodeo + s.argpo - theta, TWOPI);
            d.xfact = s.mdot + xpidot - rptim + d.dmdt + d.domdt + d.dnodt - s.no_unkozai;
        }
    }
}

struct DspaceOut {
    em: f64,
    argpm: f64,
    inclm: f64,
    mm: f64,
    nodem: f64,
    nm: f64,
}

/// Deep-space secular effects and resonance integration from epoch to `t`.
#[allow(clippy::too_many_arguments)]
fn dspace(
    s: &Sgp4,
    t: f64,
    em: f64,
    argpm: f64,
    inclm: f64,
    mm: f64,
    nodem: f64,
    nm: f64,
) -> DspaceOut {
    let d = &s.deep;
    let fasx2 = 0.13130908;
    let fasx4 = 2.8843198;
    let fasx6 = 0.37448087;
    let g22 = 5.7686396;
    let g32 = 0.95240898;
    let g44 = 1.8014998;
    let g52 = 1.0508330;
    let g54 = 4.4108898;
    let rptim = 4.37526908801129966e-3;
    let stepp = 720.0;
    let stepn = -720.0;
    let step2 = 259200.0;
    let tc = t;

    let theta = fmod(s.gsto + tc * rptim, TWOPI);
    let mut out = DspaceOut {
        em: em + d.dedt * t,
        inclm: inclm + d.didt * t,
        argpm: argpm + d.domdt * t,
        nodem: nodem + d.dnodt * t,
        mm: mm + d.dmdt * t,
        nm,
    };

    if d.irez != 0 {
        // Always integrate from epoch; this matches the reference, which
        // restarts or continues with the same fixed steps.
        let mut atime = 0.0;
        let mut xni = s.no_unkozai;
        let mut xli = d.xlamo;
        let delt = if t > 0.0 { stepp } else { stepn };
        let (mut xndt, mut xldot, mut xnddt);
        let ft;
        loop {
            if d.irez != 2 {
                xndt = d.del1 * libm::sin(xli - fasx2)
                    + d.del2 * libm::sin(2.0 * (xli - fasx4))
                    + d.del3 * libm::sin(3.0 * (xli - fasx6));
                xldot = xni + d.xfact;
                xnddt = d.del1 * libm::cos(xli - fasx2)
                    + 2.0 * d.del2 * libm::cos(2.0 * (xli - fasx4))
                    + 3.0 * d.del3 * libm::cos(3.0 * (xli - fasx6));
                xnddt *= xldot;
            } else {
                let xomi = s.argpo + s.argpdot * atime;
                let x2omi = xomi + xomi;
                let x2li = xli + xli;
                xndt = d.d2201 * libm::sin(x2omi + xli - g22)
                    + d.d2211 * libm::sin(xli - g22)
                    + d.d3210 * libm::sin(xomi + xli - g32)
                    + d.d3222 * libm::sin(-xomi + xli - g32)
                    + d.d4410 * libm::sin(x2omi + x2li - g44)
                    + d.d4422 * libm::sin(x2li - g44)
                    + d.d5220 * libm::sin(xomi + xli - g52)
                    + d.d5232 * libm::sin(-xomi + xli - g52)
                    + d.d5421 * libm::sin(xomi + x2li - g54)
                    + d.d5433 * libm::sin(-xomi + x2li - g54);
                xldot = xni + d.xfact;
                xnddt = d.d2201 * libm::cos(x2omi + xli - g22)
                    + d.d2211 * libm::cos(xli - g22)
                    + d.d3210 * libm::cos(xomi + xli - g32)
                    + d.d3222 * libm::cos(-xomi + xli - g32)
                    + d.d5220 * libm::cos(xomi + xli - g52)
                    + d.d5232 * libm::cos(-xomi + xli - g52)
                    + 2.0
                        * (d.d4410 * libm::cos(x2omi + x2li - g44)
                            + d.d4422 * libm::cos(x2li - g44)
                            + d.d5421 * libm::cos(xomi + x2li - g54)
                            + d.d5433 * libm::cos(-xomi + x2li - g54));
                xnddt *= xldot;
            }
            if libm::fabs(t - atime) >= stepp {
                xli = xli + xldot * delt + xndt * step2;
                xni = xni + xndt * delt + xnddt * step2;
                atime += delt;
            } else {
                ft = t - atime;
                break;
            }
        }
        let nm_new = xni + xndt * ft + xnddt * ft * ft * 0.5;
        let xl = xli + xldot * ft + xndt * ft * ft * 0.5;
        if d.irez != 1 {
            out.mm = xl - 2.0 * out.nodem + 2.0 * theta;
        } else {
            out.mm = xl - out.nodem - out.argpm + theta;
        }
        let dndt = nm_new - s.no_unkozai;
        out.nm = s.no_unkozai + dndt;
    }
    out
}
