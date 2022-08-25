use std::{sync::RwLock, collections::HashMap};

use crossbeam_channel::Sender;
use smallvec::SmallVec;

pub struct Command {
  cmd: String,
  args: SmallVec::<[String; 4]>
}

pub struct Console {
  msgs: RwLock<HashMap<String, Sender<Command>>>
}

impl Console {
  pub fn new() -> Self {
    Self { msgs: RwLock::new(HashMap::new()) }
  }

  pub fn install_listener(&self, prefix: &str, sender: Sender<Command>) {
    let mut lock = self.msgs.write().unwrap();
    lock.insert(prefix.to_string().to_lowercase(), sender);
  }

  pub fn write_cmd(&self, cmd: &str) {
    let mut words = cmd.split(" ");
    let base_cmd = words.next();
    if base_cmd.is_none() {
      return;
    }
    let base_cmd = base_cmd.unwrap();
    let dot_index = base_cmd.find('.');
    if dot_index.is_none() {
      return;
    }
    let dot_index = dot_index.unwrap();
    let prefix = &base_cmd[..dot_index].to_lowercase();
    let mut args = SmallVec::<[String; 4]>::new();
    for arg in words {
      args.push(arg.to_string());
    }
    let command = Command {
      cmd: (&base_cmd[(dot_index + 1)..]).to_string(),
      args
    };

    let lock = self.msgs.read().unwrap();
    let listener = lock.get(prefix);
    if listener.is_none() {
      return;
    }
    let listener = listener.unwrap();
    listener.send(command).unwrap();
  }
}