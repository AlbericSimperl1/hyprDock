use std::time::Instant;

/// Instellingen voor de dock animatie
#[derive(Clone, Copy)]
pub struct AnimSettings {
    pub max_scale: f64,
    pub effect_radius: f64,
    pub spacing_factor: f64,
    pub lerp_speed: f64,
    pub focus_duration: f64, // in milliseconden
}

impl Default for AnimSettings {
    fn default() -> Self {
        Self {
            max_scale: 1.4,       // 140% groei
            effect_radius: 120.0, // Hoe ver de muis invloed heeft
            spacing_factor: 0.5,  // Hoeveel ruimte er tussen de iconen komt
            lerp_speed: 10.0,     // Hoe hoger, hoe sneller de animatie reageert
            focus_duration: 150.0,
        }
    }
}

/// Houdt de animatiestatus van een enkel icoon bij
#[derive(Clone, Copy, Default)]
pub struct IconAnimState {
    pub original_center_x: f64,
    pub element_width: f64,
    pub wave_scale: f64,
    pub wave_translate_x: f64,
}

impl IconAnimState {
    pub fn new(center_x: f64, width: f64) -> Self {
        Self {
            original_center_x: center_x,
            element_width: width,
            wave_scale: 1.0,
            wave_translate_x: 0.0,
        }
    }
}

/// De hoofdstructuur die de animatie aanstuurt
pub struct DockAnimation {
    pub icons: Vec<IconAnimState>,
    pub settings: AnimSettings,

    pub is_mouse_inside: bool,
    pub animation_intensity: f64,
    pub last_mouse_x: f64,
    pub last_render_time: Instant,
}

impl DockAnimation {
    pub fn new() -> Self {
        Self {
            icons: Vec::new(),
            settings: AnimSettings::default(),
            is_mouse_inside: false,
            animation_intensity: 0.0,
            last_mouse_x: -1.0,
            last_render_time: Instant::now(),
        }
    }

    pub fn update_icon_positions(&mut self, positions: Vec<(f64, f64)>) {
        self.icons = positions
            .iter()
            .map(|(cx, w)| IconAnimState::new(*cx, *w))
            .collect();
    }

    fn calculate_scale(&self, distance: f64, radius: f64, max_scale: f64) -> f64 {
        if distance > radius {
            return 1.0;
        }
        let t = distance / radius;
        let factor = (t * std::f64::consts::PI).cos();
        let factor = (factor + 1.0) / 2.0;
        let factor = factor.clamp(0.0, 1.0);
        (max_scale - 1.0) * factor + 1.0
    }

    pub fn on_pointer_moved(&mut self, mouse_x: f64) {
        self.is_mouse_inside = true;
        self.last_mouse_x = mouse_x;
    }

    pub fn on_pointer_exit(&mut self) {
        self.is_mouse_inside = false;
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_render_time).as_secs_f64();
        self.last_render_time = now;

        let intensity_change = dt * (1000.0 / self.settings.focus_duration);
        if self.is_mouse_inside {
            self.animation_intensity = f64::min(1.0, self.animation_intensity + intensity_change);
        } else {
            self.animation_intensity = f64::max(0.0, self.animation_intensity - intensity_change);
        }

        if self.animation_intensity <= 0.0 && !self.is_mouse_inside {
            self.reset_all_scales();
            return;
        }

        let mouse_x = if self.last_mouse_x == -1.0 {
            0.0
        } else {
            self.last_mouse_x
        };
        self.apply_animation(mouse_x, dt);
    }

    fn apply_animation(&mut self, mouse_x: f64, dt: f64) {
        let n = self.icons.len();
        if n == 0 {
            return;
        }

        let mut scales = vec![1.0; n];
        let mut extra_widths = vec![0.0; n];
        let mut total_expansion = 0.0;

        for i in 0..n {
            let distance = (mouse_x - self.icons[i].original_center_x).abs();
            scales[i] = self.calculate_scale(
                distance,
                self.settings.effect_radius,
                self.settings.max_scale,
            );
            extra_widths[i] =
                (scales[i] - 1.0) * self.icons[i].element_width * self.settings.spacing_factor;
            total_expansion += extra_widths[i];
        }

        let mut alpha = 1.0;
        if self.settings.lerp_speed > 0.0 {
            alpha = 1.0 - (-self.settings.lerp_speed * dt).exp();
        }
        alpha = alpha.clamp(0.0, 1.0);

        let mut cumulative_shift = 0.0;

        for i in 0..n {
            let self_shift = extra_widths[i] / 2.0;
            let center_offset = total_expansion / 2.0;
            let final_shift = cumulative_shift + self_shift - center_offset;
            cumulative_shift += extra_widths[i];

            let target_scale = 1.0 + (scales[i] - 1.0) * self.animation_intensity;
            let target_translate_x = final_shift * self.animation_intensity;

            if self.settings.lerp_speed > 0.0 {
                self.icons[i].wave_scale += (target_scale - self.icons[i].wave_scale) * alpha;
                self.icons[i].wave_translate_x +=
                    (target_translate_x - self.icons[i].wave_translate_x) * alpha;
            } else {
                self.icons[i].wave_scale = target_scale;
                self.icons[i].wave_translate_x = target_translate_x;
            }
        }
    }

    fn reset_all_scales(&mut self) {
        for icon in &mut self.icons {
            icon.wave_scale = 1.0;
            icon.wave_translate_x = 0.0;
        }
    }
}
