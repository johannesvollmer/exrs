# OpenEXR Deep Data Test Files

This directory contains reference deep data images from the OpenEXR project for testing the deep data implementation.

## Test Files

### Balls.exr
- **Source**: https://raw.githubusercontent.com/AcademySoftwareFoundation/openexr-images/main/v2/LowResLeftView/Balls.exr
- **Type**: Deep scanline image
- **Size**: 1.6 MB
- **Resolution**: 764×406 pixels (data window: 131,170 to 894,575)
- **Display window**: 1024×576 (0,0 to 1023,575)
- **Channels**:
  - R, G, B, A (HALF - 16-bit float)
  - Z (FLOAT - 32-bit float depth)
- **Chunks**: 406 (one per scanline)
- **Compression**: ZIPS (ZIP compression of single scanline)
- **Purpose**: Primary test file for deep data with multiple semi-transparent spheres at different depths

### Ground.exr
- **Source**: https://raw.githubusercontent.com/AcademySoftwareFoundation/openexr-images/main/v2/LowResLeftView/Ground.exr
- **Type**: Deep scanline image
- **Size**: 4.8 MB
- **Resolution**: 1024×396 pixels (data window: 0,180 to 1023,575)
- **Display window**: 1024×576 (0,0 to 1023,575)
- **Channels**:
  - R, G, B, A (HALF - 16-bit float)
  - Z (FLOAT - 32-bit float depth)
- **Chunks**: 396 (one per scanline)
- **Compression**: ZIPS
- **Purpose**: Background/ground plane for testing deep compositing utilities

## File Format Notes

Both files use the OpenEXR v2 format with:
- **Magic number**: 0x01312f76 (20000630 in decimal)
- **Version field**: 0x00000802
  - File version: 2
  - Deep data flag: TRUE (bit 11 set)
- **Attribute**: `type = "deepscanline"`
- **Version attribute**: 1 (deep data format version)

## Usage in Tests

These files will be used to validate:

1. **Reading deep scanline images** (Phase 3)
   - Parse deep data headers correctly
   - Read and decompress pixel offset tables
   - Extract sample counts per pixel
   - Decompress and read sample data with ZIPS compression

2. **Round-trip testing** (Phase 6)
   - Read → Write → Read should preserve all data exactly

3. **Deep compositing** (Phase 5)
   - Flatten Balls.exr to a regular image
   - Composite Balls.exr over Ground.exr as background
   - Verify correct depth-based compositing

4. **Compatibility testing** (Phase 6)
   - Verify files written by exrs can be read by OpenEXR C++ tools
   - Compare compositing results with OpenEXR reference implementation

## Viewing Test Files

To view these files, use:
- OpenEXR command line tools: `exrheader Balls.exr`
- Python OpenEXR library
- DJV image viewer
- Nuke (commercial)
- RV (commercial)

Or use the examination script:
```bash
python3 examine_header.py Balls.exr
```
