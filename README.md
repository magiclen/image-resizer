Image Resizer
====================

[![Build Status](https://travis-ci.org/magiclen/image-resizer.svg?branch=master)](https://travis-ci.org/magiclen/image-resizer)

Resize or just shrink images and sharpen them appropriately.

## Help

```
EXAMPLES:
  image-resizer /path/to/image -m 1920                         # Check /path/to/image and make it resized
  image-resizer /path/to/folder -m 1920                        # Check /path/to/folder and make images inside it resized
  image-resizer /path/to/image  -o /path/to/image2 -m 1920     # Check /path/to/image and make it resized, and save it to /path/to/image2
  image-resizer /path/to/folder -o /path/to/folder2 -m 1920    # Check /path/to/folder and make images inside it resized, and save them to /path/to/folder2
  image-resizer /path/to/folder -o /path/to/folder2 -f -m 1920 # Check /path/to/folder and make images inside it resized, and save them to /path/to/folder2 without
overwriting checks
  image-resizer /path/to/folder --allow-gif -r -m 1920         # Check /path/to/folder and make images inside it including GIF images resized and also remain their
profiles
  image-resizer /path/to/image -m 1920 --shrink                # Check /path/to/image and make it shrunk if it needs to be
  image-resizer /path/to/image -m 1920 -q 75                   # Check /path/to/image and make it resized with a quality of 75 if it uses lossy compression
  image-resizer /path/to/image -m 1920 --4:2:0                 # Check /path/to/image and make it resized and output using 4:2:0 (chroma quartered) subsampling to reduce
the file size

USAGE:
    image-resizer [FLAGS] [OPTIONS] <INPUT_PATH> --side-maximum <SIDE_MAXIMUM>

FLAGS:
        --allow-gif           Allows to do GIF resizing
        --chroma-quartered    Uses 4:2:0 (chroma quartered) subsampling to reduce the file size if it is supported. [aliases: 4:2:0]
    -f, --force               Forces to overwrite files
        --only-shrink         Only shrink images, not enlarge them. [aliases: shrink]
    -r, --remain-profile      Remains the profiles of all images
    -s, --single-thread       Uses only one thread
    -h, --help                Prints help information
    -V, --version             Prints version information

OPTIONS:
    -o, --output <OUTPUT_PATH>           Assigns a destination of your generated files. It should be a path of a directory or a file depending on your input path
    -q, --quality <QUALITY>              Set the quality for lossy compression. [default: 92]
    -m, --side-maximum <SIDE_MAXIMUM>    Sets the maximum pixels of each side of an image. (Aspect ratio will be preserved.) [aliases: max]

ARGS:
    <INPUT_PATH>    Assigns an image or a directory for image interlacing. It should be a path of a file or a directory
```

## License

[MIT](LICENSE)