# Transforms

A `Transform` describes how frame **A** sits and moves relative to frame
**B** at one epoch. If the convention is not stated exactly, chains of
transforms silently produce wrong velocities. `heavenly` therefore fixes it:

**All four quantities are expressed along B's axes.**

| Field | Symbol | Meaning |
|-------|--------|---------|
| `rotation` | `R` | maps A-axis components to B-axis components (`R_BA`) |
| `translation_m` | `t` | position of A's origin relative to B's origin |
| `origin_velocity_m_s` | `ṫ` | velocity of A's origin relative to B |
| `angular_velocity_rad_s` | `ω` | angular velocity of A's axes relative to B's |

<figure class="hv-figure">
{{#heavenly-diagram transform}}
<figcaption>The four quantities of a <code>Transform</code> from A to B, all expressed along B's axes.</figcaption>
</figure>

A state `(p_A, v_A)` maps to:

```text
p_B = R p_A + t
v_B = R v_A + ω × (R p_A) + ṫ
```

The `ω ×` term comes from differentiating a rotating frame (`Ṙ = [ω]× R`).
This term is why a point at rest on the ground moves at about 465 m/s in an
inertial frame:

```rust,ignore
{{#include ../../heavenly/examples/rotating_earth.rs:earth}}
```

## Composition

`a.then(&b)` composes A → B with B → C to give A → C:

```text
R_CA = R_CB R_BA
t_CA, ṫ_CA = b.apply(t_BA, ṫ_BA)          (A's origin is just a moving point)
ω_CA = ω_CB + R_CB ω_BA
```

## Inversion

```text
R_AB = Rᵀ
t_AB = −Rᵀ t
ṫ_AB = Rᵀ (ω × t − ṫ)
ω_AB = −Rᵀ ω
```

## How this is verified

The test suite builds frames that spin and accelerate over time. It
differentiates transformed positions numerically and checks that the analytic
velocity matches. It does this for single transforms, inverses, three-deep
compositions, and multi-hop paths through a `FrameGraph`.

## Labelled transforms

`FrameTransform` wraps a `Transform` with its source frame, destination frame
and epoch. `then` and `apply` check these labels, so a transform cannot be
applied to a state in the wrong frame or at the wrong time.
