# Playground

An interactive orbit explorer. Everything on this page is computed in your
browser by the heavenly crates, compiled to WebAssembly with `wasm-pack`
(a 193 KB module). The page is a React app built on `@heavenly/react`, the
library's React components (`<Globe>`, `<GroundTrack>`, `<PassTable>`), which
draw with WebGL2. Only the drawing is JavaScript.

* **Orbit**: `heavenly::orbit` propagates the slider elements or a pasted
  TLE (`Sgp4`, in TEME). The **Dynamics** menu chooses how the slider
  elements move (see [Perturbations](perturbations.md)):
  * as a fixed conic (`TwoBodyLocator`);
  * turned by the Earth's oblateness (`SecularJ2Locator`);
  * integrated numerically with J2–J6, drag, sunlight pressure, the Sun and
    the Moon (`NumericalPropagator`, for a small satellite with
    Cd·A/m = Cr·A/m = 0.01 m²/kg).
* **Earth's rotation**: `heavenly::geo` turns GCRF into ITRF at every frame
  (IAU 2006 precession, nutation and the Earth Rotation Angle). That is why
  the graticule turns and the ground track drifts west on each orbit.
* **The ground station's view**: a `FrameGraph` resolves the satellite into
  an east-north-up frame at the station, which gives elevation, azimuth and
  range. The tabs move the station: London, any of the data centres, or your
  location. Choosing a tab replaces one fixed edge in the graph
  (`FrameGraph::remove_edge`, then `add_fixed`).
* **Sunlight**: `heavenly::query` places the Sun (`AnalyticSun`) and the
  Earth's shadow (`Occulter::shadow`). The orbit and the ground track turn
  grey where the satellite is in the Earth's shadow, and the map shades the
  night side.
* **Passes**: `find_windows` lists the passes over the station. A pass is
  visible where the satellite is sunlit while the Sun is more than 6° below
  the station's horizon.
* **Data centres**: the squares are Apple's own data centres (Maiden,
  Prineville, Reno, Mesa, Waukee, Viborg) at city-level, approximate
  coordinates. A square turns orange while the satellite is more than 10°
  above its horizon.
* **Live**: time follows your clock at real speed. A slider orbit moves its
  epoch to now. A TLE keeps its epoch, and SGP4 propagates it to now.

**Your location** uses your browser's Geolocation API. The browser asks you
first. The position is used only in this page and is sent nowhere. Nothing
on this page calls an external service.

<div id="hv-play" data-land="playground/land-110m.json"><noscript>The playground needs JavaScript and WebGL2.</noscript></div>
<script type="module" src="playground/app.js"></script>

## Things to try

* **ISS**: the track moves about 23° west on each orbit, because the Earth
  turns under the orbit plane during each ~93-minute orbit. The highlighted stretches are
  passes over London.
* **Geostationary**: the satellite stays over one longitude and the ground
  track collapses to a point. Raise the inclination and it draws a
  figure-eight.
* **Molniya**: a high, eccentric orbit whose apogee hangs over northern
  latitudes for hours. The 63.4° inclination is chosen so that Earth's
  oblateness does not rotate the apogee. Set **Dynamics** to *J2 secular*,
  then move the inclination to 50°. The "periapsis" marker on the globe now
  creeps around the orbit from one day to the next, and the apogee leaves
  the north. Set it back to 63.4° and the marker stops.
* **J2 vs two-body**: pick **Sun-synchronous**, move the time slider to the
  end, and switch **Dynamics** between *Two-body* and *J2 secular*. Under
  J2 the node has turned about 0.2° east in those five hours, and the
  satellite is about 90 km from where the fixed ellipse puts it.
  *Full* agrees with *J2 secular* to within a few kilometres. The rest is
  J2's short-period wobble, the higher harmonics, drag, and the Sun and
  Moon.
* **Epoch**: change the date and the same orbit crosses different longitudes,
  because the Earth Rotation Angle is different.
* **Constellation**: an illustrative 24-satellite Walker shell, 24/4/1
  (`design::walker_delta`), built from the slider orbit. The altitude,
  inclination and node sliders reshape it live. Every frame, the library
  moves all 24 satellites into ITRF with one batch transform. It also finds
  the inter-satellite links (`catalog::links`: under 5,000 km, and clearing
  the Earth by 80 km), drawn in orange, and counts which data centres have
  a satellite above 25°. Satellites in the Earth's shadow are grey.
  `cargo run -p heavenly --example constellation` gives the day's numbers:
  coverage and gaps over each data centre, the link network, and close
  approaches with a crossing shell.
