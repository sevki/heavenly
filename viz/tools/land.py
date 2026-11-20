#!/usr/bin/env python3
"""Convert Natural Earth 1:110m land (TopoJSON, from the world-atlas npm
package) into plain lon/lat rings for the book playground.

Natural Earth is public domain; world-atlas is ISC-licensed.

Usage:
    npm pack world-atlas@2.0.2 && tar xzf world-atlas-2.0.2.tgz
    python3 viz/tools/land.py package/land-110m.json > book/src/playground/land-110m.json
"""
import json
import sys

topo = json.load(open(sys.argv[1]))
sx, sy = topo["transform"]["scale"]
tx, ty = topo["transform"]["translate"]

arcs = []
for arc in topo["arcs"]:
    x = y = 0
    pts = []
    for dx, dy in arc:
        x += dx
        y += dy
        pts.append((x * sx + tx, y * sy + ty))
    arcs.append(pts)


def ring(indices):
    out = []
    for i in indices:
        pts = arcs[i] if i >= 0 else arcs[~i][::-1]
        out.extend(pts if not out else pts[1:])
    return out


rings = []
for geom in topo["objects"]["land"]["geometries"]:
    polys = geom["arcs"] if geom["type"] == "MultiPolygon" else [geom["arcs"]]
    for poly in polys:
        for r in poly:
            pts = ring(r)
            flat = []
            for lon, lat in pts:
                flat += [round(lon, 1), round(lat, 1)]
            rings.append(flat)

json.dump({"source": "Natural Earth 1:110m land, public domain (via world-atlas 2.0.2, ISC)", "rings": rings},
          sys.stdout, separators=(",", ":"))
