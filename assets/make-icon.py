# Собирает assets/icon.png (512) и assets/icon.ico (16–256) из assets/icon.svg.
# Каждый размер рисует сам Inkscape (а не уменьшение одной картинки) — мелкие значки чётче.
# python assets/make-icon.py
import pathlib
import struct
import subprocess
import tempfile

here = pathlib.Path(__file__).parent
inkscape = r"C:\Program Files\Inkscape\bin\inkscape.exe"
svg = here / "icon.svg"


def render(size: int) -> bytes:
    with tempfile.TemporaryDirectory() as tmp:
        out = pathlib.Path(tmp) / f"{size}.png"
        subprocess.run([inkscape, str(svg), "--export-type=png", f"--export-filename={out}",
                        "-w", str(size), "-h", str(size)], check=True, capture_output=True)
        return out.read_bytes()


(here / "icon.png").write_bytes(render(512))
sizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
images = [render(s) for s in sizes]
# ICO с PNG внутри (Windows Vista и новее): заголовок, каталог, картинки
header = struct.pack("<HHH", 0, 1, len(sizes))
offset = 6 + 16 * len(sizes)
entries = b""
for size, data in zip(sizes, images):
    side = 0 if size >= 256 else size
    entries += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(data), offset)
    offset += len(data)
(here / "icon.ico").write_bytes(header + entries + b"".join(images))
print("icon.png и icon.ico готовы")
