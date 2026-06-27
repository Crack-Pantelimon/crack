# Plan: Fix the "skewed / rotated maps" regression (and add GLB export)

## TL;DR — the rotation math is NOT broken; the pipeline is silently broken

The previews look skewed **because the `.blend` files on disk are stale** — they were
built at ~14:56 (before this session's ENU fixes) and have never been rebuilt since,
because `build_blend.py` now **crashes on every tile**. Blender exits with code `0`
even when the `-P` script raises, and `main.py` swallows the crash and just
re-renders a fresh `.jpg` from the *old* geometry. So we keep generating a "proper
manifest" and fresh previews of stale, pre-fix meshes.

This was proven directly (see "Evidence" below): rebuilding the exact same tile
(`3043627270437`) with the **current** `[E,N,U]` ENU rotation and the **current**
constant `ref_point`, after fixing the crashes, renders a clean **axis-aligned**
tile — identical in orientation to the historical "fixed" commit `a166d56` /
`efa60dc`. No change to the rotation matrix, reference point, or camera is needed.

## Evidence (how we know)

1. Same tile `3043627270437`:
   - `a166d56` preview: axis-aligned upright rectangle (the "fixed" look).
   - current `data_out/13/3043627270437.jpg`: rotated ~35–45° diamond (the regression).
2. `git diff a166d56 -- render_tile.py` and the ENU block in `build_blend.py` are
   functionally identical to today → the math did not change.
3. File timestamps: `data_out/13/3043627270437.blend` = **14:56** (pre-session),
   but its `.jpg` = **15:24**. The blend is stale; only the jpg is fresh.
4. `find data_out -name '*.glb' | wc -l` = **0** → the requested GLB export has
   never produced a single file.
5. Running `build_blend.py` by hand reproduces three hard crashes (below). Blender
   still returns exit code `0`, so `main.py`'s `subprocess.run(check=True)` does not
   raise, the stale `.blend` still satisfies `blend_path.exists()`, and the tile is
   reported as "Saved … and rendered preview".
6. After patching the three bugs in a throwaway copy and rebuilding the tile with the
   current ref_point, the preview is axis-aligned (matches `a166d56`). Numerically,
   the tile's mesh-local Y axis maps to local East under the current ENU rotation
   (yaw ≈ 0°), confirming correct alignment.

## Root cause: three crashes in `build_blend.py`

All three must be fixed; the first two abort the build before geometry is written,
the third aborts GLB export after the `.blend` is saved.

1. **`triangulate_strip` arity mismatch** (`build_blend.py:76` vs call at `:334`):
   the function is still defined as `def triangulate_strip(strip)` but is now called
   `triangulate_strip(truncated_strip, w_mask, masked_octants)` →
   `TypeError: triangulate_strip() takes 1 positional argument but 3 were given`.

2. **`masked_octants` arg parsed from the wrong position** (`build_blend.py:329`):
   uses `sys.argv[sys.argv.index("--") + 4]`, which is `ref_y` (a float string), so
   `int("2009573.25…")` → `ValueError`. The octant-mask arg, when present, is the 6th
   positional arg (`index("--") + 6`). Worse, **`main.py` no longer passes it at all**
   (the `cmd` list at `main.py:223-234` only sends 5 args), so the masking feature is
   currently dead code that only serves to crash.

3. **Invalid glTF export kwarg** (`build_blend.py` GLB block): `export_colors=True`
   is not recognized by Blender 5.1.2's `export_scene.gltf` →
   `TypeError: keyword "export_colors" unrecognized`. A minimal call
   (`export_format='GLB', use_selection=False`) was verified to write a valid GLB.

## Secondary problem: failures are invisible

`main.py` treats a Blender subprocess that exits `0` as success, and accepts a
pre-existing stale `.blend` via `blend_path.exists()`. This is why a fully broken
build script still reports "DONE! Exported 68 tiles, Failed: 0". The pipeline needs
to detect script-level failures so this class of bug can never hide again.

## The fix

### 1. `build_blend.py` — make the build actually run

- Update `triangulate_strip` to accept the optional masking args and skip a triangle
  when any of its vertices is in `masked_octants`:
  ```python
  def triangulate_strip(strip, w_mask=None, masked_octants=None) -> np.ndarray:
      triangles = []
      for i in range(len(strip) - 2):
          a, b, c = strip[i], strip[i + 1], strip[i + 2]
          if a == b or a == c or b == c:
              continue
          if w_mask is not None and masked_octants:
              if (w_mask[a] in masked_octants or w_mask[b] in masked_octants
                      or w_mask[c] in masked_octants):
                  continue
          triangles.extend([a, c, b] if (i & 1) else [a, b, c])
      return np.array(triangles, dtype=np.uint32) if triangles else np.array([], dtype=np.uint32)
  ```
- Parse `masked_octants` once near the top of `main()` from the documented arg list
  (`args = sys.argv[sys.argv.index("--") + 1:]`), not inside the mesh loop:
  ```python
  masked_str = args[5] if len(args) > 5 else ""
  masked_octants = {int(x) for x in masked_str.split(",") if x != ""}
  ```
  Then pass `masked_octants` into `triangulate_strip(...)`. Remove the broken inline
  `sys.argv[... + 4]` expression entirely.
- Fix the GLB export to use only Blender-5.1.2-valid kwargs and verify the file was
  written:
  ```python
  out_glb_path = os.path.splitext(out_blend_path)[0] + ".glb"
  bpy.ops.export_scene.gltf(
      filepath=os.path.abspath(out_glb_path),
      export_format='GLB',
      use_selection=False,
  )
  assert os.path.exists(out_glb_path), f"GLB export produced no file: {out_glb_path}"
  ```
  (Keep `export_yup=True` only if confirmed valid; the minimal call above is the
  proven-safe baseline. Do **not** pass `export_colors`.)

> Do NOT touch the `[E,N,U]` ENU rotation (`R_blend`), the constant `ref_point`, or
> `render_tile.py`'s camera. They are already correct and are what keep multiple tiles
> overlaying congruently. Changing them is what caused past thrash.

### 2. `main.py` — pass the mask and stop hiding failures

- Append the masked-octant arg to the build `cmd` so the (now-working) masking is fed
  in: `",".join(map(str, sorted(masked_octants)))` as the 6th positional arg.
  (`masked_octants` is already computed for `decode_node`.)
- Make Blender failures fatal/visible. `subprocess.run(check=True)` is not enough
  because Blender returns `0` on script exceptions. Instead:
  - Capture output (`capture_output=True, text=True`) and treat the run as failed if
    the stderr/stdout contains a Python `Traceback` / `Error:`; **or**
  - Require freshness: record `time.time()` before the call and assert the `.blend`
    (and `.glb`) `mtime` is newer than that timestamp, instead of bare `.exists()`.
  - On failure, log the captured Blender output and count it as `failed`, so the
    summary line reflects reality.

### 3. Regenerate from a clean slate

Stale artifacts are the actual thing the user is seeing, so they must be removed:
```bash
rm -rf _data/3d_data_v2/data_out/*/   # drop stale .blend/.glb/.jpg, keep .gitignore
cd _data/3d_data_v2 && ~/.local/bin/uv run main.py
```

## Verification checklist

- `find _data/3d_data_v2/data_out -name '*.glb' | wc -l` equals the `.blend` count
  (every built tile now has a GLB).
- Spot-check previews across LODs (e.g. `10/3043627270.jpg`, `13/3043627270437.jpg`,
  a level-16 tile): each map is an **upright, axis-aligned** square/rectangle, not a
  diamond — matching `a166d56`.
- All `.blend`/`.glb`/`.jpg` mtimes are from the new run (no 14:56 leftovers).
- The pipeline summary shows `Failed: 0` *and* the count of exported tiles matches the
  number of `.blend` files actually written (sanity that success is real, not stale).
- Open 2–3 GLBs together (or the combined `.blend`) and confirm tiles still overlay
  correctly (congruent placement preserved by the shared constant `ref_point`).

## Out of scope / watch-list (do not fix blindly)

- The proof render's texture looked slightly washed-out/semi-transparent. This is a
  separate material/alpha concern, independent of orientation. Investigate only after
  the geometry regeneration is confirmed; do not let it trigger more rotation changes.
