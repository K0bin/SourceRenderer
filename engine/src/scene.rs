use crate::{GameplayMessage, RendererMessage};
use async_std::sync::{channel, Sender, Receiver, Mutex};
use async_std::task;

pub struct Scene {
  entities: Mutex<Vec<Entity>>
}

pub struct Entity {

}

impl Scene {
  pub fn run(render_sender: Sender<RendererMessage>) -> Sender<GameplayMessage> {
    let (sender, receiver) = channel::<GameplayMessage>(1);
    task::spawn(async move {
      let scene = Scene::new();
      'renderer_loop: loop {
        let message = receiver.recv().await.expect("Failed to get message");
        scene.tick(message);
      }
    });
    sender
  }

  fn new() -> Self {
    Self {
      entities: Mutex::new(Vec::new())
    }
  }

  pub fn tick(&self, message: GameplayMessage) {

  }
}
