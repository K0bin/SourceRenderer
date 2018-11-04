pub use self::brush::Brush;

mod brush;

pub enum LumpData {
    Brush(Box<Vec<Brush>>)
}
