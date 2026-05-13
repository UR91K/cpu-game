#set document(title: "Audio Engine -- Technical Design Document")
#set page(
  paper: "a4",
  margin: (x: 1cm, y: 1cm),
  header: [
    #set text(size: 8pt, fill: rgb("#888"))
    #grid(
      columns: (1fr, 1fr),
      align(left)[Technical Design Document],
      align(right)[2D Tile-Map Acoustic Engine],
    )
    #line(length: 100%, stroke: 0.5pt + rgb("#ddd"))
  ],
  footer: [
    #line(length: 100%, stroke: 0.5pt + rgb("#ddd"))
    #set text(size: 8pt, fill: rgb("#888"))
    #grid(
      columns: (1fr, 1fr),
      align(left)[Status: Draft · Platform: Rust/wgpu · Audio: rodio/cpal],
      align(right)[#context(counter(page).display())],
    )
  ],
)

#set text(font: "Garamond Premier Pro", size: 10pt, fill: rgb("#1a1a1a"))
#set par(justify: true, leading: 0.65em)
#set heading(numbering: "1.")

#show heading.where(level: 1): it => {
  v(1.2em)
  block[
    #set text(size: 18pt, weight: "bold", fill: rgb("#111"))
    #it.body
    #v(0.2em)
    #line(length: 100%, stroke: 1.5pt + rgb("#2563eb"))
  ]
  v(0.4em)
}

#show heading.where(level: 2): it => {
  v(0.8em)
  block[
    #set text(size: 13pt, weight: "bold", fill: rgb("#1e3a5f"))
    #it.body
  ]
  v(0.2em)
}

#show heading.where(level: 3): it => {
  v(0.6em)
  block[
    #set text(size: 11pt, weight: "bold", fill: rgb("#334155"))
    #it.body
  ]
  v(0.15em)
}

// Callout box
#let callout(title: none, body) = {
  block(
    width: 100%,
    fill: rgb("#eff6ff"),
    stroke: (left: 3pt + rgb("#2563eb")),
    inset: (x: 14pt, y: 10pt),
    radius: (right: 4pt),
  )[
    #if title != none [
      #text(weight: "bold", fill: rgb("#1e3a5f"))[#title] \
    ]
    #body
  ]
}

// Code block
#show raw.where(block: true): it => {
  block(
    width: 100%,
    fill: rgb("#0f172a"),
    inset: (x: 14pt, y: 12pt),
    radius: 4pt,
  )[
    #set text(font: "Iosevka", size: 6pt, fill: rgb("#e2e8f0"))
    #it
  ]
}

#show raw.where(block: false): it => {
  box(
    fill: rgb("#f1f5f9"),
    inset: (x: 4pt, y: 2pt),
    radius: 3pt,
  )[
    #set text(font: "Iosevka", size: 6pt, fill: rgb("#be185d"))
    #it
  ]
}

// Title block
#block(
  width: 100%,
  fill: rgb("#0f172a"),
  inset: (x: 2.5cm, y: 1.5cm),
  radius: 0pt,
)[
  #set text(fill: white)
  #text(size: 10pt, fill: rgb("#93c5fd"))[Technical Design Document]
  #v(0.4em)
  #text(size: 26pt, weight: "bold")[2D Tile-Map ] #text(size: 26pt, weight: "bold", fill: rgb("#60a5fa"))[Acoustic Engine]
  #v(0.8em)
  #grid(
    columns: (auto, auto, auto),
    column-gutter: 2em,
    [#text(size: 9pt, fill: rgb("#94a3b8"))[STATUS] \ #text(size: 10pt)[Draft]],
    [#text(size: 9pt, fill: rgb("#94a3b8"))[PLATFORM] \ #text(size: 10pt)[Rust / wgpu]],
    [#text(size: 9pt, fill: rgb("#94a3b8"))[AUDIO] \ #text(size: 10pt)[rodio / cpal]],
  )
]

#v(1.5em)

// Table of Contents
#outline(
  title: [Contents],
  indent: auto,
  depth: 2,
)

#pagebreak()

= Overview

This document describes the design of the spatial audio engine for a 2D tile-map game rendered with wgpu. The engine approximates physically plausible room acoustics using runtime 2D path tracing on the CPU, decoupled from both the game simulation and audio mixing threads.

The map is static at runtime, which simplifies the acoustic problem: tile materials and topology never change, so path tracing results are stable between updates and can be updated lazily at 16 Hz without perceptible degradation.

#callout(title: "Key Decision")[
  The static map was considered for offline bake / precomputed probe grids, but arbitrary geometry (spirals, mazes, irregular rooms) makes dense probe grids equivalent in cost to runtime path tracing with worse generality. Runtime path tracing on a dedicated CPU thread was chosen as simpler, more correct, and well within the available CPU budget (Ryzen 5 3600 at ~0.6% baseline utilisation).
]

= Design Invariants

These principles govern every design decision in this document. When in doubt about a tradeoff, these take precedence.

