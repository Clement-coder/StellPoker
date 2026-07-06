# Pixel Art Theme Customization Guide

This guide covers everything you need to customize Stellar Poker's retro pixel-art theme. The theme is built entirely with CSS custom properties, utility classes, and pixel-art CSS techniques — no JavaScript theming library is required.

---

## 1. CSS Variable Overrides

The entire color palette is defined as CSS custom properties in `app/src/app/globals.css`. Override any variable in your own stylesheet or by editing the `:root` block directly.

### Color Palette

```css
:root {
  --sky-top: #4a90d9;
  --sky-bottom: #87ceeb;
  --grass-light: #5cb85c;
  --grass-mid: #4cae4c;
  --grass-dark: #3d8b3d;
  --grass-shadow: #2d6b2d;
  --pixel-brown: #8b6914;
  --pixel-brown-dark: #6b4f12;
  --pixel-cream: #f5e6c8;
  --pixel-red: #e74c3c;
  --pixel-gold: #f1c40f;
  --pixel-blue: #3498db;
  --pixel-green: #27ae60;
  --felt-dark: #1a5c2a;
  --felt-mid: #237a3a;
  --felt-light: #2d9648;
  --card-white: #fefefe;
  --card-shadow: #c0c0c0;
  --ui-panel: rgba(20, 12, 8, 0.85);
  --ui-border: #8b6914;
}
```

| Variable | Default | Purpose |
|---|---|---|
| `--sky-top` / `--sky-bottom` | `#4a90d9` / `#87ceeb` | Day sky gradient endpoints |
| `--grass-*` | Four green shades | Pixel hill fill colors |
| `--pixel-brown` / `--pixel-brown-dark` | `#8b6914` / `#6b4f12` | Borders and UI accents |
| `--pixel-cream` | `#f5e6c8` | Text and panel backgrounds |
| `--pixel-red` / `--pixel-gold` / `--pixel-blue` / `--pixel-green` | — | Button and accent colors |
| `--felt-*` | Three green shades | Poker table felt radial gradient |
| `--card-white` / `--card-shadow` | `#fefefe` / `#c0c0c0` | Playing card faces |
| `--ui-panel` | `rgba(20, 12, 8, 0.85)` | Semi-transparent overlays |
| `--ui-border` | `#8b6914` | Modal and panel borders |

### Overriding Variables

Create a custom CSS file and load it after `globals.css`:

```css
/* custom-theme.css */
:root {
  --felt-dark: #1a3a5c;
  --felt-mid: #234a7a;
  --felt-light: #2d5a96;
  --pixel-gold: #ff6b35;
  --pixel-brown: #4a2c0a;
}
```

---

## 2. Custom Card Back Designs

Card backs are rendered in `app/src/components/Card.tsx` in the `CardBack` sub-component. The default design uses a dark blue gradient with a crosshatch inner border and a centered "S" logo.

### Modifying the Card Back

Edit the `CardBack` component styles:

**Change the gradient and border:**
```tsx
// Card.tsx — CardBack style prop
const cardBackStyle: React.CSSProperties = {
  background: 'linear-gradient(135deg, #2c1a0e 0%, #4a2c0a 50%, #2c1a0e 100%)',
  border: '3px solid var(--pixel-gold)',
  // ...
};
```

**Replace the center logo:**
```tsx
{/* Replace the "S" logo with a custom symbol */}
<span style={{
  color: 'var(--pixel-gold)',
  fontSize: size === 'lg' ? '24px' : size === 'md' ? '18px' : '14px',
}}>♠</span>
```

### Adding Alternate Card Backs

Add a `variant` prop to `Card`:

```tsx
type CardVariant = 'default' | 'gold' | 'dark' | 'custom';

interface CardProps {
  value: number;
  faceDown?: boolean;
  size?: 'sm' | 'md' | 'lg';
  variant?: CardVariant;
}
```

