#!/usr/bin/env python3
"""
Convert a legacy colour-mapped PNG map to the .amap binary format.

Byte layout (8 bytes per tile):
  [0] wall_type       0 = floor, 1-254 = wall variant, 255 = void
  [1] floor_texture   0 = smooth, 1 = milk veins, ...
  [2] wall_height     0 = default
  [3] ceiling_texture 0 = default
  [4] prop_type       0 = none
  [5] prop_variant    0 = none
  [6] special_type    0 = none
  [7] special_data    0 = none

Header (32 bytes):
  [0:4]   magic        "AMAP"
  [4:6]   version      u16 LE = 1
  [6:8]   width        u16 LE
  [8:10]  height       u16 LE
  [10:12] tile_size    u16 LE (visual hint, default 16)
  [12:16] flags        u32 LE = 0
  [16:20] entity_offset u32 LE (points past tile data)
  [20:24] reserved     u32 LE = 0
  [24:32] level_name   8 bytes, null-padded ASCII
"""

import argparse
import struct
import sys
from pathlib import Path
from PIL import Image

MAGIC   = b"AMAP"
VERSION = 1
HEADER_SIZE    = 32
BYTES_PER_TILE = 8
DEFAULT_TILE_SIZE = 16

# ── Colour map (RGB tuples → (wall_type, floor_texture)) ────────────────────
# wall_type  = 0 means floor tile; floor_texture is then meaningful
# wall_type != 0 means wall tile; floor_texture is ignored
COLOR_MAP: dict[tuple[int,int,int], tuple[int, int]] = {
    (0x00, 0x00, 0x00): (1, 0),   # Black  → wall type 1
    (0x00, 0x26, 0xFF): (2, 0),   # Blue   → wall type 2
    (0x00, 0xFF, 0x21): (3, 0),   # Green  → wall type 3
    (0xFF, 0xFF, 0xFF): (0, 0),   # White  → floor, smooth
    (0xFD, 0x8E, 0xFF): (0, 1),   # Pink   → floor, milk veins
}
DEFAULT_TILE = (0, 0)  # unknown colour → floor, smooth


def rgb_to_tile(r: int, g: int, b: int) -> tuple[int, int]:
    return COLOR_MAP.get((r, g, b), DEFAULT_TILE)


def build_amap(img: Image.Image, level_name: str = "") -> bytes:
    img = img.convert("RGB")
    width, height = img.size
    pixels = img.load()

    name_bytes = level_name.encode("ascii", errors="replace")[:8].ljust(8, b"\x00")

    tile_data_size = width * height * BYTES_PER_TILE
    entity_offset  = HEADER_SIZE + tile_data_size

    header = struct.pack(
        "<4sHHHHIII8s",
        MAGIC,
        VERSION,
        width,
        height,
        DEFAULT_TILE_SIZE,
        0,               # flags
        entity_offset,
        0,               # reserved
        name_bytes,
    )
    assert len(header) == HEADER_SIZE

    tile_buf = bytearray(tile_data_size)
    for y in range(height):
        for x in range(width):
            r, g, b = pixels[x, y]
            wall_type, floor_tex = rgb_to_tile(r, g, b)
            base = (y * width + x) * BYTES_PER_TILE
            tile_buf[base + 0] = wall_type
            tile_buf[base + 1] = floor_tex
            # bytes 2-7 default to 0 (height, ceiling, prop, special)

    # Entity section: just a count of 0 for now
    entity_section = struct.pack("<H", 0)

    return header + bytes(tile_buf) + entity_section


def convert(input_path: Path, output_path: Path, level_name: str) -> None:
    print(f"reading  {input_path}")
    img = Image.open(input_path)
    w, h = img.size
    print(f"  size   {w} × {h}  ({w * h} tiles)")

    data = build_amap(img, level_name)
    output_path.write_bytes(data)

    tile_bytes = w * h * BYTES_PER_TILE
    print(f"wrote    {output_path}  ({len(data)} bytes, {tile_bytes} tile bytes)")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Convert a legacy colour-mapped PNG map to .amap binary format."
    )
    parser.add_argument("input",  type=Path, help="source PNG file")
    parser.add_argument("output", type=Path, nargs="?", help="destination .amap file (default: same name)")
    parser.add_argument("--name", default="", help="level name (max 8 ASCII chars)")
    args = parser.parse_args()

    if not args.input.exists():
        print(f"error: {args.input} not found", file=sys.stderr)
        sys.exit(1)

    output = args.output or args.input.with_suffix(".amap")
    convert(args.input, output, args.name)


if __name__ == "__main__":
    main()