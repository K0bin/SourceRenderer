use std::{
    collections::{HashMap, VecDeque},
    sync::{Mutex, MutexGuard},
};

use smallvec::SmallVec;
use smartstring::alias::String;

#[allow(dead_code)]
pub struct Command {
    cmd: String,
    args: SmallVec<[String; 4]>,
}

pub struct Console {
    cmds: Mutex<HashMap<String, VecDeque<Command>>>,
}

impl Console {
    pub fn new() -> Self {
        Self {
            cmds: Mutex::new(HashMap::new()),
        }
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
        let mut prefix = String::from(cmd);
        prefix.make_ascii_lowercase();
        let mut args = SmallVec::<[String; 4]>::new();
        for arg in words {
            args.push(arg.into());
        }
        let command = Command {
            cmd: (&base_cmd[(dot_index + 1)..]).into(),
            args,
        };

        let mut lock = self.cmds.lock().unwrap();
        let cmds = lock.entry(prefix).or_default();
        cmds.push_back(command);
    }

    pub fn get_cmds<'a, 'b>(&'a self, prefix: &'b str) -> ConsoleIter<'a, 'b> {
        let lock = self.cmds.lock().unwrap();

        ConsoleIter { cmds: lock, prefix }
    }
}

pub struct ConsoleIter<'a, 'b> {
    cmds: MutexGuard<'a, HashMap<String, VecDeque<Command>>>,
    prefix: &'b str,
}

impl<'a, 'b, 'c> Iterator for ConsoleIter<'a, 'b> {
    type Item = Command;

    fn next(&mut self) -> Option<Self::Item> {
        let cmds = self.cmds.get_mut(self.prefix);
        if let Some(cmds) = cmds {
            cmds.pop_front()
        } else {
            None
        }
    }
}