Then conditionally apply styles based on the variant inside `CardBack`.

---

## 3. Table Felt Patterns

The poker table oval is rendered in `app/src/components/Table.tsx` using a radial gradient with layered box-shadows.

### Changing the Felt Pattern

Edit the inline styles on the table oval container:

```tsx
// Current default
style={{
  background: 'radial-gradient(ellipse at center, var(--felt-light) 0%, var(--felt-mid) 40%, var(--felt-dark) 100%)',
  boxShadow: 'inset 0 0 60px rgba(0,0,0,0.3), 0 8px 0 0 rgba(0,0,0,0.4), inset -4px -4px 0px 0px rgba(0,0,0,0.3), inset 4px 4px 0px 0px rgba(255,255,255,0.1)',
}}
```

**Alternative patterns:**

| Pattern | Gradient |
|---|---|
| **Blue felt** | `radial-gradient(ellipse at center, #2d5a96 0%, #1a3a5c 40%, #0d1f33 100%)` |
| **Red felt** | `radial-gradient(ellipse at center, #962d2d 0%, #5c1a1a 40%, #330d0d 100%)` |
| **Purple felt** | `radial-gradient(ellipse at center, #6b2d96 0%, #3a1a5c 40%, #1f0d33 100%)` |

### Adding a Felt Texture Overlay

Add a repeating pattern pseudo-element to simulate fabric:

```css
.table-felt::before {
  content: '';
  position: absolute;
  inset: 0;
  background-image: url("data:image/svg+xml,...");
  background-repeat: repeat;
  opacity: 0.05;
  pointer-events: none;
}
```

---

## 4. Chip Colors

Chips are rendered in `app/src/components/PixelChip.tsx` as pure CSS box-shadow art (no images). Each denomination has four color values.

### Modifying Chip Colors

Edit the chip configuration in `PixelChip.tsx`:

```tsx
// Current chip colors
const chipColors = {
  white:  { outer: '#e0e0e0', inner: '#ffffff', edge: '#bdbdbd', highlight: '#f5f5f5' },
  red:    { outer: '#c0392b', inner: '#e74c3c', edge: '#962d22', highlight: '#ff6b6b' },
  blue:   { outer: '#2471a3', inner: '#3498db', edge: '#1a5276', highlight: '#5dade2' },
  gold:   { outer: '#d4ac0d', inner: '#f1c40f', edge: '#b7950b', highlight: '#f4d03f' },
};
```

**Add a new chip color:**
```tsx
const chipColors = {
  // ...existing colors
  green:  { outer: '#1e8449', inner: '#27ae60', edge: '#145a32', highlight: '#52be80' },
};

// Update denominations
const chipDenominations: Record<ChipColor, number> = {
  white: 25,
  red: 100,
  blue: 500,
  gold: 1000,
  green: 5000,  // New
};
```

---

## 5. Adding New Pixel-Art Assets

### Cat Sprites

Cat sprites are PNG images in `public/cat_sprites/`. Supported sprites are `17.png` through `21.png` and `bot.png`.

**Add a new cat sprite:**
1. Place your sprite PNG in `public/cat_sprites/22.png`
2. Update `opponentSprite()` in `PixelCat.tsx` to include the new index:

```tsx
// pixel-cat.tsx — opponentSprite function
const SPRITES = [17, 19, 20, 21, 22]; // Add your new sprite
const opponentSprite = (seatIndex: number) => {
  return SPRITES[seatIndex % SPRITES.length];
};
```

**Sprite requirements:**
- PNG format with transparency
- Pixel art at native resolution (do not scale up)
- Consistent character height for alignment
- File size under 10 KB for fast loading

### Environment Background

The day/night background is built in `PixelWorld.tsx` using layered SVG and CSS.