#callout(title: "The arrival set is the canonical acoustic output")[
  The ray solve produces a set of arrivals. That set is the primary render representation. Nothing downstream summarises or collapses it into scalar fields that discard spatial identity.
]

#callout(title: "Diffusion changes temporal character, not spatial ownership")[
  An allpass chain smears an arrival in time. It does not merge that arrival with others or reassign its direction. Late diffuse energy stays bound to the arrival that generated it.
]

#callout(title: "The tail is path-shaped, not listener-shaped")[
  A reverb tail arriving from the doorway of a room should remain narrow and directional at the listener. A single shared tail bus would make it spatially diffuse, which is wrong. Per-arrival allpass chains follow from this directly -- they are not a separate complexity bet.
]

#callout(title: "Reduce work by tracing less, not by merging outcomes")[
  Budget controls operate on the ray solve -- fewer sources, fewer rays, skip conditions, energy thresholds. They do not operate on the output by collapsing distinct arrivals together after the fact.
]

#callout(title: "Scalar summaries are diagnostics, not render primitives")[
  `reverb_time` (RT60) may appear as an analysis metric or budgeting aid. It does not define the rendered tail. Fields that imply a single aggregate tail bus -- such as `reverb_send` or `pre_delay` as top-level params -- are not part of the model.
]

#pagebreak()

= Pipeline

Three independent rates govern the system. They communicate via lock-free channels and atomically swapped structs -- no rate ever blocks another.

