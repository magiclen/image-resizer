use std::{
    ffi::{CStr, c_char},
    ptr::NonNull,
};

use image_convert::magick_rust::bindings;

use crate::Flags;

struct ImageProbe(NonNull<bindings::MagickWand>);

struct Names {
    pointer: NonNull<*mut c_char>,
    count:   usize,
}

impl Names {
    fn contains(&self, name: &[u8]) -> bool {
        (0..self.count).any(|index| {
            // The C query returns count owned, null-terminated names.
            let pointer = unsafe { *self.pointer.as_ptr().add(index) };

            !pointer.is_null() && unsafe { CStr::from_ptr(pointer) }.to_bytes() == name
        })
    }
}

impl Drop for Names {
    fn drop(&mut self) {
        // ImageMagick owns both the names and the array until they are released here.
        unsafe {
            for index in 0..self.count {
                let pointer = *self.pointer.as_ptr().add(index);

                if !pointer.is_null() {
                    bindings::MagickRelinquishMemory(pointer.cast());
                }
            }

            bindings::MagickRelinquishMemory(self.pointer.as_ptr().cast());
        }
    }
}

impl ImageProbe {
    fn new(data: &[u8]) -> Option<Self> {
        image_convert::start_call_once();

        // The probe owns this new wand and drops it before ImageMagick is shut down.
        let probe = Self(NonNull::new(unsafe { bindings::NewMagickWand() })?);

        // The byte slice stays alive for the whole ping call, which does not decode pixels.
        let result = unsafe {
            bindings::MagickPingImageBlob(probe.0.as_ptr(), data.as_ptr().cast(), data.len())
        };

        (result == bindings::MagickBooleanType::MagickTrue).then_some(probe)
    }

    fn names(&self, profiles: bool) -> Option<Names> {
        let mut count = 0;

        // The wand is owned by this probe, and the returned names are owned by Names.
        let pointer = unsafe {
            if profiles {
                bindings::MagickGetImageProfiles(self.0.as_ptr(), c"*".as_ptr(), &mut count)
            } else {
                bindings::MagickGetImageProperties(self.0.as_ptr(), c"*".as_ptr(), &mut count)
            }
        };

        Some(Names {
            pointer: NonNull::new(pointer)?,
            count,
        })
    }

    fn property(&self, name: &CStr) -> Option<String> {
        // ImageMagick returns an owned string, which is copied and released before returning.
        unsafe {
            let pointer = bindings::MagickGetImageProperty(self.0.as_ptr(), name.as_ptr());

            if pointer.is_null() {
                return None;
            }

            let value = CStr::from_ptr(pointer).to_string_lossy().into_owned();

            bindings::MagickRelinquishMemory(pointer.cast());

            Some(value)
        }
    }

    fn frame_matches(&self, flags: Flags, format: &str) -> Option<bool> {
        let pointer = self.0.as_ptr();

        // The caller selected a valid frame of this live, owned wand.
        let (mut width, mut height, orientation) = unsafe {
            (
                bindings::MagickGetImageWidth(pointer),
                bindings::MagickGetImageHeight(pointer),
                bindings::MagickGetImageOrientation(pointer),
            )
        };

        if !matches!(
            orientation,
            bindings::OrientationType::Undefined | bindings::OrientationType::TopLeft
        ) {
            return Some(false);
        }

        if matches!(format, "GIF" | "WEBP") {
            let (mut page_width, mut page_height, mut x, mut y) = (0, 0, 0, 0);

            // Page dimensions and offsets belong to the current animation frame.
            let result = unsafe {
                bindings::MagickGetImagePage(
                    pointer,
                    &mut page_width,
                    &mut page_height,
                    &mut x,
                    &mut y,
                )
            };

            if result != bindings::MagickBooleanType::MagickTrue {
                return None;
            }

            width = page_width.max(width.saturating_add(x.max(0) as usize));
            height = page_height.max(height.saturating_add(y.max(0) as usize));
        }

        if width == 0
            || height == 0
            || (flags.side_maximum != 0
                && (width > flags.side_maximum as usize || height > flags.side_maximum as usize))
        {
            return Some(false);
        }

        if !flags.remain_metadata {
            if self.names(true)?.count != 0 {
                return Some(false);
            }

            let properties = self.names(false)?;

            // Blob timestamps are added by ImageMagick; stored PNG text is checked in the original chunks.
            if properties.contains(b"comment")
                || (format == "TIFF" && properties.contains(b"label"))
            {
                return Some(false);
            }
        }

        if let Some(quality) = flags.quality {
            match format {
                "JPEG" => {
                    // JPEG quality is the estimate used by the existing encoder.
                    if unsafe { bindings::MagickGetImageCompressionQuality(pointer) }
                        != usize::from(quality)
                    {
                        return Some(false);
                    }
                },
                // WebP does not store a quality value which can be compared with this option.
                "WEBP" => return Some(false),
                _ => (),
            }
        }

        if flags.chroma_quartered && format == "JPEG" {
            let sampling = self.property(c"jpeg:sampling-factor")?;

            if sampling != "2x2,1x1,1x1" && sampling != "1x1" {
                return Some(false);
            }
        }

        if let Some(ppi) = flags.ppi {
            match format {
                "JPEG" | "PNG" | "TIFF" => {
                    let (mut x, mut y) = (0.0, 0.0);

                    // These getters only read the selected frame of the owned wand.
                    let (result, units) = unsafe {
                        (
                            bindings::MagickGetImageResolution(pointer, &mut x, &mut y),
                            bindings::MagickGetImageUnits(pointer),
                        )
                    };

                    if result != bindings::MagickBooleanType::MagickTrue {
                        return None;
                    }

                    match units {
                        bindings::ResolutionType::PixelsPerCentimeter => {
                            x *= 2.54;
                            y *= 2.54;
                        },
                        bindings::ResolutionType::PixelsPerInch => (),
                        _ => return Some(false),
                    }

                    if !ppi_matches(format, (x, y), ppi) {
                        return Some(false);
                    }
                },
                "WEBP" => return Some(false),
                _ => (),
            }
        }

        Some(true)
    }
}

