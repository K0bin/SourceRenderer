use serde::{Deserialize, Serialize};

use crate::Vec2UI;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Format {
    Unknown,
    R32UNorm,
    R16UNorm,
    R8Unorm,
    RGBA8UNorm,
    RGBA8Srgb,
    BGR8UNorm,
    BGRA8UNorm,
    BC1,
    BC1Alpha,
    BC2,
    BC3,
    R16Float,
    R32Float,
    RG32Float,
    RG16Float,
    RGB32Float,
    RGBA32Float,
    RG16UNorm,
    RG8UNorm,
    R32UInt,
    RGBA16Float,
    R11G11B10Float,
    RG16UInt,
    RG16SInt,
    R16UInt,
    R16SNorm,
    R16SInt,

    D16,
    D16S8,
    D32,
    D32S8,
    D24S8,
}

impl Format {
    pub fn is_depth(&self) -> bool {
        matches!(
            self,
            Format::D32 | Format::D16 | Format::D16S8 | Format::D24S8 | Format::D32S8
        )
    }

    pub fn is_stencil(&self) -> bool {
        matches!(self, Format::D16S8 | Format::D24S8 | Format::D32S8)
    }

    pub fn is_compressed(&self) -> bool {
        matches!(
            self,
            Format::BC1 | Format::BC1Alpha | Format::BC2 | Format::BC3
        )
    }

    pub fn element_size(&self) -> u32 {
        match self {
            Format::R32Float => 4,
            Format::R16Float => 2,
            Format::RG32Float => 8,
            Format::RGB32Float => 12,
            Format::RGBA32Float => 16,
            Format::RGBA8UNorm => 4,

            Format::BC1 => 8,
            Format::BC1Alpha => 8,
            Format::BC2 => 16,
            Format::BC3 => 16,
            _ => todo!("Format: {:?}", self),
        }
    }

    pub fn srgb_format(&self) -> Option<Format> {
        match self {
            Format::RGBA8UNorm => Some(Format::RGBA8Srgb),
            _ => None,
        }
    }

    pub fn block_size(&self) -> Vec2UI {
        match self {
            Format::BC1 | Format::BC1Alpha | Format::BC2 | Format::BC3 => Vec2UI::new(4, 4),

            _ => Vec2UI::new(1, 1),
        }
    }

    pub fn is_float(&self) -> bool {
        match self {
            Format::R16Float
            | Format::R32Float
            | Format::RG32Float
            | Format::RG16Float
            | Format::RGB32Float
            | Format::RGBA32Float
            | Format::RGBA16Float
            | Format::R11G11B10Float => true,
            _ => false,
        }
    }

    pub fn is_unorm(&self) -> bool {
        match self {
            Format::R8Unorm
            | Format::R16UNorm
            | Format::R32UNorm
            | Format::RG8UNorm
            | Format::BGR8UNorm
            | Format::RG16UNorm
            | Format::RGBA8UNorm
            | Format::BGRA8UNorm => true,
            _ => false,
        }
    }

    pub fn is_snorm(&self) -> bool {
        match self {
            Format::R16SNorm => true,
            _ => false,
        }
    }

    pub fn is_srgb(&self) -> bool {
        match self {
            Format::RGBA8Srgb => true,
            _ => false,
        }
    }

    pub fn is_uint(&self) -> bool {
        match self {
            Format::R16UInt | Format::R32UInt | Format::RG16UInt => true,
            _ => false,
        }
    }

    pub fn is_sint(&self) -> bool {
        match self {
            Format::R16SInt | Format::RG16SInt => true,
            _ => false,
        }
    }
}
