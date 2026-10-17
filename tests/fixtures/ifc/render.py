"""Render IFC files with IfcOpenShell tessellation and matplotlib (independent viewer)."""
import sys
from pathlib import Path
import ifcopenshell, ifcopenshell.geom
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from mpl_toolkits.mplot3d.art3d import Poly3DCollection

here = Path(__file__).parent
for name in sys.argv[1:]:
    f = ifcopenshell.open(str(here / name))
    settings = ifcopenshell.geom.settings()
    settings.set("use-world-coords", True)
    fig = plt.figure(figsize=(10, 7), dpi=120)
    ax = fig.add_subplot(projection="3d")
    lo, hi = [1e18] * 3, [-1e18] * 3
    for e in f.by_type("IfcBeam") + f.by_type("IfcColumn") + f.by_type("IfcPlate"):
        shape = ifcopenshell.geom.create_shape(settings, e)  # keep alive while reading
        g = shape.geometry
        v = [g.verts[i:i + 3] for i in range(0, len(g.verts), 3)]
        tris = [[v[g.faces[i]], v[g.faces[i + 1]], v[g.faces[i + 2]]] for i in range(0, len(g.faces), 3)]
        color = {"IfcBeam": "#4a7fc1", "IfcColumn": "#8a8f98", "IfcPlate": "#d98a2b"}[e.is_a()]
        ax.add_collection3d(Poly3DCollection(tris, facecolor=color, edgecolor="none", alpha=0.95))
        for p in v:
            for k in range(3):
                lo[k] = min(lo[k], p[k]); hi[k] = max(hi[k], p[k])
    span = max(h - l for l, h in zip(lo, hi))
    for k, set_lim in enumerate([ax.set_xlim, ax.set_ylim, ax.set_zlim]):
        c = (lo[k] + hi[k]) / 2
        set_lim(c - span / 2, c + span / 2)
    ax.set_title(f"IfcOpenShell {ifcopenshell.version}: {name}")
    ax.view_init(elev=25, azim=-60)
    fig.savefig(here / (Path(name).stem + "-ifcopenshell.png"))
    print("rendered", name)
