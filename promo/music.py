# Синтез фоновой музыки для ролика: 120 BPM, 76 секунд, стерео WAV.
import sys, wave
import numpy as np

SR = 44100
DUR = 76.0
BEAT = 0.5
N = int(SR * DUR)
rng = np.random.default_rng(7)

L = np.zeros(N); R = np.zeros(N)            # итоговые шины
padL = np.zeros(N); padR = np.zeros(N)      # пэд и бас — под «сайдчейн»
bass = np.zeros(N)
arpL = np.zeros(N); arpR = np.zeros(N)
drums = np.zeros(N)
fx = np.zeros(N)

def mf(m): return 440.0 * 2 ** ((m - 69) / 12)
def idx(t): return int(round(t * SR))

def add(buf, start, sig):
    i = idx(start)
    if i >= N: return
    j = min(N, i + len(sig))
    buf[i:j] += sig[: j - i]

def saw(freq, n, harm=14, detune=0.0):
    t = np.arange(n) / SR
    f = freq * 2 ** (detune / 1200)
    out = np.zeros(n)
    for k in range(1, harm + 1):
        if f * k > 9000: break
        out += np.sin(2 * np.pi * f * k * t + k * 0.7) / k
    return out

def env_adsr(n, a, r):
    e = np.ones(n)
    na, nr = int(a * SR), int(r * SR)
    if na: e[:na] = np.linspace(0, 1, na)
    if nr: e[-nr:] *= np.linspace(1, 0, nr)
    return e

def smooth(x, k):
    return np.convolve(x, np.ones(k) / k, mode='same')

