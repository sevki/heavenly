//! The NAIF Double precision Array File (DAF) container used by SPK and
//! binary PCK kernels.

use alloc::vec::Vec;

use crate::SpiceError;

const RECORD: usize = 1024;

/// One array ("segment") summary: `nd` doubles and `ni` integers.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Summary {
    pub doubles: Vec<f64>,
    pub ints: Vec<i32>,
}

/// A parsed DAF file. Holds the raw bytes; arrays are decoded on demand.
#[derive(Debug)]
pub(crate) struct Daf {
    bytes: Vec<u8>,
    little_endian: bool,
    pub summaries: Vec<Summary>,
}

impl Daf {
    /// Parses a DAF whose identification word is `expected` (e.g. `"DAF/SPK"`).
    pub fn parse(bytes: Vec<u8>, expected: &str) -> Result<Daf, SpiceError> {
        if bytes.len() < RECORD {
            return Err(SpiceError::Format("file is shorter than one record"));
        }
        let idword = core::str::from_utf8(&bytes[0..8])
            .map_err(|_| SpiceError::Format("identification word is not ASCII"))?;
        if idword.trim_end() != expected {
            return Err(SpiceError::Format("unexpected DAF identification word"));
        }
        // FTP validation string guards against text-mode transfer damage.
        const FTP: &[u8] = b"FTPSTR:\r:\n:\r\n:\r\x00:\x81:\x10\xce:ENDFTP";
        if bytes[699..699 + FTP.len()] != *FTP && bytes[699..706] == *b"FTPSTR:" {
            return Err(SpiceError::Format(
                "file was corrupted by a text-mode transfer",
            ));
        }
        let little_endian = match &bytes[88..96] {
            b"LTL-IEEE" => true,
            b"BIG-IEEE" => false,
            _ => {
                return Err(SpiceError::Format(
                    "unknown binary format (expected LTL-IEEE or BIG-IEEE)",
                ))
            }
        };
        let mut daf = Daf {
            bytes,
            little_endian,
            summaries: Vec::new(),
        };
        let nd = daf.i32_at(8)?;
        let ni = daf.i32_at(12)?;
        if !(0..=124).contains(&nd) || !(2..=250).contains(&ni) {
            return Err(SpiceError::Format("implausible summary sizes"));
        }
        let (nd, ni) = (nd as usize, ni as usize);
        let summary_doubles = nd + (ni + 1) / 2;

        let mut record = daf.i32_at(76)?;
        let mut visited = 0usize;
        while record > 0 {
            visited += 1;
            if visited > daf.bytes.len() / RECORD {
                return Err(SpiceError::Format("summary record chain loops"));
            }
            let base = (record as usize - 1) * RECORD;
            let next = daf.f64_at(base)?;
            let count = daf.f64_at(base + 16)?;
            if !(0.0..=125.0).contains(&count) {
                return Err(SpiceError::Format("implausible summary count"));
            }
            for i in 0..count as usize {
                let start = base + 24 + i * summary_doubles * 8;
                let doubles = (0..nd)
                    .map(|k| daf.f64_at(start + 8 * k))
                    .collect::<Result<Vec<_>, _>>()?;
                let ints = (0..ni)
                    .map(|k| daf.i32_at(start + 8 * nd + 4 * k))
                    .collect::<Result<Vec<_>, _>>()?;
                daf.summaries.push(Summary { doubles, ints });
            }
            record = next as i32;
        }
        Ok(daf)
    }

    /// Reads the 1-based double-precision word at `address`.
    pub fn word(&self, address: usize) -> Result<f64, SpiceError> {
        if address == 0 {
            return Err(SpiceError::Format("address 0 is invalid"));
        }
        self.f64_at((address - 1) * 8)
    }

    fn f64_at(&self, offset: usize) -> Result<f64, SpiceError> {
        let b: [u8; 8] = self
            .bytes
            .get(offset..offset + 8)
            .and_then(|s| s.try_into().ok())
            .ok_or(SpiceError::Format("read past end of file"))?;
        Ok(if self.little_endian {
            f64::from_le_bytes(b)
        } else {
            f64::from_be_bytes(b)
        })
    }

    fn i32_at(&self, offset: usize) -> Result<i32, SpiceError> {
        let b: [u8; 4] = self
            .bytes
            .get(offset..offset + 4)
            .and_then(|s| s.try_into().ok())
            .ok_or(SpiceError::Format("read past end of file"))?;
        // The file record precedes the format word, but is written in the
        // same byte order as the rest of the file.
        Ok(if self.bytes[88..96] == *b"LTL-IEEE" {
            i32::from_le_bytes(b)
        } else {
            i32::from_be_bytes(b)
        })
    }
}

/// A Chebyshev segment of SPK type 2/3 or PCK type 2: fixed-length records
/// of `[mid, radius, coeffs...]`, followed by `[init, intlen, rsize, n]`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChebyshevSegment {
    pub start: usize,
    pub end: usize,
}

/// Evaluates `components` Chebyshev series (each `degree + 1` coefficients)
/// at `et`, returning values and time derivatives (per second).
pub(crate) fn evaluate_chebyshev<const N: usize>(
    daf: &Daf,
    seg: ChebyshevSegment,
    et: f64,
) -> Result<([f64; N], [f64; N]), SpiceError> {
    if seg.end < seg.start + 4 {
        return Err(SpiceError::Format("segment too short"));
    }
    let init = daf.word(seg.end - 3)?;
    let intlen = daf.word(seg.end - 2)?;
    let rsize = daf.word(seg.end - 1)? as usize;
    let n = daf.word(seg.end)? as usize;
    if intlen.is_nan() || intlen <= 0.0 || n == 0 || rsize < 2 + N || (rsize - 2) % N != 0 {
        return Err(SpiceError::Format("bad Chebyshev segment directory"));
    }
    if seg.start + rsize * n + 3 > seg.end {
        return Err(SpiceError::Format("Chebyshev records overrun segment"));
    }
    let ncoef = (rsize - 2) / N;
    let index = (libm::floor((et - init) / intlen).max(0.0) as usize).min(n - 1);
    let rec = seg.start + index * rsize;
    let mid = daf.word(rec)?;
    let radius = daf.word(rec + 1)?;
    let x = (et - mid) / radius;

    let mut value = [0.0; N];
    let mut rate = [0.0; N];
    for c in 0..N {
        // T_k and dT_k/dx by recurrence.
        let (mut t0, mut t1) = (1.0, x);
        let (mut d0, mut d1) = (0.0, 1.0);
        let base = rec + 2 + c * ncoef;
        let mut v = daf.word(base)?;
        let mut dv = 0.0;
        for k in 1..ncoef {
            let a = daf.word(base + k)?;
            if k >= 2 {
                let t2 = 2.0 * x * t1 - t0;
                let d2 = 2.0 * t1 + 2.0 * x * d1 - d0;
                t0 = t1;
                t1 = t2;
                d0 = d1;
                d1 = d2;
            }
            v += a * t1;
            dv += a * d1;
        }
        value[c] = v;
        rate[c] = dv / radius;
    }
    Ok((value, rate))
}
