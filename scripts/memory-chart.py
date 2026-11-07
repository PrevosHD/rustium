# Regenerates docs/memory-light.svg and docs/memory-dark.svg from the numbers below.
# Usage: python3 scripts/memory-chart.py   (None draws "not measured yet" instead of a bar)
DATA = [
    ("macOS 26", "MacBook Pro, M1 Pro, 16 GB",
     [("WhatsApp app", 215), ("Ferdium", 1140), ("rustium", 1408)]),
    ("Windows 10", "AtlasOS, Dell Latitude 5490, Core i5-8350U, 16 GB DDR4",
     [("WhatsApp app", 935), ("Ferdium", 1141), ("rustium", 680)]),
]
THEMES = {
    "light": dict(surface="#fcfcfb", ink="#0b0b0b", ink2="#52514e", muted="#898781", grid="#e1e0d9",
                  axis="#c3c2b7", other="#a3a19a", accent="#eb6834"),
    "dark": dict(surface="#1a1a19", ink="#f0efec", ink2="#c3c2b7", muted="#898781", grid="#2c2c2a",
                 axis="#383835", other="#6f6d68", accent="#d95926"),
}
WIDTH, X0, SCALE, BAR, PITCH = 880, 176, 0.4, 22, 34  # SCALE: 1,500 MB = 600 px


def bar(x, y, w, h, r=4):
    """Bar path with rounded data end, square at the baseline."""
    return f"M{x} {y}h{w-r}a{r} {r} 0 0 1 {r} {r}v{h-2*r}a{r} {r} 0 0 1 -{r} {r}h-{w-r}z"


def render(t):
    out, y = [], 88
    for panel, hardware, rows in DATA:
        out.append(f'<text x="24" y="{y}" class="panel">{panel}<tspan class="hw"> · {hardware}</tspan></text>')
        top = y + 12
        for i, (label, mb) in enumerate(rows):
            ry = top + i * PITCH
            out.append(f'<text x="{X0-12}" y="{ry+BAR/2+5}" class="label" text-anchor="end">{label}</text>')
            if mb is None:
                out.append(f'<text x="{X0+8}" y="{ry+BAR/2+5}" class="note">not measured yet</text>')
                continue
            w = round(mb * SCALE)
            fill = t["accent"] if label == "rustium" else t["other"]
            out.append(f'<path d="{bar(X0, ry, w, BAR)}" fill="{fill}"><title>{panel}, {label}: {mb:,} MB</title></path>')
            out.append(f'<text x="{X0+w+8}" y="{ry+BAR/2+5}" class="value">{mb:,} MB</text>')
        y = top + len(rows) * PITCH + 36
    bottom = y - 30
    grid = [f'<line x1="{X0}" y1="96" x2="{X0}" y2="{bottom}" stroke="{t["axis"]}" stroke-width="1"/>',
            f'<text x="{X0}" y="{bottom+18}" class="tick" text-anchor="middle">0</text>']
    for mb in (500, 1000, 1500):
        gx = X0 + mb * SCALE
        grid.append(f'<line x1="{gx}" y1="96" x2="{gx}" y2="{bottom}" stroke="{t["grid"]}" stroke-width="1"/>')
        grid.append(f'<text x="{gx}" y="{bottom+18}" class="tick" text-anchor="middle">{mb:,} MB</text>')
    height = bottom + 34
    desc = "; ".join(
        f"{p} on {hw}: " + ", ".join(f"{l} {v:,} MB" if v else f"{l} not measured yet" for l, v in rows)
        for p, hw, rows in DATA)
    halo = f'stroke: {t["surface"]}; stroke-width: 4px; paint-order: stroke;'  # keeps gridlines off the text
    nl = "\n"
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}" role="img" aria-labelledby="t d">
<title id="t">Memory with one WhatsApp account</title>
<desc id="d">{desc}. Lower is better.</desc>
<style>
text {{ font-family: system-ui, -apple-system, "Segoe UI", sans-serif; }}
.title {{ font-size: 18px; font-weight: 600; fill: {t["ink"]}; }}
.sub {{ font-size: 13px; fill: {t["ink2"]}; }}
.panel {{ font-size: 14px; font-weight: 600; fill: {t["ink"]}; {halo} }}
.hw {{ font-weight: 400; fill: {t["muted"]}; }}
.label {{ font-size: 13px; fill: {t["ink2"]}; }}
.value {{ font-size: 13px; font-weight: 600; fill: {t["ink"]}; font-variant-numeric: tabular-nums; }}
.note {{ font-size: 13px; font-style: italic; fill: {t["muted"]}; {halo} }}
.tick {{ font-size: 11px; fill: {t["muted"]}; font-variant-numeric: tabular-nums; }}
</style>
<rect width="{WIDTH}" height="{height}" rx="12" fill="{t["surface"]}"/>
<text x="24" y="36" class="title">Memory with one WhatsApp account</text>
<text x="24" y="58" class="sub">Lower is better. macOS: physical footprint (Activity Monitor). Windows: private working set (Task Manager).</text>
{nl.join(grid)}
{nl.join(out)}
</svg>
'''


if __name__ == "__main__":
    from pathlib import Path
    docs = Path(__file__).resolve().parent.parent / "docs"
    for name, theme in THEMES.items():
        (docs / f"memory-{name}.svg").write_text(render(theme))
    print("wrote", docs / "memory-light.svg", "and memory-dark.svg")