# аккорды по тактам (такт = 2 c): F – G – Am – C
CHORDS = [
    ([53, 57, 60, 64], 41),
    ([55, 59, 62, 67], 43),
    ([57, 60, 64, 67], 45),
    ([55, 60, 64, 67], 48),
]
def chord_at(t): return CHORDS[int(t // 2) % 4]

# ---- пэд: весь ролик, мягче во вступлении ----
for bar in range(38):
    t0 = bar * 2.0
    notes, _ = chord_at(t0)
    n = int(2.6 * SR)
    lvl = 0.5 if t0 < 6 else (0.75 if t0 < 10 else 0.62)
    for m in notes:
        e = env_adsr(n, 0.35, 0.9)
        add(padL, t0, saw(mf(m), n, 10, -8) * e * 0.05 * lvl)
        add(padR, t0, saw(mf(m), n, 10, +8) * e * 0.05 * lvl)
    # высокая «струнная» октава в припеве
    if 10 <= t0 < 66:
        e = env_adsr(n, 0.5, 0.9)
        add(padL, t0, np.sin(2 * np.pi * mf(notes[-1] + 12) * np.arange(n) / SR) * e * 0.018)
        add(padR, t0, np.sin(2 * np.pi * mf(notes[-2] + 12) * np.arange(n) / SR) * e * 0.018)

# ---- арпеджио 16-ми ----
pattern = [0, 1, 2, 3, 2, 1, 3, 2]
step = BEAT / 4
t = 2.0
k = 0
while t < 74:
    notes, _ = chord_at(t)
    m = notes[pattern[k % 8]] + 12 + (12 if (k // 8) % 2 and t >= 10 else 0)
    n = int(0.5 * SR)
    tt = np.arange(n) / SR
    sig = (np.sin(2 * np.pi * mf(m) * tt) + 0.35 * np.sin(2 * np.pi * mf(m) * 2 * tt) + 0.12 * np.sin(2 * np.pi * mf(m) * 3 * tt))
    sig *= np.exp(-tt * 9) * np.minimum(1, tt * 400)
    lvl = 0.05 if t < 6 else (0.07 if t < 68 else 0.06)
    if 6 <= t < 10: lvl *= 0.6 + 0.4 * (t - 6) / 4
    pan = 0.3 * np.sin(k * 0.9)
    add(arpL, t, sig * lvl * (1 - pan))
    add(arpR, t, sig * lvl * (1 + pan))
    t += step; k += 1
# эхо на 3/16 (пинг-понг)
d = idx(0.375)
for tap, g in enumerate([0.38, 0.22, 0.12]):
    sh = d * (tap + 1)
    src_l, src_r = (arpR, arpL) if tap % 2 == 0 else (arpL, arpR)
    arpL[sh:] += src_l[:-sh] * g * 0.5
    arpR[sh:] += src_r[:-sh] * g * 0.5

# ---- бас восьмыми с 10 по 68 c ----
t = 10.0
while t < 68:
    _, root = chord_at(t)
    n = int(0.24 * SR)
    m = root + (12 if int(t / (BEAT / 2)) % 2 else 0)
    sig = saw(mf(m), n, 6) * np.exp(-np.arange(n) / SR * 7) * env_adsr(n, 0.004, 0.03)
    sig += np.sin(2 * np.pi * mf(root - 12) * np.arange(n) / SR) * 0.8 * np.exp(-np.arange(n) / SR * 5)
    add(bass, t, sig * 0.13)
    t += BEAT / 2

# ---- ударные ----
def kick():
    n = int(0.4 * SR); tt = np.arange(n) / SR
    f = 45 + 95 * np.exp(-tt * 30)
    ph = 2 * np.pi * np.cumsum(f) / SR
    return np.sin(ph) * np.exp(-tt * 7) * 0.9 + np.exp(-tt * 300) * rng.normal(0, 0.2, n)
def clap():
    n = int(0.25 * SR); tt = np.arange(n) / SR
    no = rng.normal(0, 1, n); no = no - smooth(no, 6); no = smooth(no, 2)
    e = np.exp(-tt * 18) * (1 + 0.6 * (np.sin(tt * 2 * np.pi * 90) > 0) * (tt < 0.03))
    return no * e * 0.28
def hat(open_=False):
    n = int((0.18 if open_ else 0.06) * SR); tt = np.arange(n) / SR
    no = rng.normal(0, 1, n); no = no - smooth(no, 3)
    return no * np.exp(-tt * (18 if open_ else 70)) * 0.09

kicks = []
for b in range(int(DUR / BEAT)):
    t = b * BEAT
    if 6 <= t < 10 and b % 2 == 0 and t < 9.5:      # «шаги» под слова
        add(drums, t, kick() * 0.7); kicks.append(t)
    if 10 <= t < 66:
        if not (46 <= t < 48):
            add(drums, t, kick()); kicks.append(t)
        if b % 2 == 1 and t >= 18: add(drums, t, clap())
        add(drums, t + BEAT / 2, hat(open_=(b % 4 == 3)))
        if t >= 18: add(drums, t + BEAT / 4, hat() * 0.5); add(drums, t + 3 * BEAT / 4, hat() * 0.5)
# дробь перед финалом 66–68
for i in range(16):
    t = 66 + i * 0.125
    add(drums, t, clap() * (0.3 + 0.7 * i / 15))

# ---- сайдчейн для пэда и баса ----
side = np.ones(N)
tt = np.arange(int(0.35 * SR)) / SR
curve = 1 - 0.55 * np.exp(-tt / 0.09)
for t in kicks:
    i = idx(t); j = min(N, i + len(curve))
    side[i:j] = np.minimum(side[i:j], curve[: j - i])
padL *= side; padR *= side; bass *= side

# ---- эффекты: подъём к дропу, удар, «вжухи» на сменах сцен ----
def riser(t0, t1, gain):
    n = idx(t1) - idx(t0); x = np.linspace(0, 1, n)
    no = rng.normal(0, 1, n); hp = no - smooth(no, 4)
    sig = (hp * x + no * (1 - x) * 0.3) * x ** 2 * gain
    ph = 2 * np.pi * np.cumsum(200 + 1400 * x ** 2) / SR
    sig += np.sin(ph) * x ** 3 * gain * 0.25
    add(fx, t0, sig)
riser(7.0, 10.0, 0.22)
riser(66.0, 68.0, 0.12)
def impact(t0, g):
    n = int(2.5 * SR); tt = np.arange(n) / SR
    boom = np.sin(2 * np.pi * (42 + 30 * np.exp(-tt * 8)) * tt) * np.exp(-tt * 2.2)
    no = rng.normal(0, 1, n); crash = (no - smooth(no, 3)) * np.exp(-tt * 2.5) * 0.25
    add(fx, t0, (boom * 0.8 + crash) * g)
impact(10.0, 0.5)
impact(68.0, 0.35)
def whoosh(t_center, g=0.1):
    n = int(0.7 * SR); x = np.linspace(0, 1, n)
    no = rng.normal(0, 1, n)
    bands = smooth(no, 3) - smooth(no, 9)
    e = np.sin(np.pi * x) ** 2
    add(fx, t_center - 0.45, bands * e * g)
for tc in [18, 28, 34, 40, 46, 54, 62]:
    whoosh(tc, 0.16)

# ---- реверберация пэда и арпеджио (свёртка с затухающим шумом) ----
def reverb(x, secs=2.2, wet=0.25, seed=1):
    r = np.random.default_rng(seed)
    n = int(secs * SR); tt = np.arange(n) / SR
    ir = r.normal(0, 1, n) * np.exp(-tt * 3.2); ir = smooth(ir, 4); ir /= np.sqrt((ir ** 2).sum())
    size = 1 << int(np.ceil(np.log2(len(x) + n)))
    y = np.fft.irfft(np.fft.rfft(x, size) * np.fft.rfft(ir, size), size)[: len(x)]
    return x + y * wet

padL = reverb(padL, wet=0.35, seed=1); padR = reverb(padR, wet=0.35, seed=2)
arpL = reverb(arpL, wet=0.3, seed=3); arpR = reverb(arpR, wet=0.3, seed=4)

L = padL + arpL + bass + drums + fx
R = padR + arpR + bass + drums + fx

# общая громкость: плавное начало и конец
tt = np.arange(N) / SR
master = np.clip(tt / 0.8, 0, 1) * np.clip((DUR - tt) / 3.0, 0, 1)
L *= master; R *= master
st = np.stack([L, R], axis=1)
st = np.tanh(st * 1.6) / np.tanh(1.6)
st /= np.abs(st).max() / 0.89
out = sys.argv[1] if len(sys.argv) > 1 else 'music.wav'
with wave.open(out, 'wb') as w:
    w.setnchannels(2); w.setsampwidth(2); w.setframerate(SR)
    w.writeframes((st * 32767).astype('<i2').tobytes())
print('ok', out, st.shape)
