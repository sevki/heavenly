# Playground

An interactive orbit explorer. Everything on this page is computed in your
browser by the heavenly crates, compiled to WebAssembly (a 167 KB module
with no JavaScript bindings). Only the drawing is JavaScript.

* **Orbit**: `heavenly::orbit` propagates the slider elements
  (`TwoBodyLocator`) or a pasted TLE (`Sgp4`, in TEME).
* **Earth's rotation**: `heavenly::geo` turns GCRF into ITRF at every frame
  (IAU 2006 precession, nutation and the Earth Rotation Angle). That is why
  the graticule turns and the ground track drifts west on each orbit.
* **London's view**: a `FrameGraph` resolves the satellite into an
  east-north-up frame at the site, which gives elevation, azimuth and range.
* **Sunlight**: `heavenly::query` places the Sun (`AnalyticSun`) and the
  Earth's shadow (`Occulter::shadow`). The orbit and the ground track turn
  grey where the satellite is in the Earth's shadow, and the map shades the
  night side.
* **Passes**: `find_windows` lists the passes over London. A pass is
  visible where the satellite is sunlit while the Sun is more than 6° below
  London's horizon.

<div id="hv-play" class="hv-play">
<div class="hv-presets">
<button id="hv-preset-iss">ISS</button>
<button id="hv-preset-sso">Sun-synchronous</button>
<button id="hv-preset-molniya">Molniya</button>
<button id="hv-preset-gps">GPS</button>
<button id="hv-preset-geo">Geostationary</button>
<button id="hv-preset-tle">ISS (TLE, SGP4)</button>
</div>
<details class="hv-tle"><summary>Paste a TLE</summary>
<textarea id="hv-tle" rows="3" spellcheck="false" aria-label="Two-line element set"></textarea>
<button id="hv-tle-btn">Propagate with SGP4</button>
<span class="hv-note">from <a href="https://celestrak.org/NORAD/elements/">CelesTrak</a>, for example. The epoch moves to the TLE's.</span>
</details>
<div class="hv-controls">
<label>Periapsis altitude <input id="hv-alt" type="range" min="0" max="1000" step="1"> <span id="hv-alt-v"></span></label>
<label>Eccentricity e <input id="hv-e" type="range" min="0" max="0.9" step="0.001"> <span id="hv-e-v"></span></label>
<label>Inclination i <input id="hv-i" type="range" min="0" max="180" step="0.1"> <span id="hv-i-v"></span></label>
<label>Ascending node Ω <input id="hv-raan" type="range" min="0" max="360" step="1"> <span id="hv-raan-v"></span></label>
<label>Arg. of periapsis ω <input id="hv-argp" type="range" min="0" max="360" step="1"> <span id="hv-argp-v"></span></label>
<label>Epoch (UTC) <input id="hv-epoch" type="datetime-local" value="2026-11-19T17:00" step="60"></label>
</div>
<div class="hv-canvases">
<canvas id="hv-orbit" class="hv-canvas-orbit" aria-label="3D view of the orbit around the Earth"></canvas>
<canvas id="hv-track" class="hv-canvas-track" aria-label="Ground track on a map of the Earth"></canvas>
</div>
<div class="hv-timebar">
<button id="hv-play-btn">Pause</button>
<input id="hv-time" type="range" min="0" max="10800" step="1" value="0" aria-label="Time after the epoch">
<label>speed <select id="hv-speed"><option value="60">×60</option><option value="300" selected>×300</option><option value="1200">×1200</option><option value="3600">×3600</option></select></label>
</div>
<div id="hv-readout" class="hv-readout"></div>
<div id="hv-passes" class="hv-passes"></div>
<div id="hv-status" class="hv-status">Loading the WebAssembly module…</div>
</div>
<script src="playground/playground.js"></script>

## Things to try

* **ISS**: the track moves about 23° west on each orbit, because the Earth
  turns under the orbit plane during each ~93-minute orbit. The highlighted stretches are
  passes over London.
* **Geostationary**: the satellite stays over one longitude and the ground
  track collapses to a point. Raise the inclination and it draws a
  figure-eight.
* **Molniya**: a high, eccentric orbit whose apogee hangs over northern
  latitudes for hours. The 63.4° inclination is chosen so that Earth's
  oblateness does not rotate the apogee. That effect is not modelled here.
* **Epoch**: change the date and the same orbit crosses different longitudes,
  because the Earth Rotation Angle is different.
* **ISS (TLE, SGP4)**: an ISS element set from July 2020, propagated with
  SGP4. In midsummer at London's latitude the ISS stays in sunlight all
  night, so every night pass is visible. Its eclipses (dashed on the map)
  shrink from 13 minutes to nothing within a day, as the orbit turns to
  face the Sun. Paste a current TLE to see today's.

## What it does not model

With the sliders, the orbit is a pure two-body conic: no Earth oblateness
(J2), drag, or Sun and Moon perturbations, so a real ISS track drifts from
it within hours. A TLE goes through SGP4, which includes those effects, to
SGP4's own accuracy of about a kilometre near the TLE's epoch. The Sun is
the analytic series, good to 32″.
Earth orientation uses zero EOP, which is good to a few hundred metres. See
[Orbits and trajectories](orbits.md) and [Earth, Moon and Mars](earth-and-moon.md)
for the models and their measured accuracy.

## Building it

The module is built in CI. To build it locally and view the page:

```sh
book/build-playground.sh   # cargo build -p heavenly-viz → book/src/playground/
mdbook serve book          # WebAssembly must be served over HTTP, not file://
```

Coastlines: Natural Earth 1:110m land (public domain), via the
`world-atlas` package (ISC).
