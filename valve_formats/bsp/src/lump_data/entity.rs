use crate::StringRead;
use std::collections::HashMap;
use std::io::{Read, Result as IOResult};

pub struct Entities {
    pub entities: Vec<Entity>,
}

impl Entities {
    pub fn read(read: &mut dyn Read) -> IOResult<Entities> {
        let mut entities = Vec::<Entity>::new();
        let text = read.read_null_terminated_string().unwrap();

        let mut remaining_text = text.as_str();
        loop {
            let block_begin = remaining_text.find('{');
            if block_begin.is_none() {
                break;
            }
            let block_begin = block_begin.unwrap();
            remaining_text = &remaining_text[block_begin + 1..];
            let block_end = remaining_text.find('}').expect("Unfinished block");
            let block = &remaining_text[..block_end];
            entities.push(Entity {
                key_values: parse_key_value(block, true),
            });
        }

        Ok(Self { entities })
    }
}

pub struct Entity {
    key_values: HashMap<String, String>,
}

impl Entity {
    pub fn get(&self, key: &str) -> Option<&str> {
        let lower_key = key.to_lowercase();
        self.key_values.get(&lower_key).map(|s| s.as_str())
    }

    pub fn class_name(&self) -> EntityClass {
        let class_name = self.key_values.get("classname").unwrap().as_str();
        match class_name {
            "prop_detail" => EntityClass::PropDetail,
            "prop_static" => EntityClass::PropStatic,
            "prop_physics" => EntityClass::PropPhysics,
            "prop_ragdoll" => EntityClass::PropRagdoll,
            "prop_dynamic" => EntityClass::PropDynamic,
            "prop_physics_multiplayer" => EntityClass::PropPhysicsMultiplayer,
            "prop_physics_override" => EntityClass::PropPhysicsOverride,
            "prop_dynamic_override" => EntityClass::PropDynamicOverride,
            _ => EntityClass::Unknown(class_name.to_string()),
        }
    }
}

pub fn parse_key_value(text: &str, turn_keys_lower_case: bool) -> HashMap<String, String> {
    let mut data = HashMap::<String, String>::new();
    let text = text.replace("\r\n", "\n");
    let lines = text.trim().split('\n');
    for line in lines {
        let space_pos = line.find(' ').unwrap();
        let key = (&line[..space_pos]).trim().trim_matches('\"').trim();
        let value = (&line[space_pos + 1..]).trim().trim_matches('\"').trim();
        let owned_key = if turn_keys_lower_case {
            key.to_lowercase()
        } else {
            key.to_string()
        };
        data.insert(owned_key, value.to_string());
    }
    data
}

#[derive(Eq, PartialEq, Hash, Debug)]
pub enum EntityClass {
    PropDetail,
    PropStatic,
    PropPhysics,
    PropRagdoll,
    PropDynamic,
    PropPhysicsMultiplayer,
    PropPhysicsOverride,
    PropDynamicOverride,
    Unknown(String),
}
