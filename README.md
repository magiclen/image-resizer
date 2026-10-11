Image Resizer
====================

[![CI](https://github.com/magiclen/image-resizer/actions/workflows/ci.yml/badge.svg)](https://github.com/magiclen/image-resizer/actions/workflows/ci.yml)

Resize or just shrink images and sharpen them appropriately.

## Installation

This program links against **ImageMagick 7.1.1-31** or later built with **HDRI** enabled, so the library and its development headers have to be installed before building. Distribution packages are often ImageMagick 6, or are built without HDRI, in which case building from source is the reliable route.

#### Debian / Ubuntu

```bash
sudo apt install liblcms2-dev libwebp-dev
wget https://download.imagemagick.org/archive/ImageMagick.tar.gz
tar xf ImageMagick.tar.gz
cd ImageMagick-*
./configure --enable-hdri
make -j$(nproc)
sudo make install
sudo ldconfig
```

#### macOS

```bash
brew install imagemagick
```

Once ImageMagick is in place,

```bash
cargo install image-resizer
```

The [CI workflow](.github/workflows/ci.yml) is a working reference for both platforms, and the [Makefile](Makefile) builds a statically linked musl binary. Set `MAGICK_PATH` to your ImageMagick build directory and `MUSL_PATH` to your musl toolchain directory before using the Makefile.

## Note

JPEG, PNG, TIFF, WebP, PGM and (with `--allow-gif`) GIF images are handled. A single file is checked by its contents. Directory mode first selects the supported file extensions, ignoring case, then checks those files by their contents. Files with unsupported extensions are not scanned.

`-m` is the maximum length of either side, and the aspect ratio is preserved, so `-m 1920` turns a 4000x3000 image into 1920x1440. Add `--shrink` to prevent enlargement. If every frame or animation canvas already fits and the other image options also match, the original bytes are kept, or copied unchanged when `-o` is given. Otherwise the image is still processed to apply metadata, orientation, quality, PPI or JPEG subsampling changes. `-m 0` keeps the image dimensions.

Without `-o` the input image is overwritten in place. The new image is written to a sibling temporary file and renamed over the original, so an interrupted run cannot leave a half-written image behind.

An image is decoded, resized and encoded again. PNG, TIFF and PGM use lossless encoding; JPEG and WebP use lossy encoding. Resizing, sharpening and color conversion can change pixels in every format. `--quality` is optional: when it is not given, a JPEG image keeps the quality **ImageMagick** estimates from its quantization tables, so resizing does not silently re-compress it at a different quality. WebP uses the encoder default when it needs to be encoded and no quality is given. With `--shrink`, an unspecified quality does not require re-encoding. An explicit WebP quality does, because its original quality cannot be read back reliably.

Sharpening is applied automatically, with an amount derived from how much the image was scaled down. `--no-sharpen` turns it off.

An animated GIF, an animated WebP and a multi-page TIFF keep every frame, and animation frames are put onto their canvas before resizing, while TIFF pages keep their own sizes. An animated GIF which is resized can come out considerably bigger than it went in, because the current `magick_rust` wrapper does not expose the layer optimization which would pack the frames back into patches.

An animated PNG (APNG) is skipped and left alone, because APNG output is not supported and writing a plain PNG would throw the animation away.

Unless `--remain-metadata` is given, the metadata is removed. Either way, the orientation an image asks for in its metadata is applied to the image itself first, so a photo which was taken sideways does not end up lying on its side, and `-m` applies to the sides as they are displayed.

Images with an embedded ICC color profile are converted to sRGB before that profile is removed. This requires the ImageMagick `lcms` delegate and can change pixel values, especially for colors outside the sRGB range. `--remain-metadata` keeps the profile and its original colors.

`--ppi` accepts finite positive values. For JPEG, PNG, TIFF and WebP, an explicit PPI is written even when other metadata is removed. PNG stores the density in whole pixels per meter, so the reported PPI can have a small rounding difference. JPEG only stores values below 32767. WebP stores the density in EXIF, which requires ImageMagick built with `webpmux`. Options which are not supported by an image format keep their existing behavior.

The `--shrink` match check only considers image options, including the default metadata removal and orientation handling. It does not require the encoder defaults for interlacing or lossless compression. JPEG quality is compared with the ImageMagick estimate, and density is compared at the precision stored by the format. If a requested value cannot be checked, the image is encoded.

An output directory inside the input directory is excluded from the scan, including its existing files. An output directory which is a parent of the input directory is rejected. Using the same directory for both still allows in-place processing.

`--single-thread` processes one image at a time. ImageMagick can still use several threads inside each image operation.

Multi-frame images are encoded through a temporary file, so the system temporary directory has to be writable.

## Help

```
EXAMPLES:
image-resizer /path/to/image -m 1920                           # Resize /path/to/image
image-resizer /path/to/folder -m 1920                          # Resize the images inside /path/to/folder
image-resizer /path/to/image -o /path/to/image2 -m 1920        # Resize /path/to/image, and save it to /path/to/image2
image-resizer /path/to/folder -o /path/to/folder2 -m 1920      # Resize the images inside /path/to/folder, and save them to /path/to/folder2
image-resizer /path/to/folder -o /path/to/folder2 -f -m 1920   # Resize the images inside /path/to/folder, and save them to /path/to/folder2 without overwriting checks
image-resizer /path/to/folder --allow-gif -r -m 1920           # Resize the images inside /path/to/folder including GIF images and also remain their metadata
image-resizer /path/to/image -m 1920 --shrink                  # Shrink /path/to/image to fit 1920 without enlarging it
image-resizer /path/to/image -m 1920 -q 75                     # Resize /path/to/image with a quality of 75 if it uses lossy compression
image-resizer /path/to/image -m 1920 --4:2:0                   # Resize /path/to/image and output it with 4:2:0 (chroma quartered) subsampling to reduce the file size
image-resizer /path/to/image -m 1920 --no-sharpen              # Resize /path/to/image without auto sharpening
image-resizer /path/to/image -m 1920 --ppi 150                 # Resize /path/to/image, and set its PPI to 150

Usage: image-resizer [OPTIONS] --side-maximum <SIDE_MAXIMUM> <INPUT_PATH>

Arguments:
  <INPUT_PATH>  Assign an image or a directory for image resizing. It should be a path of a file or a directory

Options:
  -o, --output-path <OUTPUT_PATH>    Assign a destination of your generated files. It should be a path of a directory or a file depending on your input path [alias: --output]
  -s, --single-thread                Process one image at a time; ImageMagick may use multiple threads
  -f, --force                        Force to overwrite files
      --allow-gif                    Allow to do GIF resizing
  -r, --remain-metadata              Remain the metadata of all images [alias: --remain-profile]
  -m, --side-maximum <SIDE_MAXIMUM>  Set the maximum pixels of each side of an image (Aspect ratio will be preserved) [alias: --max]
      --only-shrink                  Only shrink images, not enlarge them [alias: --shrink]
      --no-sharpen                   Disable automatically sharpening
  -q, --quality <QUALITY>            Set the quality for lossy compression. JPEG keeps its input quality by default; WebP uses the encoder default
      --ppi <PPI>                    Set pixels per inch (ppi) if it is supported
      --chroma-quartered             Use 4:2:0 (chroma quartered) subsampling to reduce the file size if it is supported [alias: --4:2:0]
  -h, --help                         Print help
  -V, --version                      Print version
```

## License

[MIT](LICENSE)
