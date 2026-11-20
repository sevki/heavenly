# Placement topology

> **Placement topology is policy.**

Coordinates say where something *is*. They say nothing about what it shares
a fate with, or whose rules apply to it. `heavenly::topology` keeps those
facts in a separate model, so they are never confused with position:

* a **domain** is anything whose failure or authority spans locations: an
  availability zone, a power grid, a network, a provider, a constellation's
  ground segment, a jurisdiction;
* **locations** (by `LocationId`) are *assigned* to domains;
* **domains depend on domains**, forming a directed acyclic graph. Adding a
  cycle is rejected.

This is the `heavenly::topology` module of the `heavenly` crate (cargo feature `topology`, on by default).

<figure class="hv-figure">
{{#heavenly-diagram topology}}
<figcaption>Two London racks and a satellite rack. The satellite depends on the UK ground segment, which depends on the same grid as the racks.</figcaption>
</figure>

## Queries

| Query | Answers |
|-------|---------|
| `failure_domains(loc)` | every domain whose failure affects `loc`, transitively |
| `shared_failure_domains(a, b)` | single points of failure common to both |
| `independent(a, b, ignore)` | no shared domain, optionally ignoring kinds such as `Jurisdiction` |
| `blast_radius(domain)` | every location affected if `domain` fails |
| `domains_of_kind(loc, kind)` | e.g. every jurisdiction that applies to `loc` |

The design proposal requires both of these, and the tests check both:

* **Physically distant nodes can share a failure dependency.** A rack in
  London and a rack in orbit both depend on `grid:uk-south`.
* **One logical region can span several physical sites.** `region:europe`
  covers two zones in different buildings.

## Jurisdiction

A jurisdiction is a domain you *assign*. It is never computed from
coordinates: whose law applies to a server, a satellite or a lunar base is a
legal and contractual question. The library only records the answer, and
lets you query it alongside failure domains (or exclude it with
`independent(…, &[DomainKind::Jurisdiction])`).

## Putting it together

The *placement* example combines everything. It takes a rack in London and
three candidate replicas, and for each one reports the failure domains it
shares with the original, whether it is independent, and the one-way light
time. For the satellite it also gives the pass windows over London:

```text
LHR zone B rack  independent: false shared: {"grid:uk-south", "law:gb"}  one-way light time now: 0.076 ms
Frankfurt rack   independent: true  shared: {}                           one-way light time now: 2.132 ms
sat-42 rack      independent: false shared: {"grid:uk-south"}            one-way light time now: 37.909 ms
sat-42 passes above 10° from London in the next 24 h: 5
```

Run it with `cargo run -p heavenly --example placement`. The code
is on the [Signals and visibility](signals.md) page.