#v(0.5em)
#block(
  width: 100%,
  stroke: 1pt + rgb("#e2e8f0"),
  radius: 6pt,
  clip: true,
)[
  #grid(
    columns: (1fr, auto, 1fr, auto, 1fr, auto, 1fr),
    rows: auto,
    // Stage 1
    block(fill: rgb("#eff6ff"), inset: 12pt, width: 100%)[
      #text(size: 8pt, fill: rgb("#2563eb"), weight: "bold")[GAME THREAD]
      #v(4pt)
      #text(size: 11pt, weight: "bold")[Sim Tick]
      #v(2pt)
      #text(size: 9pt, fill: rgb("#2563eb"), weight: "bold")[64 Hz]
      #v(6pt)
      #text(size: 8.5pt)[Emits SoundEvents when entities produce sounds. Writes latest entity positions.]
    ],
    // Arrow
    block(inset: (y: 12pt, x: 4pt))[#text(size: 20pt, fill: rgb("#94a3b8"))[›]],
    // Stage 2
    block(fill: rgb("#f0fdf4"), inset: 12pt, width: 100%)[
      #text(size: 8pt, fill: rgb("#16a34a"), weight: "bold")[ACOUSTIC THREAD]
      #v(4pt)
      #text(size: 11pt, weight: "bold")[Path Trace]
      #v(2pt)
      #text(size: 9pt, fill: rgb("#16a34a"), weight: "bold")[16 Hz]
      #v(6pt)
      #text(size: 8.5pt)[Reads positions, traces paths through tile map, writes AcousticParams per source.]
    ],
    // Arrow
    block(inset: (y: 12pt, x: 4pt))[#text(size: 20pt, fill: rgb("#94a3b8"))[›]],
    // Stage 3
    block(fill: rgb("#fdf4ff"), inset: 12pt, width: 100%)[
      #text(size: 8pt, fill: rgb("#9333ea"), weight: "bold")[AUDIO THREAD]
      #v(4pt)
      #text(size: 11pt, weight: "bold")[cpal Callback]
      #v(2pt)
      #text(size: 9pt, fill: rgb("#9333ea"), weight: "bold")[~44100 Hz]
      #v(6pt)
      #text(size: 8.5pt)[Reads latest params (non-blocking). Applies volume, pan, LPF. Rodio mixer fills sample buffer.]
    ],
    // Arrow
    block(inset: (y: 12pt, x: 4pt))[#text(size: 20pt, fill: rgb("#94a3b8"))[›]],
    // Stage 4
    block(fill: rgb("#fff7ed"), inset: 12pt, width: 100%)[
      #text(size: 8pt, fill: rgb("#ea580c"), weight: "bold")[HARDWARE]
      #v(4pt)
      #text(size: 11pt, weight: "bold")[Speakers]
      #v(2pt)
      #text(size: 9pt, fill: rgb("#ea580c"), weight: "bold")[Continuous]
      #v(6pt)
      #text(size: 8.5pt)[cpal hands the filled buffer to the OS audio API (WASAPI / CoreAudio / ALSA).]
    ],
  )
]
#v(0.5em)

Sound events travel from the game thread directly to the audio thread via a lock-free channel, bypassing the acoustic thread entirely. This means sounds play immediately -- the acoustic thread only governs the _treatment_ applied to already-playing sounds, not whether they play at all.

= Thread Architecture

#grid(
  columns: (1fr, 1fr, 1fr),
  column-gutter: 12pt,
  // Game Thread
  block(
    stroke: (top: 3pt + rgb("#2563eb")),
    fill: rgb("#f8fafc"),
    inset: 12pt,
    radius: (bottom: 4pt),
    width: 100%,
  )[
    #text(weight: "bold", size: 10pt)[Game Thread]
    #v(2pt)
    #text(size: 9pt, fill: rgb("#2563eb"), weight: "bold")[64 Hz]
    #v(6pt)
    #set text(size: 8.5pt)
    - Runs sim tick (pure function)
    - Detects sound-emitting events
    - Pushes SoundEvent to audio channel
    - Writes entity positions to shared slot
  ],
  // Acoustic Thread
  block(
    stroke: (top: 3pt + rgb("#16a34a")),
    fill: rgb("#f8fafc"),
    inset: 12pt,
    radius: (bottom: 4pt),
    width: 100%,
  )[
    #text(weight: "bold", size: 10pt)[Acoustic Thread]
    #v(2pt)
    #text(size: 9pt, fill: rgb("#16a34a"), weight: "bold")[16 Hz]
    #v(6pt)
    #set text(size: 8.5pt)
    - Reads entity positions
    - Reads listener position / orientation
    - DDA + omnidirectional ray cast per source
    - Publishes AcousticParams via ArcSwap per source as each trace completes
    - Self-clocks via `std::thread::sleep`
    - `FixedStepSlot` used only to signal/gate from game thread if needed
  ],
  // Audio Thread
  block(
    stroke: (top: 3pt + rgb("#9333ea")),
    fill: rgb("#f8fafc"),
    inset: 12pt,
    radius: (bottom: 4pt),
    width: 100%,
  )[
    #text(weight: "bold", size: 10pt)[Audio Thread]
    #v(2pt)
    #text(size: 9pt, fill: rgb("#9333ea"), weight: "bold")[~44100 Hz (cpal)]
    #v(6pt)
    #set text(size: 8.5pt)
    - Drains SoundEvent channel
    - Spawns/stops Sinks in pool
    - Reads latest AcousticParams
    - Applies volume / pan / LPF
    - Rodio mixer → cpal buffer fill
  ],
)

== Shared State

```rust
struct SharedAcousticState {
    // written by acoustic thread, read by audio thread
    // ArcSwap: the full struct is built before the pointer is swapped,
    // so readers always see either the complete old params or the complete
    // new params -- never a partial write.
    // the acoustic thread publishes per-source as each trace completes
    // rather than batching all sources before any swap.
    params: HashMap<EntityId, ArcSwap<AcousticParams>>,
}

struct SoundEvent {
    entity_id: EntityId,
    sound:     SoundId,
    position:  Vec2,
    volume:    f32,
}
```

Stale acoustic params on the audio thread (up to ~62ms behind at 16 Hz) are imperceptible. Acoustic parameters change only as the listener or source moves through space -- at normal movement speeds the perceptible change between updates is negligible.

= Acoustic Path Tracing

The core acoustic calculation is 2D path tracing on the tile grid -- the same family of algorithms as light transport but with different physical rules. Sound does not find a single optimal path: it propagates omnidirectionally and the listener hears the superposition of every ray that happens to arrive. The ray set is what the reverb is -- there are no separate path statistics to derive reverb parameters from.

64 rays are cast per source per update cycle. Each ray that reaches the listener within `LISTENER_RADIUS` is recorded as an arrival. The full set of arrivals drives the acoustic params struct for that source -- effectively a per-source reverb VST whose parameters are derived from geometry rather than set by hand.

The tile map provides free axis-aligned wall normals and uniform wall thickness, both of which simplify the per-bounce math considerably. Complex geometry -- spiral rooms, mazes, irregular caverns -- is handled correctly by construction: rays that navigate the geometry arrive with the correct delay, attenuation, and direction without any special-casing.

== Ray Casting

Rays are cast in a uniform angular distribution from the source. A first-pass DDA ray to the listener is always cast directly; if it arrives unoccluded it is treated as the direct-path arrival. The remaining rays fan out omnidirectionally and recurse on wall hits.

```rust
fn cast_acoustic_rays(
    map:         &TileMap,
    source:      Vec2,
    listener:    Vec2,
    n_rays:      usize,   // fixed at 64
    max_bounces: usize,   // fixed at 4
) -> Vec<AcousticArrival> {
    let mut arrivals = vec![];

    // direct path first
    trace_ray(map, source, (listener - source).normalize(),
              listener, max_bounces, 1.0, 0.0, 0.0, 0, Default::default(), &mut arrivals);

    // omnidirectional fan
    for i in 0..n_rays {
        let angle = (i as f32 / n_rays as f32) * TAU;
        trace_ray(map, source, Vec2::from_angle(angle),
                  listener, max_bounces, 1.0, 0.0, 0.0, 0, Default::default(), &mut arrivals);
    }

    arrivals
}

fn trace_ray(
    map:          &TileMap,
    origin:       Vec2,
    dir:          Vec2,
    listener:     Vec2,
    bounces_left: usize,
    energy:       f32,
    distance:     f32,
    lpf_acc:      f32,
    diffuse_bounces: u32,
    allpass_acc:  [AllpassStage; 3],  // built inline; written to arrival on hit
    arrivals:     &mut Vec<AcousticArrival>,
) {
    if ray_passes_listener(origin, dir, listener, LISTENER_RADIUS) {
        let total_dist = distance + dist_to_listener(origin, dir, listener);
        arrivals.push(AcousticArrival {
            delay:           total_dist / SPEED_OF_SOUND,
            energy,
            lpf:             lpf_acc,
            direction:       dir,
            diffuse_bounces,
            allpass:         allpass_acc,
        });
        return;
    }

    if bounces_left == 0 || energy < MIN_ENERGY { return; }

    let Some((hit_pos, mat, normal)) = cast_to_wall(map, origin, dir) else { return; };
    let seg_dist = (hit_pos - origin).length();
    let new_dist = distance + seg_dist;

    // transmission branch -- continues through wall in same direction
    if energy * mat.transmission > MIN_ENERGY {
        trace_ray(map, hit_pos, dir, listener, bounces_left,
            energy * mat.transmission, new_dist,
            // compounds correctly: sequential walls each attenuate high frequencies
            (lpf_acc + mat.transmission_lpf).min(1.0),
            diffuse_bounces, allpass_acc, arrivals);
    }

    // reflection branch -- allpass stage added here if surface is diffuse
    let reflected_energy = energy * (1.0 - mat.absorption);
    if reflected_energy > MIN_ENERGY {
        let reflected_dir = reflect(dir, normal);
        let new_diffuse   = diffuse_bounces + mat.is_diffuse as u32;
        let new_allpass   = if mat.is_diffuse {
            push_allpass_stage(allpass_acc, AllpassStage {
                delay_samples: (seg_dist / SPEED_OF_SOUND * SAMPLE_RATE) as usize,
                feedback:      1.0 - mat.absorption,
            })
        } else { allpass_acc };
        trace_ray(map, hit_pos, reflected_dir, listener, bounces_left - 1,
            reflected_energy, new_dist, lpf_acc, new_diffuse, new_allpass, arrivals);
    }
}
```

== Reflection & Transmission Splitting

At each wall hit the ray splits into a reflected branch and a transmitted branch. Both branches only continue if their energy exceeds `MIN_ENERGY`. For most wall materials this gating collapses the tree quickly -- a concrete wall with 2% transmission kills the transmitted branch on the first wall. Only highly transparent materials like glass produce meaningful branching, which is physically correct: glass is the surface where both branches carry significant energy.

This approach is deterministic and produces no variance, which matters for a system updating at 16 Hz -- Russian roulette termination would introduce jitter in the derived params.

== Arrival Classification

Each arrival carries a `diffuse_bounces` count -- the number of reflections off diffuse (rough) surfaces. This is a rendering hint that determines how temporal smearing is applied, not a routing decision into a shared tail bus. All arrivals, including `DiffuseTail`, remain spatially independent and are mixed at their own direction and energy.

```rust
enum ArrivalKind { Direct, EarlyReflection, LateReflection, DiffuseTail }

fn classify(a: &AcousticArrival) -> ArrivalKind {
    match a.diffuse_bounces {
        0     => ArrivalKind::Direct,
        1     => ArrivalKind::EarlyReflection,
        2..=3 => ArrivalKind::LateReflection,
        // DiffuseTail: temporal character fully smeared by allpass chain.
        // Does NOT mean routed to a shared reverb bus -- direction and energy
        // are preserved. The tail is path-shaped, not listener-shaped.
        _     => ArrivalKind::DiffuseTail,
    }
}
```

RT60 can be derived from the arrival set as an analysis metric -- plot energy against delay time across all arrivals, fit an exponential decay, read off the time to reach −60 dB. This is useful for budgeting and debugging but is not the thing that drives the rendered tail.

= Allpass Diffusion

Arrivals with two or more diffuse bounces have lost their discrete echo character and become part of the reverb tail. Rather than treating them as point arrivals, they are routed through a chain of allpass filters that smear energy in time without altering frequency content -- matching what repeated diffuse reflections do physically.

This is the same building block used in Schroeder and Freeverb reverbs, but here the chain parameters are derived from actual ray path geometry rather than being fixed constants tuned by ear. Crucially, the chain is built per-arrival and lives on `AcousticArrival` -- each arrival carries its own allpass history from the specific surfaces it bounced off. Two arrivals that took different paths through different materials will have different chains even if they accumulate the same number of diffuse bounces.
#pagebreak()
== Construction During the Trace

The allpass chain is built _inline during `trace_ray`_, not post-hoc from a frozen struct. Segment geometry -- the length and material of each bounce -- is only available while the trace is executing and that wall hit is live on the stack.
Once the arrival is pushed to the arrivals vec the per-segment data is gone; there is no `diffuse_segments` field on `AcousticArrival`. The chain accumulates as a parameter passed through the recursion alongside energy, distance, and lpf_acc:

```rust
fn trace_ray(
    map:             &TileMap,
    origin:          Vec2,
    dir:             Vec2,
    listener:        Vec2,
    bounces_left:    usize,
    energy:          f32,
    distance:        f32,
    lpf_acc:         f32,
    diffuse_bounces: u32,
    allpass_acc:     [AllpassStage; 3],  // accumulates inline, written to arrival
    arrivals:        &mut Vec<AcousticArrival>,
) {
    if ray_passes_listener(origin, dir, listener, LISTENER_RADIUS) {
        let total_dist = distance + dist_to_listener(origin, dir, listener);
        arrivals.push(AcousticArrival {
            delay:           total_dist / SPEED_OF_SOUND,
            energy,
            lpf:             lpf_acc,
            direction:       dir,
            diffuse_bounces,
            allpass:         allpass_acc,  // chain is complete, written once
        });
        return;
    }

    if bounces_left == 0 || energy < MIN_ENERGY { return; }

    let Some((hit_pos, mat, normal)) = cast_to_wall(map, origin, dir) else { return; };
    let seg_dist = (hit_pos - origin).length();
    let new_dist  = distance + seg_dist;

    // transmission branch
    if energy * mat.transmission > MIN_ENERGY {
        trace_ray(map, hit_pos, dir, listener, bounces_left,
            energy * mat.transmission, new_dist,
            (lpf_acc + mat.transmission_lpf).min(1.0),
            diffuse_bounces, allpass_acc, arrivals);
    }

    // reflection branch -- if diffuse, add a stage to the allpass accumulator
    let reflected_energy = energy * (1.0 - mat.absorption);
    if reflected_energy > MIN_ENERGY {
        let reflected_dir  = reflect(dir, normal);
        let new_diffuse    = diffuse_bounces + mat.is_diffuse as u32;
        let new_allpass    = if mat.is_diffuse {
            push_allpass_stage(allpass_acc, AllpassStage {
                // segment length at this bounce → delay time for this stage
                delay_samples: (seg_dist / SPEED_OF_SOUND * SAMPLE_RATE) as usize,
                // surface absorption → feedback coefficient
                feedback:      1.0 - mat.absorption,
            })
        } else {
            allpass_acc
        };
        trace_ray(map, hit_pos, reflected_dir, listener, bounces_left - 1,
            reflected_energy, new_dist, lpf_acc, new_diffuse, new_allpass, arrivals);
    }
}

// pushes a stage into the fixed-size accumulator, shifting out the oldest if full
fn push_allpass_stage(
    acc:   [AllpassStage; 3],
    stage: AllpassStage,
) -> [AllpassStage; 3] {
    [acc[1], acc[2], stage]
}
```

The LPF accumulated along the ray path is applied _after_ the allpass chain, not before. The allpass stages process the input signal before frequency shaping -- applying LPF to the input would incorrectly filter the signal feeding the delay buffers, altering the diffusion character.

The overall spatial diffusion of the source -- how enveloping vs focused it sounds -- is not stored anywhere. It is an emergent property of mixing all arrivals at their individual directions and energies. A source heard through a narrow doorway produces arrivals clustered from one direction; a source in the same room produces arrivals from many directions. No pre-summarisation is needed.

#callout(title: "Analogy")[
  The full system per source is equivalent to a reverb VST insert: direct path → early reflections as discrete taps → late arrivals through per-arrival allpass chains → LPF on the tail. The difference from a conventional reverb is that every parameter of every stage is derived from ray geometry rather than set manually. Each source in the scene has its own independent instance with different parameters depending on where it is relative to the listener.
]

