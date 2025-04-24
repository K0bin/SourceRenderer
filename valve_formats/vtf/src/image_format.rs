use std::collections::HashMap;

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ImageFormat {
    RGBA8888 = 0,
    ABGR8888,
    RGB8888,
    BGR888,
    RGB565,
    I8,
    IA88,
    P8,
    A8,
    RGB888Bluescreen,
    BGR888Bluescreen,
    ARGB8888,
    BGRA8888,
    DXT1,
    DXT3,
    DXT5,
    BGRX8888,
    BGR565,
    BGRX5551,
    BGRA4444,
    DXT1OneBitAlpha,
    BGRA5551,
    UV88,
    UVWQ8888,
    RGBA16161616F,
    RGBA16161616,
    UV1X8888,
}

pub enum FormatSizeInfo {
    Pixel {
        red_bits_per_pixel: u8,
        green_bits_per_pixel: u8,
        blue_bits_per_pixel: u8,
        alpha_bits_per_pixel: u8,
        total_bits_per_pixel: u8,
    },
    Block {
        block_width: u8,
        block_height: u8,
        block_depth: u8,
        total_bits_per_block: u8,
    },
}

pub struct ImageFormatInfo {
    pub size_info: FormatSizeInfo,
    pub is_supported: bool,
}

lazy_static! {
    static ref IMAGE_FORMAT_INFO_MAP: HashMap<ImageFormat, ImageFormatInfo> = {
        let mut m = HashMap::new();
        m.insert(
            ImageFormat::A8,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 0,
                    green_bits_per_pixel: 0,
                    blue_bits_per_pixel: 0,
                    alpha_bits_per_pixel: 8,
                    total_bits_per_pixel: 8,
                },
            },
        );
        m.insert(
            ImageFormat::ARGB8888,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 8,
                    green_bits_per_pixel: 8,
                    blue_bits_per_pixel: 8,
                    alpha_bits_per_pixel: 8,
                    total_bits_per_pixel: 32,
                },
            },
        );
        m.insert(
            ImageFormat::ABGR8888,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 8,
                    green_bits_per_pixel: 8,
                    blue_bits_per_pixel: 8,
                    alpha_bits_per_pixel: 8,
                    total_bits_per_pixel: 32,
                },
            },
        );
        m.insert(
            ImageFormat::BGR565,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 5,
                    green_bits_per_pixel: 6,
                    blue_bits_per_pixel: 5,
                    alpha_bits_per_pixel: 0,
                    total_bits_per_pixel: 16,
                },
            },
        );
        m.insert(
            ImageFormat::BGR888,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 8,
                    green_bits_per_pixel: 8,
                    blue_bits_per_pixel: 8,
                    alpha_bits_per_pixel: 0,
                    total_bits_per_pixel: 24,
                },
            },
        );
        m.insert(
            ImageFormat::BGR888Bluescreen,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 8,
                    green_bits_per_pixel: 8,
                    blue_bits_per_pixel: 8,
                    alpha_bits_per_pixel: 0,
                    total_bits_per_pixel: 24,
                },
            },
        );
        m.insert(
            ImageFormat::BGRA4444,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 4,
                    green_bits_per_pixel: 4,
                    blue_bits_per_pixel: 4,
                    alpha_bits_per_pixel: 4,
                    total_bits_per_pixel: 16,
                },
            },
        );
        m.insert(
            ImageFormat::BGRA5551,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 5,
                    green_bits_per_pixel: 5,
                    blue_bits_per_pixel: 5,
                    alpha_bits_per_pixel: 1,
                    total_bits_per_pixel: 16,
                },
            },
        );
        m.insert(
            ImageFormat::BGRA8888,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 8,
                    green_bits_per_pixel: 8,
                    blue_bits_per_pixel: 8,
                    alpha_bits_per_pixel: 8,
                    total_bits_per_pixel: 32,
                },
            },
        );
        m.insert(
            ImageFormat::BGRX8888,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 8,
                    green_bits_per_pixel: 8,
                    blue_bits_per_pixel: 8,
                    alpha_bits_per_pixel: 0,
                    total_bits_per_pixel: 32,
                },
            },
        );
        m.insert(
            ImageFormat::DXT1,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Block {
                    block_width: 4,
                    block_height: 4,
                    block_depth: 1,
                    total_bits_per_block: 8,
                },
            },
        );
        m.insert(
            ImageFormat::DXT1OneBitAlpha,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Block {
                    block_width: 4,
                    block_height: 4,
                    block_depth: 1,
                    total_bits_per_block: 8,
                },
            },
        );
        m.insert(
            ImageFormat::DXT1,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Block {
                    block_width: 4,
                    block_height: 4,
                    block_depth: 1,
                    total_bits_per_block: 8,
                },
            },
        );
        m.insert(
            ImageFormat::DXT5,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Block {
                    block_width: 4,
                    block_height: 4,
                    block_depth: 1,
                    total_bits_per_block: 16,
                },
            },
        );
        m.insert(
            ImageFormat::RGB565,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 5,
                    green_bits_per_pixel: 6,
                    blue_bits_per_pixel: 5,
                    alpha_bits_per_pixel: 0,
                    total_bits_per_pixel: 16,
                },
            },
        );
        m.insert(
            ImageFormat::RGB888Bluescreen,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 8,
                    green_bits_per_pixel: 8,
                    blue_bits_per_pixel: 8,
                    alpha_bits_per_pixel: 0,
                    total_bits_per_pixel: 24,
                },
            },
        );
        m.insert(
            ImageFormat::RGBA16161616,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 16,
                    green_bits_per_pixel: 16,
                    blue_bits_per_pixel: 16,
                    alpha_bits_per_pixel: 16,
                    total_bits_per_pixel: 64,
                },
            },
        );
        m.insert(
            ImageFormat::RGBA16161616F,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 16,
                    green_bits_per_pixel: 16,
                    blue_bits_per_pixel: 16,
                    alpha_bits_per_pixel: 16,
                    total_bits_per_pixel: 64,
                },
            },
        );
        m.insert(
            ImageFormat::RGBA8888,
            ImageFormatInfo {
                is_supported: true,
                size_info: FormatSizeInfo::Pixel {
                    red_bits_per_pixel: 8,
                    green_bits_per_pixel: 8,
                    blue_bits_per_pixel: 8,
                    alpha_bits_per_pixel: 8,
                    total_bits_per_pixel: 32,
                },
            },
        );
        m
    };
}

pub(crate) fn is_image_format_supported(format: ImageFormat) -> bool {
    IMAGE_FORMAT_INFO_MAP
        .get(&format)
        .map_or(false, |format_info| format_info.is_supported)
}

pub(crate) fn calculate_image_size(
    width: u32,
    height: u32,
    depth: u32,
    format: ImageFormat,
) -> u32 {
    let info = IMAGE_FORMAT_INFO_MAP
        .get(&format)
        .expect("Unsupported format");
    match info.size_info {
        FormatSizeInfo::Pixel {
            total_bits_per_pixel,
            ..
        } => total_bits_per_pixel as u32 * width * height * depth,
        FormatSizeInfo::Block {
            block_width,
            block_height,
            block_depth,
            total_bits_per_block,
        } => {
            ((width + block_width as u32 - 1) / block_width as u32)
                * ((height + block_height as u32 - 1) / block_height as u32)
                * ((depth + block_depth as u32 - 1) / block_depth as u32)
                * total_bits_per_block as u32
        }
    }
}
