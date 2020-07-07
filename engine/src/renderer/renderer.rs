use std::sync::Arc;

use async_std::sync::{channel, Sender, Receiver};
use async_std::task;

use sourcerenderer_core::platform::{Platform, Window};
use sourcerenderer_core::graphics::{Instance, Adapter, Device};

use crate::RendererMessage;

pub struct Renderer {
  interface: Arc<RendererInterface>
}

pub struct RendererInterface {
  sender: Sender<RenderMessage>
}

impl Renderer {
  pub fn run<P: Platform>(platform: &mut P) -> Arc<RendererInterface> {
    let instance = platform.create_graphics(true).expect("Failed to initialize graphics");
    let surface = platform.window().create_surface(instance.clone());

    let (sender, receiver) = channel::<RendererMessage>(1);
    task::spawn(async move {
      let mut adapters = instance.list_adapters();
      println!("n devices: {}", adapters.len());
      let device = adapters.remove(0).create_device(&surface);

      let renderer = Renderer::new();
      'renderer_loop: loop  {
        let message = receiver.recv().await.expect("Failed to get message");
        renderer.render(message);
      }
    });
    sender
  }

  fn new(sender: Sender<RendererMessage>) -> Self {
    Self {
      interface: RendererInterface {
        sender
      }

    }
  }

  pub fn render(&self, message: RendererMessage) {

  }
}