* **Orbital data centre**: an illustrative solar-powered data-centre
  satellite in a dawn–dusk sun-synchronous orbit at 550 km. Published
  reports of such satellites give only a range of orbits, "500 to 2,000
  kilometers" and "30 degrees to Sun-Synchronous Orbit", so this is one
  choice from that range, not a real satellite's orbit. The library works
  out the inclination (`design::sun_synchronous_inclination_rad`) and the
  node that faces the Sun at your epoch. The orbit rides the day–night
  line and stays in sunlight, so the track has no grey stretches. Compare
  the ISS preset, which is in shadow for about a third of each orbit.
  `cargo run -p heavenly --example orbital_datacenter` gives the numbers:
  contacts with each data centre, the gaps between them, and the light time.
* **ISS (TLE, SGP4)**: an ISS element set from July 2020, propagated with
  SGP4. In midsummer at London's latitude the ISS stays in sunlight all
  night, so every night pass is visible. Its eclipses (dashed on the map)
  shrink from 13 minutes to nothing within a day, as the orbit turns to
  face the Sun. Paste a current TLE to see today's.
* **Where is it now?** Paste a current TLE (CelesTrak publishes one for the
  ISS several times a day), then press **Live**. The map then shows where
  the satellite is now, and the pass table lists the next two days' passes
  from the station tab you have chosen. Try **Your location** to see when it
  passes over you.
* **Data centre tabs**: with **Orbital data centre**, switch between Viborg
  and Mesa. The 550 km dawn–dusk orbit passes over each a few times a day,
  at different times, and every pass is in sunlight.

## What it does not model

With *Two-body* dynamics, the slider orbit is a fixed conic, so a real ISS
track drifts from it within hours. *J2 secular* adds the oblateness's
steady effects but not its short-period wobble. *Full* integrates the
perturbations listed in [Perturbations](perturbations.md), with a static
atmosphere and the analytic Sun and Moon; it has no tesseral harmonics. A
TLE goes through SGP4, which has its own perturbation model, to SGP4's own
accuracy of about a kilometre near the TLE's epoch. The Sun is the
analytic series, good to 32″.
Earth orientation uses zero EOP, which is good to a few hundred metres. See
[Orbits and trajectories](orbits.md) and [Earth, Moon and Mars](earth-and-moon.md)
for the models and their measured accuracy.

## Building it

The playground is built in CI. To build it locally and view the page, with
`wasm-pack` and Node 20.19 or later installed:

```sh
book/build-playground.sh   # wasm-pack → @heavenly/react, then Vite → book/src/playground/
mdbook serve book          # WebAssembly must be served over HTTP, not file://
```

The JavaScript lives in `web/`. `web/packages/react` is the library,
`@heavenly/react`. It contains the wasm-bindgen module, a typed wrapper
(`loadHeavenly`, `Heavenly`), the hooks (`useHeavenly`, `useClock`,
`useLand`) and the components. `web/playground` is this page's app. Run
`npm run dev` in `web/` to work on it with hot reload.

Using the components in your own app looks like this:

```js
import { Globe, GroundTrack, useClock, useHeavenly } from "@heavenly/react";

function Orbit() {
  const { heavenly } = useHeavenly();
  const clock = useClock(300, 3 * 3600); // ×300, wrapping after three hours
  if (!heavenly) return null;
  heavenly.setElements({ periapsisAltitudeKm: 420, eccentricity: 0.0005, inclinationDeg: 51.6, raanDeg: 0, argPeriapsisDeg: 0 });
  const now = heavenly.snapshot(clock.t);
  return (
    <>
      <Globe snapshot={now} path={heavenly.orbitPath()} />
      <GroundTrack snapshot={now} track={heavenly.groundTrack(0, 3 * 3600)} sites={heavenly.sites} />
    </>
  );
}
```

That example recomputes the path and the track on every frame. The
playground computes them once per orbit change, with `useMemo`.

Coastlines: Natural Earth 1:110m land (public domain), via the
`world-atlas` package (ISC).
