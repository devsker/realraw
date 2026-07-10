//! Non-destructive develop adjustment settings (per photo).

/// Develop adjustment settings, matching Lightroom's basic panel.
#[derive(Debug, Clone, PartialEq)]
pub struct DevelopSettings {
    // Light
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    // Presence
    pub clarity: f32,
    pub vibrance: f32,
    pub saturation: f32,
    // Color (relative; temp offset from 5500K in UI units)
    pub temp: f32,
    pub tint: f32,
}

impl Default for DevelopSettings {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            clarity: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            temp: 0.0,
            tint: 0.0,
        }
    }
}

impl DevelopSettings {
    /// True when every slider is at its neutral default.
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }
}
