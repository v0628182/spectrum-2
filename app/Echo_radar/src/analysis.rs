const SPEAKER_FRONT_LEFT: u32 = 0x0000_0001;
const SPEAKER_FRONT_RIGHT: u32 = 0x0000_0002;
const SPEAKER_FRONT_CENTER: u32 = 0x0000_0004;
const SPEAKER_LOW_FREQUENCY: u32 = 0x0000_0008;
const SPEAKER_BACK_LEFT: u32 = 0x0000_0010;
const SPEAKER_BACK_RIGHT: u32 = 0x0000_0020;
const SPEAKER_FRONT_LEFT_OF_CENTER: u32 = 0x0000_0040;
const SPEAKER_FRONT_RIGHT_OF_CENTER: u32 = 0x0000_0080;
const SPEAKER_BACK_CENTER: u32 = 0x0000_0100;
const SPEAKER_SIDE_LEFT: u32 = 0x0000_0200;
const SPEAKER_SIDE_RIGHT: u32 = 0x0000_0400;

/// Compresión perceptual: gamma < 1.0 amplifica señales débiles (pasos)
/// sin saturar las fuertes (explosiones, música).
///   0.008 RMS → 27%  |  0.02 RMS → 50%  |  0.08+ RMS → 100%
const PERCEPTUAL_GAMMA: f32 = 0.50;
/// Noise gate por speaker. En 7.1 el audio ambiente se distribuye en los
/// 8 canales a ~0.002-0.004 RMS. Este umbral los elimina para que solo
/// se muestren sonidos con energía direccional real.
const NOISE_FLOOR: f32 = 0.005;
/// Gate post-mezcla: si un canal del radar queda por debajo de este
/// nivel tras combinar speakers, se pone a cero. Elimina el residuo
/// de cross-bleed cuando varios canales tienen energía mínima.
const RADAR_GATE: f32 = 0.04;

#[allow(dead_code)]
/// Estado actual del analisis de audio estereo.
#[derive(Debug, Clone, Copy)]
pub struct AudioState {
    pub pan: f32,
    pub volume: f32,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            pan: 0.0,
            volume: 0.0,
        }
    }
}

