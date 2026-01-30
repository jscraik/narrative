#!/usr/bin/env python3
"""
Generate Tauri app icons using Pillow (Python Imaging Library).
Creates a gradient background with a timeline/narrative design.
"""

import os
import subprocess
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter

def create_gradient(width, height, color1, color2, color3=None):
    """Create a horizontal gradient image."""
    base = Image.new('RGB', (width, height), color1)
    draw = ImageDraw.Draw(base)
    
    for x in range(width):
        # Calculate position in gradient (0.0 to 1.0)
        ratio = x / width
        
        if color3 and ratio > 0.5:
            # Three-color gradient
            local_ratio = (ratio - 0.5) * 2
            r = int(color2[0] + (color3[0] - color2[0]) * local_ratio)
            g = int(color2[1] + (color3[1] - color2[1]) * local_ratio)
            b = int(color2[2] + (color3[2] - color2[2]) * local_ratio)
        else:
            # Two-color gradient
            local_ratio = ratio * 2 if color3 else ratio
            if local_ratio > 1:
                local_ratio = 1
            r = int(color1[0] + (color2[0] - color1[0]) * local_ratio)
            g = int(color1[1] + (color2[1] - color1[1]) * local_ratio)
            b = int(color1[2] + (color2[2] - color1[2]) * local_ratio)
        
        draw.line([(x, 0), (x, height)], fill=(r, g, b))
    
    return base

def draw_rounded_rect(draw, xy, radius, fill):
    """Draw a rounded rectangle."""
    x1, y1, x2, y2 = xy
    draw.rounded_rectangle(xy, radius=radius, fill=fill)

def draw_circle(draw, center, radius, fill, outline=None, outline_width=0):
    """Draw a circle."""
    x, y = center
    bbox = [x - radius, y - radius, x + radius, y + radius]
    draw.ellipse(bbox, fill=fill, outline=outline, width=outline_width)

def create_icon(size):
    """Create the Narrative app icon at the specified size."""
    # Colors
    sky_blue = (14, 165, 233)      # #0ea5e9
    violet = (139, 92, 246)        # #8b5cf6
    pink = (236, 72, 153)          # #ec4899
    white = (255, 255, 255)
    
    # Corner radius (22% of size for iOS-style rounded corners)
    corner_radius = int(size * 0.22)
    
    # Create base image with gradient
    img = create_gradient(size, size, sky_blue, violet, pink)
    
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
    line_width = max(3, size // 32)
    node_radius = max(8, size // 16)
    
    # Draw the timeline/narrative flow
    # Main curved line
    # We'll approximate the curve with a thick line
    line_y = center
    start_x = int(size * 0.15)
    end_x = int(size * 0.85)
    
    # Draw connecting line
    draw.line([(start_x, line_y - size//16), (end_x, line_y + size//16)], 
              fill=(*white, 240), width=line_width*4)
    
    # Draw nodes (circles with gradients effect)
    # Start node (left)
    start_pos = (start_x, line_y - size//16)
    draw_circle(draw, start_pos, node_radius, white)
    draw_circle(draw, start_pos, int(node_radius*0.75), sky_blue)
    
    # Middle node (center, larger - AI session)
    mid_pos = (center, line_y)
    draw_circle(draw, mid_pos, int(node_radius*1.2), white)
    draw_circle(draw, mid_pos, int(node_radius*0.9), violet)
    
    # End node (right)
    end_pos = (end_x, line_y + size//16)
    draw_circle(draw, end_pos, node_radius, white)
    draw_circle(draw, end_pos, int(node_radius*0.75), pink)
    
    # Small connector dots
    dot_radius = max(3, size // 64)
    dot1_pos = ((start_x + center)//2, (line_y - size//16 + line_y)//2)
    dot2_pos = ((center + end_x)//2, (line_y + line_y + size//16)//2)
    draw_circle(draw, dot1_pos, dot_radius, (*white, 200))
    draw_circle(draw, dot2_pos, dot_radius, (*white, 200))
    
    # Add subtle inner border/highlight
    border_width = max(1, size // 256)
    draw.rounded_rectangle(
        [size//32, size//32, size - size//32, size - size//32],
        radius=corner_radius - size//32,
        outline=(*white, 30),
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
    # Prepare images in various sizes for Windows ICO
    ico_images = []
    for size in [16, 32, 48, 64, 128, 256]:
        if size <= max(img.width for img in images):
            # Resize from the largest image
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
        # Generate or resize
        if size in generated_images:
            img = generated_images[size]
        else:
            # Resize from 1024
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
        # Clean up iconset
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
