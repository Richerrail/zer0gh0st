#!/usr/bin/env python3
"""Génère les WAV du pack `castle` (SFX rétro, originaux, CC0).

Pas de sample sous licence : tout est synthétisé ici (ondes carrées, bruit,
enveloppes). Les fichiers produits sont versionnés dans assets/sfx/castle/.
Usage : python3 scripts/gen-sfx.py
"""
import math
import os
import random
import struct
import wave

RATE = 44100
OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "sfx", "castle")
random.seed(1993)

def write(name, samples, vol=0.28):
    path = os.path.join(OUT, name)
    peak = max(1e-9, max(abs(s) for s in samples))
    scale = (vol * 32767) / peak
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(
            b"".join(struct.pack("<h", int(max(-32767, min(32767, s * scale)))) for s in samples)
        )
    print(f"{name}: {len(samples) / RATE:.2f}s, {os.path.getsize(path)} octets")

def env(i, n, attack=0.005, decay=1.0):
    a = int(RATE * attack) or 1
    if i < a:
        return i / a
    return max(0.0, 1.0 - (i - a) / max(1, n - a)) ** decay

def square(freq, dur, vol=1.0, decay=1.5, sweep=None, duty=0.5):
    n = int(RATE * dur)
    out = []
    phase = 0.0
    for i in range(n):
        f = freq if sweep is None else freq + (sweep - freq) * (i / max(1, n - 1))
        phase += f / RATE

        s = 1.0 if (phase % 1.0) < duty else -1.0
        out.append(s * vol * env(i, n, decay=decay))
    return out

def noise(dur, vol=1.0, decay=2.0, lowpass=0.0):
    n = int(RATE * dur)
    out = []
    prev = 0.0
    for i in range(n):
        v = random.uniform(-1, 1)
        if lowpass > 0:
            prev = prev * lowpass + v * (1 - lowpass)
            v = prev
        out.append(v * vol * env(i, n, decay=decay))
    return out

def silence(dur):
    return [0.0] * int(RATE * dur)

def mix(*tracks):
    n = max(len(t) for t in tracks)
    out = []
    for i in range(n):
        out.append(sum(t[i] for t in tracks if i < len(t)))
    return out

def concat(*tracks):
    out = []
    for t in tracks:
        out.extend(t)
    return out

os.makedirs(OUT, exist_ok=True)

write("tick.wav", square(950, 0.035, decay=3.0))

write("select.wav", mix(noise(0.09, 0.7, decay=3.0, lowpass=0.5), square(320, 0.09, 0.6, sweep=110)))

write(
    "theme_castle.wav",
    concat(
        mix(noise(0.55, 0.9, decay=1.2, lowpass=0.85), square(70, 0.55, 0.5, sweep=48)),
        silence(0.05),
        mix(noise(0.18, 0.5, decay=2.0, lowpass=0.4), square(1400, 0.18, 0.25, decay=2.5)),
        silence(0.05),
        mix(noise(0.25, 0.6, decay=1.5, lowpass=0.8), square(60, 0.25, 0.4)),
    ),
)

write("panel_open.wav", mix(noise(0.16, 0.6, decay=2.2, lowpass=0.6), square(180, 0.16, 0.5, sweep=90)))

write("toggle_on.wav", concat(square(440, 0.05, 0.6), square(660, 0.06, 0.6)))
write("toggle_off.wav", concat(square(660, 0.05, 0.6), square(440, 0.06, 0.6)))

write("error.wav", square(200, 0.2, 0.7, sweep=90, decay=1.0, duty=0.3))

write("done.wav", concat(noise(0.05, 0.7, decay=3.0, lowpass=0.5), silence(0.06), noise(0.06, 0.7, decay=3.0, lowpass=0.5)))
