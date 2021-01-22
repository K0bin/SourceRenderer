use std::collections::HashMap;
use std::io::{Read, Result as IOResult};
use crate::lump_data::{LumpData, LumpType};
use crate::PrimitiveRead;
use crate::StringRead;

pub struct Entities {
  entities: Vec<HashMap<String, String>>
}

impl Entities {
  pub fn read(mut read: &mut dyn Read) -> IOResult<Entities> {
    let mut entities = Vec::<HashMap<String, String>>::new();
    let text = read.read_null_terminated_string().unwrap();

    let mut remaining_text = text.as_str();
    loop {
      let block_begin = remaining_text.find("{");
      if block_begin.is_none() {
        break;
      }
      let block_begin = block_begin.unwrap();
      remaining_text = &remaining_text[block_begin + 1..];
      let block_end = remaining_text.find("}").expect("Unfinished block");
      let block = &remaining_text[..block_end];
      entities.push(parse_key_value(block));
    }

    Ok(Self {
      entities
    })
  }
}

pub fn parse_key_value(text: &str) -> HashMap<String, String> {
  let mut data = HashMap::<String, String>::new();

  let text = text.replace("\r\n", "\n");
  let lines = text.trim().split("\n");
  for line in lines {
    let space_pos = line.find(" ").unwrap();
    let key = (&line[..space_pos]).trim().trim_matches('\"').trim();
    let value = (&line[space_pos + 1..]).trim().trim_matches('\"').trim();
    data.insert(key.to_string(), value.to_string());
  }
  data
}