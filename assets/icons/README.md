# Open BIM application logo

The primary mark is the silver structural **O** with a cyan bolted plate on a
dark blue rounded square. `master.png` is the approved artwork with transparent
outer corners. It was prepared with the built-in imagegen tool from the second
logo concept chosen on 2026-10-30.

Platform assets derived from this single master:

- `open-bim.icns`: macOS app bundle / Finder / Dock.
- `open-bim.ico`: Windows EXE resources / Explorer / shortcuts.
- `16x16.png` through `1024x1024.png`: Linux hicolor application icons.
- `256x256.png`: embedded window icon for native launches.

Regenerate encoded sizes and containers (Python is not needed to build the app):

```sh
uv run --python 3.13 --with pillow==12.0.0 scripts/generate-icons.py
```

Image preparation prompt: preserve the selected silver structural O, cyan plate
and dark rounded-square tile; remove only the white exterior and cast shadow;
keep genuine transparency outside the tile. Resizing and platform encoding are
performed by the script above.

Validation on 2026-10-30: macOS release package and ZIP contain the declared
ICNS; Apple's iconutil decodes it successfully. PNG dimensions and alpha, all
seven ICO frames, Cargo check, Clippy and formatting passed. Linux archive and
installer layout were exercised with mock binaries (including a prefix with
spaces and reinstall). Native Windows/Linux builds and desktop icon appearance
have not been verified locally. A macOS launch initialized the example and
Metal renderer; a GUI capture was unavailable while the Mac was locked.
