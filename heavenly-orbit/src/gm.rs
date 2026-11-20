//! Gravitational parameters (GM) in m³/s², from JPL DE440 (`gm_de440.tpc`).
#![allow(clippy::excessive_precision)]

/// Sun.
pub const SUN: f64 = 1.327_124_400_412_794_2e20;
/// Earth.
pub const EARTH: f64 = 3.986_004_355_070_226_6e14;
/// Moon.
pub const MOON: f64 = 4.902_800_118_457_549_6e12;
/// Earth–Moon system (barycentre).
pub const EARTH_MOON: f64 = 4.035_032_356_254_801_9e14;
/// Mars.
pub const MARS: f64 = 4.282_837_362_069_909e13;
/// Mars system (barycentre).
pub const MARS_SYSTEM: f64 = 4.282_837_581_575_610_2e13;
/// Jupiter system (barycentre).
pub const JUPITER_SYSTEM: f64 = 1.267_127_641e17;
