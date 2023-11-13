Image Resizer
====================

[![CI](https://github.com/magiclen/image-resizer/actions/workflows/ci.yml/badge.svg)](https://github.com/magiclen/image-resizer/actions/workflows/ci.yml)

Resize or just shrink images and sharpen them appropriately.

## Help

```
EXAMPLES:
image-resizer /path/to/image -m 1920                           # Make /path/to/image resized
image-resizer /path/to/folder -m 1920                          # Make images inside /path/to/folder and make resized
image-resizer /path/to/image -o /path/to/image2 -m 1920        # Make /path/to/image resized, and save it to /path/to/image2
image-resizer /path/to/folder -o /path/to/folder2 -m 1920      # Make images inside /path/to/folder resized, and save them to /path/to/folder2
image-resizer /path/to/folder -o /path/to/folder2 -f -m 1920   # Make images inside /path/to/folder resized, and save them to /path/to/folder2 without overwriting checks
image-resizer /path/to/folder --allow-gif -r -m 1920           # Make images inside /path/to/folder including GIF resized and also remain their profiles
image-resizer /path/to/image -m 1920 --shrink                  # Make /path/to/image shrunk if it needs to be
image-resizer /path/to/image -m 1920 -q 75                     # Make /path/to/image resized with a quality of 75 if it uses lossy compression
image-resizer /path/to/image -m 1920 --4:2:0                   # Make /path/to/image resized and output using 4:2:0 (chroma quartered) subsampling to reduce the file size
image-resizer /path/to/image -m 1920 --no-sharpen              # Make /path/to/image resized without auto sharpening
image-resizer /path/to/image -m 1920 --ppi 150                 # Make /path/to/image resized, and set their PPI to 150

Usage: image-resizer [OPTIONS] --side-maximum <SIDE_MAXIMUM> <INPUT_PATH>

Arguments:
  <INPUT_PATH>  Assign an image or a directory for image resizing. It should be a path of a file or a directory

Options:
  -o, --output-path <OUTPUT_PATH>    Assign a destination of your generated files. It should be a path of a directory or a file depending on your input path [aliases: output]
  -s, --single-thread                Use only one thread
  -f, --force                        Force to overwrite files
      --allow-gif                    Allow to do GIF interlacing
  -r, --remain-profile               Remain the profiles of all images
  -m, --side-maximum <SIDE_MAXIMUM>  Set the maximum pixels of each side of an image (Aspect ratio will be preserved) [aliases: max]
      --only-shrink                  Only shrink images, not enlarge them [aliases: shrink]
      --no-sharpen                   Disable automatically sharpening
  -q, --quality <QUALITY>            Set the quality for lossy compression [default: 92]
      --ppi <PPI>                    Set pixels per inch (ppi)
      --chroma-quartered             Use 4:2:0 (chroma quartered) subsampling to reduce the file size if it is supported [aliases: 4:2:0]
  -h, --help                         Print help
  -V, --version                      Print version
```

## License

[MIT](LICENSE)