**Custom sky colors:**
```tsx
// PixelWorld.tsx — daySky/nightSky gradient strings
const daySky = 'linear-gradient(180deg, #ff9a56 0%, #ffcc8a 30%, #87ceeb 70%, #a8dcf0 100%)';
const nightSky = 'linear-gradient(180deg, #0a0a2e 0%, #1a1045 40%, #0d1225 100%)';
```

**Custom cloud shapes:**
Clouds are defined as character-map grids in `PixelWorld.tsx`. Each cloud is an array of strings where `#` represents a filled pixel. Edit or add new entries to the `clouds` array.

### Sound Assets

Ambient music files are in `public/music/`:
- `day-music.mp3` — plays during day mode
- `night-music.mp3` — plays during night mode

Replace these with your own MP3 files (same filenames) or update the references in `PixelWorld.tsx`.

---

## 6. Pixel Border & Button Utility Classes

### Border Classes

| Class | Thickness | Use Case |
|---|---|---|
| `.pixel-border` | 4px | Main panels, modals |
| `.pixel-border-thin` | 3px | Cards, small elements |
| `.pixel-border-white` | 3px white | Playing card faces |

These classes apply raised 3D pixel borders using box-shadow. To create a custom border, copy the pattern:

```css
.custom-pixel-border {
  border: 3px solid var(--pixel-brown);
  box-shadow:
    inset -3px -3px 0 0 rgba(0,0,0,0.3),
    inset 3px 3px 0 0 rgba(255,255,255,0.15),
    3px 3px 0 0 rgba(0,0,0,0.3);
}
```

### Button Variants

Six button color variants are available:

- `.pixel-btn-green` — action/confirm
- `.pixel-btn-red` — fold/danger
- `.pixel-btn-blue` — join/info
- `.pixel-btn-gold` — important/primary
- `.pixel-btn-orange` — warning
- `.pixel-btn-dark` — secondary

Add a custom button variant by copying the `.pixel-btn` base and setting your own background and shadow colors.

---

## 7. Theme Development Checklist

Use this checklist when creating a new theme:

### Foundation
- [ ] Override CSS custom properties in `:root`
- [ ] Test all button variants with new palette
- [ ] Verify text contrast against `--ui-panel` backgrounds
- [ ] Check day and night sky gradients

### Table & Cards
- [ ] Customize felt gradient and inner border
- [ ] Update card back gradient and center logo
- [ ] Verify card readability (rank/suit contrast)
- [ ] Test all three card sizes (sm, md, lg)

### Chips & Bets
- [ ] Update chip color hex values
- [ ] Test chip stack rendering (overlap)
- [ ] Verify pot pile display with mixed denominations

### Assets
- [ ] Replace or augment cat sprites
- [ ] Test cat idle animation with new sprites
- [ ] Replace ambient music (day and night)
- [ ] Verify cloud and hill rendering

### Layout & Responsiveness
- [ ] Test at 1280px+ desktop width
- [ ] Verify mobile viewport (375px width)
- [ ] Check seat positioning on table oval
- [ ] Review action panel button sizing

### Polish
- [ ] Verify pixel-border classes on all panels
- [ ] Test modal (GameBoy) appearance
- [ ] Confirm identicon generation still contrasts
- [ ] Review animation timing and easing
- [ ] Validate in both Chromium and Firefox

---

## 8. File Reference

| File | Purpose |
|---|---|
| `app/src/app/globals.css` | CSS variables, utility classes, animations |
| `app/src/components/Card.tsx` | Card rendering (face and back) |
| `app/src/components/PixelChip.tsx` | CSS chip sprites |
| `app/src/components/PixelCat.tsx` | Cat sprite component |
| `app/src/components/PixelWorld.tsx` | Background, sky, clouds, hills |
| `app/src/components/Table.tsx` | Table felt and layout |
| `app/src/components/PlayerSeat.tsx` | Player seat with avatar and cards |
| `app/public/cat_sprites/` | Cat PNG assets |
| `app/public/music/` | Ambient music MP3 files |
