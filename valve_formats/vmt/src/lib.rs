use io_util;

use io_util::RawDataRead;
use std::collections::HashMap;
use std::io::{Error as IOError, Read};

pub const SHADER_LIGHT_MAPPED_GENERIC: &str = "lightmappedgeneric";
pub const SHADER_VERTEX_LIT_GENERIC: &str = "vertexlitgeneric";
pub const SHADER_UNLIT_GENERIC: &str = "unlitgeneric";
pub const SHADER_ENVMAP_TINT: &str = "envmaptint";
pub const SHADER_WORLD_VERTEX_TRANSITION: &str = "worldvertextransition";
pub const SHADER_WATER: &str = "water";
pub const BASE_TEXTURE_NAME: &str = "basetexture";
pub const PATCH: &str = "patch";
pub const PATCH_INCLUDE: &str = "include";
#[allow(dead_code)]
const PATCH_INSERT: &str = "insert";

#[derive(Debug)]
pub enum VMTError {
    IOError(IOError),
    FileError(String),
}

pub struct VMTMaterial {
    shader_name: String,
    values: HashMap<String, String>,
}

impl VMTMaterial {
    pub fn new(reader: &mut dyn Read, length: u32) -> Result<Self, VMTError> {
        let mut values = HashMap::<String, String>::new();

        let data = reader
            .read_data(length as usize)
            .map_err(VMTError::IOError)?;
        let mut text = String::from_utf8(data.to_vec())
            .map_err(|_e| VMTError::FileError("Could not read text".to_string()))?;
        text = text.replace("\r\n", "\n");
        text = text.replace('\t', " ");
        text = text.trim_end_matches('\0').to_string();
        let block_start = text.find('{').ok_or_else(|| {
            VMTError::FileError("Could not find start of material block".to_string())
        })?;

        let mut shader_name: String = String::new();
        let shader_name_lines = (&text[..block_start]).split('\n');
        for line in shader_name_lines {
            let line = remove_line_comments(line);
            if !line.trim().is_empty() {
                shader_name = line.replace("\"", "").trim().to_lowercase();
            }
        }

        if shader_name != SHADER_LIGHT_MAPPED_GENERIC
            && shader_name != PATCH
            && shader_name != SHADER_UNLIT_GENERIC
            && shader_name != SHADER_VERTEX_LIT_GENERIC
            && shader_name != SHADER_ENVMAP_TINT
            && shader_name != SHADER_WORLD_VERTEX_TRANSITION
            && shader_name != SHADER_WATER
        {
            println!("Found unsupported shader: \"{}\"", shader_name);
        }

        let block_end = text.find('}').ok_or_else(|| {
            VMTError::FileError("Could not find end of material block".to_string())
        })?;
        let block = &text[block_start..block_end];
        let lines = block.split('\n');
        for line in lines {
            let trimmed_line = line.trim().replace(&['$', '%', '"', '\''][..], "");
            if trimmed_line.is_empty()
                || trimmed_line == "{"
                || trimmed_line == "}"
                || trimmed_line == PATCH_INCLUDE
            {
                continue;
            }

            let key_end_opt = trimmed_line.find(' ');
            if key_end_opt.is_none() {
                continue;
            }
            let key_end = key_end_opt.unwrap();
            let key = (&trimmed_line[..key_end]).trim().to_lowercase();
            let value = remove_line_comments(&trimmed_line[key_end + 1..])
                .trim()
                .to_string();
            values.insert(key, value);
        }

        Ok(Self {
            shader_name,
            values,
        })
    }

    pub fn get_value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|v| v.as_str())
    }

    pub fn get_shader(&self) -> &str {
        self.shader_name.as_str()
    }

    pub fn get_base_texture_name(&self) -> Option<&str> {
        self.get_value(BASE_TEXTURE_NAME)
    }

    pub fn get_patch_base(&self) -> Option<&str> {
        self.get_value(PATCH_INCLUDE)
    }

    pub fn is_patch(&self) -> bool {
        self.shader_name == PATCH
    }

    pub fn apply_patch(&mut self, patch: &VMTMaterial) {
        if !patch.is_patch() {
            panic!("Material must be a patch");
        }

        self.values.extend(
            patch
                .values
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
    }
}

fn remove_line_comments(text: &str) -> &str {
    let comment_start = text.find("//");
    if comment_start.is_none() {
        return text;
    }
    &text[..comment_start.unwrap()]
}
