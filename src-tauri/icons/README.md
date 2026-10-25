# ChessPush application icon

The application mark is an original sculpted silver pawn on a transparent
background, generated with the built-in image generation tool. It is separate
from the official chessboard piece assets.

- `icon.png`: original transparent master.
- `icon.ico`: Windows icon with 16, 24, 32, 48, 64 and 256 px RGBA frames.
- `../../public/favicon.png`: transparent 64 px browser icon from the same master.

Generate platform sizes into a temporary folder, then copy its `icon.ico` into
this directory and its `64x64.png` to `public/favicon.png`:

```powershell
npm run tauri icon -- src-tauri/icons/icon.png --output .qa/icon-refresh/generated
```

## Generation prompt

Use case: logo-brand. Asset type: Windows desktop application icon for ChessPush, a focused chess training application. Create one exceptionally well-crafted contemporary chess pawn, isolated on a genuinely transparent background. The pawn has a coherent Staunton structure: a generous spherical head, a distinct slim neck, a crisp elliptical collar, a smoothly tapering sculpted body, and a broad stepped circular foot. Three-dimensional, precision product illustration with a satin ivory and soft silver surface, charcoal grey contour and side shading for clear separation on both light and dark desktops; subtle warm graphite reflections. Front view with only a slight view onto the elliptical collar and base. Calm soft studio light from upper left; broad clean highlights describe the solid forms. Strong compact silhouette readable at 16, 24 and 32 pixels, generous head, few substantial structural divisions, restrained material detail. Square composition; centered pawn occupies approximately 90 percent of the canvas height with even small clear margins. One continuous physical pawn, elegant proportions, no extra objects, no letters, no text, no logo badge, no square or circular backdrop, no tile, no frame, no floor, no cast shadow outside the pawn, no glow, no decorative sparkles. The entire canvas outside the pawn is fully transparent alpha, including corners and space under its base.
