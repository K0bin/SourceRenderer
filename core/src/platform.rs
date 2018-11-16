#[derive(PartialEq)]
pub enum PlatformEvent {
    Continue,
    Quit
}

pub trait Platform {
    fn get_window(&mut self) -> &mut Window;
    fn handle_events(&mut self) -> PlatformEvent;
}

pub trait Window {

}
