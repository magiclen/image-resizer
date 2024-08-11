EXECUTABLE_NAME := image-resizer

all: ./target/x86_64-unknown-linux-musl/release/$(EXECUTABLE_NAME)

./target/x86_64-unknown-linux-musl/release/$(EXECUTABLE_NAME): $(shell find . -type f -iname '*.rs' -o -name 'Cargo.toml' | grep -v ./target | sed 's/ /\\ /g')
	PWD=$$(pwd)
	cd $$MAGICK_PATH && bash build.sh
	cd $$PWD
	IMAGE_MAGICK_INCLUDE_DIRS="$$MAGICK_PATH/linux/include/ImageMagick-7" IMAGE_MAGICK_LIB_DIRS="$$MUSL_PATH/x86_64-linux-musl/lib:$$MUSL_PATH/lib/gcc/x86_64-linux-musl/11.4.0:$$MAGICK_PATH/linux/lib" IMAGE_MAGICK_LIBS=z:bz2:lzma:zstd:jpeg:png:tiff:openjp2:jbig:sharpyuv:webpmux:webpdemux:webp:de265:x265:aom:stdc++:heif:iconv:gcc:xml2:freetype:fontconfig:gomp:MagickWand-7.Q16HDRI:MagickCore-7.Q16HDRI IMAGE_MAGICK_STATIC=1 cargo build --release --target x86_64-unknown-linux-musl

install:
	$(MAKE)
	sudo cp ./target/x86_64-unknown-linux-musl/release/$(EXECUTABLE_NAME) /usr/local/bin/$(EXECUTABLE_NAME)
	sudo chown root: /usr/local/bin/$(EXECUTABLE_NAME)
	sudo chmod 0755 /usr/local/bin/$(EXECUTABLE_NAME)
	
test:
	cargo test --verbose
	
clean:
	cargo clean
