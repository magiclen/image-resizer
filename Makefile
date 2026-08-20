EXECUTABLE_NAME := image-resizer

all: ./target/x86_64-unknown-linux-musl/release/$(EXECUTABLE_NAME)

./target/x86_64-unknown-linux-musl/release/$(EXECUTABLE_NAME): $(shell find . -type f \( -iname '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) | sed 's/ /\\ /g')
	PWD=$$(pwd)
	cd $$MAGICK_PATH && bash build.sh
	cd $$PWD
	IMAGE_MAGICK_INCLUDE_DIRS="$$MAGICK_PATH/linux/include/ImageMagick-7" IMAGE_MAGICK_LIB_DIRS="$$MAGICK_PATH/linux/lib:$$MUSL_PATH/x86_64-linux-musl/lib:$$MUSL_PATH/lib/gcc/x86_64-linux-musl/15.1.0" IMAGE_MAGICK_LIBS=MagickWand-7.Q16HDRI:jbig:lcms2:m:pthread:tiff:webp:sharpyuv:zstd:lzma:jpeg:z:freetype:bz2:png16:harfbuzz:brotlidec:brotlicommon:raqm:fribidi:jxl:stdc++:hwy:brotlienc:jxl_cms:jxl_threads:fontconfig:xml2:iconv:heif:de265:x265:gcc:aom:dav1d:dl:openjp2:webpmux:webpdemux:raw_r:gomp:MagickCore-7.Q16HDRI:stdc++:gcc IMAGE_MAGICK_STATIC=1 cargo build --release --target x86_64-unknown-linux-musl

install:
	$(MAKE)
	sudo cp ./target/x86_64-unknown-linux-musl/release/$(EXECUTABLE_NAME) /usr/local/bin/$(EXECUTABLE_NAME)
	sudo chown root: /usr/local/bin/$(EXECUTABLE_NAME)
	sudo chmod 0755 /usr/local/bin/$(EXECUTABLE_NAME)

test:
	cargo test --verbose

clean:
	cargo clean