= Worked Examples

These examples demonstrate why the arrival-based model was chosen over collapsed parameter approaches. Both effects emerge from the ray solve with no special-casing -- the geometry produces the right arrivals naturally.

== Large Room With Offset Doorway

The listener is standing in a corridor. A large room is to their right, with a door several tiles away to the left. A sound source is inside the room.

The ray solve produces two distinct populations of arrivals:

*Through-wall arrivals* -- rays that transmit directly through the wall facing the listener. Each wall crossing compounds transmission loss and LPF. These arrivals are quiet, heavily low-passed, arrive from the direction of the wall (straight right), and carry no diffuse bounce history -- no allpass smearing. They contribute a faint, muffled ghost of the source.

*Doorway arrivals* -- rays that exit through the open doorway, travel through open air, and reach the listener. These arrive from the direction of the door (to the left), at higher energy, lower LPF, and with whatever diffuse bounce history they accumulated bouncing around the room before exiting. They carry a reverberant tail that is directional -- arriving from the left, not surrounding the listener.

Both populations land in the same arrival set. The audio thread mixes them simultaneously. The listener hears the muffled through-wall signal and the reverberant doorway signal at the same time, from different directions. This requires no zone detection, no portal graph, and no source position redirect -- it falls out of the ray geometry.

A collapsed model (single redirected source position + shared reverb tail) cannot reproduce this. It can place the source at the doorway or at the wall, not both simultaneously.

