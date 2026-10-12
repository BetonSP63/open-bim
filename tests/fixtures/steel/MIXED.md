# Independent mixed-operation references

`mixed-masses.json` was calculated with Python's standard `math.pi`, outside the Rust model and geometry kernel. Units: mm, mm3, kg. Density: 7850 kg/m3.

The base plate is 100 x 100 x 20. The through circle is centred at (50,50), diameter 20. The through rectangular cut spans x=20..40 and y=20..40. The retained half-space is x<=75. These removed regions are disjoint and entirely within the retained outline.

| Case | Reference volume |
|---|---|
| hole_box | 200000 - pi * 100 * 20 - 8000 |
| hole_plane | 150000 - pi * 100 * 20 |
| hole_box_plane | 150000 - pi * 100 * 20 - 8000 |

Mass = volume * 7850 / 1e9. `crates/steel/tests/mixed.rs` compares model mass, body volume and separately integrated signed tetrahedral mesh volume to these references at relative tolerance 1e-6. These fixtures do not establish accuracy for intersecting or arbitrary curved cuts.
