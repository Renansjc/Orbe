"""Gera a imagem-base do ícone do Orbe (esfera âmbar). Uso: python make_icon.py saida.png

Depois: npx tauri icon saida.png
"""
import sys

from PIL import Image

SIZE = 1024
out = sys.argv[1] if len(sys.argv) > 1 else "orbe-icon.png"

img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
px = img.load()
cx = cy = SIZE / 2
radius = SIZE * 0.44
light = (SIZE * 0.36, SIZE * 0.32)  # ponto de luz

for y in range(SIZE):
    for x in range(SIZE):
        d = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5
        if d > radius:
            continue
        # Sombreamento radial a partir do ponto de luz.
        ld = ((x - light[0]) ** 2 + (y - light[1]) ** 2) ** 0.5 / (radius * 1.55)
        t = min(max(ld, 0.0), 1.0)
        hi, mid, lo = (255, 244, 210), (242, 180, 65), (138, 79, 0)
        if t < 0.35:
            k = t / 0.35
            c = tuple(hi[i] + (mid[i] - hi[i]) * k for i in range(3))
        else:
            k = (t - 0.35) / 0.65
            c = tuple(mid[i] + (lo[i] - mid[i]) * k for i in range(3))
        # Borda suave (antialias).
        a = min(1.0, max(0.0, (radius - d) / 2.0))
        px[x, y] = (int(c[0]), int(c[1]), int(c[2]), int(255 * a))

img.save(out)
print("ok", out)