== Spiral Room -- Source at Centre

The listener is outside a spiral-shaped room. The sound source is at the centre of the spiral.

*Through-wall arrivals* -- rays attempting to travel directly through the spiral walls accumulate compounding transmission loss with each layer. After two or three walls the energy drops below threshold and those branches die. The listener hears an extremely faint, heavily low-passed signal from the general direction of the source centre -- barely audible.

*Corridor arrivals* -- rays that find their way through the spiral corridor travel a long winding path, bouncing off the inner and outer walls repeatedly. These walls are stone or concrete -- diffuse, high absorption. By the time they exit the spiral opening, each such ray has accumulated several diffuse bounces, significant path length, and a fully populated per-arrival allpass chain. Critically, all rays that successfully navigate the spiral happen to exit through the same opening. Their directions therefore cluster -- the listener hears them arriving from one focused direction.

Three properties of the corridor arrivals reinforce each other perceptually:

*Pre-delay* -- the long spiral path means significant travel time before any corridor arrival reaches the listener. The faint through-wall ghost arrives first; a moment later the reverberant tail arrives from the opening. The gap is a strong cue that the sound comes from somewhere spatially complex.

*Directional focus* -- all corridor exits are in the same place, so the reverb tail is narrow, arriving from the opening. It does not surround the listener. Hearing a spatially focused reverb tail correctly implies the listener is outside the reverberant space, not inside it.

