#!/usr/bin/env python3
"""
Generate Tauri app icons from SVG source.
Creates PNG files in all required sizes for macOS, Windows, and Linux.
"""

import os
import subprocess
import sys
from pathlib import Path

# Try to use cairosvg for SVG to PNG conversion
try:
    import cairosvg
    CAIRO_AVAILABLE = True
except ImportError:
    CAIRO_AVAILABLE = False

try:
    from PIL import Image
    PIL_AVAILABLE = True
except ImportError:
    PIL_AVAILABLE = False

# Icon sizes required by Tauri
ICON_SIZES = [32, 128, 256, 512, 1024]

# macOS iconset sizes (for .icns)
MACOS_SIZES = [16, 32, 64, 128, 256, 512, 1024]

def svg_to_png_cairosvg(svg_path, png_path, size):
    """Convert SVG to PNG using cairosvg."""
    cairosvg.svg2png(
        url=str(svg_path),
        write_to=str(png_path),
        output_width=size,
        output_height=size
    )
    print(f"Generated: {png_path} ({size}x{size})")

def create_icns(iconset_dir, output_path):
    """Create .icns file from iconset using iconutil (macOS)."""
    try:
        subprocess.run(
            ['iconutil', '-c', 'icns', str(iconset_dir), '-o', str(output_path)],
            check=True,
            capture_output=True
        )
        print(f"Created: {output_path}")
        return True
    except subprocess.CalledProcessError as e:
        print(f"Failed to create .icns: {e}")
        return False
    except FileNotFoundError:
        print("iconutil not found (macOS only)")
        return False

def create_ico(png_files, output_path):
    """Create .ico file from PNG files using ImageMagick or Pillow."""
    if not PIL_AVAILABLE:
        print("Pillow not available, cannot create .ico")
        return False
    
    images = []
    # Sort by size (smallest first for .ico)
    for size in sorted(set([16, 32, 48, 64, 128, 256])):
        png_path = next((p for p in png_files if f"{size}x{size}" in p.name), None)
        if png_path:
            img = Image.open(png_path)
            # Convert to RGBA if necessary
            if img.mode != 'RGBA':
                img = img.convert('RGBA')
            images.append(img)
    
    if images:
        # Save multi-resolution ICO
        images[0].save(
            output_path,
            format='ICO',
            sizes=[(img.width, img.height) for img in images],
            append_images=images[1:]
        )
        print(f"Created: {output_path}")
        return True
    return False

def main():
    script_dir = Path(__file__).parent
    project_root = script_dir.parent
    
    svg_path = script_dir / "generate-icon.svg"
    icons_dir = project_root / "src-tauri" / "icons"
    iconset_dir = icons_dir / "icon.iconset"
    
    if not svg_path.exists():
        print(f"SVG not found: {svg_path}")
        sys.exit(1)
    
    if not CAIRO_AVAILABLE:
        print("Error: cairosvg is required for SVG conversion")
        print("Install with: pip3 install cairosvg")
        sys.exit(1)
    
    print(f"Generating icons from: {svg_path}")
    print(f"Output directory: {icons_dir}")
    print()
    
    # Ensure directories exist
    icons_dir.mkdir(parents=True, exist_ok=True)
    iconset_dir.mkdir(parents=True, exist_ok=True)
    
    generated_pngs = []
    
    # Generate standard Tauri icon sizes
    print("=== Standard PNG Icons ===")
    for size in ICON_SIZES:
        png_path = icons_dir / f"icon{size}x{size}.png"
        svg_to_png_cairosvg(svg_path, png_path, size)
        generated_pngs.append(png_path)
    
    # Also create the main icon.png (1024x1024)
    main_icon = icons_dir / "icon.png"
    svg_to_png_cairosvg(svg_path, main_icon, 1024)
    
    print()
    print("=== macOS Icon Set ===")
    # Generate macOS iconset files
    for size in MACOS_SIZES:
        # Normal DPI
        png_name = f"icon_{size}x{size}.png"
        png_path = iconset_dir / png_name
        svg_to_png_cairosvg(svg_path, png_path, size)
        
        # High DPI (@2x)
        if size <= 512:
            png_name_2x = f"icon_{size}x{size}@2x.png"
            png_path_2x = iconset_dir / png_name_2x
            svg_to_png_cairosvg(svg_path, png_path_2x, size * 2)
    
    # Create .icns file
    print()
    print("=== Creating .icns (macOS) ===")
    icns_path = icons_dir / "icon.icns"
    if create_icns(iconset_dir, icns_path):
        # Clean up iconset directory after creating .icns
        import shutil
        shutil.rmtree(iconset_dir)
        print(f"Cleaned up: {iconset_dir}")
    
    # Create .ico file (Windows)
    print()
    print("=== Creating .ico (Windows) ===")
    # Generate additional sizes needed for .ico
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    ico_pngs = []
    for size in ico_sizes:
        png_path = icons_dir / f"icon_ico_{size}.png"
        svg_to_png_cairosvg(svg_path, png_path, size)
        ico_pngs.append(png_path)
    
    ico_path = icons_dir / "icon.ico"
    if create_ico(ico_pngs, ico_path):
        # Clean up temporary ICO PNGs
        for png in ico_pngs:
            png.unlink()
            print(f"Cleaned up: {png.name}")
    
    # Create Store logo (Windows Store)
    print()
    print("=== Store Logo ===")
    store_logo = icons_dir / "StoreLogo.png"
    svg_to_png_cairosvg(svg_path, store_logo, 50)
    
    print()
    print("=== Icon Generation Complete ===")
    print(f"Icons located in: {icons_dir}")
    
    # List all generated files
    print()
    print("Generated files:")
    for f in sorted(icons_dir.iterdir()):
        if f.is_file():
            size_kb = f.stat().st_size / 1024
            print(f"  - {f.name} ({size_kb:.1f} KB)")

if __name__ == "__main__":
    main()
