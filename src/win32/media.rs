use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYEVENTF_KEYUP, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK,
};

#[derive(Debug, Clone)]
pub struct MediaSessionState {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub is_playing: bool,
    pub position_secs: f32,
    pub duration_secs: f32,
}

impl Default for MediaSessionState {
    fn default() -> Self {
        Self {
            title: "Starboy".to_string(),
            artist: "The Weeknd, Daft Punk".to_string(),
            album: "Starboy".to_string(),
            is_playing: true,
            position_secs: 74.0,
            duration_secs: 230.0,
        }
    }
}

pub struct MediaBridge {
    pub state: MediaSessionState,
}

impl Default for MediaBridge {
    fn default() -> Self {
        Self {
            state: MediaSessionState::default(),
        }
    }
}

impl MediaBridge {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn play_pause(&mut self) {
        self.state.is_playing = !self.state.is_playing;
        Self::send_key(VK_MEDIA_PLAY_PAUSE);
    }

    pub fn next_track(&mut self) {
        self.state.position_secs = 0.0;
        Self::send_key(VK_MEDIA_NEXT_TRACK);
    }

    pub fn prev_track(&mut self) {
        self.state.position_secs = 0.0;
        Self::send_key(VK_MEDIA_PREV_TRACK);
    }

    pub fn update_progress(&mut self, dt: f32) {
        if self.state.is_playing {
            self.state.position_secs += dt;
            if self.state.position_secs > self.state.duration_secs {
                self.state.position_secs = 0.0;
            }
        }
    }

    fn send_key(vk: u16) {
        unsafe {
            keybd_event(vk as u8, 0, 0, 0);
            keybd_event(vk as u8, 0, KEYEVENTF_KEYUP, 0);
        }
    }
}
