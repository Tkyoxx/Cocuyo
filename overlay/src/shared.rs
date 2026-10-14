pub const MAP_NAME: &str = "Local\\CocuyoOverlay.v2";
pub const TEXT_LEN: usize = 300;

pub const PILL_HIDDEN: u32 = 0;
pub const PILL_LISTENING: u32 = 1;
pub const PILL_THINKING: u32 = 2;
pub const PILL_DONE: u32 = 3;

pub const CAPTION_NONE: u32 = 0;
pub const CAPTION_YOU: u32 = 1;
pub const CAPTION_APP: u32 = 2;

#[repr(C)]
pub struct Shared {
    pub enabled: u32,
    pub pill: u32,
    pub level: f32,
    pub caption: u32,
    pub caption_seq: u32,
    pub scale: f32,
    pub screen_h: f32,
    pub view_h: f32,
    pub inset_bottom: f32,
    pub who: [u16; 16],
    pub text: [u16; TEXT_LEN],
}