*Allpass smearing* -- multiple diffuse bounces off absorptive walls produce dense per-arrival allpass chains. The tail is temporally smeared without being spatially spread. The listener hears blurred, muffled reverb from one direction.

None of this required detecting that the geometry is a spiral, finding the exit, or computing a bottleneck. The ray solve just cast rays and the physics produced the correct arrivals.

= Physical Phenomena

Assessment of each acoustic phenomenon for relevance in the tile-map model.

#set table(
  stroke: (x, y) => if y == 0 { (bottom: 1.5pt + rgb("#334155")) } else { (bottom: 0.5pt + rgb("#e2e8f0")) },
  fill: (x, y) => if y == 0 { rgb("#1e3a5f") } else if calc.odd(y) { rgb("#f8fafc") } else { white },
  inset: (x: 10pt, y: 8pt),
)

#table(
  columns: (1.8fr, 0.8fr, 3fr),
  table.header(
    text(fill: white, weight: "bold")[Phenomenon],
    text(fill: white, weight: "bold")[Priority],
    text(fill: white, weight: "bold")[Approach],
  ),
  [Spreading / divergence],
  box(fill: rgb("#dcfce7"), inset: (x:5pt, y:2pt), radius: 3pt)[#text(size: 8pt, fill: rgb("#166534"), weight: "bold")[MUST]],
  [Inverse square falloff on direct path length. Modified by path transmission for indirect paths.],

  [Absorption],
  box(fill: rgb("#dcfce7"), inset: (x:5pt, y:2pt), radius: 3pt)[#text(size: 8pt, fill: rgb("#166534"), weight: "bold")[MUST]],
  [Per-material coefficient accumulated during DDA. Approximated with two frequency bands: full-range and low-passed.],

  [Transmission],
  box(fill: rgb("#dcfce7"), inset: (x:5pt, y:2pt), radius: 3pt)[#text(size: 8pt, fill: rgb("#166534"), weight: "bold")[MUST]],
  [Per-material transmission fraction. All walls are one tile thick so no thickness modelling is needed. Multiple walls compound multiplicatively.],

  [Reflection],
  box(fill: rgb("#dcfce7"), inset: (x:5pt, y:2pt), radius: 3pt)[#text(size: 8pt, fill: rgb("#166534"), weight: "bold")[MUST]],
  [Up to 2 specular bounce rays. Axis-aligned tile walls give free wall normals. Material diffusion flag routes energy to discrete echo vs reverb tail.],

  [Diffusion],
  box(fill: rgb("#dcfce7"), inset: (x:5pt, y:2pt), radius: 3pt)[#text(size: 8pt, fill: rgb("#166534"), weight: "bold")[MUST]],
  [Per-material flag (smooth → specular, rough → diffuse). Diffuse reflections feed the reverb tail rather than producing discrete echoes.],

  [Diffraction],
  box(fill: rgb("#fef9c3"), inset: (x:5pt, y:2pt), radius: 3pt)[#text(size: 8pt, fill: rgb("#854d0e"), weight: "bold")[SHOULD]],
  [Corner tiles treated as weak secondary emitters. Prevents unnatural hard cutoff when listener rounds a corner.],

  [Interference],
  box(fill: rgb("#fee2e2"), inset: (x:5pt, y:2pt), radius: 3pt)[#text(size: 8pt, fill: rgb("#991b1b"), weight: "bold")[IGNORE\*]],
  [Full wave interference requires phase simulation. Exception: flutter echo in narrow parallel corridors approximated via path width variance feeding a feedback delay.],

  [Refraction],
  box(fill: rgb("#fee2e2"), inset: (x:5pt, y:2pt), radius: 3pt)[#text(size: 8pt, fill: rgb("#991b1b"), weight: "bold")[IGNORE]],
  [Requires medium density / temperature gradients. Not relevant to tile-map geometry. A per-medium speed-of-sound constant is sufficient if underwater tiles exist.],
)

#pagebreak()

= Materials

Each wall tile carries a material that determines its acoustic behaviour. All walls are exactly one tile thick -- thickness is not a variable.

```rust
struct WallMaterial {
    /// energy lost on reflection (0 = perfect mirror, 1 = full absorber)
    absorption:       f32,
    /// fraction of energy that passes through the wall
    transmission:     f32,
    /// low-pass cutoff for transmitted signal (0–1 normalised)
    transmission_lpf: f32,
    /// specular vs diffuse reflection character
    reflection:       ReflectionKind,
}
```

#v(0.5em)

// Material cards as a grid
#let mat_bar(label, pct, color) = {
  grid(
    columns: (50pt, 1fr),
    column-gutter: 6pt,
    align(right + horizon)[#text(size: 7.5pt, fill: rgb("#64748b"))[#label]],
    block(
      height: 7pt,
      width: 100%,
      fill: rgb("#e2e8f0"),
      radius: 2pt,
      clip: true,
    )[
      #block(height: 7pt, width: pct, fill: color, radius: 2pt)[]  // pct is now a ratio
    ],
  )
}

#let mat_card(name, abs_pct, tx_pct, lpf_pct) = {
  block(
    stroke: 1pt + rgb("#e2e8f0"),
    fill: white,
    inset: 10pt,
    radius: 6pt,
    width: 100%,
  )[
    #text(weight: "bold", size: 10pt)[#name]
    #v(6pt)
    #mat_bar("absorption", abs_pct, rgb("#3b82f6"))
    #v(3pt)
    #mat_bar("transmit", tx_pct, rgb("#22c55e"))
    #v(3pt)
    #mat_bar("lpf", lpf_pct, rgb("#a855f7"))
  ]
}

#grid(
  columns: (1fr, 1fr, 1fr),
  column-gutter: 10pt,
  row-gutter: 10pt,
  mat_card("Concrete",   95%,  2%,  10%),
  mat_card("Stone",      80%,  5%,  15%),
  mat_card("Wood",       60%,  20%, 45%),
  mat_card("Glass",      25%,  65%, 80%),
  mat_card("Drywall",    50%,  30%, 50%),
  mat_card("Rough Brick",70%,  8%,  20%),
)

#v(4pt)
#text(size: 8.5pt, fill: rgb("#94a3b8"))[Bar values are illustrative starting points. All material parameters are data-driven and tunable.]

#pagebreak()

= Acoustic Params

The output of one path trace cycle for a single source. Written by the acoustic thread, read by the audio thread. The arrival set is the primary representation -- everything the renderer needs is on the arrivals themselves. No top-level fields imply a single aggregate tail bus.

Params are published via `arc-swap`, giving the audio thread wait-free reads at all times. Memory overhead is negligible given the struct size -- the cost of keeping up to three live allocations per source is immaterial.

```rust
struct AcousticArrival {
    delay:           f32,  // path length / SPEED_OF_SOUND
    energy:          f32,  // compounded absorption along path
    lpf:             f32,  // accumulated low-pass mix (applied after allpass chain)
    direction:       Vec2, // arrival direction at listener -- pan derived at mix time
    diffuse_bounces: u32,  // rendering hint: temporal smearing, not routing
    // per-arrival allpass chain built from this arrival's bounce history.
    // the tail is path-shaped: two arrivals from different paths through
    // different materials have different chains. spatial identity is preserved.
    allpass:         [AllpassStage; 3],
}

struct AcousticParams {
    /// overall source volume after distance falloff
    volume:         f32,

    /// the full arrival set -- this is the render representation.
    /// pan, spatial diffusion, reverb tail character, and pre-delay are all
    /// emergent from mixing arrivals at their individual directions, energies,
    /// delays, and allpass chains. nothing is pre-summarised here.
    arrivals:       Vec<AcousticArrival>,

    /// RT60 fitted from arrival energy decay curve -- diagnostic / budgeting aid only.
    /// this is not the thing that drives the rendered tail. capped at 3.0s.
    rt60_diagnostic: f32,
}
```

= State Model

The acoustic update follows the same pure-function pattern as the game simulation. `update_acoustics` takes the current state and produces a new one -- no mutation, easy to reason about, compatible with the existing architecture.

```rust
pub fn update_acoustics(
    state:  &AcousticState,
    game:   &GameState,
    level:  &Level,
) -> AcousticState {
    let mut next = state.clone();
    next.listener = extract_listener(game);
    next.sources  = collect_sound_sources(game);
    next.params   = next.sources.iter()
        .map(|(&id, src)| {
            let params = trace_acoustic_path(level, src, &next.listener);
            (id, params)
        })
        .collect();
    next
}
```

#pagebreak()

= Stability Controls

Budget guards are not optional. The acoustic thread must degrade gracefully under load without affecting the audio thread. All controls operate on the ray solve -- they reduce tracing work. They do not operate on the output by collapsing or merging arrivals.

#table(
  columns: (1.5fr, 3fr),
  table.header(
    text(fill: white, weight: "bold")[Control],
    text(fill: white, weight: "bold")[Mechanism],
  ),
  [Max processing distance],
  [Sources beyond a tile-distance threshold are skipped entirely. No arrivals are computed, existing params are left stale. Stale params at distance are inaudible.],

  [Energy threshold pruning],
  [`MIN_ENERGY` gates both reflection and transmission branches during tracing. Branches below threshold are abandoned early. This is the primary cost control for dense geometry.],

  [Per-source update throttling],
  [Sources that have not moved significantly since the last trace can skip re-evaluation. A positional delta threshold determines what counts as significant. Stationary ambient sources may only need tracing once.],

  [Source priority ordering],
  [Within a 16 Hz budget cycle, closer sources are traced first. If the cycle runs long, distant sources are deferred to the next cycle rather than blocking the publish of nearer results.],

  [Fixed ray and bounce counts],
  [64 rays and 4 max bounces are hard limits, not tunable per-source. Predictable worst-case cost per source regardless of geometry.],

  [Sound rate limiting],
  [Rapidly repeating sounds of the same type from the same source can reuse the most recent params rather than triggering a new trace. Params are position-derived, not sound-derived, so reuse is valid as long as position has not changed significantly.],
)

#callout(title: "Lesson from prior art")[
  The SoundPhysics Minecraft mod required explicit stability controls in production -- processing distance limits, rate limiting, ambient sound skipping, and moving sound update throttling. These were not afterthoughts; they were necessary to keep the system well-behaved across the full range of in-game situations. This system needs the same from day one.
]

#pagebreak()

= PS1 Fidelity Simplifications

The game targets a deliberate aesthetic dissonance: PS1-era visuals with disproportionately detailed simulation underneath. The audio engine is part of that same hierarchy -- spatially accurate, physically derived acoustics coming out of something that looks like 1997. The following simplifications are intentional and fit that context without compromising the core physical accuracy.

#table(
  columns: (1.8fr, 3fr),
  table.header(
    text(fill: white, weight: "bold")[Simplification],
    text(fill: white, weight: "bold")[Rationale],
  ),
  [Integer delay times in allpass buffers],
  [No subsample interpolation. Quantisation to the nearest sample introduces slight comb artefacts that read as vintage character rather than error.],

  [Fixed sample rate allpass buffers],
  [No rate-adaptive resizing. Buffer sizes are fixed at init and never reallocated.],

  [64 rays per source, fixed],
  [No per-scene or per-source tuning. Consistent CPU budget, predictable worst-case cost. 64 provides good angular coverage at tile-map scales.],

  [3 allpass stages maximum],
  [Beyond 3 stages the ear cannot distinguish additional diffusion. The chain is capped regardless of how many diffuse bounces the arrival accumulated.],

  [Hard parameter snaps at 16 Hz],
  [No interpolation between acoustic updates. At PS1 fidelity the snap every 62ms is inaudible. See Non-Goals.],
)

#callout(title: "What is not simplified")[
  Ray count and bounce depth are explicitly preserved at full fidelity. These are where the emergent spatial behaviour lives -- the spiral room, the focused doorway reverb, the maze flutter echo. Reducing them would cut the thing that makes the system distinctive, not just reduce visual quality.
]

#pagebreak()

= Non-Goals

The following are explicitly out of scope for this implementation.

#table(
  columns: (1.8fr, 3fr),
  table.header(
    text(fill: white, weight: "bold")[Item],
    text(fill: white, weight: "bold")[Reason],
  ),
  [GPU compute for path tracing],
  [Workload is too small to saturate a GPU. Dispatch overhead exceeds computation cost. CPU has ample headroom.],

  [Offline acoustic bake],
  [Probe grids dense enough for arbitrary geometry (spirals, mazes) offer no meaningful advantage over runtime tracing. Adds pipeline complexity for no perceptible gain.],

  [HRTF / binaural rendering],
  [Requires head-related transfer function datasets. Stereo pan via `rodio`'s built-in `.pan()` is sufficient for 2D gameplay.],

  [Wave simulation / full interference],
  [Requires per-frequency phase tracking. Computationally prohibitive. Flutter echo in narrow parallel corridors emerges naturally from the ray arrival set without any special handling.],

  [Smooth parameter interpolation between acoustic updates],
  [Hard parameter snaps every 62ms are inaudible at PS1 fidelity. Interpolation would add per-sample lerp cost on the audio thread for no perceptible gain.],

  [Frequency-dependent absorption beyond 2 bands],
  [Diminishing perceptual returns beyond full-range + low-passed blend.],

  [Per-source atomic locking / seqlock],
  [`ArcSwap` publish-swap pattern gives wait-free reads and wait-free writes at negligible memory cost for structs this size. Seqlocks or mutexes would add contention risk on the audio thread for no benefit.],
)
