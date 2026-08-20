Image Resizer
====================

[![CI](https://github.com/magiclen/image-resizer/actions/workflows/ci.yml/badge.svg)](https://github.com/magiclen/image-resizer/actions/workflows/ci.yml)

Resize or just shrink images and sharpen them appropriately.

## Installation

This program links against **ImageMagick 7** built with **HDRI** enabled, so the library and its development headers have to be installed before building. Distribution packages are often ImageMagick 6, or are built without HDRI, in which case building from source is the reliable route.

#### Debian / Ubuntu

```bash
sudo apt install libwebp-dev
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

The [CI workflow](.github/workflows/ci.yml) is a working reference for both platforms, and the [Makefile](Makefile) builds a statically linked musl binary.

## Note

JPEG, PNG, TIFF, WebP, PGM and (with `--allow-gif`) GIF images are handled. The format is taken from the image itself rather than from its file extension, so a mislabelled file is still recognized.

`-m` is the maximum length of either side, and the aspect ratio is preserved, so `-m 1920` turns a 4000x3000 image into 1920x1440. Add `--shrink` to leave an image which is already smaller than that alone.

Without `-o` the input image is overwritten in place. The new image is written to a sibling temporary file and renamed over the original, so an interrupted run cannot leave a half-written image behind.

An image is decoded, resized and encoded again. For PNG, TIFF and PGM that round trip is lossless, but for JPEG and WebP it is not. `--quality` is optional: when it is not given, a JPEG image keeps the quality **ImageMagick** estimates from its quantization tables, so resizing does not silently re-compress it at a different quality.

Sharpening is applied automatically, with an amount derived from how much the image was scaled down. `--no-sharpen` turns it off.

An animated GIF, an animated WebP and a multi-page TIFF keep every frame, and each frame is resized on its own. An animated GIF which is resized can come out considerably bigger than it went in, because **ImageMagick** does not expose the layer optimization which would pack the frames back into patches.

An animated PNG (APNG) is skipped and left alone, because **ImageMagick** reads its first frame only and resizing it would throw the animation away.

Unless `--remain-metadata` is given, the metadata is removed. Either way, the orientation an image asks for in its metadata is applied to the image itself first, so a photo which was taken sideways does not end up lying on its side, and `-m` applies to the sides as they are displayed.

## Help

```
EXAMPLES:
image-resizer /path/to/image -m 1920                           # Resize /path/to/image
image-resizer /path/to/folder -m 1920                          # Resize the images inside /path/to/folder
image-resizer /path/to/image -o /path/to/image2 -m 1920        # Resize /path/to/image, and save it to /path/to/image2
image-resizer /path/to/folder -o /path/to/folder2 -m 1920      # Resize the images inside /path/to/folder, and save them to /path/to/folder2
image-resizer /path/to/folder -o /path/to/folder2 -f -m 1920   # Resize the images inside /path/to/folder, and save them to /path/to/folder2 without overwriting checks
image-resizer /path/to/folder --allow-gif -r -m 1920           # Resize the images inside /path/to/folder including GIF images and also remain their metadata
image-resizer /path/to/image -m 1920 --shrink                  # Resize /path/to/image only if it is bigger than 1920
image-resizer /path/to/image -m 1920 -q 75                     # Resize /path/to/image with a quality of 75 if it uses lossy compression
image-resizer /path/to/image -m 1920 --4:2:0                   # Resize /path/to/image and output it with 4:2:0 (chroma quartered) subsampling to reduce the file size
image-resizer /path/to/image -m 1920 --no-sharpen              # Resize /path/to/image without auto sharpening
image-resizer /path/to/image -m 1920 --ppi 150                 # Resize /path/to/image, and set its PPI to 150

Usage: image-resizer [OPTIONS] --side-maximum <SIDE_MAXIMUM> <INPUT_PATH>

Arguments:
  <INPUT_PATH>  Assign an image or a directory for image resizing. It should be a path of a file or a directory

Options:
  -o, --output-path <OUTPUT_PATH>    Assign a destination of your generated files. It should be a path of a directory or a file depending on your input path [alias: --output]
  -s, --single-thread                Use only one thread
  -f, --force                        Force to overwrite files
      --allow-gif                    Allow to do GIF resizing
  -r, --remain-metadata              Remain the metadata of all images [alias: --remain-profile]
  -m, --side-maximum <SIDE_MAXIMUM>  Set the maximum pixels of each side of an image (Aspect ratio will be preserved) [alias: --max]
      --only-shrink                  Only shrink images, not enlarge them [alias: --shrink]
      --no-sharpen                   Disable automatically sharpening
  -q, --quality <QUALITY>            Set the quality for lossy compression. The quality of the input image is kept when it is not set
      --ppi <PPI>                    Set pixels per inch (ppi) if it is supported
      --chroma-quartered             Use 4:2:0 (chroma quartered) subsampling to reduce the file size if it is supported [alias: --4:2:0]
  -h, --help                         Print help
  -V, --version                      Print version
```

## License

[MIT](LICENSE)
