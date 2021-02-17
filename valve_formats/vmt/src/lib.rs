mod read_util;

use std::collections::HashMap;
use std::io::{Read, Error as IOError};
use crate::read_util::RawDataRead;

pub const SHADER_LIGHT_MAPPED_GENERIC: &'static str = "lightmappedgeneric";
pub const BASE_TEXTURE_NAME: &'static str = "basetexture";
pub const PATCH: &'static str = "patch";
pub const PATCH_INCLUDE: &'static str = "include";
#[allow(dead_code)]
const PATCH_INSERT: &'static str = "insert";

#[derive(Debug)]
pub enum VMTError {
  IOError(IOError),
  FileError(String)
}

pub struct VMTMaterial {
  shader_name: String,
  values: HashMap<String, String>
}

impl VMTMaterial {
  pub fn new(reader: &mut dyn Read, length: u32) -> Result<Self, VMTError> {
    let mut values = HashMap::<String, String>::new();

    let data = reader.read_data(length as usize).map_err(|e| VMTError::IOError(e))?;
    let mut text = String::from_utf8(data.to_vec()).map_err(|_e| VMTError::FileError("Could not read text".to_string()))?;
    text = text.replace("\r\n", "\n");
    text = text.replace('\t', " ");
    text = text.trim_end_matches("\0").to_string();
    let block_start = text.find('{').ok_or_else(|| VMTError::FileError("Could not find start of material block".to_string()))?;
    let shader_name = remove_comments(&text[0 .. block_start]).replace("\"", "").trim().to_lowercase();

    if shader_name != SHADER_LIGHT_MAPPED_GENERIC && shader_name != PATCH {
      println!("Found unsupported shader: \"{}\"", shader_name);
    }

    let block_end = text.find('}').ok_or_else(|| VMTError::FileError("Could not find end of material block".to_string()))?;
    let block = &text[block_start .. block_end];
    let lines = block.split("\n");
    for line in lines {
      let trimmed_line = line.trim().replace(&['$', '%', '"', '\''][..], "");
      if trimmed_line.is_empty() || trimmed_line == "{" || trimmed_line == "}" || trimmed_line == PATCH_INCLUDE {
        continue;
      }

      let key_end_opt = trimmed_line.find(' ');
      if key_end_opt.is_none() {
        continue;
      }
      let key_end = key_end_opt.unwrap();
      let key = (&trimmed_line[.. key_end]).trim().to_lowercase();
      let value = remove_comments(&trimmed_line[key_end + 1 ..]).trim().to_string();
      values.insert(key, value);
    }

    Ok(Self {
      shader_name,
      values
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

    self.values.extend(patch.values.iter().map(|(key, value)| (key.clone(), value.clone())));
  }
}

fn remove_comments(text: &str) -> &str {
  let comment_start = text.find("//");
  if comment_start.is_none() {
    return text;
  }
  let comment_start = comment_start.unwrap() + 2;
  let comment_end = text[comment_start..].find("\n");
  if comment_end.is_some() {
    &text[comment_start .. (comment_end.unwrap() - comment_start)]
  } else {
    &text[comment_start ..]
  }
}
