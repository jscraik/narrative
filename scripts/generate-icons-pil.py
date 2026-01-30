#!/usr/bin/env python3
"""
Generate Tauri app icons using Pillow (Python Imaging Library).
Creates an icon that matches the app's warm stone + sky blue aesthetic.
"""

import os
import subprocess
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter

def create_icon(size):
    """Create the Narrative app icon at the specified size.
    
    Design matches the app aesthetic:
    - Warm stone gray background (like --bg-page: #f5f5f4)
    - Sky blue accent (#0ea5e9) for the timeline/nodes
    - White elements with subtle shadows
    - Rounded corners matching iOS style
    """
    # App color palette
    stone_100 = (245, 245, 244)      # #f5f5f4 - main background
    stone_200 = (231, 229, 228)      # #e7e5e4 - subtle variation
    stone_300 = (214, 211, 209)      # #d6d3d1 - border/line
    sky_500 = (14, 165, 233)         # #0ea5e9 - accent blue
    sky_600 = (2, 132, 199)          # #0284c7 - darker blue
    white = (255, 255, 255)
    
    # Corner radius (22% of size for iOS-style rounded corners)
    corner_radius = int(size * 0.22)
    
    # Create base image with subtle gradient (stone-100 to stone-200)
    img = Image.new('RGB', (size, size), stone_100)
    draw = ImageDraw.Draw(img)
    
    # Add subtle gradient overlay
    for y in range(size):
        ratio = y / size
        r = int(stone_100[0] + (stone_200[0] - stone_100[0]) * ratio * 0.3)
        g = int(stone_100[1] + (stone_200[1] - stone_100[1]) * ratio * 0.3)
        b = int(stone_100[2] + (stone_200[2] - stone_100[2]) * ratio * 0.3)
        draw.line([(0, y), (size, y)], fill=(r, g, b))
    
    # Convert to RGBA for transparency support
    img = img.convert('RGBA')
    
    # Create mask for rounded corners
    mask = Image.new('L', (size, size), 0)
    mask_draw = ImageDraw.Draw(mask)
    mask_draw.rounded_rectangle([0, 0, size, size], radius=corner_radius, fill=255)
    
    # Apply mask
    img.putalpha(mask)
    
    # Create draw object for the icon elements
    draw = ImageDraw.Draw(img)
    
    # Calculate sizes based on icon size
    center = size // 2
    line_width = max(4, size // 24)
    node_radius = max(10, size // 14)
    
    # Draw the timeline/narrative flow with app colors
    # Main connecting line (stone-300, subtle)
    start_x = int(size * 0.18)
    end_x = int(size * 0.82)
    line_y = center
    
    # Draw subtle connecting line
    draw.line([(start_x, line_y), (end_x, line_y)], 
              fill=stone_300, width=line_width)
    
    # Draw nodes with sky blue accent (matching app UI)
    # Start node (smaller)
    start_pos = (start_x, line_y)
    draw.ellipse([start_pos[0] - node_radius, start_pos[1] - node_radius,
                  start_pos[0] + node_radius, start_pos[1] + node_radius], 
                 fill=white, outline=stone_300, width=2)
    draw.ellipse([start_pos[0] - node_radius//2, start_pos[1] - node_radius//2,
                  start_pos[0] + node_radius//2, start_pos[1] + node_radius//2], 
                 fill=stone_300)
    
    # Middle node (larger, primary accent)
    mid_pos = (center, line_y)
    mid_radius = int(node_radius * 1.3)
    # Shadow/glow effect
    shadow_offset = 3
    draw.ellipse([mid_pos[0] - mid_radius + shadow_offset, mid_pos[1] - mid_radius + shadow_offset,
                  mid_pos[0] + mid_radius + shadow_offset, mid_pos[1] + mid_radius + shadow_offset], 
                 fill=(*stone_200, 100))
    # Main node
    draw.ellipse([mid_pos[0] - mid_radius, mid_pos[1] - mid_radius,
                  mid_pos[0] + mid_radius, mid_pos[1] + mid_radius], 
                 fill=white, outline=sky_500, width=3)
    # Inner accent
    inner_radius = int(mid_radius * 0.6)
    draw.ellipse([mid_pos[0] - inner_radius, mid_pos[1] - inner_radius,
                  mid_pos[0] + inner_radius, mid_pos[1] + inner_radius], 
                 fill=sky_500)
    
    # End node
    end_pos = (end_x, line_y)
    draw.ellipse([end_pos[0] - node_radius, end_pos[1] - node_radius,
                  end_pos[0] + node_radius, end_pos[1] + node_radius], 
                 fill=white, outline=stone_300, width=2)
    draw.ellipse([end_pos[0] - node_radius//2, end_pos[1] - node_radius//2,
                  end_pos[0] + node_radius//2, end_pos[1] + node_radius//2], 
                 fill=stone_300)
    
    # Add subtle inner border/highlight (like cards in the app)
    border_width = max(1, size // 256)
    padding = size // 40
    draw.rounded_rectangle(
        [padding, padding, size - padding, size - padding],
        radius=corner_radius - padding,
        outline=(*white, 80),
        width=border_width
    )
    
    return img

def create_icns(iconset_dir, output_path):
    """Create .icns file from iconset using iconutil (macOS)."""
    try:
        subprocess.run(
            ['iconutil', '-c', 'icns', str(iconset_dir), '-o', str(output_path)],
            check=True,
            capture_output=True
        )
        print(f"Created: {output_path.name}")
        return True
    except subprocess.CalledProcessError as e:
        print(f"Failed to create .icns: {e}")
        return False
    except FileNotFoundError:
        print("iconutil not found (macOS only)")
        return False

def create_ico(images, output_path):
    """Create .ico file from PIL images."""
    ico_images = []
    for size in [16, 32, 48, 64, 128, 256]:
        if size <= max(img.width for img in images):
            largest = max(images, key=lambda x: x.width)
            resized = largest.resize((size, size), Image.Resampling.LANCZOS)
            ico_images.append(resized)
    
    if ico_images:
        ico_images[0].save(
            output_path,
            format='ICO',
            sizes=[(img.width, img.height) for img in ico_images],
            append_images=ico_images[1:]
        )
        print(f"Created: {output_path.name}")
        return True
    return False

def main():
    script_dir = Path(__file__).parent
    project_root = script_dir.parent
    icons_dir = project_root / "src-tauri" / "icons"
    iconset_dir = icons_dir / "icon.iconset"
    
    print(f"Output directory: {icons_dir}")
    print()
    
    # Ensure directories exist
    icons_dir.mkdir(parents=True, exist_ok=True)
    iconset_dir.mkdir(parents=True, exist_ok=True)
    
    # Generate standard Tauri icon sizes
    print("=== Generating PNG Icons ===")
    standard_sizes = [32, 128, 256, 512, 1024]
    generated_images = {}
    
    for size in standard_sizes:
        img = create_icon(size)
        png_path = icons_dir / f"icon{size}x{size}.png"
        img.save(png_path, 'PNG')
        generated_images[size] = img
        print(f"  icon{size}x{size}.png")
    
    # Create main icon.png (copy of 1024)
    main_icon_path = icons_dir / "icon.png"
    generated_images[1024].save(main_icon_path, 'PNG')
    print(f"  icon.png (1024x1024)")
    
    # Generate macOS iconset
    print()
    print("=== Generating macOS Icon Set ===")
    macos_sizes = [16, 32, 64, 128, 256, 512, 1024]
    
    for size in macos_sizes:
        if size in generated_images:
            img = generated_images[size]
        else:
            img = generated_images[1024].resize((size, size), Image.Resampling.LANCZOS)
        
        # Normal DPI
        png_path = iconset_dir / f"icon_{size}x{size}.png"
        img.save(png_path, 'PNG')
        
        # High DPI (@2x)
        if size <= 512:
            png_path_2x = iconset_dir / f"icon_{size}x{size}@2x.png"
            img_2x = generated_images[1024].resize((size * 2, size * 2), Image.Resampling.LANCZOS)
            img_2x.save(png_path_2x, 'PNG')
    
    print(f"  Generated {len(macos_sizes)} iconset entries")
    
    # Create .icns
    print()
    print("=== Creating Platform-Specific Formats ===")
    icns_path = icons_dir / "icon.icns"
    if create_icns(iconset_dir, icns_path):
        import shutil
        shutil.rmtree(iconset_dir)
        print(f"  Cleaned up iconset directory")
    
    # Create .ico for Windows
    ico_path = icons_dir / "icon.ico"
    create_ico(list(generated_images.values()), ico_path)
    
    # Create StoreLogo
    store_logo = icons_dir / "StoreLogo.png"
    store_img = generated_images[1024].resize((50, 50), Image.Resampling.LANCZOS)
    store_img.save(store_logo, 'PNG')
    print(f"  StoreLogo.png")
    
    print()
    print("=== Icon Generation Complete ===")
    print(f"All icons saved to: {icons_dir}")
    print()
    print("Generated files:")
    for f in sorted(icons_dir.iterdir()):
        if f.is_file():
            size_kb = f.stat().st_size / 1024
            print(f"  - {f.name:20s} ({size_kb:6.1f} KB)")

if __name__ == "__main__":
    main()
