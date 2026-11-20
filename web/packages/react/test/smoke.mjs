// Loads the wasm-pack build in Node and checks that the bindings work:
// node test/smoke.mjs (after `npm run wasm`).
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
import { initSync, Playground } from "../wasm/heavenly_viz.js";

initSync({ module: readFileSync(new URL("../wasm/heavenly_viz_bg.wasm", import.meta.url)) });
const p = new Playground();

// The ISS-like default: a ~92-minute orbit at ~420 km.
p.setEpoch(2026, 11, 19, 17, 0, 0);
p.setOrbit(420, 0.0005, 51.64, 0, 0, 0);
const [period] = p.info();
assert.ok(period > 5500 && period < 5600, `period ${period}`);
const s = p.state(600);
assert.equal(s.length, 24);
const alt = s[8];
assert.ok(alt > 410 && alt < 440, `altitude ${alt}`);
assert.ok(Math.abs(Math.hypot(s[21], s[22], s[23]) - 1) < 1e-9, "Sun direction is a unit vector");

// Moving the station changes the look angles but not the orbit; a bad
// latitude throws and leaves the station alone.
p.setStation(56.4532, 9.402, 0);
const v = p.state(600);
assert.equal(v[8], s[8]);
assert.notEqual(v[9], s[9]);
assert.throws(() => p.setStation(91, 0, 0), Error);
assert.equal(p.state(600)[9], v[9]);

// A TLE moves the epoch to the TLE's; a bad one throws a JavaScript error.
p.setTle(
  "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n" +
    "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008",
);
assert.deepEqual(Array.from(p.epochFields()).slice(0, 3), [2020, 7, 12]);
assert.throws(() => p.setTle("not a TLE"), Error);
assert.equal(Playground.siteNames().length * 2, Playground.sites().length);
p.free();
console.log("wasm bindings: ok");