impl Drop for ImageProbe {
    fn drop(&mut self) {
        // This handle was created by NewMagickWand and is destroyed exactly once.
        unsafe {
            bindings::DestroyMagickWand(self.0.as_ptr());
        }
    }
}

fn ppi_matches(format: &str, actual: (f64, f64), requested: f64) -> bool {
    if !actual.0.is_finite() || !actual.1.is_finite() {
        return false;
    }

    match format {
        // ImageMagick writes PNG density in whole pixels per meter.
        "PNG" => {
            let x = (requested * 100.0 / 2.54).floor();
            let y = ((requested * 100.0 + 0.5) / 2.54).floor();

            x.is_finite()
                && y.is_finite()
                && (actual.0 / 0.0254).round() == x
                && (actual.1 / 0.0254).round() == y
        },
        "JPEG" => {
            let expected = requested.trunc();

            (actual.0 - expected).abs() <= f64::EPSILON * expected * 8.0
                && (actual.1 - expected).abs() <= f64::EPSILON * expected * 8.0
        },
        // TIFF's resolution fields are written from single-precision values.
        "TIFF" => actual.0 as f32 == requested as f32 && actual.1 as f32 == requested as f32,
        _ => false,
    }
}

fn png_metadata_matches(data: &[u8], keep_ppi: bool) -> bool {
    if !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return false;
    }

    let mut offset = 8usize;

    while let Some(header) = data.get(offset..).and_then(|rest| rest.get(..8)) {
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let Some(end) = offset.checked_add(12).and_then(|start| start.checked_add(length)) else {
            return false;
        };

        if end > data.len() {
            return false;
        }

        let kind = &header[4..8];

        if matches!(
            kind,
            b"bKGD"
                | b"caNv"
                | b"cHRM"
                | b"eXIf"
                | b"gAMA"
                | b"iCCP"
                | b"iTXt"
                | b"sRGB"
                | b"tEXt"
                | b"zCCP"
                | b"zTXt"
                | b"tIME"
        ) || (kind == b"pHYs" && !keep_ppi)
        {
            return false;
        }

        if kind == b"IEND" {
            return end == data.len();
        }

        offset = end;
    }

    false
}

// ImageMagick's WebP ping skips the ICCP, EXIF and XMP chunks, so they are checked in the original data.
fn webp_metadata_matches(data: &[u8], keep_metadata: bool) -> bool {
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return false;
    }

    let mut offset = 12usize;

    while let Some(header) = data.get(offset..).and_then(|rest| rest.get(..8)) {
        let length = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
        // A chunk with an odd length is followed by a padding byte.
        let Some(end) = offset
            .checked_add(8)
            .and_then(|start| start.checked_add(length))
            .and_then(|end| end.checked_add(length % 2))
        else {
            return false;
        };

        match &header[..4] {
            // The orientation is stored in EXIF, which the ping cannot read.
            b"EXIF" => return false,
            b"ICCP" | b"XMP " | b"XMP\0" if !keep_metadata => return false,
            _ => (),
        }

        offset = end;
    }

    offset == data.len()
}

pub(super) fn goals_match(data: &[u8], format: &str, flags: Flags) -> bool {
    if !flags.only_shrink {
        return false;
    }

    if format == "PNG" && !flags.remain_metadata && !png_metadata_matches(data, flags.ppi.is_some())
    {
        return false;
    }

    if format == "WEBP" && !webp_metadata_matches(data, flags.remain_metadata) {
        return false;
    }

    let Some(probe) = ImageProbe::new(data) else {
        return false;
    };

    // The probe owns a live wand, and each index is checked against its frame count.
    unsafe {
        let count = bindings::MagickGetNumberImages(probe.0.as_ptr());

        count != 0
            && (0..count).all(|index| {
                let Ok(index) = isize::try_from(index) else {
                    return false;
                };

                bindings::MagickSetIteratorIndex(probe.0.as_ptr(), index)
                    == bindings::MagickBooleanType::MagickTrue
                    && probe.frame_matches(flags, format) == Some(true)
            })
    }
}