/// Estado visual del radar.
///
/// `far_left` / `far_right` capturan side/back para que una mezcla 7.1
/// tenga una firma visual distinta a un simple estereo.
#[derive(Debug, Clone, Copy, Default)]
pub struct RadarState {
    pub far_left: f32,
    pub left: f32,
    pub center: f32,
    pub right: f32,
    pub far_right: f32,
    pub ambience: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct SpeakerEnergies {
    front_left: f32,
    front_right: f32,
    front_center: f32,
    low_frequency: f32,
    back_left: f32,
    back_right: f32,
    front_left_of_center: f32,
    front_right_of_center: f32,
    back_center: f32,
    side_left: f32,
    side_right: f32,
    unknown_left: f32,
    unknown_right: f32,
}

#[derive(Debug, Clone, Copy)]
enum SpeakerPosition {
    FrontLeft,
    FrontRight,
    FrontCenter,
    LowFrequency,
    BackLeft,
    BackRight,
    FrontLeftOfCenter,
    FrontRightOfCenter,
    BackCenter,
    SideLeft,
    SideRight,
    UnknownLeft,
    UnknownRight,
}

#[allow(dead_code)]
/// Analiza un chunk de PCM interleaved [L,R,L,R...] y retorna el estado de audio.
/// `channels` debe ser 2 (stereo). Para mono retorna center.
pub fn analyze_stereo(samples: &[f32], channels: u16) -> AudioState {
    if samples.is_empty() || channels == 0 {
        return AudioState::default();
    }

    let channels = channels as usize;

    if channels == 1 {
        let rms: f32 = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        return AudioState {
            pan: 0.0,
            volume: rms.min(1.0),
        };
    }

    let mut sum_l: f32 = 0.0;
    let mut sum_r: f32 = 0.0;
    let mut count: usize = 0;

    for frame in samples.chunks_exact(channels) {
        let l = frame[0];
        let r = if channels >= 2 { frame[1] } else { l };
        sum_l += l * l;
        sum_r += r * r;
        count += 1;
    }

    if count == 0 {
        return AudioState::default();
    }

    let rms_l = (sum_l / count as f32).sqrt();
    let rms_r = (sum_r / count as f32).sqrt();

    let epsilon = 1e-10;
    let pan = (rms_r - rms_l) / (rms_r + rms_l + epsilon);
    let volume = rms_l.max(rms_r).min(1.0);

    AudioState { pan, volume }
}

#[allow(dead_code)]
pub fn smooth(current: &AudioState, previous: &AudioState, alpha: f32) -> AudioState {
    AudioState {
        pan: alpha * current.pan + (1.0 - alpha) * previous.pan,
        volume: alpha * current.volume + (1.0 - alpha) * previous.volume,
    }
}

/// Suavizado asimétrico: attack rápido cuando la señal sube (transitorios
/// como pasos aparecen instantáneamente) y release lento cuando baja
/// (el glow persiste un momento para que el jugador reaccione).
pub fn smooth_radar(
    current: &RadarState,
    previous: &RadarState,
    attack: f32,
    release: f32,
) -> RadarState {
    RadarState {
        far_left: blend_ar(previous.far_left, current.far_left, attack, release),
        left: blend_ar(previous.left, current.left, attack, release),
        center: blend_ar(previous.center, current.center, attack, release),
        right: blend_ar(previous.right, current.right, attack, release),
        far_right: blend_ar(previous.far_right, current.far_right, attack, release),
        ambience: blend_ar(previous.ambience, current.ambience, attack, release),
    }
}

/// Convierte un chunk multicanal a un estado visual de radar.
///
/// Usa `dwChannelMask` cuando esta disponible para conservar mejor la
/// informacion de configuraciones 5.1 y 7.1.
pub fn analyze_radar(samples: &[f32], channels: u16, channel_mask: u32) -> RadarState {
    if samples.is_empty() || channels == 0 {
        return RadarState::default();
    }

    let channel_count = channels as usize;
    let frame_count = samples.len() / channel_count;
    if frame_count == 0 {
        return RadarState::default();
    }

    let positions = speaker_positions(channel_count, channel_mask);
    let mut sums = vec![0.0f32; channel_count];

    for frame in samples.chunks_exact(channel_count) {
        for (idx, sample) in frame.iter().enumerate() {
            sums[idx] += sample * sample;
        }
    }

    let mut energies = SpeakerEnergies::default();
    for (idx, sum) in sums.iter().enumerate() {
        let rms = (sum / frame_count as f32).sqrt();
        energies.add(positions.get(idx).copied().unwrap_or(SpeakerPosition::UnknownRight), rms);
    }

    radar_from_energies(&energies)
}

fn radar_from_energies(energies: &SpeakerEnergies) -> RadarState {
    let amp = 3.5;

    // Compresión perceptual por speaker: boost fuerte a señales débiles,
    // efecto mínimo en señales fuertes (ya cerca del clamp).
    let fl = compress(energies.front_left + energies.unknown_left * 0.6) * amp;
    let fr = compress(energies.front_right + energies.unknown_right * 0.6) * amp;
    let fc = compress(energies.front_center) * amp;
    let lfe = compress(energies.low_frequency) * amp;
    let bl = compress(energies.back_left) * amp;
    let br = compress(energies.back_right) * amp;
    let flc = compress(energies.front_left_of_center) * amp;
    let frc = compress(energies.front_right_of_center) * amp;
    let bc = compress(energies.back_center) * amp;
    let sl = compress(energies.side_left) * amp;
    let sr = compress(energies.side_right) * amp;

    let far_left =
        (sl * 0.95 + bl * 0.82 + fl * 0.18 + flc * 0.74 + bc * 0.16).clamp(0.0, 1.0);
    let left =
        (fl * 1.00 + flc * 0.90 + sl * 0.26 + bl * 0.20 + fc * 0.12 + bc * 0.06).clamp(0.0, 1.0);
    let center =
        (fc * 1.00 + (fl + fr) * 0.24 + (sl + sr) * 0.10 + bc * 0.18 + lfe * 0.05).clamp(0.0, 1.0);
    let right =
        (fr * 1.00 + frc * 0.90 + sr * 0.26 + br * 0.20 + fc * 0.12 + bc * 0.06).clamp(0.0, 1.0);
    let far_right =
        (sr * 0.95 + br * 0.82 + fr * 0.18 + frc * 0.74 + bc * 0.16).clamp(0.0, 1.0);
    let ambience = (lfe * 0.32
        + (sl + sr) * 0.08
        + (bl + br) * 0.10
        + (fl + fr) * 0.03
        + center * 0.10)
        .clamp(0.0, 1.0);

    RadarState {
        far_left: gate(far_left),
        left: gate(left),
        center: gate(center),
        right: gate(right),
        far_right: gate(far_right),
        ambience: gate(ambience),
    }
}

fn speaker_positions(channels: usize, channel_mask: u32) -> Vec<SpeakerPosition> {
    if channel_mask != 0 {
        let ordered_bits = [
            (SPEAKER_FRONT_LEFT, SpeakerPosition::FrontLeft),
            (SPEAKER_FRONT_RIGHT, SpeakerPosition::FrontRight),
            (SPEAKER_FRONT_CENTER, SpeakerPosition::FrontCenter),
            (SPEAKER_LOW_FREQUENCY, SpeakerPosition::LowFrequency),
            (SPEAKER_BACK_LEFT, SpeakerPosition::BackLeft),
            (SPEAKER_BACK_RIGHT, SpeakerPosition::BackRight),
            (SPEAKER_FRONT_LEFT_OF_CENTER, SpeakerPosition::FrontLeftOfCenter),
            (SPEAKER_FRONT_RIGHT_OF_CENTER, SpeakerPosition::FrontRightOfCenter),
            (SPEAKER_BACK_CENTER, SpeakerPosition::BackCenter),
            (SPEAKER_SIDE_LEFT, SpeakerPosition::SideLeft),
            (SPEAKER_SIDE_RIGHT, SpeakerPosition::SideRight),
        ];

        let mut positions = Vec::with_capacity(channels);
        for (bit, position) in ordered_bits {
            if channel_mask & bit != 0 {
                positions.push(position);
                if positions.len() == channels {
                    return positions;
                }
            }
        }

        while positions.len() < channels {
            positions.push(if positions.len() % 2 == 0 {
                SpeakerPosition::UnknownLeft
            } else {
                SpeakerPosition::UnknownRight
            });
        }

        return positions;
    }

    fallback_positions(channels)
}

fn fallback_positions(channels: usize) -> Vec<SpeakerPosition> {
    match channels {
        1 => vec![SpeakerPosition::FrontCenter],
        2 => vec![SpeakerPosition::FrontLeft, SpeakerPosition::FrontRight],
        3 => vec![
            SpeakerPosition::FrontLeft,
            SpeakerPosition::FrontRight,
            SpeakerPosition::FrontCenter,
        ],
        4 => vec![
            SpeakerPosition::FrontLeft,
            SpeakerPosition::FrontRight,
            SpeakerPosition::BackLeft,
            SpeakerPosition::BackRight,
        ],
        5 => vec![
            SpeakerPosition::FrontLeft,
            SpeakerPosition::FrontRight,
            SpeakerPosition::FrontCenter,
            SpeakerPosition::SideLeft,
            SpeakerPosition::SideRight,
        ],
        6 => vec![
            SpeakerPosition::FrontLeft,
            SpeakerPosition::FrontRight,
            SpeakerPosition::FrontCenter,
            SpeakerPosition::LowFrequency,
            SpeakerPosition::SideLeft,
            SpeakerPosition::SideRight,
        ],
        7 => vec![
            SpeakerPosition::FrontLeft,
            SpeakerPosition::FrontRight,
            SpeakerPosition::FrontCenter,
            SpeakerPosition::LowFrequency,
            SpeakerPosition::BackCenter,
            SpeakerPosition::SideLeft,
            SpeakerPosition::SideRight,
        ],
        8 => vec![
            SpeakerPosition::FrontLeft,
            SpeakerPosition::FrontRight,
            SpeakerPosition::FrontCenter,
            SpeakerPosition::LowFrequency,
            SpeakerPosition::BackLeft,
            SpeakerPosition::BackRight,
            SpeakerPosition::SideLeft,
            SpeakerPosition::SideRight,
        ],
        _ => (0..channels)
            .map(|idx| {
                if idx % 2 == 0 {
                    SpeakerPosition::UnknownLeft
                } else {
                    SpeakerPosition::UnknownRight
                }
            })
            .collect(),
    }
}

/// Attack/release blending: respuesta rápida cuando la señal sube,
/// desvanecimiento lento cuando baja. Hace que los transitorios (pasos)
/// aparezcan instantáneamente y persistan brevemente en pantalla.
fn blend_ar(previous: f32, current: f32, attack: f32, release: f32) -> f32 {
    let alpha = if current > previous { attack } else { release };
    alpha * current + (1.0 - alpha) * previous
}

/// Compresión perceptual: curva gamma que amplifica señales débiles
/// manteniendo las fuertes en el mismo rango.
///
/// ```text
///  RMS input  │  compressed  │  × amp(3.5)  │  visual
/// ────────────┼──────────────┼──────────────┼──────────
///  0.004      │  (gated)     │  0.00        │  silencio
///  0.008      │  0.089       │  0.31        │  visible
///  0.015      │  0.122       │  0.43        │  claro
///  0.050      │  0.224       │  0.78        │  brillante
///  0.082+     │  0.286+      │  1.00        │  máximo
/// ```
fn compress(x: f32) -> f32 {
    if x <= NOISE_FLOOR {
        return 0.0;
    }
    x.powf(PERCEPTUAL_GAMMA)
}

/// Gate post-mezcla: elimina el residuo visual de canales que,
/// tras la combinación en la mixing matrix, quedan con energía
/// insignificante (cross-bleed de canales casi silenciosos).
fn gate(x: f32) -> f32 {
    if x < RADAR_GATE { 0.0 } else { x }
}

impl SpeakerEnergies {
    fn add(&mut self, position: SpeakerPosition, value: f32) {
        match position {
            SpeakerPosition::FrontLeft => self.front_left = self.front_left.max(value),
            SpeakerPosition::FrontRight => self.front_right = self.front_right.max(value),
            SpeakerPosition::FrontCenter => self.front_center = self.front_center.max(value),
            SpeakerPosition::LowFrequency => self.low_frequency = self.low_frequency.max(value),
            SpeakerPosition::BackLeft => self.back_left = self.back_left.max(value),
            SpeakerPosition::BackRight => self.back_right = self.back_right.max(value),
            SpeakerPosition::FrontLeftOfCenter => {
                self.front_left_of_center = self.front_left_of_center.max(value)
            }
            SpeakerPosition::FrontRightOfCenter => {
                self.front_right_of_center = self.front_right_of_center.max(value)
            }
            SpeakerPosition::BackCenter => self.back_center = self.back_center.max(value),
            SpeakerPosition::SideLeft => self.side_left = self.side_left.max(value),
            SpeakerPosition::SideRight => self.side_right = self.side_right.max(value),
            SpeakerPosition::UnknownLeft => self.unknown_left = self.unknown_left.max(value),
            SpeakerPosition::UnknownRight => self.unknown_right = self.unknown_right.max(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASK_5POINT1: u32 = 0x60F;
    const MASK_7POINT1: u32 = 0x63F;

    #[test]
    fn test_silence() {
        let samples = vec![0.0f32; 100];
        let state = analyze_stereo(&samples, 2);
        assert!((state.pan).abs() < 0.01);
        assert!(state.volume < 0.01);
    }

    #[test]
    fn test_full_left() {
        let mut samples = Vec::new();
        for _ in 0..100 {
            samples.push(0.5);
            samples.push(0.0);
        }
        let state = analyze_stereo(&samples, 2);
        assert!(state.pan < -0.9, "Pan should be near -1.0, got {}", state.pan);
    }

    #[test]
    fn test_full_right() {
        let mut samples = Vec::new();
        for _ in 0..100 {
            samples.push(0.0);
            samples.push(0.5);
        }
        let state = analyze_stereo(&samples, 2);
        assert!(state.pan > 0.9, "Pan should be near +1.0, got {}", state.pan);
    }

    #[test]
    fn test_center() {
        let mut samples = Vec::new();
        for _ in 0..100 {
            samples.push(0.5);
            samples.push(0.5);
        }
        let state = analyze_stereo(&samples, 2);
        assert!((state.pan).abs() < 0.01, "Pan should be near 0.0, got {}", state.pan);
    }

    #[test]
    fn test_75_left() {
        let mut samples = Vec::new();
        for _ in 0..100 {
            samples.push(0.75);
            samples.push(0.25);
        }
        let state = analyze_stereo(&samples, 2);
        assert!(state.pan < -0.3, "Pan should be negative, got {}", state.pan);
        assert!(state.pan > -0.8, "Pan should not be fully left, got {}", state.pan);
    }

    #[test]
    fn test_smoothing() {
        let current = AudioState { pan: 1.0, volume: 1.0 };
        let previous = AudioState { pan: 0.0, volume: 0.0 };
        let smoothed = smooth(&current, &previous, 0.3);
        assert!((smoothed.pan - 0.3).abs() < 0.01);
        assert!((smoothed.volume - 0.3).abs() < 0.01);
    }

    #[test]
    fn test_stereo_radar_center_weighted() {
        let radar = analyze_radar(&[0.5, 0.5, 0.5, 0.5], 2, 0);
        assert!(radar.center > 0.3);
        assert!(radar.left > 0.0);
        assert!(radar.right > 0.0);
    }

    #[test]
    fn test_surround_5point1_side_left_hits_far_left() {
        let mut samples = Vec::new();
        for _ in 0..64 {
            samples.extend_from_slice(&[0.0, 0.0, 0.0, 0.0, 0.7, 0.0]);
        }

        let radar = analyze_radar(&samples, 6, MASK_5POINT1);
        assert!(radar.far_left > radar.far_right);
        assert!(radar.far_left > 0.5);
    }

    #[test]
    fn test_surround_7point1_back_right_hits_far_right() {
        let mut samples = Vec::new();
        for _ in 0..64 {
            samples.extend_from_slice(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.8, 0.0, 0.0]);
        }

        let radar = analyze_radar(&samples, 8, MASK_7POINT1);
        assert!(radar.far_right > radar.far_left);
        assert!(radar.far_right > 0.5);
    }

    #[test]
    fn test_surround_7point1_center_hits_center() {
        let mut samples = Vec::new();
        for _ in 0..64 {
            samples.extend_from_slice(&[0.0, 0.0, 0.8, 0.0, 0.0, 0.0, 0.0, 0.0]);
        }

        let radar = analyze_radar(&samples, 8, MASK_7POINT1);
        assert!(radar.center > radar.left);
        assert!(radar.center > radar.right);
        assert!(radar.center > 0.7);
    }
}
