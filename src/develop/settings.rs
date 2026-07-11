//! Non-destructive develop adjustment settings (per photo).

/// Develop adjustment settings (basic panel).
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

    /// Light-panel params used by the pixel tone stage.
    pub fn tone(&self) -> ToneParams {
        ToneParams {
            exposure: self.exposure,
            contrast: self.contrast,
            highlights: self.highlights,
            shadows: self.shadows,
            whites: self.whites,
            blacks: self.blacks,
        }
    }
}

/// Light-panel tone parameters applied in the develop pixel pipeline.
///
/// Ranges: `exposure` in EV stops; others `-100..=100` (0 = identity).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToneParams {
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
}

impl Default for ToneParams {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
        }
    }
}

impl ToneParams {
    /// Exposure only; all other sliders neutral.
    pub fn exposure_only(exposure: f32) -> Self {
        Self {
            exposure,
            ..Self::default()
        }
    }

    /// True when no tone op changes pixels (all neutral).
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// Compare tone params for re-render dirty checks.
    pub fn approx_eq(&self, other: &Self) -> bool {
        (self.exposure - other.exposure).abs() < 1e-6
            && (self.contrast - other.contrast).abs() < 1e-6
            && (self.highlights - other.highlights).abs() < 1e-6
            && (self.shadows - other.shadows).abs() < 1e-6
            && (self.whites - other.whites).abs() < 1e-6
            && (self.blacks - other.blacks).abs() < 1e-6
    }
}
