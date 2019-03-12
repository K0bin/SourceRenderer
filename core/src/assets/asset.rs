pub trait Asset {
  fn load(&self);
  fn request(&self);
}

pub trait Material : Asset {

}

pub trait Model : Asset {

}
